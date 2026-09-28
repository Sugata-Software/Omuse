use image::{Rgba, RgbaImage};
use omuse::{
    model::{Document, Layer},
    raster,
};
fn layer(color: [u8; 4], w: u32, h: u32) -> Layer {
    let mut l = Layer::paint("test", w, h);
    l.image = Some(RgbaImage::from_pixel(w, h, Rgba(color)).into());
    l
}
fn doc(w: u32, h: u32, layers: Vec<Layer>) -> Document {
    let mut d = Document::new(w, h);
    d.background = [0; 4];
    d.layers = layers;
    d
}
#[test]
fn alpha_over_is_straight_not_premultiplied() {
    let d = doc(
        1,
        1,
        vec![layer([255, 0, 0, 128], 1, 1), layer([0, 0, 255, 128], 1, 1)],
    );
    assert_eq!(raster::composite(&d).get_pixel(0, 0).0, [85, 0, 170, 192]);
}
#[test]
fn opacity_visibility_and_pass_through_groups() {
    let mut group = Layer::group("Group");
    group.opacity = 0.5;
    group.children = vec![layer([255, 0, 0, 255], 1, 1), layer([0, 0, 255, 255], 1, 1)];
    let mut hidden = layer([0, 255, 0, 255], 1, 1);
    hidden.visible = false;
    let d = doc(1, 1, vec![group, hidden]);
    assert_eq!(raster::composite(&d).get_pixel(0, 0).0, [85, 0, 170, 192]);
}
#[test]
fn masks_combine_luminance_and_alpha_in_local_space() {
    let mut l = layer([255, 0, 0, 255], 3, 1);
    l.mask = Some(
        RgbaImage::from_raw(
            3,
            1,
            vec![0, 0, 0, 255, 255, 255, 255, 128, 255, 255, 255, 255],
        )
        .unwrap()
        .into(),
    );
    l.offset_x = 1.0;
    let out = raster::composite(&doc(4, 1, vec![l]));
    assert_eq!(out.get_pixel(0, 0).0, [0; 4]);
    assert_eq!(out.get_pixel(1, 0).0, [0; 4]);
    assert_eq!(out.get_pixel(2, 0).0, [255, 0, 0, 128]);
    assert_eq!(out.get_pixel(3, 0).0, [255, 0, 0, 255]);
}
#[test]
fn blend_modes_known_opaque_pixels() {
    for (mode, want) in [
        ("Multiply", [32, 64, 64, 255]),
        ("Screen", [160, 192, 255, 255]),
        ("Difference", [64, 0, 191, 255]),
        ("Linear Dodge (Add)", [192, 255, 255, 255]),
        ("Subtract", [64, 0, 0, 255]),
        ("Linear Burn", [0, 1, 64, 255]),
    ] {
        let mut top = layer([64, 128, 255, 255], 1, 1);
        top.blend_mode = mode.into();
        let d = doc(1, 1, vec![layer([128, 128, 64, 255], 1, 1), top]);
        assert_eq!(raster::composite(&d).get_pixel(0, 0).0, want, "{mode}");
    }
}
#[test]
fn blend_on_transparent_backdrop_preserves_source() {
    for mode in [
        "Multiply",
        "Color Burn",
        "Hue",
        "Saturation",
        "Luminosity",
        "Divide",
        "Hard Mix",
    ] {
        let mut l = layer([42, 77, 139, 128], 1, 1);
        l.blend_mode = mode.into();
        assert_eq!(
            raster::composite(&doc(1, 1, vec![l])).get_pixel(0, 0).0,
            [42, 77, 139, 128],
            "{mode}"
        );
    }
}
#[test]
fn transform_scale_flip_and_clockwise_rotation() {
    let mut l = layer([0; 4], 2, 2);
    l.image = Some(
        RgbaImage::from_raw(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ],
        )
        .unwrap()
        .into(),
    );
    l.rotation = 90.0;
    let out = raster::composite(&doc(2, 2, vec![l.clone()]));
    assert_eq!(out.get_pixel(0, 0).0, [0, 0, 255, 255]);
    assert_eq!(out.get_pixel(1, 0).0, [255, 0, 0, 255]);
    assert_eq!(out.get_pixel(0, 1).0, [255, 255, 0, 255]);
    l.metadata = serde_json::json!({"transform":{"sampling":"Nearest"}});
    l.rotation = 0.0;
    l.scale_x = -2.0;
    l.offset_x = 1.0;
    let out = raster::composite(&doc(5, 2, vec![l]));
    assert_eq!(out.get_pixel(0, 0).0, [0; 4]);
    assert_eq!(out.get_pixel(1, 0).0, [0, 255, 0, 255]);
    assert_eq!(out.get_pixel(2, 0).0, [0, 255, 0, 255]);
    assert_eq!(out.get_pixel(4, 0).0, [255, 0, 0, 255]);
}
#[test]
fn group_transforms_do_not_move_children_and_opacity_multiplies() {
    let mut inner = Layer::group("inner");
    inner.children = vec![layer([100, 50, 200, 255], 1, 1)];
    inner.offset_x = 1.0;
    let mut outer = Layer::group("outer");
    outer.children = vec![inner];
    outer.offset_y = 1.0;
    outer.opacity = 0.5;
    let out = raster::composite(&doc(3, 3, vec![outer]));
    assert_eq!(out.get_pixel(0, 0).0, [100, 50, 200, 128]);
    assert_eq!(out.get_pixel(1, 1).0, [0; 4]);
}
#[test]
fn hostile_transform_and_bounds_do_not_panic() {
    let mut l = layer([255; 4], 1, 1);
    l.offset_x = f32::MAX;
    assert_eq!(
        raster::composite(&doc(1, 1, vec![l.clone()]))
            .get_pixel(0, 0)
            .0,
        [0; 4]
    );
    l.rotation = f32::NAN;
    let d = doc(1, 1, vec![l]);
    assert!(!raster::validate(&d).is_empty());
    // Invalid semantics fail closed; callers must not present an apparently valid blank image.
    assert_eq!(raster::composite(&d).dimensions(), (0, 0));
    let mut d = doc(1, 1, vec![]);
    d.width = u32::MAX;
    d.height = u32::MAX;
    assert_eq!(raster::composite(&d).dimensions(), (0, 0));
}
#[test]
fn export_roundtrips_lossless_and_flattens_jpeg() {
    let temp = tempfile::tempdir().unwrap();
    let d = doc(2, 2, vec![layer([255, 0, 0, 128], 1, 1)]);
    for ext in ["png", "webp", "tiff"] {
        let path = temp.path().join(format!("image.{ext}"));
        raster::export(&d, &path).unwrap();
        let out = image::open(path).unwrap().to_rgba8();
        assert_eq!(out, raster::composite(&d));
    }
    let empty = doc(8, 8, vec![]);
    let path = temp.path().join("image.jpg");
    raster::export(&empty, &path).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgb8().get_pixel(0, 0).0,
        [255; 3]
    );
}
#[test]
fn failed_export_preserves_destination() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing.png");
    std::fs::write(&path, b"previous").unwrap();
    let mut l = layer([255; 4], 1, 1);
    l.blend_mode = "future unsupported mode".into();
    assert!(raster::export(&doc(1, 1, vec![l]), &path).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"previous");
}

#[test]
fn smooth_sampling_uses_premultiplied_alpha_without_color_halos() {
    let mut l = layer([0; 4], 2, 1);
    l.image = Some(
        RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 0])
            .unwrap()
            .into(),
    );
    l.scale_x = 2.0;
    l.metadata = serde_json::json!({"transform":{"sampling":"Smooth"}});
    let out = raster::composite(&doc(4, 1, vec![l]));
    assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(out.get_pixel(1, 0).0, [255, 0, 0, 191]);
    assert_eq!(out.get_pixel(2, 0).0, [255, 0, 0, 64]);
    assert_eq!(out.get_pixel(3, 0).0, [0; 4]);
}
#[test]
fn nested_group_masks_and_disabled_leaf_mask() {
    let mut l = layer([255, 0, 0, 255], 4, 1);
    l.mask = Some(RgbaImage::from_pixel(4, 1, Rgba([0, 0, 0, 255])).into());
    l.metadata = serde_json::json!({"maskEnabled":false});
    let mut group = Layer::group("masked");
    group.children = vec![l];
    group.mask = Some(
        RgbaImage::from_raw(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255])
            .unwrap()
            .into(),
    );
    group.metadata =
        serde_json::json!({"isGroup":true,"transform":{"size":[4,1],"sampling":"Nearest"}});
    let mut outer = Layer::group("outer");
    outer.children = vec![group];
    outer.mask = Some(RgbaImage::from_pixel(4, 1, Rgba([255, 255, 255, 128])).into());
    let out = raster::composite(&doc(4, 1, vec![outer]));
    assert_eq!(out.get_pixel(0, 0).0, [0; 4]);
    assert_eq!(out.get_pixel(1, 0).0, [0; 4]);
    assert_eq!(out.get_pixel(2, 0).0, [255, 0, 0, 128]);
    assert_eq!(out.get_pixel(3, 0).0, [255, 0, 0, 128]);
}

#[test]
fn nonseparable_blends_preserve_expected_luminosity() {
    for (name, expected) in [
        ("Hue", [0, 130, 0, 255]),
        ("Color", [0, 130, 0, 255]),
        ("Saturation", [255, 0, 0, 255]),
        ("Luminosity", [255, 106, 106, 255]),
    ] {
        let mut top = layer([0, 255, 0, 255], 1, 1);
        top.blend_mode = name.into();
        let result = raster::composite(&doc(1, 1, vec![layer([255, 0, 0, 255], 1, 1), top]));
        assert_eq!(result.get_pixel(0, 0).0, expected, "{name}");
    }
}
#[test]
fn folder_child_blends_with_external_backdrop() {
    let mut child = layer([255, 0, 0, 255], 1, 1);
    child.blend_mode = "Multiply".into();
    let mut folder = Layer::group("pass-through");
    folder.children = vec![child];
    let result = raster::composite(&doc(1, 1, vec![layer([0, 0, 255, 255], 1, 1), folder]));
    assert_eq!(result.get_pixel(0, 0).0, [0, 0, 0, 255]);
}

#[test]
fn normal_fast_path_matches_masked_reference_for_all_alpha_pairs() {
    let mut backdrop = layer([0; 4], 256, 256);
    backdrop.image =
        Some(RgbaImage::from_fn(256, 256, |_, y| Rgba([13, 137, 241, y as u8])).into());
    let mut front = layer([0; 4], 256, 256);
    front.image = Some(RgbaImage::from_fn(256, 256, |x, _| Rgba([235, 73, 25, x as u8])).into());
    let fast = raster::composite(&doc(256, 256, vec![backdrop.clone(), front.clone()]));
    front.mask = Some(RgbaImage::from_pixel(1, 1, Rgba([255; 4])).into());
    let reference = raster::composite(&doc(256, 256, vec![backdrop, front]));
    assert_eq!(fast, reference);
}
#[test]
fn fast_path_clips_negative_offsets() {
    let mut l = layer([0; 4], 3, 1);
    l.image = Some(
        RgbaImage::from_raw(3, 1, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255])
            .unwrap()
            .into(),
    );
    l.offset_x = -1.0;
    let out = raster::composite(&doc(2, 1, vec![l]));
    assert_eq!(out.get_pixel(0, 0).0, [0, 255, 0, 255]);
    assert_eq!(out.get_pixel(1, 0).0, [0, 0, 255, 255]);
}

#[test]
fn contiguous_clipping_stack_preserves_base_alpha() {
    let mut base = Layer::paint("Base", 1, 1);
    base.image = Some(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 255, 128])).into());
    let mut top = Layer::paint("Clip", 1, 1);
    top.image = Some(RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255])).into());
    top.metadata["maskSourceID"] = serde_json::json!(base.id);
    let doc = Document {
        width: 1,
        height: 1,
        name: "stack".into(),
        background: [0; 4],
        layers: vec![base, top],
        metadata: serde_json::json!({}),
    };
    assert_eq!(
        omuse::raster::composite(&doc).get_pixel(0, 0).0,
        [255, 0, 0, 128]
    );
}
#[test]
fn nested_source_supports_chained_live_masks() {
    let mut base = Layer::paint("Base", 1, 1);
    base.image = Some(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 255, 128])).into());
    base.visible = false;
    let mut source = Layer::paint("Source", 1, 1);
    source.image = Some(RgbaImage::from_pixel(1, 1, Rgba([0, 255, 0, 255])).into());
    source.metadata["maskSourceID"] = serde_json::json!(base.id);
    source.visible = false;
    let mut folder = Layer::group("Group");
    folder.children = vec![base, source.clone()];
    folder.opacity = 0.5;
    let mut top = Layer::paint("Top", 1, 1);
    top.image = Some(RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255])).into());
    top.metadata["maskSourceID"] = serde_json::json!(source.id);
    let doc = Document {
        width: 1,
        height: 1,
        name: "chain".into(),
        background: [0; 4],
        layers: vec![folder, top],
        metadata: serde_json::json!({}),
    };
    assert!(omuse::raster::validate(&doc).is_empty());
    assert_eq!(
        omuse::raster::composite(&doc).get_pixel(0, 0).0,
        [255, 0, 0, 32]
    );
}
