use omuse::{
    creative_commands::{CreativePlan, document_context},
    document,
    editor::Editor,
    model::Document,
    vector_commands::{self, VectorCommand},
    vector_scene::{VectorObject, VectorScene},
};
use serde_json::json;
use std::sync::atomic::AtomicBool;

fn artwork() -> VectorScene {
    VectorScene {
        version: 1,
        width: 64,
        height: 48,
        objects: vec![
            VectorObject::rectangle("Left", 5., 5., 24., 24., Some([200, 60, 30, 255]), None)
                .unwrap(),
            VectorObject::rectangle("Right", 17., 13., 24., 24., Some([30, 80, 200, 255]), None)
                .unwrap(),
        ],
    }
}

#[test]
fn typed_command_is_transactional_and_rejects_missing_duplicate_and_cancelled_ids() {
    let source = artwork();
    let ids = source
        .objects
        .iter()
        .map(|o| o.id.clone())
        .collect::<Vec<_>>();
    let result = vector_commands::apply(
        &source,
        &ids,
        &VectorCommand::Unite,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result.scene.objects.len(), 1);
    assert_eq!(source.objects.len(), 2);
    assert!(
        vector_commands::apply(
            &source,
            &[ids[0].clone(), ids[0].clone()],
            &VectorCommand::Unite,
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert!(
        vector_commands::apply(
            &source,
            &["missing".into()],
            &VectorCommand::Simplify { tolerance: 1. },
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert!(
        vector_commands::apply(&source, &ids, &VectorCommand::Unite, &AtomicBool::new(true))
            .is_err()
    );
}

#[test]
fn assistant_vector_edit_is_advertised_prepared_and_saved_as_editable_geometry() {
    let scene = artwork();
    let ids = scene
        .objects
        .iter()
        .map(|o| o.id.clone())
        .collect::<Vec<_>>();
    let mut editor = Editor::new(Document::new(64, 48));
    let id = editor
        .insert_vector_scene(
            "Logo",
            editor.revision(),
            scene.clone(),
            scene.render(&AtomicBool::new(false)).unwrap(),
        )
        .unwrap();
    let context = document_context(&editor.document);
    let described = context["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == id)
        .unwrap();
    assert_eq!(described["photoAdjustable"], false);
    assert_eq!(described["vectorArtwork"]["objects"][0]["id"], ids[0]);
    let plan = CreativePlan::parse(
        &json!({"summary":"Combine logo pieces", "operations":[{
            "type":"edit_vector", "layer_id":id, "object_ids":ids, "command":{"operation":"unite"}
        }]})
        .to_string(),
    )
    .unwrap();
    let result = plan.prepare(&editor.document).unwrap();
    let output = result
        .find_layer(&id)
        .unwrap()
        .vector_scene
        .as_ref()
        .unwrap();
    assert_eq!(output.objects.len(), 1);
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .vector_scene
            .as_ref()
            .unwrap()
            .objects
            .len(),
        2
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("logo.omuse");
    document::save(&result, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    assert_eq!(
        reopened.find_layer(&id).unwrap().vector_scene.as_ref(),
        Some(output)
    );
    editor.document.find_layer_mut(&id).unwrap().locked = true;
    assert!(plan.prepare(&editor.document).is_err());
}

#[test]
fn assistant_geometry_schema_refuses_unbounded_and_unknown_parameters() {
    for command in [
        json!({"operation":"simplify","tolerance":1000}),
        json!({"operation":"offset","distance":1e10}),
        json!({"operation":"unite","shell":"ignored?"}),
    ] {
        let encoded = json!({"summary":"Invalid", "operations":[{"type":"edit_vector", "layer_id":"x", "object_ids":["x"], "command":command}]}).to_string();
        assert!(CreativePlan::parse(&encoded).is_err(), "Accepted {encoded}");
    }
    for operation in [
        "outline_stroke",
        "unite",
        "subtract",
        "intersect",
        "exclude",
        "divide",
    ] {
        let valid = json!({"operation":operation});
        let parsed: VectorCommand = serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), valid);
        assert!(
            serde_json::from_value::<VectorCommand>(json!({
                "operation":operation, "shell":"unexpected"
            }))
            .is_err(),
            "Unexpected field accepted by {operation}"
        );
    }
}

#[test]
fn filled_translucent_stroke_outline_refuses_to_change_overlap_compositing() {
    let mut source = artwork();
    source.objects[0].opacity = 0.5;
    source.objects[0].stroke = Some(omuse::vector_path::StrokeStyle {
        color: [0, 0, 0, 255],
        width: 4.,
    });
    let ids = [source.objects[0].id.clone()];
    let error = vector_commands::apply(
        &source,
        &ids,
        &VectorCommand::OutlineStroke,
        &AtomicBool::new(false),
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("grouped transparency"));
}
