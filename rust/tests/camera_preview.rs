use image::{Rgba, RgbaImage};
use omuse::{
    camera_raw::{
        self, Settings, ToneMapping,
        preview::{self, DraftSource},
    },
    model::Document,
    raster,
};
use std::sync::{Arc, atomic::AtomicBool};

fn source(width: u32, height: u32) -> Arc<RgbaImage> {
    Arc::new(RgbaImage::from_fn(width, height, |x, y| {
        Rgba([
            (x * 19 + y * 3) as u8,
            (y * 31 + x) as u8,
            (x + y * 7) as u8,
            [0, 1, 17, 127, 254, 255][(x as usize + y as usize) % 6],
        ])
    }))
}

#[test]
fn draft_pixels_are_exact_samples_of_full_grade_including_low_alpha() {
    let source = source(517, 513);
    let size = preview::dimensions(517, 513, 0.4).unwrap();
    let draft = DraftSource::new(source.clone(), size, &AtomicBool::new(false)).unwrap();
    let mut settings = Settings {
        exposure: 0.7,
        contrast: 19.,
        shadows: 24.,
        temperature: -12.,
        vibrance: 31.,
        ..Settings::for_new_edit()
    };
    settings
        .curve
        .rgb
        .insert(1, camera_raw::CurvePoint { x: 0.4, y: 0.53 });
    settings.mixer.hue[2] = 23.;
    settings.mixer.points.push(camera_raw::PointColor {
        hue: 35.,
        saturation: 0.5,
        luminance: 0.5,
        saturation_shift: 12.,
        ..Default::default()
    });
    settings.grading.shadows.saturation = 17.;
    settings.grading.shadows.hue = 215.;
    settings.calibration.blue_hue = -12.;
    settings.calibration.red_saturation = 17.;
    settings.calibration.shadow_tint = 10.;
    for mapping in [ToneMapping::Legacy, ToneMapping::SmoothV1] {
        settings.tone_mapping = mapping;
        let full = camera_raw::apply(&source, &settings).unwrap();
        let small = camera_raw::apply(draft.pixels(), &settings).unwrap();
        let expected =
            DraftSource::new(Arc::new(full.clone()), size, &AtomicBool::new(false)).unwrap();
        assert_eq!(&small, expected.pixels());
        for (shadows, highlights) in [(true, false), (false, true), (true, true)] {
            let overlay = camera_raw::clipping_preview(&full, shadows, highlights);
            let expected =
                DraftSource::new(Arc::new(overlay), size, &AtomicBool::new(false)).unwrap();
            assert_eq!(
                &camera_raw::clipping_preview(&small, shadows, highlights),
                expected.pixels()
            );
        }
    }
    assert!(draft.matches(&source, size));
    assert!(
        !draft.matches(&Arc::new((*source).clone()), size),
        "equal bytes do not establish source identity"
    );
    assert!(!draft.matches(&source, (size.0 - 1, size.1)));
}

#[test]
fn reduced_composite_retains_background_opacity_and_exact_sample_colours() {
    let pixels = source(512, 512);
    let settings = Settings {
        exposure: -0.5,
        saturation: 22.,
        ..Settings::for_new_edit()
    };
    let mut full = Document::new(512, 512);
    full.background = [40, 170, 90, 153];
    full.layers[0].opacity = 0.63;
    full.layers[0].image = Some(pixels.as_ref().clone().into());
    assert!(preview::supported(
        &full,
        &full.layers[0].id,
        false,
        &settings
    ));
    let draft = DraftSource::new(pixels.clone(), (173, 173), &AtomicBool::new(false)).unwrap();
    let mut small = full.clone();
    small.width = 173;
    small.height = 173;
    small.layers[0].image = Some(camera_raw::apply(draft.pixels(), &settings).unwrap().into());
    full.layers[0].image = Some(camera_raw::apply(&pixels, &settings).unwrap().into());
    let expected = DraftSource::new(
        Arc::new(raster::composite(&full)),
        (173, 173),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(&raster::composite(&small), expected.pixels());
}

#[test]
fn complex_document_and_spatial_settings_use_full_resolution() {
    let document = Document::new(512, 512);
    let settings = Settings {
        exposure: 0.5,
        ..Settings::for_new_edit()
    };
    let id = &document.layers[0].id;
    assert!(preview::supported(&document, id, false, &settings));
    assert!(!preview::supported(&document, id, true, &settings));
    assert!(!preview::supported(
        &document,
        "another layer",
        false,
        &settings
    ));
    let changes: Vec<Box<dyn Fn(&mut Document)>> = vec![
        Box::new(|d| d.layers[0].mask = Some(RgbaImage::new(512, 512).into())),
        Box::new(|d| d.layers[0].offset_x = 0.5),
        Box::new(|d| d.layers[0].scale_x = -1.),
        Box::new(|d| d.layers[0].rotation = 15.),
        Box::new(|d| d.layers[0].metadata["clipped"] = true.into()),
        Box::new(|d| d.layers[0].blend_mode = "Multiply".into()),
        Box::new(|d| d.layers[0].visible = false),
        Box::new(|d| d.layers[0].locked = true),
        Box::new(|d| d.layers[0].opacity = f32::NAN),
        Box::new(|d| d.layers[0].image = Some(RgbaImage::new(512, 511).into())),
        Box::new(|d| d.layers.push(d.layers[0].clone())),
    ];
    for change in changes {
        let mut d = document.clone();
        change(&mut d);
        assert!(!preview::supported(&d, id, false, &settings));
    }
    let changes: Vec<Box<dyn Fn(&mut Settings)>> = vec![
        Box::new(|s| s.clarity = 1.),
        Box::new(|s| s.texture = 1.),
        Box::new(|s| s.dehaze = 1.),
        Box::new(|s| s.grain_amount = 1.),
        Box::new(|s| s.glow = 1.),
        Box::new(|s| s.vignette_amount = 1.),
        Box::new(|s| s.detail.noise_color = 1.),
        Box::new(|s| s.detail.sharpen_amount = 1.),
        Box::new(|s| s.optics.distortion = 1.),
        Box::new(|s| s.geometry.rotate = 1.),
        Box::new(|s| s.exposure = f32::NAN),
    ];
    for change in changes {
        let mut s = settings.clone();
        change(&mut s);
        assert!(!preview::supported(&document, id, false, &s));
    }
}

#[test]
fn sampling_is_bounded_cancellable_and_never_upsamples() {
    for scale in [0., -0.1, f32::NAN, f32::INFINITY, 1., 2.] {
        assert_eq!(preview::dimensions(4096, 4096, scale), None);
    }
    assert_eq!(preview::dimensions(64, 64, 0.1), None);
    assert_eq!(preview::dimensions(4097, 4096, 0.1), None);
    for (w, h) in [
        (4096, 4096),
        (30000, 1),
        (1, 30000),
        (6000, 2000),
        (517, 513),
    ] {
        if let Some((dw, dh)) = preview::dimensions(w, h, 0.4) {
            assert!(dw <= w && dh <= h);
            assert!(u64::from(dw) * u64::from(dh) <= preview::MAX_DRAFT_PIXELS);
        }
    }
    let pixels = source(10, 9);
    for size in [(0, 1), (1, 0), (11, 9), (10, 10), (u32::MAX, u32::MAX)] {
        assert!(DraftSource::new(pixels.clone(), size, &AtomicBool::new(false)).is_err());
    }
    assert!(DraftSource::new(pixels, (5, 4), &AtomicBool::new(true)).is_err());
}

#[test]
fn ordinary_reopened_and_colour_managed_photos_keep_the_fast_path() {
    let mut document = Document::new(512, 512);
    document.layers[0].metadata = serde_json::json!({
        "id": document.layers[0].id, "name": "Photo", "isGroup": false,
        "imageFile": "image.png", "opacity": 1.0, "isVisible": true,
        "transform": { "origin": [0, 0], "size": [512, 512], "rotation": 0,
            "sampling": "High quality" },
        "sourceColorProfile": { "description": "Converted to sRGB", "iccBytes": 536, "fnv1a64": "1234" }
    });
    let id = document.layers[0].id.clone();
    assert!(preview::supported(
        &document,
        &id,
        false,
        &Settings::default()
    ));
    document.layers[0].metadata[omuse::advanced::RASTER_BLEND_IF_KEY] = serde_json::json!({});
    assert!(!preview::supported(
        &document,
        &id,
        false,
        &Settings::default()
    ));
}

#[test]
#[ignore = "manual CPU benchmark; does not measure input or display latency"]
fn benchmark_first_photo_preview() {
    let source = source(4000, 3000);
    let size = preview::dimensions(4000, 3000, 0.3).unwrap();
    let settings = Settings {
        exposure: 0.7,
        contrast: 15.,
        shadows: 20.,
        ..Settings::for_new_edit()
    };
    let cancel = AtomicBool::new(false);
    let start = std::time::Instant::now();
    let draft = DraftSource::new(source.clone(), size, &cancel).unwrap();
    let reduced = camera_raw::apply(draft.pixels(), &settings).unwrap();
    let draft_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = std::time::Instant::now();
    let full = camera_raw::apply(&source, &settings).unwrap();
    let full_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(
        &reduced,
        DraftSource::new(Arc::new(full), size, &cancel)
            .unwrap()
            .pixels()
    );
    println!(
        "{{\"source\":[4000,3000],\"draft\":[{},{}],\"draft_prepare_grade_ms\":{draft_ms:.3},\"full_grade_ms\":{full_ms:.3}}}",
        size.0, size.1
    );
}
