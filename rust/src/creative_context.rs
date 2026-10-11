//! Bounded text targets for assistant edits to pages other than the active one.
//! Prepared from a private snapshot on the assistant preparation worker. No
//! image bytes, external paths or arbitrary document metadata enter this brief.

use crate::{create_project::Project, model::Layer, objects};
use serde_json::{Value, json};

const MAX_PAGES: usize = 24;
const MAX_LAYERS: usize = 64;
const MAX_TEXT_CHARS: usize = 2048;
const MAX_CONTEXT_BYTES: usize = 64 * 1024;

pub fn project_text_context(project: &Project) -> Value {
    let ids = project.page_ids();
    let mut pages = Vec::new();
    let mut bytes = 0;
    let mut truncated = false;
    for id in ids.iter().filter(|id| *id != project.active_page_id()) {
        if pages.len() == MAX_PAGES {
            truncated = true;
            break;
        }
        let page = project
            .inspect_page_document(id, |document| {
                let mut layers = Vec::new();
                let mut omitted = false;
                describe_text(&document.layers, false, true, &mut layers, &mut omitted);
                Ok(json!({"pageID": id, "layers": layers, "truncated": omitted}))
            })
            .unwrap_or_else(|_| json!({"pageID": id, "unavailable": true}));
        let size = serde_json::to_vec(&page)
            .expect("JSON values serialize")
            .len();
        if bytes + size > MAX_CONTEXT_BYTES {
            truncated = true;
            break;
        }
        bytes += size;
        pages.push(page);
    }
    json!({"pages": pages, "truncated": truncated})
}

fn describe_text(
    layers: &[Layer],
    parent_locked: bool,
    parent_visible: bool,
    output: &mut Vec<Value>,
    truncated: &mut bool,
) {
    for layer in layers {
        let locked = parent_locked || layer.locked;
        let visible = parent_visible && layer.visible;
        if let Ok(Some(style)) = objects::live_text(layer) {
            if output.len() == MAX_LAYERS {
                *truncated = true;
                return;
            }
            let text: String = style.content.chars().take(MAX_TEXT_CHARS).collect();
            output.push(json!({
                "id": layer.id,
                "name": layer.name.chars().take(256).collect::<String>(),
                "textTruncated": text.len() < style.content.len(),
                "text": text,
                "locked": locked,
                "visible": visible,
            }));
        }
        describe_text(&layer.children, locked, visible, output, truncated);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_history::documents_match,
        creative_commands::{CreativeOperation, CreativePlan},
        model::Document,
        objects::{LiveTextStyle, ObjectPoint},
    };

    fn page(text: &str) -> Document {
        let mut doc = Document::new(320, 240);
        doc.layers = vec![
            objects::live_text_layer(
                "Headline",
                ObjectPoint { x: 10., y: 10. },
                LiveTextStyle {
                    content: text.into(),
                    font_size: 20.,
                    box_size: Some(objects::ObjectSize {
                        width: 280.,
                        height: 180.,
                    }),
                    ..Default::default()
                },
            )
            .unwrap(),
        ];
        doc
    }

    #[test]
    fn inactive_page_context_supports_targeted_edit_without_loading_source() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("carousel.omuse");
        let mut project = Project::new("Carousel", page("Cover"));
        let first = project.active_page_id().to_owned();
        for index in 2..=6 {
            project
                .add_page(format!("Page {index}"), page(&format!("Step {index}")))
                .unwrap();
        }
        project.set_active_page(&first).unwrap();
        project.save(&path).unwrap();
        let source = Project::open(&path).unwrap();
        let loaded: Vec<_> = source
            .page_summaries()
            .iter()
            .map(|p| p.is_loaded)
            .collect();
        let context = project_text_context(&source);
        assert_eq!(context["pages"].as_array().unwrap().len(), 5);
        assert_eq!(
            loaded,
            source
                .page_summaries()
                .iter()
                .map(|p| p.is_loaded)
                .collect::<Vec<_>>()
        );
        let target = &context["pages"][1];
        assert_eq!(target["layers"][0]["text"], "Step 3");
        let plan = CreativePlan {
            summary: "Revise page three".into(),
            operations: vec![
                CreativeOperation::SelectPage {
                    page_id: target["pageID"].as_str().unwrap().into(),
                },
                CreativeOperation::SetText {
                    layer_id: target["layers"][0]["id"].as_str().unwrap().into(),
                    content: "Revised step".into(),
                },
            ],
        };
        let mut revised = plan.prepare_project(&source).unwrap();
        let mut original = source.clone();
        for id in source.page_ids() {
            let unchanged = documents_match(
                original.page_document(&id).unwrap(),
                revised.page_document(&id).unwrap(),
            );
            assert_eq!(unchanged, id != target["pageID"].as_str().unwrap());
        }
    }

    #[test]
    fn context_carries_parent_locks_visibility_and_unicode_truncation() {
        let mut doc = page("é".repeat(MAX_TEXT_CHARS + 1).as_str());
        let mut group = Layer::group("Protected");
        group.locked = true;
        group.visible = false;
        group.children = std::mem::take(&mut doc.layers);
        doc.layers.push(group);
        let mut source = Project::new("Context", page("Active"));
        source.add_page("Other", doc).unwrap();
        let context = project_text_context(&source);
        let text = &context["pages"][0]["layers"][0];
        assert_eq!(text["locked"], true);
        assert_eq!(text["visible"], false);
        assert_eq!(text["textTruncated"], true);
        assert_eq!(
            text["text"].as_str().unwrap().chars().count(),
            MAX_TEXT_CHARS
        );
    }

    #[test]
    fn context_marks_omitted_layers_and_keeps_source_metadata_private() {
        let mut source = Project::new("Context", page("Active"));
        let mut other = page("Short copy");
        other.layers[0].metadata["privateNote"] = json!("do not send this metadata");
        let layer = other.layers[0].clone();
        for _ in 1..=MAX_LAYERS {
            let mut copy = layer.clone();
            copy.id = uuid::Uuid::new_v4().to_string();
            other.layers.push(copy);
        }
        source.add_page("Other", other).unwrap();
        let context = project_text_context(&source);
        assert_eq!(
            context["pages"][0]["layers"].as_array().unwrap().len(),
            MAX_LAYERS
        );
        assert_eq!(context["pages"][0]["truncated"], true);
        assert!(!context.to_string().contains("privateNote"));
        assert!(!context.to_string().contains("do not send"));
    }

    #[test]
    fn context_bounds_pages_and_serialized_copy() {
        let mut source = Project::new("Context", page("Active"));
        for i in 0..30 {
            source.add_page(format!("Page {i}"), page("Copy")).unwrap();
        }
        let context = project_text_context(&source);
        assert_eq!(context["pages"].as_array().unwrap().len(), MAX_PAGES);
        assert_eq!(context["truncated"], true);
        for id in source.page_ids().into_iter().skip(1) {
            let doc = source.page_document_mut(&id).unwrap();
            let text = objects::live_text_layer(
                "Long copy",
                ObjectPoint { x: 0., y: 0. },
                LiveTextStyle {
                    content: "界".repeat(MAX_TEXT_CHARS),
                    box_size: Some(objects::ObjectSize {
                        width: 280.,
                        height: 180.,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
            doc.layers = vec![text; MAX_LAYERS + 1];
        }
        let context = project_text_context(&source);
        assert_eq!(context["truncated"], true);
        assert!(serde_json::to_vec(&context).unwrap().len() < MAX_CONTEXT_BYTES + 1024);
    }
}
