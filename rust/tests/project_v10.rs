use image::{Rgba, RgbaImage};
use omuse::{
    document,
    editor::Editor,
    model::{Document, Layer},
    objects,
};
use serde_json::{Value, json};
use std::path::Path;

fn fixture(root: &Path, version: u64, runs: Value) -> (std::path::PathBuf, Vec<u8>) {
    let path = root.join("External.comp");
    let mut doc = Document::new(8, 6);
    let mut layer = Layer::paint("Unicode text", 8, 6);
    layer.image = Some(RgbaImage::from_pixel(8, 6, Rgba([42, 71, 99, 127])).into());
    let style = objects::LiveTextStyle {
        content: "A😀éZ".into(),
        font_name: "sans-serif".into(),
        font_size: 16.,
        ..Default::default()
    };
    layer.metadata["text"] = serde_json::to_value(style).unwrap();
    doc.layers = vec![layer];
    document::save(&doc, &path).unwrap();
    let manifest_path = path.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["version"] = json!(version);
    manifest["layers"][0]["text"]["colorRuns"] = runs;
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    std::fs::write(manifest_path, &bytes).unwrap();
    (path, bytes)
}

fn run(location: usize, length: usize) -> Value {
    json!({"location":location,"length":length,"red":0.2,"green":0.8,"blue":0.4})
}

#[test]
fn format10_converts_utf16_ranges_without_rerendering_or_rewriting_original() {
    let temp = tempfile::tempdir().unwrap();
    let (path, original) = fixture(temp.path(), 10, json!([run(1, 2), run(3, 1)]));
    let doc = document::open(&path).unwrap();
    let layer = &doc.layers[0];
    let style = objects::live_text(layer).unwrap().unwrap();
    assert_eq!((style.runs[0].start, style.runs[0].end), (1, 5));
    assert_eq!((style.runs[1].start, style.runs[1].end), (5, 7));
    assert_eq!(style.runs[0].color, Some([0.2, 0.8, 0.4, 1.]));
    assert!(layer.metadata["text"].get("colorRuns").is_none());
    assert_eq!(
        layer.image.as_ref().unwrap().as_raw(),
        &vec![42, 71, 99, 127].repeat(48)
    );
    let native = temp.path().join("Converted.omuse");
    document::save(&doc, &native).unwrap();
    let reopened = document::open(&native).unwrap();
    assert_eq!(
        objects::live_text(&reopened.layers[0]).unwrap(),
        Some(style.clone())
    );
    assert_eq!(reopened.layers[0].image, layer.image);
    let saved: Value =
        serde_json::from_slice(&std::fs::read(native.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(saved["version"], document::PROJECT_WRITE_VERSION);
    let id = layer.id.clone();
    let mut editor = Editor::new(reopened);
    let mut edited = style;
    edited.font_size = 18.;
    assert!(editor.set_live_text(&id, edited).unwrap());
    assert!(editor.undo());
    assert_eq!(editor.document.find_layer(&id).unwrap().image, layer.image);
    assert_eq!(std::fs::read(path.join("manifest.json")).unwrap(), original);
}

#[test]
fn malformed_legacy_runs_and_unknown_versions_are_rejected_without_disk_changes() {
    for (version, runs) in [
        (9, json!([run(0, 1)])),
        (11, json!([])),
        (10, json!([run(2, 1)])),
        (10, json!([run(1, 1)])),
        (10, json!([run(0, 0)])),
        (10, json!([run(4, 2)])),
        (10, json!([run(1, 2), run(1, 2)])),
        (10, json!([run(3, 1), run(0, 1)])),
        (10, json!([run(usize::MAX, 2)])),
        (
            10,
            json!([{"location":0,"length":1,"red":2,"green":0,"blue":0}]),
        ),
        (
            10,
            json!([{"location":-1,"length":1,"red":0,"green":0,"blue":0}]),
        ),
        (10, json!({"unexpected":"object"})),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (path, original) = fixture(temp.path(), version, runs);
        assert!(
            document::open(&path).is_err(),
            "accepted invalid format {version}"
        );
        assert_eq!(std::fs::read(path.join("manifest.json")).unwrap(), original);
    }
}

#[test]
fn native_and_legacy_colour_encodings_cannot_silently_override_each_other() {
    let temp = tempfile::tempdir().unwrap();
    let (path, _) = fixture(temp.path(), 10, json!([run(0, 1)]));
    let file = path.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    manifest["layers"][0]["text"]["runs"] = json!([{"start":0,"end":1,"weight":700}]);
    std::fs::write(file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("conflicting")
    );
}

#[test]
fn new_mask_coverage_requires_version10_instead_of_a_silent_downgrade() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Mask.omuse");
    let mut doc = Document::new(8, 6);
    doc.layers[0].image = Some(RgbaImage::from_pixel(8, 6, Rgba([40, 80, 120, 255])).into());
    doc.layers[0].mask = Some(RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255])).into());
    doc.layers[0].metadata["maskOutsideCoverage"] = json!(255);
    doc.layers[0].metadata["maskPlacement"] =
        json!({"origin":[2,2],"size":[2,2],"sampling":"Nearest"});
    document::save(&doc, &path).unwrap();
    let file = path.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(manifest["version"], 10);
    let reopened = document::open(&path).unwrap();
    assert_eq!(
        omuse::raster::composite(&reopened),
        omuse::raster::composite(&doc)
    );
    manifest["version"] = json!(9);
    std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("format 10")
    );
}
