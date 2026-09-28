//! Human-readable conversion notes retained with imported layers.
use crate::model::{Document, Layer};

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
}
