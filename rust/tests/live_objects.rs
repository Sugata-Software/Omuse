use omuse::{
    document,
    model::Document,
    objects::{
        self, LiveShapeKind, LiveShapeStyle, LiveTextStyle, ObjectPoint, ObjectSize, TextAlignment,
    },
};

#[test]
fn swift_live_objects_roundtrip_source_and_cached_pixels() {
    let text = LiveTextStyle {
        runs: vec![],
        content: "Hello\nworld".into(),
        font_name: "sans-serif".into(),
        font_size: 24.,
        red: 0.2,
        green: 0.4,
        blue: 0.8,
        alignment: TextAlignment::Center,
        tracking: 2.,
        leading: 31.,
        box_size: Some(ObjectSize {
            width: 180.,
            height: 90.,
        }),
    };
    let mut text_layer =
        objects::live_text_layer("Text", ObjectPoint { x: 12., y: 18. }, text.clone()).unwrap();
    assert_eq!(objects::live_text(&text_layer).unwrap(), Some(text.clone()));
    assert_eq!(
        text_layer.metadata["text"]["boxSize"],
        serde_json::json!([180.0, 90.0])
    );
    assert!(
        text_layer
            .image
            .as_ref()
            .unwrap()
            .pixels()
            .any(|p| p[3] > 0)
    );
    let shape = LiveShapeStyle {
        kind: LiveShapeKind::Rectangle,
        red: 1.,
        green: 0.25,
        blue: 0.,
        corner_radius: 12.,
        line_width: None,
        start: None,
        end: None,
    };
    let mut shape_layer =
        objects::live_shape_layer("Shape", ObjectPoint { x: 4., y: 5. }, 80, 50, shape.clone())
            .unwrap();
    assert_eq!(objects::live_shape(&shape_layer).unwrap(), Some(shape));
    assert!(shape_layer.metadata["shape"].get("start").is_none());
    let source_id = shape_layer.id.clone();
    shape_layer.metadata["effects"] = serde_json::json!({"stroke":{"size":1.0}});
    text_layer.metadata["maskSourceID"] = serde_json::json!(source_id);
    let mut doc = Document::new(300, 200);
    doc.layers = vec![text_layer, shape_layer];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Live.comp");
    document::save(&doc, &path).unwrap();
    let reopened = document::open(&path).unwrap();
    assert_eq!(objects::live_text(&reopened.layers[0]).unwrap(), Some(text));
    assert_eq!(reopened.layers[0].image, doc.layers[0].image);
    assert_eq!(reopened.layers[1].image, doc.layers[1].image);
}

#[test]
fn malformed_or_ambiguous_sources_are_rejected_without_losing_pixels() {
    let mut layer = objects::live_shape_layer(
        "Line",
        ObjectPoint { x: 0., y: 0. },
        40,
        30,
        LiveShapeStyle {
            kind: LiveShapeKind::Line,
            red: 0.,
            green: 0.,
            blue: 0.,
            corner_radius: 0.,
            line_width: Some(3.),
            start: Some(ObjectPoint { x: 0.1, y: 0.2 }),
            end: Some(ObjectPoint { x: 0.9, y: 0.8 }),
        },
    )
    .unwrap();
    let pixels = layer.image.clone();
    layer.metadata["text"] = serde_json::json!({"fontSize": 0});
    assert!(objects::validate_live_object(&layer).is_err());
    assert_eq!(layer.image, pixels);
    objects::detach_live_object(&mut layer);
    assert!(objects::live_shape(&layer).unwrap().is_none());
    assert_eq!(layer.image, pixels);
}
