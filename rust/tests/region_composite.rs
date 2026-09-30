use image::{Rgba, RgbaImage};
use omuse::{
    document,
    model::{Document, Layer, PixelRect},
    raster,
};
use serde_json::json;

fn patterned(width: u32, height: u32, seed: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        Rgba([
            ((x * 37 + y * 11 + seed * 3) % 256) as u8,
            ((x * 7 + y * 53 + seed * 5) % 256) as u8,
            ((x * 29 + y * 17 + seed * 13) % 256) as u8,
            [0, 1, 31, 127, 254, 255][((x + y * 3 + seed) % 6) as usize],
        ])
    })
}

fn simple_document() -> Document {
    let mut document = Document::new(73, 59);
    document.background = [17, 39, 81, 143];
    document.layers.clear();
    for (index, (width, height, x, y, opacity, visible)) in [
        (61, 47, -9.0, -5.0, 1.0, true),
        (39, 31, 13.0, 17.0, 0.63, true),
        (27, 23, 51.0, 41.0, 0.81, true),
        (11, 9, 4.0, 7.0, 0.4, false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut layer = Layer::paint(format!("Layer {index}"), width, height);
        layer.image = Some(patterned(width, height, index as u32 + 1).into());
        layer.offset_x = x;
        layer.offset_y = y;
        layer.opacity = opacity;
        layer.visible = visible;
        document.layers.push(layer);
    }
    document
}

#[test]
fn randomized_patch_sequence_matches_complete_composite() {
    let mut document = simple_document();
    let mut incremental = raster::composite(&document);
    let mut state = 0x4d59_5df4_d0f3_3173u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state as u32
    };
    for step in 0..160 {
        let layer_index = next() as usize % document.layers.len();
        let layer = &mut document.layers[layer_index];
        let image = layer.image.as_mut().unwrap();
        let source_x = next() % image.width();
        let source_y = next() % image.height();
        image.put_pixel(
            source_x,
            source_y,
            Rgba([
                next() as u8,
                next() as u8,
                next() as u8,
                [0, 1, 73, 128, 231, 255][next() as usize % 6],
            ]),
        );
        let canvas_x = i64::from(source_x) + layer.offset_x as i64;
        let canvas_y = i64::from(source_y) + layer.offset_y as i64;
        let x = canvas_x.saturating_sub((next() % 4) as i64).max(0) as u32;
        let y = canvas_y.saturating_sub((next() % 4) as i64).max(0) as u32;
        let width = (next() % 8 + 1).max((canvas_x - i64::from(x) + 1).max(1) as u32);
        let height = (next() % 8 + 1).max((canvas_y - i64::from(y) + 1).max(1) as u32);
        assert!(raster::composite_region(
            &document,
            &mut incremental,
            PixelRect {
                x,
                y,
                width,
                height
            },
        ));
        assert_eq!(
            incremental,
            raster::composite(&document),
            "difference after patch {step} in layer {layer_index}"
        );
    }
}

#[test]
fn blend_if_paint_preview_matches_full_render_for_source_and_backdrop_edits() {
    use omuse::advanced_ops::{BlendIf, BlendIfChannel, BlendIfRange};

    let mut document = simple_document();
    document.layers[1].metadata[omuse::advanced::RASTER_BLEND_IF_KEY] =
        serde_json::to_value(BlendIf {
            source_channel: BlendIfChannel::Red,
            backdrop_channel: BlendIfChannel::Blue,
            source: BlendIfRange {
                black: 0.1,
                black_split: 0.3,
                white_split: 0.7,
                white: 0.9,
            },
            backdrop: BlendIfRange {
                black: 0.05,
                black_split: 0.2,
                white_split: 0.6,
                white: 0.95,
            },
        })
        .unwrap();
    let mut incremental = raster::composite(&document);
    for (layer_index, source_x, source_y) in [(1, 8, 9), (0, 30, 31), (1, 19, 14)] {
        let layer = &mut document.layers[layer_index];
        layer
            .image
            .as_mut()
            .unwrap()
            .put_pixel(source_x, source_y, Rgba([143, 71, 233, 195]));
        let damage = PixelRect {
            x: (source_x as f32 + layer.offset_x) as u32,
            y: (source_y as f32 + layer.offset_y) as u32,
            width: 1,
            height: 1,
        };
        assert!(raster::composite_region(
            &document,
            &mut incremental,
            damage
        ));
        assert_eq!(incremental, raster::composite(&document));
    }
}

#[test]
fn clipped_and_outside_regions_are_exact_safe_noops() {
    let document = simple_document();
    let expected = raster::composite(&document);
    let mut refreshed = RgbaImage::from_pixel(document.width, document.height, Rgba([1, 2, 3, 4]));
    assert!(raster::composite_region(
        &document,
        &mut refreshed,
        PixelRect {
            x: 61,
            y: 49,
            width: u32::MAX,
            height: u32::MAX,
        },
    ));
    for y in 0..document.height {
        for x in 0..document.width {
            if x >= 61 && y >= 49 {
                assert_eq!(refreshed.get_pixel(x, y), expected.get_pixel(x, y));
            } else {
                assert_eq!(refreshed.get_pixel(x, y).0, [1, 2, 3, 4]);
            }
        }
    }
    let before = refreshed.clone();
    assert!(raster::composite_region(
        &document,
        &mut refreshed,
        PixelRect {
            x: u32::MAX,
            y: u32::MAX,
            width: 20,
            height: 20,
        },
    ));
    assert_eq!(refreshed, before);
}

#[test]
fn saved_reopened_and_then_edited_rasters_use_region_path_exactly() {
    let mut document = simple_document();
    document.background = [0; 4];
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("region.omuse");
    document::save(&document, &path).unwrap();
    let mut reopened = document::open(&path).unwrap();

    // Persisted metadata deliberately remains a stale copy after ordinary
    // edits. The full renderer reads these parsed Layer fields instead.
    let layer = &mut reopened.layers[1];
    layer.name = "Renamed after reopen".into();
    layer.offset_x += 4.0;
    layer.offset_y -= 3.0;
    layer.opacity = 0.47;
    layer
        .image
        .as_mut()
        .unwrap()
        .put_pixel(7, 9, Rgba([231, 17, 99, 183]));
    assert_ne!(
        layer.metadata.pointer("/transform/origin").unwrap(),
        &json!([layer.offset_x, layer.offset_y])
    );

    let expected = raster::composite(&reopened);
    let mut incremental = RgbaImage::new(reopened.width, reopened.height);
    assert!(raster::composite_region(
        &reopened,
        &mut incremental,
        PixelRect {
            x: 0,
            y: 0,
            width: reopened.width,
            height: reopened.height,
        },
    ));
    assert_eq!(incremental, expected);
}

#[test]
fn imported_color_profile_provenance_is_inert_but_unknown_metadata_is_not() {
    let mut document = simple_document();
    document.layers[0].metadata = json!({
        "sourceColorProfile": {
            "description": "Synthetic Display P3",
            "iccBytes": 536,
            "fnv1a64": "0123456789abcdef"
        }
    });
    let expected = raster::composite(&document);
    let mut target = RgbaImage::new(document.width, document.height);
    assert!(raster::composite_region(
        &document,
        &mut target,
        PixelRect {
            x: 0,
            y: 0,
            width: document.width,
            height: document.height,
        },
    ));
    assert_eq!(target, expected);

    for metadata in [
        json!({"plugin-note": {"preserve": true}}),
        json!({"transform": {
            "origin": [0, 0], "size": [61, 47], "rotation": 0,
            "flipX": false, "flipY": false, "sampling": "High quality",
            "futureSemanticField": true
        }}),
        json!({"transform": {"sampling": "Future sampler"}}),
        json!({"sourceColorProfile": {
            "description": "Profile", "iccBytes": 12, "fnv1a64": "abcd",
            "futureSemanticField": true
        }}),
    ] {
        let mut unsupported = simple_document();
        unsupported.layers[0].metadata = metadata;
        let mut unchanged = patterned(unsupported.width, unsupported.height, 101);
        let before = unchanged.clone();
        assert!(!raster::composite_region(
            &unsupported,
            &mut unchanged,
            PixelRect {
                x: 2,
                y: 3,
                width: 11,
                height: 13,
            },
        ));
        assert_eq!(unchanged, before);
    }
}

#[test]
fn unsupported_or_invalid_inputs_never_mutate_target() {
    let base = simple_document();
    let cases: Vec<Document> = vec![
        {
            let mut document = base.clone();
            document.layers[0].blend_mode = "Multiply".into();
            document
        },
        {
            let mut document = base.clone();
            document.layers[0].rotation = 0.5;
            document
        },
        {
            let mut document = base.clone();
            document.layers[0].scale_x = -1.0;
            document
        },
        {
            let mut document = base.clone();
            document.layers[0].offset_x = 0.5;
            document
        },
        {
            let mut document = base.clone();
            document.layers[0].mask = Some(RgbaImage::from_pixel(1, 1, Rgba([255; 4])).into());
            document
        },
        {
            let mut document = base.clone();
            document.layers[0].metadata = json!({"effects": {"shadow": {}}});
            document
        },
        {
            let mut document = base.clone();
            document.layers.push(Layer::group("Group"));
            document
        },
        {
            let mut document = base.clone();
            document.layers[0].opacity = f32::NAN;
            document
        },
    ];
    for document in cases {
        let mut target = patterned(document.width, document.height, 99);
        let before = target.clone();
        assert!(!raster::composite_region(
            &document,
            &mut target,
            PixelRect {
                x: 3,
                y: 4,
                width: 17,
                height: 19,
            },
        ));
        assert_eq!(target, before);
    }

    let mut wrong_size = patterned(base.width - 1, base.height, 7);
    let before = wrong_size.clone();
    assert!(!raster::composite_region(
        &base,
        &mut wrong_size,
        PixelRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        },
    ));
    assert_eq!(wrong_size, before);
}
