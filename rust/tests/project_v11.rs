use image::{Rgba, RgbaImage};
use omuse::{document, editor::Editor, model::Document, objects};
use serde_json::{Value, json};
use std::path::Path;

fn fixture(root: &Path, colors: Value, fonts: Value) -> (std::path::PathBuf, Vec<u8>) {
    let path = root.join("External.comp");
    let mut doc = Document::new(8, 6);
    let layer = &mut doc.layers[0];
    layer.image = Some(RgbaImage::from_pixel(8, 6, Rgba([42, 71, 99, 127])).into());
    layer.metadata["text"] = serde_json::to_value(objects::LiveTextStyle {
        content: "A😀éZ".into(),
        font_name: "sans-serif".into(),
        font_size: 16.,
        ..Default::default()
    })
    .unwrap();
    document::save(&doc, &path).unwrap();
    let file = path.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    manifest["version"] = json!(11);
    manifest["layers"][0]["text"]["colorRuns"] = colors;
    manifest["layers"][0]["text"]["fontRuns"] = fonts;
    let original = serde_json::to_vec(&manifest).unwrap();
    std::fs::write(file, &original).unwrap();
    (path, original)
}

fn font(location: usize, length: usize, name: &str) -> Value {
    json!({"location": location, "length": length, "fontName": name})
}

#[test]
fn independent_font_and_colour_ranges_combine_without_changing_cached_artwork() {
    let temp = tempfile::tempdir().unwrap();
    let (path, original) = fixture(
        temp.path(),
        json!([{"location": 1, "length": 3, "red": 0.2, "green": 0.8, "blue": 0.4}]),
        json!([font(0, 3, "serif"), font(3, 2, "monospace")]),
    );
    let doc = document::open(&path).unwrap();
    let layer = &doc.layers[0];
    let style = objects::live_text(layer).unwrap().unwrap();
    let expected = [
        (0, 1, "serif", None),
        (1, 5, "serif", Some([0.2, 0.8, 0.4, 1.])),
        (5, 7, "monospace", Some([0.2, 0.8, 0.4, 1.])),
        (7, 8, "monospace", None),
    ];
    assert_eq!(style.runs.len(), expected.len());
    for (run, (start, end, font, color)) in style.runs.iter().zip(expected) {
        assert_eq!(
            (run.start, run.end, run.font_name.as_deref(), run.color),
            (start, end, Some(font), color)
        );
    }
    assert!(layer.metadata["text"].get("fontRuns").is_none());
    assert!(layer.metadata["text"].get("colorRuns").is_none());
    assert_eq!(
        layer.image.as_ref().unwrap().as_raw(),
        &vec![42, 71, 99, 127].repeat(48)
    );
    let saved = temp.path().join("Converted.omuse");
    document::save(&doc, &saved).unwrap();
    let reopened = document::open(&saved).unwrap();
    assert_eq!(
        objects::live_text(&reopened.layers[0]).unwrap(),
        Some(style.clone())
    );
    assert_eq!(reopened.layers[0].image, layer.image);
    let mut editor = Editor::new(reopened);
    let mut changed = style;
    changed.font_size = 18.;
    assert!(editor.set_live_text(&layer.id, changed).unwrap());
    assert!(editor.undo());
    assert_eq!(
        editor.document.find_layer(&layer.id).unwrap().image,
        layer.image
    );
    assert_eq!(std::fs::read(path.join("manifest.json")).unwrap(), original);
}

#[test]
fn ordinary_external_format11_needs_neither_font_runs_nor_vector_assets() {
    for runs in [Value::Null, json!([]), json!([font(1, 2, "serif")])] {
        let temp = tempfile::tempdir().unwrap();
        let (path, _) = fixture(temp.path(), Value::Null, runs);
        let doc = document::open(&path).unwrap();
        assert!(doc.layers[0].vector_scene.is_none());
    }
}

#[test]
fn malformed_font_runs_fail_without_rewriting_the_source() {
    for runs in [
        json!([font(2, 1, "serif")]),
        json!([font(1, 1, "serif")]),
        json!([font(0, 0, "serif")]),
        json!([font(4, 2, "serif")]),
        json!([font(0, 3, "serif"), font(1, 2, "monospace")]),
        json!([font(3, 1, "serif"), font(0, 1, "monospace")]),
        json!([font(usize::MAX, 2, "serif")]),
        json!([font(0, 1, " ")]),
        json!([font(0, 1, &"x".repeat(513))]),
        json!([{"location": -1, "length": 1, "fontName": "serif"}]),
        json!([{"location": 0, "length": 1, "fontName": 42}]),
        json!({}),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (path, original) = fixture(temp.path(), Value::Null, runs);
        assert!(document::open(&path).is_err());
        assert_eq!(std::fs::read(path.join("manifest.json")).unwrap(), original);
    }
}

#[test]
fn font_runs_require_version11_and_cannot_override_native_runs() {
    for (version, native) in [
        (10, Value::Null),
        (11, json!([{"start":0,"end":1,"weight":700}])),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (path, _) = fixture(temp.path(), Value::Null, json!([font(0, 1, "serif")]));
        let file = path.join("manifest.json");
        let mut manifest: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        manifest["version"] = json!(version);
        if !native.is_null() {
            manifest["layers"][0]["text"]["runs"] = native;
        }
        std::fs::write(file, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(document::open(&path).is_err());
    }
}

#[test]
fn external_compatibility_never_discards_an_unreferenced_native_scene() {
    let temp = tempfile::tempdir().unwrap();
    let (path, _) = fixture(temp.path(), Value::Null, Value::Null);
    let file = path.join("manifest.json");
    let manifest: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let id = manifest["layers"][0]["id"].as_str().unwrap();
    std::fs::write(
        path.join("images")
            .join(format!("{id}.vector-scene.json.z")),
        b"unreferenced",
    )
    .unwrap();
    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("Unreferenced")
    );
}

#[test]
fn an_incomplete_native_scene_inventory_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let (path, _) = fixture(temp.path(), Value::Null, Value::Null);
    let file = path.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    manifest["rustVectorSceneCount"] = json!(1);
    std::fs::write(file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("inventory")
    );
}
