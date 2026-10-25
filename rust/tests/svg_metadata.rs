use omuse::{
    document,
    editor::Editor,
    model::Document,
    svg_metadata,
    vector_path::Point,
    vector_repeat::{self, RepeatSpec},
    vector_scene_ops,
    vector_svg_scene::{decode_scene_with_report, encode_scene},
};
use std::sync::atomic::AtomicBool;

fn svg(body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:i="{}" xmlns:s="{}" width="32" height="24">{body}</svg>"#,
        svg_metadata::INKSCAPE,
        svg_metadata::SODIPODI
    )
}
fn source() -> String {
    svg(
        r##"<rect id="source" x="2" y="3" width="12" height="10" fill="#c04020" i:label="Hero &amp; headline" s:docname="Studio.svg" data-tag="campaign-01"/>"##,
    )
}

#[test]
fn leaf_metadata_survives_svg_native_group_copy_and_undo_without_changing_pixels() {
    let imported = decode_scene_with_report(source().as_bytes()).unwrap();
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    let scene = imported.scene;
    assert!(scene.has_extended_metadata());
    assert_eq!(scene.version, 6);
    assert_eq!(scene.objects[0].svg_metadata.len(), 3);
    let cancel = AtomicBool::new(false);
    let clean = decode_scene_with_report(
        svg(r##"<rect x="2" y="3" width="12" height="10" fill="#c04020"/>"##).as_bytes(),
    )
    .unwrap()
    .scene;
    assert_eq!(
        scene.render(&cancel).unwrap(),
        clean.render(&cancel).unwrap()
    );
    let encoded = encode_scene(&scene).unwrap();
    let reopened = decode_scene_with_report(encoded.as_bytes()).unwrap().scene;
    assert_eq!(
        reopened.objects[0].svg_metadata,
        scene.objects[0].svg_metadata
    );
    assert_eq!(
        reopened.render(&cancel).unwrap(),
        scene.render(&cancel).unwrap()
    );
    let repeated = vector_repeat::generate(
        &scene.objects,
        &RepeatSpec::Grid {
            columns: 2,
            rows: 1,
            column_step: Point { x: 14., y: 0. },
            row_step: Point { x: 0., y: 0. },
        },
        "cdce0494-0321-4f6d-9917-71e75b53b3b5",
        &cancel,
    )
    .unwrap();
    assert_eq!(repeated.objects.len(), 2);
    assert!(
        repeated
            .objects
            .iter()
            .all(|object| object.svg_metadata == scene.objects[0].svg_metadata)
    );
    let mut copied = scene.clone();
    copied.objects = repeated.objects;
    let (grouped, _) = vector_scene_ops::group(&copied, &[0, 1], "Campaign").unwrap();
    assert!(
        grouped
            .objects
            .iter()
            .all(|object| object.svg_metadata == scene.objects[0].svg_metadata)
    );
    let mut editor = Editor::new(Document::new(32, 24));
    let id = editor
        .insert_vector_scene(
            "Metadata",
            editor.revision(),
            scene.clone(),
            scene.render(&cancel).unwrap(),
        )
        .unwrap();
    editor
        .replace_vector_scene(
            &id,
            editor.revision(),
            grouped.clone(),
            grouped.render(&cancel).unwrap(),
        )
        .unwrap();
    assert!(editor.undo());
    assert_eq!(
        editor
            .document
            .find_layer(&id)
            .unwrap()
            .vector_scene
            .as_deref(),
        Some(&scene)
    );
    assert!(editor.redo());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Metadata.omuse");
    document::save(&editor.document, &path).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], 18);
    let document = document::open(&path).unwrap();
    assert_eq!(
        document.find_layer(&id).unwrap().vector_scene.as_deref(),
        Some(&grouped)
    );
}

#[test]
fn root_and_group_metadata_has_explicit_conversion_notices() {
    let input = source()
        .replace(
            "width=\"32\"",
            "i:version=\"1.4\" s:docname=\"Original.svg\" width=\"32\"",
        )
        .replace("<rect", "<g i:groupmode=\"layer\" i:label=\"Layer\"><rect")
        .replace("</svg>", "</g></svg>");
    let imported = decode_scene_with_report(input.as_bytes()).unwrap();
    assert!(
        imported
            .warnings
            .iter()
            .any(|notice| notice.contains("root, groups"))
    );
    assert_eq!(imported.scene.objects[0].svg_metadata.len(), 3);
    assert_eq!(
        imported.scene.objects[0].svg_metadata[0].value,
        "Hero & headline"
    );
}

#[test]
fn metadata_never_admits_events_foreign_elements_resources_or_geometry_recipes() {
    for attributes in [
        "i:original-d=\"M0 0H2V2Z\"",
        "i:href=\"file:///tmp/a\"",
        "onclick=\"alert(1)\"",
        "data-onload=\"alert(1)\"",
        "data-href=\"https://example.org/a\"",
        "data-tag=\"url(https://example.org/a)\"",
        "data-tag=\"javascript:alert(1)\"",
        "data-tag=\"@import a\"",
        "xmlns:q=\"https://unknown.example/ns\" q:label=\"x\"",
    ] {
        let input = svg(&format!("<rect width=\"8\" height=\"8\" {attributes}/>"));
        assert!(
            decode_scene_with_report(input.as_bytes()).is_err(),
            "{attributes}"
        );
    }
    assert!(
        decode_scene_with_report(
            svg("<i:foreign><rect width='8' height='8'/></i:foreign>").as_bytes()
        )
        .is_err()
    );
    assert!(
        decode_scene_with_report(
            source()
                .replace("<svg ", "<!DOCTYPE svg [<!ENTITY x 'hidden'>]><svg ")
                .as_bytes()
        )
        .is_err()
    );
}

#[test]
fn metadata_budget_and_native_version_guards_hold_before_publication() {
    let too_long = svg(&format!(
        "<rect width='8' height='8' data-tag='{}'/>",
        "x".repeat(svg_metadata::MAX_VALUE_BYTES + 1)
    ));
    assert!(decode_scene_with_report(too_long.as_bytes()).is_err());
    let many = (0..17)
        .map(|i| format!(" data-key-{i}='value'"))
        .collect::<String>();
    assert!(
        decode_scene_with_report(svg(&format!("<rect width='8' height='8'{many}/>")).as_bytes())
            .is_err()
    );
    let bytes = (0..5)
        .map(|i| format!(" data-key-{i}='{}'", "v".repeat(900)))
        .collect::<String>();
    assert!(
        decode_scene_with_report(svg(&format!("<rect width='8' height='8'{bytes}/>")).as_bytes())
            .is_err()
    );
    let body = (0..74)
        .map(|_| {
            format!(
                "<rect width='8' height='8' data-tag='{}'/>",
                "v".repeat(900)
            )
        })
        .collect::<String>();
    let error = decode_scene_with_report(svg(&body).as_bytes())
        .err()
        .expect("document-wide metadata must be rejected");
    assert!(error.to_string().contains("64 KiB"), "{error:#}");
    let mut scene = decode_scene_with_report(source().as_bytes()).unwrap().scene;
    scene.version = 5;
    assert!(scene.validate().is_err());
    scene.objects[0].svg_metadata.clear();
    scene.version = 1;
    scene.validate().unwrap();
    let encoded = serde_json::to_string(&scene).unwrap();
    assert!(!encoded.contains("svg_metadata"));
}
