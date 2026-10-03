use omuse::{
    document,
    model::Document,
    vector_path::{Anchor, FillRule, Point, Subpath, VectorPath},
    vector_scene::{TextOnPath, TextPathAlignment, VectorObject, VectorScene, text},
};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn text_recipe_roundtrips_without_reshaping_and_requires_format14() {
    let recipe = TextOnPath {
        text: "Omuse café".into(),
        font_family: "Outfit".into(),
        font_size: 24.,
        letter_spacing: 1.,
        start_offset: 0.5,
        alignment: TextPathAlignment::Center,
        guide: VectorPath {
            subpaths: vec![Subpath {
                closed: false,
                anchors: vec![
                    Anchor {
                        position: Point { x: 10., y: 70. },
                        incoming: None,
                        outgoing: Some(Point { x: 95., y: 10. }),
                    },
                    Anchor {
                        position: Point { x: 310., y: 70. },
                        incoming: Some(Point { x: 225., y: 10. }),
                        outgoing: None,
                    },
                ],
            }],
            fill_rule: FillRule::NonZero,
        },
        transform: [1., 0., 0., 1., 0., 0.],
        resolved_fonts: Vec::new(),
    };
    let object = text::update_object(
        &VectorObject::new(
            "Curved headline",
            VectorPath::default(),
            Some([240, 125, 60, 255]),
            None,
        ),
        &recipe,
        &AtomicBool::new(false),
    )
    .unwrap();
    let scene = VectorScene {
        version: 4,
        width: 320,
        height: 100,
        objects: vec![object],
    };
    let mut doc = Document::new(320, 100);
    doc.layers[0].image = Some(scene.render(&AtomicBool::new(false)).unwrap().into());
    doc.layers[0].vector_scene = Some(Arc::new(scene));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Curved headline.omuse");
    document::save(&doc, &path).unwrap();
    let file = path.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(manifest["version"], 14);
    let restored = document::open(&path).unwrap();
    assert_eq!(restored.layers[0].vector_scene, doc.layers[0].vector_scene);
    assert_eq!(restored.layers[0].image, doc.layers[0].image);
    for version in [11, 12, 13] {
        manifest["version"] = serde_json::json!(version);
        std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            document::open(&path).is_err(),
            "format {version} must not accept editable text"
        );
    }
}
