use image::{Rgba, RgbaImage};
use omuse::{
    advanced::LayerState,
    advanced_ops::{BlendIf, BlendIfChannel, BlendIfRange},
    model::{Document, Layer},
    precision::{Rgba16Image, TiledImage16},
    raster,
};
use serde_json::json;
use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

fn exact_image_layer(name: &str, exact: Rgba16Image) -> Layer {
    let (width, height) = exact.dimensions();
    let mut state = LayerState::from_image(&RgbaImage::new(width, height), name).unwrap();
    state.source = Arc::new(TiledImage16::from_rgba16(&exact).unwrap());
    state.result = state.source.clone();
    let mut layer = Layer::paint(name, width, height);
    layer.image = Some(state.proxy().unwrap().into());
    layer.advanced = Some(Arc::new(state));
    layer
}

fn exact_layer(name: &str, pixel: [u16; 4]) -> Layer {
    exact_image_layer(name, Rgba16Image::from_pixel(1, 1, Rgba(pixel)))
}

#[test]
fn high_quality_reduction_integrates_fine_patterns_instead_of_aliasing() {
    let pattern = Rgba16Image::from_fn(64, 1, |x, _| {
        if x % 8 < 2 {
            Rgba([60_001, 50_003, 40_009, 65_535])
        } else {
            Rgba([1_001, 2_003, 3_001, 65_535])
        }
    });
    let mut layer = exact_image_layer("Fine stripes", pattern);
    layer.scale_x = 0.125;
    layer.metadata["transform"] = json!({"sampling": "High quality"});
    let mut document = Document::new(8, 1);
    document.layers = vec![layer.clone()];

    let high_quality = raster::composite16(&document).unwrap();
    document.layers[0].metadata = json!({});
    assert_eq!(
        raster::composite16(&document).unwrap(),
        high_quality,
        "missing sampling metadata must retain the saved High quality default"
    );
    document.layers[0].metadata["transform"] = json!({"sampling": "Smooth"});
    let smooth = raster::composite16(&document).unwrap();
    for x in 2..6 {
        let integrated = high_quality.get_pixel(x, 0)[0];
        let aliased = smooth.get_pixel(x, 0)[0];
        assert!(
            (10_000..25_000).contains(&integrated),
            "scale-aware result at {x} was {integrated}"
        );
        assert!(
            aliased < 5_000,
            "bilinear fixture must expose its phase alias at {x}: {aliased}"
        );
    }
}

#[test]
fn high_quality_sampling_is_alpha_correct_at_hidden_rgb_edges() {
    let source = Rgba16Image::from_fn(16, 1, |x, _| {
        if x < 8 {
            Rgba([60_001, 1_001, 2_003, 65_535])
        } else {
            Rgba([3_001, 4_009, 65_001, 0])
        }
    });
    let mut layer = exact_image_layer("Transparent edge", source);
    layer.scale_x = 0.25;
    layer.offset_x = 0.5;
    layer.metadata["transform"] = json!({"sampling": "High quality"});
    let mut document = Document::new(5, 1);
    document.layers = vec![layer];

    let edge = raster::composite16(&document).unwrap().get_pixel(2, 0).0;
    assert!((20_000..45_000).contains(&edge[3]), "{edge:?}");
    assert!(edge[0] > 58_000 && edge[2] < 4_000, "{edge:?}");
}

#[test]
fn rotated_reduction_uses_transform_axes_and_keeps_sub_byte_precision() {
    let source = Rgba16Image::from_fn(8, 8, |_, y| {
        if y < 4 {
            Rgba([50_003, 1_001, 2_003, 65_535])
        } else {
            Rgba([2_003, 1_001, 50_003, 65_535])
        }
    });
    let mut layer = exact_image_layer("Rotated bands", source);
    layer.scale_x = 0.5;
    layer.scale_y = 0.5;
    layer.rotation = 90.;
    layer.metadata["transform"] = json!({"sampling": "High quality"});
    let mut document = Document::new(4, 4);
    document.layers = vec![layer];

    let output = raster::composite16(&document).unwrap();
    let left = output.get_pixel(0, 2).0;
    let right = output.get_pixel(3, 2).0;
    assert!(left[2] > left[0], "clockwise rotation left edge: {left:?}");
    assert!(
        right[0] > right[2],
        "clockwise rotation right edge: {right:?}"
    );
    assert!(
        left[..3]
            .iter()
            .chain(&right[..3])
            .any(|value| value % 257 != 0),
        "16-bit samples were quantized through an 8-bit proxy"
    );
}

#[test]
fn transformed_constant_source_retains_exact_sixteen_bit_samples() {
    let exact = [1_001, 2_003, 60_001, 65_535];
    let mut layer = exact_image_layer(
        "Exact transformed color",
        Rgba16Image::from_pixel(9, 7, Rgba(exact)),
    );
    layer.scale_x = 0.6;
    layer.scale_y = 0.7;
    layer.rotation = 17.;
    layer.offset_x = 2.;
    layer.offset_y = 3.;
    layer.metadata["transform"] = json!({"sampling": "High quality"});
    let mut document = Document::new(10, 10);
    document.layers = vec![layer];

    assert_eq!(
        raster::composite16(&document).unwrap().get_pixel(4, 5).0,
        exact
    );
}

#[test]
fn high_quality_reduction_filters_correlated_colour_and_local_mask_together() {
    let source = Rgba16Image::from_fn(64, 1, |x, _| {
        if x % 2 == 0 {
            Rgba([60_001, 1_001, 2_003, 65_535])
        } else {
            Rgba([2_003, 1_001, 60_001, 65_535])
        }
    });
    let mask = RgbaImage::from_fn(64, 1, |x, _| {
        let value = if x % 2 == 0 { 255 } else { 0 };
        Rgba([value, value, value, 255])
    });
    let mut layer = exact_image_layer("Correlated stripes", source);
    layer.mask = Some(mask.into());
    layer.scale_x = 0.125;
    layer.metadata["transform"] = json!({"sampling": "High quality"});
    let mut document = Document::new(8, 1);
    document.layers = vec![layer];

    let output = raster::composite16(&document).unwrap();
    for x in 2..6 {
        let pixel = output.get_pixel(x, 0).0;
        assert!(
            pixel[0] > 55_000 && pixel[2] < 5_000,
            "pixel {x}: {pixel:?}"
        );
        assert!((25_000..40_000).contains(&pixel[3]), "pixel {x}: {pixel:?}");
    }
}

#[test]
fn high_quality_reduction_integrates_a_fine_folder_mask_before_compositing() {
    let exact = [1_001, 2_003, 60_001, 65_535];
    let mut child = exact_image_layer(
        "Constant child",
        Rgba16Image::from_pixel(64, 1, Rgba(exact)),
    );
    child.scale_x = 0.125;
    child.metadata["transform"] = json!({"sampling": "High quality"});
    let mask = RgbaImage::from_fn(64, 1, |x, _| {
        let value = if x % 8 < 2 { 255 } else { 0 };
        Rgba([value, value, value, 255])
    });
    let mut folder = Layer::group("Reduced mask");
    folder.mask = Some(mask.into());
    folder.scale_x = 0.125;
    folder.metadata["transform"] = json!({"sampling": "High quality"});
    folder.children = vec![child];
    let mut document = Document::new(8, 1);
    document.layers = vec![folder];

    let output = raster::composite16(&document).unwrap();
    for x in 2..6 {
        let pixel = output.get_pixel(x, 0).0;
        assert_eq!(&pixel[..3], &exact[..3], "pixel {x}: {pixel:?}");
        assert!((10_000..23_000).contains(&pixel[3]), "pixel {x}: {pixel:?}");
    }
}

#[test]
fn identity_and_rotated_upscale_keep_local_mask_coordinates() {
    let exact = [50_003, 2_003, 1_001, 65_535];
    let mask = RgbaImage::from_fn(2, 1, |x, _| {
        let value = if x == 0 { 255 } else { 0 };
        Rgba([value, value, value, 255])
    });
    let mut layer = exact_image_layer("Placed mask", Rgba16Image::from_pixel(2, 1, Rgba(exact)));
    layer.mask = Some(mask.into());
    let mut identity = Document::new(2, 1);
    identity.layers = vec![layer.clone()];
    let identity_output = raster::composite16(&identity).unwrap();
    assert_eq!(identity_output.get_pixel(0, 0).0, exact);
    assert_eq!(identity_output.get_pixel(1, 0)[3], 0);

    layer.scale_x = 2.;
    layer.scale_y = 2.;
    layer.rotation = 180.;
    layer.metadata["transform"] = json!({"sampling": "High quality"});
    let mut transformed = Document::new(4, 2);
    transformed.layers = vec![layer];
    let transformed_output = raster::composite16(&transformed).unwrap();
    assert!(transformed_output.get_pixel(0, 0)[3] < 2_000);
    assert!(transformed_output.get_pixel(3, 1)[3] > 60_000);
    assert_eq!(&transformed_output.get_pixel(3, 1).0[..3], &exact[..3]);
}

#[test]
fn mid_flight_cancelled_high_quality_export_preserves_destination() {
    let mut document = Document::new(512, 512);
    document.layers[0] = exact_image_layer(
        "Cancelled pattern",
        Rgba16Image::from_fn(2_048, 2_048, |x, y| {
            Rgba([
                ((x * 31 + y * 17) & 65_535) as u16,
                ((x * 11 + y * 29) & 65_535) as u16,
                3_001,
                65_535,
            ])
        }),
    );
    document.layers[0].scale_x = 0.25;
    document.layers[0].scale_y = 0.25;
    document.layers[0].metadata["transform"] = json!({"sampling": "High quality"});
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.png");
    std::fs::write(&path, b"Keep the previous export").unwrap();

    let cancel = Arc::new(AtomicBool::new(false));
    let start = Arc::new(Barrier::new(2));
    let error = std::thread::scope(|scope| {
        let worker_cancel = cancel.clone();
        let worker_start = start.clone();
        scope.spawn(move || {
            worker_start.wait();
            // The fixed 32 MiB source conversion followed by a bounded
            // scale-aware reduction keeps the export in flight well beyond
            // this grace period, while allowing it to pass its entry check.
            std::thread::sleep(Duration::from_millis(50));
            worker_cancel.store(true, Ordering::Relaxed);
        });
        start.wait();
        raster::export16_cancellable(&document, &path, &cancel).unwrap_err()
    });
    assert!(error.to_string().contains("cancelled"));
    assert!(cancel.load(Ordering::Relaxed));
    assert_eq!(std::fs::read(path).unwrap(), b"Keep the previous export");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
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
