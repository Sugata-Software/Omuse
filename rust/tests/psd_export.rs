//! Adapter interoperability tests use the existing, separately implemented
//! Omuse PSD reader and explicit expected pixels. They are not Photoshop QA.
use image::{Rgba, RgbaImage};
use omuse::{
    model::{Document, Layer},
    psd, psd_export, raster,
};
use serde_json::json;
use std::sync::{Arc, atomic::AtomicBool};

fn pixels(name: &str, width: u32, height: u32, color: [u8; 4]) -> Layer {
    let mut layer = Layer::paint(name, width, height);
    layer.image = Some(RgbaImage::from_pixel(width, height, Rgba(color)).into());
    layer
}

fn reopen(encoded: &[u8]) -> Document {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Exchange.psd");
    std::fs::write(&path, encoded).unwrap();
    psd::open(&path).unwrap()
}

fn assert_merged_preview_appearance(stored: &[u8], expected: &[u8]) {
    assert_eq!(stored.len(), expected.len());
    for (stored, expected) in stored.chunks_exact(4).zip(expected.chunks_exact(4)) {
        assert_eq!(stored[3], expected[3]);
        let alpha = f64::from(expected[3]) / 255.;
        if alpha == 0. {
            continue;
        }
        for channel in 0..3 {
            // Independently reconstruct straight colour using a PSD reader's
            // white-matte removal. At most half a stored 8-bit step is lost;
            // judging RGB without alpha would exaggerate low-coverage errors.
            let straight =
                ((f64::from(stored[channel]) - 255. * (1. - alpha)) / alpha).clamp(0., 255.);
            assert!(
                (straight - f64::from(expected[channel])).abs() * alpha <= 0.500_001,
                "merged preview colour changed: stored={stored:?}, expected={expected:?}"
            );
        }
    }
}

#[test]
fn transparent_edges_have_a_white_matted_preview_and_exact_straight_layer_pixels() {
    let samples = [
        [210, 96, 61, 31],
        [64, 128, 192, 128],
        [0, 0, 0, 1],
        [3, 19, 247, 254],
        [21, 55, 89, 255],
        [17, 23, 31, 0],
    ];
    let mut doc = Document::new(samples.len() as u32, 1);
    doc.background = [0; 4];
    doc.layers[0].image =
        Some(RgbaImage::from_fn(samples.len() as u32, 1, |x, _| Rgba(samples[x as usize])).into());
    let original = doc.layers[0].image.as_ref().unwrap().as_raw().clone();
    let appearance = raster::composite(&doc);
    let encoded = psd_export::encode_document(&doc, &AtomicBool::new(false)).unwrap();
    let parsed = photocraft_psd::PsdFile::from_bytes(&encoded.bytes).unwrap();
    assert!(parsed.merged_has_alpha());
    assert_eq!(parsed.layer(0).unwrap().rgba8().unwrap().data, original);
    let stored = parsed.composite_rgba8().unwrap().data;
    // Fixed independently specified samples catch straight/premultiplied RGB,
    // wrong matte colour, wrong channel order and early low-alpha clipping.
    assert_eq!(
        stored,
        [
            250, 236, 231, 31, 159, 191, 223, 128, 254, 254, 254, 1, 4, 20, 247, 254, 21, 55, 89,
            255, 0, 0, 0, 0,
        ]
    );
    assert_merged_preview_appearance(&stored, appearance.as_raw());
    assert_eq!(raster::composite(&reopen(&encoded.bytes)), appearance);
    assert_eq!(doc.layers[0].image.as_ref().unwrap().as_raw(), &original);
}

#[test]
fn conversion_report_includes_later_layers_beyond_256_notes() {
    let mut doc = Document::new(2, 2);
    doc.layers = (0..300)
        .map(|index| {
            let mut layer = pixels(&format!("Detail {index}"), 2, 2, [30, 60, 90, 255]);
            layer.advanced = Some(Arc::new(
                omuse::advanced::LayerState::from_image(
                    layer.image.as_ref().unwrap(),
                    "report fixture",
                )
                .unwrap(),
            ));
            layer
        })
        .collect();
    let encoded = psd_export::encode_document(&doc, &AtomicBool::new(false)).unwrap();
    assert_eq!(encoded.report.layer_count, 300);
    assert_eq!(encoded.report.warnings.len(), 301);
    assert!(
        encoded
            .report
            .warnings
            .last()
            .unwrap()
            .starts_with("Detail 299:")
    );
}

#[test]
fn nested_layers_offsets_visibility_masks_names_and_composite_survive_exchange() {
    let mut doc = Document::new(40, 30);
    doc.layers.clear();
    doc.metadata["resolution"] = json!(144);
    doc.background = [18, 27, 45, 255];
    let mut outer = Layer::group("Social campaign — café");
    let mut inner = Layer::group("Details");
    let mut photo = pixels("Photo outside canvas", 24, 18, [200, 130, 70, 210]);
    photo.offset_x = -3.;
    photo.offset_y = 7.;
    photo.opacity = 170. / 255.;
    photo.blend_mode = "Multiply".into();
    photo.mask =
        Some(RgbaImage::from_fn(24, 18, |x, _| Rgba([if x < 8 { 0 } else { 255 }; 4])).into());
    // A mask uses opaque grayscale samples even where coverage is zero.
    for p in photo.mask.as_mut().unwrap().pixels_mut() {
        p[3] = 255;
    }
    photo.metadata["maskOutsideCoverage"] = json!(255);
    photo.metadata["maskPlacement"] = json!({"origin":[-3,7],"size":[24,18],"sampling":"Nearest"});
    let original_pixels = photo.image.as_ref().unwrap().as_raw().clone();
    let original_mask = photo.mask.as_ref().unwrap().as_raw().clone();
    inner.children.push(photo);
    outer.children.push(inner);
    let mut hidden = pixels("Hidden alternative", 8, 8, [0, 255, 0, 255]);
    hidden.visible = false;
    doc.layers.extend([outer, hidden]);
    let expected = raster::composite(&doc);
    let encoded = psd_export::encode_document(&doc, &AtomicBool::new(false)).unwrap();
    assert_eq!(encoded.report.layer_count, 3);
    assert_eq!(encoded.report.group_count, 2);
    assert!(
        encoded
            .report
            .warnings
            .iter()
            .any(|x| x.contains("bottom pixel layer"))
    );
    let reopened = reopen(&encoded.bytes);
    assert_eq!(reopened.layers[1].name, "Social campaign — café");
    let imported = &reopened.layers[1].children[0].children[0];
    assert_eq!((imported.offset_x, imported.offset_y), (-3., 7.));
    assert_eq!(imported.blend_mode, "Multiply");
    assert_eq!(imported.image.as_ref().unwrap().as_raw(), &original_pixels);
    assert_eq!(imported.mask.as_ref().unwrap().as_raw(), &original_mask);
    assert!(!reopened.layers[2].visible);
    assert_eq!(reopened.metadata["resolution"], 144.);
    assert_eq!(raster::composite(&reopened), expected);
    let parsed = photocraft_psd::PsdFile::from_bytes(&encoded.bytes).unwrap();
    assert_eq!(parsed.composite_rgba8().unwrap().data, *expected.as_raw());
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(
        doc.layers[0].children[0].children[0]
            .image
            .as_ref()
            .unwrap()
            .as_raw(),
        &original_pixels
    );
}

#[test]
fn live_text_vector_and_local_effects_are_reported_and_keep_their_visible_pixels() {
    use omuse::objects::{LiveTextStyle, ObjectPoint};
    let mut doc = Document::new(160, 90);
    doc.layers.clear();
    let text = omuse::objects::live_text_layer(
        "Headline",
        ObjectPoint { x: 5., y: 6. },
        LiveTextStyle {
            content: "Omuse".into(),
            font_name: "Outfit".into(),
            font_size: 22.,
            red: 0.8,
            ..Default::default()
        },
    )
    .unwrap();
    let scene = omuse::vector_svg_scene::decode_scene(br##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><circle cx="115" cy="45" r="20" fill="#e77947"/></svg>"##).unwrap();
    let mut vector = Layer::paint("Vector badge", 160, 90);
    vector.image = Some(scene.render(&AtomicBool::new(false)).unwrap().into());
    vector.vector_scene = Some(Arc::new(scene));
    let mut card = pixels("Styled card", 36, 20, [34, 67, 81, 255]);
    card.offset_x = 12.;
    card.offset_y = 55.;
    card.metadata["effects"] = json!({"shadow":{"distance":2.0,"blur":1.0,"angle":90.0,"opacity":0.4},"stroke":{"size":2.0,"red":0.9,"green":0.6,"blue":0.2}});
    doc.layers.extend([vector, text, card]);
    let expected = raster::composite(&doc);
    let encoded = psd_export::encode_document(&doc, &AtomicBool::new(false)).unwrap();
    let warnings = encoded.report.warnings.join(" ");
    assert!(warnings.contains("Text was converted"));
    assert!(warnings.contains("Vector artwork was converted"));
    assert!(warnings.contains("Layer effects"));
    let reopened = reopen(&encoded.bytes);
    assert_eq!(raster::composite(&reopened), expected);
    assert!(reopened.layers.iter().all(|x| x.vector_scene.is_none()
        && x.metadata.get("text").is_none()
        && x.metadata.get("effects").is_none()));
    assert!(doc.layers[0].vector_scene.is_some());
    assert!(doc.layers[1].metadata.get("text").is_some());
    assert!(doc.layers[2].metadata.get("effects").is_some());
}

#[test]
fn supported_blend_keys_roundtrip_without_being_silently_replaced() {
    for mode in [
        "Normal",
        "Multiply",
        "Screen",
        "Overlay",
        "Darken",
        "Lighten",
        "Color Dodge",
        "Color Burn",
        "Hard Light",
        "Soft Light",
        "Difference",
        "Exclusion",
        "Linear Dodge (Add)",
        "Subtract",
        "Linear Burn",
        "Vivid Light",
        "Linear Light",
        "Pin Light",
        "Hard Mix",
        "Divide",
        "Hue",
        "Saturation",
        "Color",
        "Luminosity",
    ] {
        let mut doc = Document::new(3, 2);
        doc.layers[0] = pixels("Base", 3, 2, [42, 110, 207, 255]);
        let mut top = pixels("Blend", 3, 2, [190, 61, 84, 180]);
        top.blend_mode = mode.into();
        doc.layers.push(top);
        let encoded = psd_export::encode_document(&doc, &AtomicBool::new(false)).unwrap();
        let imported = reopen(&encoded.bytes);
        assert_eq!(imported.layers[1].blend_mode, mode);
        assert_eq!(
            raster::composite(&imported),
            raster::composite(&doc),
            "{mode}"
        );
    }
}

#[test]
fn opacity_precision_and_editable_high_precision_source_conversion_are_visible() {
    let mut doc = Document::new(2, 2);
    let mut layer = pixels("Editable original", 2, 2, [123, 64, 210, 200]);
    layer.opacity = 0.5;
    layer.advanced = Some(Arc::new(
        omuse::advanced::LayerState::from_image(layer.image.as_ref().unwrap(), "Original").unwrap(),
    ));
    let original = layer.advanced.clone().unwrap();
    doc.layers[0] = layer;
    let encoded = psd_export::encode_document(&doc, &AtomicBool::new(false)).unwrap();
    let warnings = encoded.report.warnings.join(" ");
    assert!(warnings.contains("8-bit appearance"));
    assert!(warnings.contains("Opacity was rounded"));
    let imported = reopen(&encoded.bytes);
    assert_eq!(imported.layers[0].opacity, 128. / 255.);
    let parsed = photocraft_psd::PsdFile::from_bytes(&encoded.bytes).unwrap();
    assert_merged_preview_appearance(
        &parsed.composite_rgba8().unwrap().data,
        raster::composite(&imported).as_raw(),
    );
    assert!(Arc::ptr_eq(
        doc.layers[0].advanced.as_ref().unwrap(),
        &original
    ));
    assert_eq!(doc.layers[0].opacity, 0.5);
}

#[test]
fn disabled_and_offset_pixel_masks_remain_separate_mask_channels() {
    let mut doc = Document::new(10, 10);
    let mut layer = pixels("Offset mask", 6, 7, [200, 80, 30, 255]);
    layer.offset_x = 2.;
    layer.offset_y = 1.;
    layer.mask = Some(RgbaImage::from_pixel(3, 4, Rgba([0, 0, 0, 255])).into());
    layer.metadata = json!({"maskEnabled":false,"maskOutsideCoverage":255,"maskPlacement":{"origin":[3,2],"size":[3,4],"sampling":"Nearest"}});
    doc.layers[0] = layer;
    let encoded = psd_export::encode_document(&doc, &AtomicBool::new(false)).unwrap();
    let imported = reopen(&encoded.bytes);
    assert_eq!(imported.layers[0].metadata["maskEnabled"], false);
    assert_eq!(
        imported.layers[0].metadata["maskPlacement"]["origin"],
        json!([3, 2])
    );
    assert_eq!(
        imported.layers[0].mask.as_ref().unwrap().dimensions(),
        (3, 4)
    );
    assert_eq!(raster::composite(&imported), raster::composite(&doc));
}

#[test]
fn unsupported_semantics_fail_before_any_destination_is_created() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Refused.psd");
    let mut cases = Vec::new();
    let base = Document::new(4, 4);
    let mut d = base.clone();
    d.layers[0].scale_x = 2.;
    cases.push(d);
    let mut d = base.clone();
    d.layers[0].offset_x = 0.5;
    cases.push(d);
    let mut d = base.clone();
    d.layers[0].metadata["adjustment"] = json!({"kind":"Invert"});
    cases.push(d);
    let mut d = base.clone();
    d.layers[0].metadata["maskSourceID"] = json!("other");
    cases.push(d);
    let mut d = base.clone();
    d.layers[0].blend_mode = "Unsupported".into();
    cases.push(d);
    let mut d = base.clone();
    let mut g = Layer::group("Faded group");
    g.opacity = 0.5;
    g.children = d.layers;
    d.layers = vec![g];
    cases.push(d);
    let mut d = base.clone();
    d.layers[0].mask = Some(RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255])).into());
    cases.push(d);
    let mut d = base.clone();
    d.layers[0].metadata["effects"] = json!({"shadow":{"distance":5000,"blur":500}});
    cases.push(d);
    let mut d = base.clone();
    d.width = 6000;
    d.height = 6000;
    cases.push(d); // admission before allocating composite
    for doc in cases {
        assert!(psd_export::prepare_export(&path, &doc, &AtomicBool::new(false)).is_err());
        assert!(!path.exists());
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn preparation_cancellation_drop_and_raced_publication_leave_existing_files_intact() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Layers.psd");
    let doc = Document::new(4, 4);
    assert!(psd_export::prepare_export(&path, &doc, &AtomicBool::new(true)).is_err());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    let prepared = psd_export::prepare_export(&path, &doc, &AtomicBool::new(false)).unwrap();
    assert!(!path.exists());
    drop(prepared);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    let prepared = psd_export::prepare_export(&path, &doc, &AtomicBool::new(false)).unwrap();
    std::fs::write(&path, b"keep original").unwrap();
    assert!(prepared.prepared.publish().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep original");
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    let good = temp.path().join("New.psd");
    psd_export::export(&doc, &good).unwrap();
    assert_eq!(
        raster::composite(&psd::open(&good).unwrap()),
        raster::composite(&doc)
    );
}
