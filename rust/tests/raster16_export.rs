use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{BlendIf, BlendIfChannel, BlendIfRange},
    model::{Document, Layer},
    precision::{Rgba16Image, TiledImage16},
    raster,
};
use serde_json::json;
use std::sync::Arc;

fn exact_layer(name: &str, pixel: [u16; 4]) -> Layer {
    let exact = Rgba16Image::from_pixel(1, 1, Rgba(pixel));
    let mut state = LayerState::from_image(&RgbaImage::new(1, 1), name).unwrap();
    state.source = Arc::new(TiledImage16::from_rgba16(&exact).unwrap());
    state.result = state.source.clone();
    let mut layer = Layer::paint(name, 1, 1);
    layer.image = Some(state.proxy().unwrap().into());
    layer.advanced = Some(Arc::new(state));
    layer
}

#[test]
fn clipped_photo_export_preserves_base_alpha_and_sub_byte_color() {
    let base = exact_layer("Translucent photo", [1_001, 2_003, 60_001, 30_001]);
    let mut top = exact_layer("Clipped color", [50_003, 13_001, 1_001, 65_535]);
    top.metadata["maskSourceID"] = json!(base.id);
    let mut document = Document::new(1, 1);
    document.layers = vec![base, top];

    let expected = [50_003, 13_001, 1_001, 30_001];
    assert_eq!(
        raster::composite16(&document).unwrap().get_pixel(0, 0).0,
        expected
    );
    let directory = tempfile::tempdir().unwrap();
    for extension in ["png", "tiff"] {
        let path = directory.path().join(format!("clipped.{extension}"));
        raster::export16(&document, &path).unwrap();
        let decoded = image::open(path).unwrap();
        assert_eq!(decoded.color(), image::ColorType::Rgba16);
        assert_eq!(decoded.to_rgba16().get_pixel(0, 0).0, expected);
    }
}

#[test]
fn clipped_photo_stack_applies_base_mask_once_and_child_opacity_to_color() {
    let mut base = exact_layer("Masked photo", [1_001, 20_003, 60_001, 30_001]);
    base.mask = Some(RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255])).into());
    let mut top = exact_layer("Clipped color", [50_003, 10_001, 1_001, 65_535]);
    top.opacity = 0.5;
    top.metadata["maskSourceID"] = json!(base.id);
    let mut document = Document::new(1, 1);
    document.layers = vec![base, top];

    let pixel = raster::composite16(&document).unwrap().get_pixel(0, 0).0;
    assert_eq!(&pixel[..3], &[25_502, 15_002, 30_501]);
    assert_eq!(pixel[3], (30_001f64 * 128. / 255.).round() as u16);
}

#[test]
fn chained_live_masks_are_order_independent_and_keep_parent_opacity() {
    let mut base = exact_layer("Hidden source", [1_001, 2_003, 60_001, 32_768]);
    base.visible = false;
    let mut middle = exact_layer("Hidden linked source", [1_001, 60_001, 2_003, 65_535]);
    middle.metadata["maskSourceID"] = json!(base.id);
    middle.visible = false;
    let mut top = exact_layer("Visible photo", [50_003, 13_001, 1_001, 65_535]);
    top.metadata["maskSourceID"] = json!(middle.id);
    let mut folder = Layer::group("Source opacity");
    folder.opacity = 0.5;
    folder.children = vec![middle, base];

    for layers in [vec![top.clone(), folder.clone()], vec![folder, top]] {
        let mut document = Document::new(1, 1);
        document.layers = layers;
        assert_eq!(
            raster::composite16(&document).unwrap().get_pixel(0, 0).0,
            [50_003, 13_001, 1_001, 8_192],
            "each source's parent opacity must apply even when its dependency is later in the layer list"
        );
    }
}

#[test]
fn clipped_photo_uses_base_blend_if_and_blend_mode_against_the_real_backdrop() {
    let backdrop_pixel = [30_007, 35_009, 40_013, 65_535];
    let backdrop = exact_layer("Backdrop", backdrop_pixel);
    let mut base = exact_layer("Base", [10_001, 20_003, 60_001, 30_001]);
    base.blend_mode = "Multiply".into();
    let blend_if = BlendIf {
        source_channel: BlendIfChannel::Red,
        source: BlendIfRange {
            white_split: 0.4,
            white: 0.4,
            ..Default::default()
        },
        backdrop_channel: BlendIfChannel::Blue,
        backdrop: BlendIfRange {
            black: 0.4,
            black_split: 0.4,
            ..Default::default()
        },
    };
    Arc::make_mut(base.advanced.as_mut().unwrap())
        .recipe
        .blend_if = Some(blend_if.clone());
    let original_state = base.advanced.as_ref().unwrap().clone();
    let color = [50_003, 13_001, 1_001, 65_535];
    let mut top = exact_layer("Clipped color", color);
    top.metadata["maskSourceID"] = json!(base.id);
    let mut hidden = exact_layer("Hidden clip", [65_535, 65_535, 65_535, 65_535]);
    hidden.metadata["maskSourceID"] = json!(base.id);
    hidden.visible = false;
    let mut document = Document::new(1, 1);
    document.layers = vec![backdrop, base, top, hidden];

    let actual = raster::composite16(&document).unwrap().get_pixel(0, 0).0;
    let alpha = 30_001. / 65_535.;
    for channel in 0..3 {
        let backdrop = f64::from(backdrop_pixel[channel]);
        let multiplied = backdrop * f64::from(color[channel]) / 65_535.;
        let expected = (backdrop * (1. - alpha) + multiplied * alpha).round() as u16;
        assert_eq!(actual[channel], expected, "channel {channel}");
    }
    assert_eq!(actual[3], 65_535);
    assert!(Arc::ptr_eq(
        &original_state,
        document.layers[1].advanced.as_ref().unwrap()
    ));
    assert_eq!(original_state.recipe.blend_if, Some(blend_if));
}

#[test]
fn oversized_live_mask_surfaces_fail_before_allocation_or_file_replacement() {
    let base = exact_layer("Mask source", [10_001, 20_003, 30_001, 40_009]);
    let mut document = Document::new(1, 1);
    document.width = 4_096;
    document.height = 4_096;
    document.layers = (0..7)
        .map(|index| {
            let mut layer = Layer::paint(format!("Linked {index}"), 1, 1);
            layer.metadata["maskSourceID"] = json!(base.id);
            layer
        })
        .collect();
    document.layers.push(base);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.png");
    std::fs::write(&path, b"Keep the previous export").unwrap();
    let error = raster::export16(&document, &path).unwrap_err();
    assert!(error.to_string().contains("renderer memory limit"));
    assert_eq!(std::fs::read(path).unwrap(), b"Keep the previous export");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn cyclic_live_masks_fail_without_replacing_a_previous_export() {
    let mut first = exact_layer("First", [10_001, 20_003, 30_001, 40_009]);
    let mut second = exact_layer("Second", [50_003, 13_001, 1_001, 65_535]);
    first.metadata["maskSourceID"] = json!(second.id);
    second.metadata["maskSourceID"] = json!(first.id);
    let mut document = Document::new(1, 1);
    document.layers = vec![first, second];
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.tiff");
    std::fs::write(&path, b"Keep the previous export").unwrap();
    let error = raster::export16(&document, &path).unwrap_err();
    assert!(error.to_string().contains("dependency cycle"));
    assert_eq!(std::fs::read(path).unwrap(), b"Keep the previous export");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
