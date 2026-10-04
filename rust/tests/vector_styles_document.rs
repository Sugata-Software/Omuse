use omuse::{
    document,
    model::Document,
    vector_path::Point,
    vector_scene::{
        GradientFill, GradientKind, GradientSpread, GradientStop, VectorObject, VectorScene,
    },
};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn styled_scene_roundtrips_as_format13_and_cannot_be_read_as_format12() {
    let mut object =
        VectorObject::rectangle("Gradient", 0., 0., 16., 12., Some([255, 0, 0, 255]), None)
            .unwrap();
    object.fill_gradient = Some(GradientFill {
        kind: GradientKind::Linear {
            start: Point { x: 0., y: 0. },
            end: Point { x: 16., y: 0. },
        },
        stops: vec![
            GradientStop {
                offset: 0.,
                color: [240, 60, 30, 255],
            },
            GradientStop {
                offset: 1.,
                color: [20, 80, 200, 128],
            },
        ],
        spread: GradientSpread::Pad,
        transform: [1., 0., 0., 1., 0., 0.],
    });
    let scene = VectorScene {
        version: 3,
        width: 16,
        height: 12,
        objects: vec![object],
    };
    let mut doc = Document::new(16, 12);
    doc.layers[0].image = Some(scene.render(&AtomicBool::new(false)).unwrap().into());
    doc.layers[0].vector_scene = Some(Arc::new(scene));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Gradient.omuse");
    document::save(&doc, &path).unwrap();
    let file = path.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(manifest["version"], 13);
    assert_eq!(manifest["rustVectorSceneCount"], 1);
    let restored = document::open(&path).unwrap();
    assert_eq!(restored.layers[0].vector_scene, doc.layers[0].vector_scene);
    assert_eq!(restored.layers[0].image, doc.layers[0].image);
    manifest["version"] = serde_json::json!(12);
    std::fs::write(file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        document::open(&path)
            .unwrap_err()
            .to_string()
            .contains("format 13")
    );
}
