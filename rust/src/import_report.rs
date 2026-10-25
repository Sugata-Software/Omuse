//! Human-readable conversion notes retained with imported layers.
use crate::model::{Document, Layer};

/// Cheap inspector admission check; report formatting belongs to the action.
pub fn has_conversion_notes(doc: &Document) -> bool {
    fn visit(layers: &[Layer], depth: usize) -> bool {
        depth < 64
            && layers.iter().any(|layer| {
                layer
                    .metadata
                    .get("psdConversions")
                    .and_then(|v| v.as_array())
                    .is_some_and(|notes| notes.iter().any(|note| note.as_str().is_some()))
                    || layer
                        .metadata
                        .get(crate::compatibility_report::METADATA_KEY)
                        .is_some()
                    || visit(&layer.children, depth + 1)
            })
    }
    visit(&doc.layers, 0)
}

/// Bound display work even for documents with many imported objects.
pub fn conversion_notes(doc: &Document) -> Vec<String> {
    fn visit(layers: &[Layer], depth: usize, output: &mut Vec<String>) {
        if depth >= 64 || output.len() >= 256 {
            return;
        }
        for layer in layers {
            if output.len() >= 256 {
                break;
            }
            if let Some(notes) = layer
                .metadata
                .get("psdConversions")
                .and_then(|v| v.as_array())
            {
                for note in notes.iter().filter_map(|v| v.as_str()) {
                    if output.len() >= 256 {
                        break;
                    }
                    output.push(format!(
                        "{}: {}",
                        layer.name.chars().take(120).collect::<String>(),
                        note.chars().take(600).collect::<String>()
                    ));
                }
            }
            if output.len() < 256
                && let Some(report) = crate::compatibility_report::from_metadata(&layer.metadata)
            {
                output.push(format!(
                    "Layer: {}",
                    layer
                        .name
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(120)
                        .collect::<String>()
                ));
                for note in report.lines() {
                    if output.len() >= 256 {
                        break;
                    }
                    output.push(note);
                }
            }
            visit(&layer.children, depth + 1, output);
        }
    }
    let mut notes = Vec::new();
    visit(&doc.layers, 0, &mut notes);
    notes
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_notes_are_preserved_with_layer_names_and_bounded() {
        let mut doc = Document::new(2, 2);
        let mut group = Layer::group("Folder");
        let mut layer = Layer::group("Title");
        layer.metadata["psdConversions"] = serde_json::json!(["Text imported as cached pixels"]);
        group.children.push(layer);
        doc.layers.push(group);
        assert_eq!(
            conversion_notes(&doc),
            ["Title: Text imported as cached pixels"]
        );
        doc.layers[1].metadata["psdConversions"] = serde_json::json!(vec!["conversion"; 300]);
        assert_eq!(conversion_notes(&doc).len(), 256);
    }

    #[test]
    fn retained_typed_reports_keep_layer_context_and_direct_category_prefixes() {
        use crate::compatibility_report::{CompatibilityReport, METADATA_KEY};
        let mut doc = Document::new(2, 2);
        let report = CompatibilityReport::svg_import(
            std::path::Path::new("brand.svg"),
            None,
            &["Text converted to outlines".into()],
        );
        let mut layer = Layer::group("Clipped gradient");
        layer.metadata[METADATA_KEY] = serde_json::to_value(&report).unwrap();
        let mut folder = Layer::group("Imported artwork");
        folder.children.push(layer.clone());
        doc.layers.push(folder);
        let notes = conversion_notes(&doc);
        assert_eq!(notes[0], "Layer: Clipped gradient");
        assert_eq!(&notes[1..], report.lines());
        assert!(notes.iter().any(|line| line.starts_with("Preserved: ")));
        assert!(notes.iter().any(|line| line.starts_with("Converted: ")));
        assert!(notes.iter().any(|line| line.starts_with("Not included: ")));
        assert!(notes.iter().any(|line| line == "File: brand.svg"));
        assert!(
            !notes
                .iter()
                .any(|line| line.starts_with("Clipped gradient:"))
        );

        // Repeated layer reports share the same global display budget.
        doc.layers[1].children = vec![layer; 100];
        assert_eq!(conversion_notes(&doc).len(), 256);
    }

    #[test]
    fn typed_report_layer_headings_and_traversal_remain_bounded() {
        use crate::compatibility_report::{CompatibilityReport, METADATA_KEY};
        let report = CompatibilityReport::svg_import(std::path::Path::new("brand.svg"), None, &[]);
        let mut layer = Layer::group(format!("{}\n", "µ".repeat(200)));
        layer.metadata[METADATA_KEY] = serde_json::to_value(&report).unwrap();
        let mut doc = Document::new(2, 2);
        doc.layers.push(layer.clone());
        assert_eq!(
            conversion_notes(&doc)[0],
            format!("Layer: {}", "µ".repeat(120))
        );
        // A report below the existing 64-level display boundary is not visited.
        for _ in 0..64 {
            let mut parent = Layer::group("Nested");
            parent.children.push(layer);
            layer = parent;
        }
        doc.layers = vec![layer];
        assert!(conversion_notes(&doc).is_empty());
    }
}
