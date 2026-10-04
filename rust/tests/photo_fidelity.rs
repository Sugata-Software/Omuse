//! Independent, byte-authored PSD/PSB records and numeric image references.
//! The fixture writer follows Adobe's 2019 specification, rather than using the
//! importer or an Omuse serializer to produce its input records.
use image::{Rgba, RgbaImage};
use omuse::{
    document, editor::Editor, effects, filters::Filter, import_report, model::Document, psd, raster,
};
use serde_json::json;

#[derive(Default)]
struct PhotoshopLayer {
    rect: [i32; 4],
    pixels: Vec<[u8; 4]>,
    mask: Option<([i32; 4], u8, Vec<u8>)>,
    blocks: Vec<([u8; 4], Vec<u8>)>,
}

fn length(bytes: &mut Vec<u8>, value: usize, wide: bool) {
    if wide {
        bytes.extend_from_slice(&(value as u64).to_be_bytes());
    } else {
        bytes.extend_from_slice(&(value as u32).to_be_bytes());
    }
}

fn photoshop_file(width: u32, height: u32, layers: &[PhotoshopLayer], psb: bool) -> Vec<u8> {
    let mut records = Vec::new();
    let mut planes = Vec::new();
    for layer in layers {
        let [top, left, bottom, right] = layer.rect;
        assert_eq!(
            layer.pixels.len(),
            ((bottom - top) * (right - left)) as usize
        );
        for coordinate in layer.rect {
            records.extend_from_slice(&coordinate.to_be_bytes());
        }
        let count = if layer.pixels.is_empty() { 0 } else { 4 } + u16::from(layer.mask.is_some());
        records.extend_from_slice(&count.to_be_bytes());
        if !layer.pixels.is_empty() {
            for (component, channel) in [0i16, 1, 2, -1].into_iter().enumerate() {
                records.extend_from_slice(&channel.to_be_bytes());
                length(&mut records, layer.pixels.len() + 2, psb);
                planes.extend_from_slice(&0u16.to_be_bytes());
                planes.extend(layer.pixels.iter().map(|pixel| pixel[component]));
            }
        }
        if let Some((_, _, mask)) = &layer.mask {
            records.extend_from_slice(&(-2i16).to_be_bytes());
            length(&mut records, mask.len() + 2, psb);
            planes.extend_from_slice(&0u16.to_be_bytes());
            planes.extend_from_slice(mask);
        }
        records.extend_from_slice(b"8BIMnorm");
        records.extend_from_slice(&[255, 0, 0, 0]);
        let mut extra = Vec::new();
        if let Some((rect, background, _)) = &layer.mask {
            extra.extend_from_slice(&20u32.to_be_bytes());
            for coordinate in rect {
                extra.extend_from_slice(&coordinate.to_be_bytes());
            }
            extra.extend_from_slice(&[*background, 0, 0, 0]);
        } else {
            extra.extend_from_slice(&0u32.to_be_bytes());
        }
        extra.extend_from_slice(&0u32.to_be_bytes());
        extra.extend_from_slice(b"\x03Art"); // padded Pascal layer name
        for (key, payload) in &layer.blocks {
            extra.extend_from_slice(b"8BIM");
            extra.extend_from_slice(key);
            length(&mut extra, payload.len(), false);
            extra.extend_from_slice(payload);
            if payload.len() % 2 != 0 {
                extra.push(0);
            }
        }
        length(&mut records, extra.len(), false);
        records.extend_from_slice(&extra);
    }
    let mut info = (layers.len() as i16).to_be_bytes().to_vec();
    info.extend_from_slice(&records);
    info.extend_from_slice(&planes);
    if info.len() % 2 != 0 {
        info.push(0);
    }
    let mut section = Vec::new();
    length(&mut section, info.len(), psb);
    section.extend_from_slice(&info);
    section.extend_from_slice(&0u32.to_be_bytes());
    let mut file = b"8BPS".to_vec();
    file.extend_from_slice(&(if psb { 2u16 } else { 1u16 }).to_be_bytes());
    file.extend_from_slice(&[0; 6]);
    file.extend_from_slice(&3u16.to_be_bytes());
    file.extend_from_slice(&height.to_be_bytes());
    file.extend_from_slice(&width.to_be_bytes());
    file.extend_from_slice(&8u16.to_be_bytes());
    file.extend_from_slice(&3u16.to_be_bytes());
    file.extend_from_slice(&0u32.to_be_bytes());
    file.extend_from_slice(&0u32.to_be_bytes());
    length(&mut file, section.len(), psb);
    file.extend_from_slice(&section);
    file
}

fn open_bytes(bytes: &[u8]) -> anyhow::Result<Document> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("Reference.psd");
    std::fs::write(&path, bytes)?;
    psd::open(&path)
}

fn levels_payload(ranges: [[u16; 5]; 4]) -> Vec<u8> {
    let mut bytes = 2u16.to_be_bytes().to_vec();
    for range in ranges.into_iter().chain([[0; 5]; 25]) {
        for value in range {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    bytes
}

fn hue_payload(colorize: bool, color: [i16; 3], master: [i16; 3]) -> Vec<u8> {
    let mut bytes = vec![0, 2, u8::from(colorize), 0];
    for value in color.into_iter().chain(master) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    for range in [
        [315i16, 345, 15, 45],
        [15, 45, 75, 105],
        [75, 105, 135, 165],
        [135, 165, 195, 225],
        [195, 225, 255, 285],
        [255, 285, 315, 345],
    ] {
        for value in range.into_iter().chain([0; 3]) {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    bytes
}

fn adjustment_file(key: [u8; 4], payload: Vec<u8>, pixels: &[[u8; 4]], psb: bool) -> Vec<u8> {
    photoshop_file(
        pixels.len() as u32,
        1,
        &[
            PhotoshopLayer {
                rect: [0, 0, 1, pixels.len() as i32],
                pixels: pixels.to_vec(),
                ..Default::default()
            },
            PhotoshopLayer {
                blocks: vec![(key, payload)],
                ..Default::default()
            },
        ],
        psb,
    )
}

fn round_trip(document: &Document, expected: &RgbaImage) {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("Photo fidelity.omuse");
    document::save(document, &path).unwrap();
    let restored = document::open(&path).unwrap();
    assert_eq!(raster::composite(&restored), *expected);
    assert!(raster::validate(&restored).is_empty());
}

#[test]
fn psd_and_psb_levels_gamma_uses_hundredths_and_preserves_identity() {
    let pixels = [[64, 128, 192, 255], [25, 100, 225, 255]];
    for psb in [false, true] {
        for (gamma, expected) in [
            (100, pixels),
            (200, [[128, 181, 221, 255], [80, 160, 240, 255]]),
        ] {
            let mut ranges = [[0, 255, 0, 255, 100]; 4];
            ranges[0][4] = gamma;
            let doc = open_bytes(&adjustment_file(
                *b"levl",
                levels_payload(ranges),
                &pixels,
                psb,
            ))
            .unwrap();
            assert_eq!(
                doc.layers[1].metadata["adjustment"]["levels"]["ranges"][0]["gamma"],
                f64::from(gamma) / 100.
            );
            let rendered = raster::composite(&doc);
            assert_eq!(
                rendered.into_raw(),
                expected.into_iter().flatten().collect::<Vec<_>>()
            );
            round_trip(&doc, &raster::composite(&doc));
        }
    }
}

#[test]
fn psd_and_psb_levels_apply_channels_before_master_and_keep_output_ranges() {
    for psb in [false, true] {
        let ranges = [
            [0, 255, 0, 255, 200],
            [0, 255, 128, 128, 100],
            [0, 255, 255, 0, 100],
            [0, 255, 32, 224, 100],
        ];
        let doc = open_bytes(&adjustment_file(
            *b"levl",
            levels_payload(ranges),
            &[[17, 191, 255, 255]],
            psb,
        ))
        .unwrap();
        // Red becomes constant 128 before sqrt; green reverses to 64; blue
        // becomes output-white 224. Master gamma 2 gives 181, 128 and 239.
        let rendered = raster::composite(&doc);
        assert_eq!(rendered.get_pixel(0, 0).0, [181, 128, 239, 255]);
        round_trip(&doc, &rendered);
    }
}

#[test]
fn master_hue_uses_its_own_triple_and_colorize_uses_the_other() {
    for psb in [false, true] {
        for key in [*b"hue2", *b"hue "] {
            let bytes = adjustment_file(
                key,
                hue_payload(false, [-120, 75, 25], [120, 0, 0]),
                &[[255, 0, 0, 255]],
                psb,
            );
            let doc = open_bytes(&bytes).unwrap();
            assert_eq!(doc.layers[1].metadata["adjustment"]["hue"], 120);
            assert_eq!(raster::composite(&doc).get_pixel(0, 0).0, [0, 255, 0, 255]);
            round_trip(&doc, &raster::composite(&doc));
        }
        let bytes = adjustment_file(
            *b"hue2",
            hue_payload(true, [120, 100, 0], [-120, -50, -30]),
            &[[64, 64, 64, 255]],
            psb,
        );
        let doc = open_bytes(&bytes).unwrap();
        assert_eq!(doc.layers[1].metadata["adjustment"]["colorize"], true);
        assert_eq!(raster::composite(&doc).get_pixel(0, 0).0, [0, 128, 0, 255]);
        round_trip(&doc, &raster::composite(&doc));
    }
}

#[test]
fn selective_hue_bands_have_a_visible_conversion_notice() {
    let mut payload = hue_payload(false, [0, 0, 0], [0, 0, 0]);
    payload[24..26].copy_from_slice(&30i16.to_be_bytes());
    let doc = open_bytes(&adjustment_file(
        *b"hue2",
        payload,
        &[[255, 0, 0, 255]],
        false,
    ))
    .unwrap();
    let notes = import_report::conversion_notes(&doc);
    assert!(format!("{notes:?}").contains("selective Hue/Saturation ranges were not imported"));
    round_trip(&doc, &raster::composite(&doc));
}

#[test]
fn malformed_adjustment_records_fail_before_rendering() {
    let levels = levels_payload([[0, 255, 0, 255, 100]; 4]);
    let hue = hue_payload(false, [0, 0, 0], [0, 0, 0]);
    let mut malformed = Vec::new();
    malformed.push((*b"levl", levels[..41].to_vec()));
    malformed.push((*b"levl", levels[..291].to_vec()));
    for version in [0, 1, 3, u16::MAX] {
        for (key, good) in [(*b"levl", &levels), (*b"hue2", &hue)] {
            let mut bytes = good.clone();
            bytes[..2].copy_from_slice(&version.to_be_bytes());
            malformed.push((key, bytes));
        }
    }
    for end in [0, 1, 2, 10, 15, 16, 99] {
        malformed.push((*b"hue2", hue[..end].to_vec()));
    }
    for gamma in [0u16, 9, 1000, u16::MAX] {
        let mut bytes = levels.clone();
        bytes[10..12].copy_from_slice(&gamma.to_be_bytes());
        malformed.push((*b"levl", bytes));
    }
    for (offset, value) in [(2, 2u16 << 8), (10, 181), (12, 101), (24, 181)] {
        let mut bytes = hue.clone();
        bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        malformed.push((*b"hue2", bytes));
    }
    for psb in [false, true] {
        for (key, data) in &malformed {
            assert!(
                open_bytes(&adjustment_file(
                    *key,
                    data.clone(),
                    &[[10, 20, 30, 255]],
                    psb
                ))
                .is_err(),
                "accepted malformed {key:?}: {data:?}"
            );
        }
    }
}

#[test]
fn explicit_psd_mask_background_beats_patch_edges_for_raster_and_adjustments() {
    for psb in [false, true] {
        for background in [0, 255] {
            for adjustment in [false, true] {
                let mut layer = PhotoshopLayer {
                    rect: [0, 0, 3, 4],
                    pixels: vec![[80, 80, 80, 255]; 12],
                    mask: Some(([1, 1, 2, 3], background, vec![255 - background; 2])),
                    ..Default::default()
                };
                let mut layers = Vec::new();
                if adjustment {
                    layers.push(PhotoshopLayer {
                        rect: layer.rect,
                        pixels: layer.pixels.clone(),
                        ..Default::default()
                    });
                    layer.rect = [0; 4];
                    layer.pixels.clear();
                    let mut ranges = [[0, 255, 0, 255, 100]; 4];
                    ranges[0][2..4].copy_from_slice(&[200, 200]);
                    layer.blocks.push((*b"levl", levels_payload(ranges)));
                }
                layers.push(layer);
                let doc = open_bytes(&photoshop_file(4, 3, &layers, psb)).unwrap();
                let masked = doc.layers.last().unwrap();
                assert_eq!(masked.metadata["maskOutsideCoverage"], background);
                let rendered = raster::composite(&doc);
                for (x, y, pixel) in rendered.enumerate_pixels() {
                    let inside = y == 1 && (x == 1 || x == 2);
                    let coverage = if inside { 255 - background } else { background };
                    let expected = if adjustment {
                        [if coverage == 0 { 80 } else { 200 }; 4]
                    } else {
                        [80, 80, 80, coverage]
                    };
                    if adjustment {
                        assert_eq!(
                            pixel.0,
                            [expected[0], expected[1], expected[2], 255],
                            "{x},{y}"
                        );
                    } else if coverage == 0 {
                        assert_eq!(pixel[3], 0, "{x},{y}");
                    } else {
                        assert_eq!(pixel.0, expected, "{x},{y}");
                    }
                }
                round_trip(&doc, &rendered);
                let mut editor = Editor::new(doc);
                let id = editor.document.layers.last().unwrap().id.clone();
                assert!(editor.remove_mask(&id, false));
                assert!(editor.undo());
                assert_eq!(raster::composite(&editor.document), rendered);
                assert_eq!(
                    editor.document.find_layer(&id).unwrap().metadata["maskOutsideCoverage"],
                    background
                );
            }
        }
    }
}

#[test]
fn offset_layer_masks_keep_canvas_placement_including_negative_patch_origins() {
    for psb in [false, true] {
        for (rect, values, visible) in [
            ([0, 2, 2, 3], vec![255; 2], (2, 1)),
            ([-1, -1, 2, 2], vec![255; 9], (1, 1)),
        ] {
            let layer = PhotoshopLayer {
                rect: [1, 1, 3, 4],
                pixels: vec![[220, 70, 35, 255]; 6],
                mask: Some((rect, 0, values)),
                ..Default::default()
            };
            let doc = open_bytes(&photoshop_file(5, 4, &[layer], psb)).unwrap();
            let image = raster::composite(&doc);
            for (x, y, pixel) in image.enumerate_pixels() {
                assert_eq!(pixel[3], if (x, y) == visible { 255 } else { 0 }, "{x},{y}");
            }
            round_trip(&doc, &image);
        }
        let layer = PhotoshopLayer {
            rect: [0, 0, 1, 1],
            pixels: vec![[220, 70, 35, 255]],
            mask: Some(([0, 0, 1, 1], 127, vec![255])),
            ..Default::default()
        };
        assert!(
            open_bytes(&photoshop_file(1, 1, &[layer], psb))
                .unwrap_err()
                .to_string()
                .contains("mask default colour")
        );
    }
}

#[test]
fn legacy_master_only_levels_and_relative_hsl_keep_exact_pixels_and_alpha() {
    let source = RgbaImage::from_fn(256, 2, |x, y| {
        Rgba([
            x as u8,
            255 - x as u8,
            (x / 2) as u8,
            if y == 0 { 137 } else { 0 },
        ])
    });
    for filter in [
        Filter::Levels {
            black: 15. / 255.,
            white: 230. / 255.,
            gamma: 1.85,
        },
        Filter::Hsl {
            hue_degrees: -37.,
            saturation: 0.12,
            lightness: -0.08,
        },
    ] {
        let settings = effects::adjustment_for_filter(&filter).unwrap();
        let mut expected = source.clone();
        omuse::filters::apply(&mut expected, &filter).unwrap();
        assert_eq!(
            effects::apply_adjustment(&source, &settings).unwrap(),
            expected
        );
    }
}

#[test]
fn levels_and_colorize_have_independent_pixels_and_strict_validation() {
    let source = RgbaImage::from_raw(
        5,
        1,
        vec![
            0, 0, 0, 255, 64, 64, 64, 128, 128, 128, 128, 255, 192, 192, 192, 255, 7, 21, 99, 0,
        ],
    )
    .unwrap();
    let settings = json!({"kind":"Hue/Saturation", "colorize":true, "hue":120, "saturation":100, "lightness":0});
    assert_eq!(
        effects::apply_adjustment(&source, &settings)
            .unwrap()
            .into_raw(),
        vec![
            0, 0, 0, 255, 0, 128, 0, 128, 1, 255, 1, 255, 129, 255, 129, 255, 7, 21, 99, 0
        ]
    );
    let mut bright = settings.clone();
    bright["lightness"] = json!(50);
    assert_eq!(
        effects::apply_adjustment(&source, &bright)
            .unwrap()
            .get_pixel(2, 0)
            .0,
        [128, 255, 128, 255]
    );
    let mut negative_hue = settings;
    negative_hue["hue"] = json!(-120);
    assert_eq!(
        effects::apply_adjustment(&source, &negative_hue)
            .unwrap()
            .get_pixel(1, 0)
            .0,
        [0, 0, 128, 128]
    );
    for invalid in [
        json!({"kind":"Levels", "levels":{"ranges":[]}}),
        json!({"kind":"Levels", "levels":{"ranges":[{"black":200,"white":100}]}}),
        json!({"kind":"Levels", "levels":{"ranges":[{}, {"gamma":0}]}}),
        json!({"kind":"Levels", "levels":{"ranges":[{}, {"outputWhite":256}]}}),
        json!({"kind":"Hue/Saturation", "colorize":true, "saturation":-1}),
        json!({"kind":"Hue/Saturation", "colorize":"true"}),
    ] {
        assert!(
            effects::apply_adjustment(&source, &invalid).is_err(),
            "accepted {invalid}"
        );
    }
}
