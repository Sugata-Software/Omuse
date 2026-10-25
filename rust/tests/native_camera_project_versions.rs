//! Selective reader admission: retained native camera edits must not masquerade
//! as an older byte-compatible project, even while a node is disabled.
use image::Rgba;
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, FilterNode},
    camera_raw::Settings,
    document,
    filters::Filter,
    model::{Document, Layer},
    precision::Rgba16Image,
    vector_path::Point,
    vector_scene::{
        GradientFill, GradientKind, GradientMethod, GradientSpread, GradientStop, VectorObject,
        VectorScene,
    },
};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
fn photo() -> LayerState {
    LayerState::from_rgba16(
        &Rgba16Image::from_fn(8, 6, |x, y| {
            Rgba([
                10001 + x as u16 * 117,
                20003 + y as u16 * 139,
                30007,
                [0, 1, 32768, 65535][((x + y) % 4) as usize],
            ])
        }),
        "Exact source",
    )
    .unwrap()
}
fn camera_node(settings: Settings, enabled: bool, opacity: f32) -> FilterNode {
    FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Camera Raw".into(),
        enabled,
        opacity,
        operation: AdvancedOperation::CameraRaw(settings),
        soft_mask: None,
    }
}
fn document_for(state: LayerState) -> Document {
    let mut d = Document::new(8, 6);
    d.layers[0].image = Some(state.proxy().unwrap().into());
    d.layers[0].advanced = Some(Arc::new(state));
    d
}
fn version(path: &Path) -> u64 {
    serde_json::from_slice::<serde_json::Value>(&std::fs::read(path.join("manifest.json")).unwrap())
        .unwrap()["version"]
        .as_u64()
        .unwrap()
}
#[test]
fn ordinary_retained_words_and_legacy_camera_nodes_keep_the_prior_format() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.omuse");
    let mut state = photo();
    document::save(&document_for(state.clone()), &path).unwrap();
    assert_eq!(version(&path), 10);
    let missing: Settings = serde_json::from_value(serde_json::json!({"exposure":0.5})).unwrap();
    assert!(
        serde_json::to_value(&missing)
            .unwrap()
            .get("processing")
            .is_none()
    );
    state.recipe.nodes.push(camera_node(missing, false, 0.));
    let words = state.source.to_rgba16();
    document::save(&document_for(state), &path).unwrap();
    assert_eq!(version(&path), 10);
    let reopened = document::open(&path).unwrap();
    assert_eq!(
        reopened.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        words
    );
}
#[test]
fn inactive_or_zero_opacity_native_nodes_require_19_and_a_lowered_header_is_refused() {
    for (enabled, opacity) in [(false, 1.), (true, 0.)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native.omuse");
        let mut state = photo();
        state.recipe.nodes.push(camera_node(
            Settings {
                exposure: 0.7,
                ..Settings::for_native_edit()
            },
            enabled,
            opacity,
        ));
        let expected = state.result.to_rgba16();
        document::save(&document_for(state), &path).unwrap();
        assert_eq!(version(&path), 19);
        let reopened = document::open(&path).unwrap();
        assert_eq!(
            reopened.layers[0]
                .advanced
                .as_ref()
                .unwrap()
                .evaluate(&AtomicBool::new(false))
                .unwrap()
                .result
                .to_rgba16(),
            expected
        );
        let manifest = path.join("manifest.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
        value["version"] = 18.into();
        std::fs::write(manifest, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            document::open(&path)
                .unwrap_err()
                .to_string()
                .contains("format 19")
        );
    }
}
#[test]
fn native_camera_priority_exceeds_embedded_looks_and_extended_vector_paints() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.omuse");
    let mut state = photo();
    state
        .recipe
        .nodes
        .push(camera_node(Settings::for_native_edit(), false, 0.));
    state.recipe.nodes.push(FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Embedded look".into(),
        enabled: false,
        opacity: 0.,
        soft_mask: None,
        operation: AdvancedOperation::Filter(Filter::CubeLook(omuse::look_lut::Settings::new(
            omuse::look_lut::parse("LUT_1D_SIZE 2\n0 0 0\n1 1 1\n", "Identity").unwrap(),
        ))),
    });
    let mut doc = document_for(state);
    let mut object =
        VectorObject::rectangle("Perceptual", 0., 0., 8., 6., Some([0, 0, 255, 255]), None)
            .unwrap();
    object.fill_gradient = Some(GradientFill {
        kind: GradientKind::Linear {
            start: Point { x: 0., y: 0. },
            end: Point { x: 8., y: 0. },
        },
        stops: vec![
            GradientStop {
                offset: 0.,
                color: [0, 0, 255, 255],
            },
            GradientStop {
                offset: 1.,
                color: [255, 255, 0, 255],
            },
        ],
        spread: GradientSpread::Pad,
        transform: [1., 0., 0., 1., 0., 0.],
        method: GradientMethod::Perceptual,
        dither: true,
    });
    let scene = VectorScene {
        version: 6,
        width: 8,
        height: 6,
        objects: vec![object],
    };
    let mut layer = Layer::paint("Vector", 8, 6);
    layer.image = Some(scene.render(&AtomicBool::new(false)).unwrap().into());
    layer.vector_scene = Some(Arc::new(scene));
    doc.layers.push(layer);
    document::save(&doc, &path).unwrap();
    assert_eq!(version(&path), 19);
    let reopened = document::open(&path).unwrap();
    assert_eq!(
        omuse::raster::composite(&reopened),
        omuse::raster::composite(&doc)
    );
}
