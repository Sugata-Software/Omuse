//! 16-bit compatibility import is deliberately one merged image, never a
//! claim that Photoshop layers or their colour-space blending were retained.
use image::{Rgba, RgbaImage};
use omuse::{document, import_report, psd};
use photocraft_psd::{Compression, LayerSpec, PixelData, PsdBuilder, Version};
use std::io::Write;

fn open_bytes(bytes: &[u8]) -> omuse::model::Document {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Photo.psd");
    std::fs::write(&path, bytes).unwrap();
    psd::open(&path).unwrap()
}

fn rejects(bytes: &[u8]) -> String {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Unsupported.psd");
    std::fs::write(&path, bytes).unwrap();
    format!("{:#}", psd::open(&path).unwrap_err())
}

/// Minimal RGB16 PSD authored directly from the format, without PsdBuilder.
fn independent_rgb16(samples: &[[u16; 3]], compression: u16, profile: Option<&[u8]>) -> Vec<u8> {
    let rgba: Vec<_> = samples.iter().map(|p| [p[0], p[1], p[2], 65535]).collect();
    independent_merged16(&rgba, 3, 1, compression, profile)
}

/// Store supplied source-space merged samples verbatim. Four channels have an
/// explicit Mt16 declaration; these RGB samples must already be white-matted.
/// This author uses neither the production decoder nor PsdBuilder.
fn independent_merged16(
    samples: &[[u16; 4]],
    channels: u16,
    version: u16,
    compression: u16,
    profile: Option<&[u8]>,
) -> Vec<u8> {
    assert!(matches!(channels, 3 | 4));
    assert!(matches!(version, 1 | 2));
    let mut bytes = b"8BPS".to_vec();
    bytes.extend(version.to_be_bytes());
    bytes.extend([0; 6]);
    bytes.extend(channels.to_be_bytes());
    bytes.extend(1u32.to_be_bytes());
    bytes.extend((samples.len() as u32).to_be_bytes());
    bytes.extend(16u16.to_be_bytes());
    bytes.extend(3u16.to_be_bytes());
    bytes.extend(0u32.to_be_bytes());
    let mut resources = Vec::new();
    if let Some(profile) = profile {
        resources.extend(b"8BIM");
        resources.extend(1039u16.to_be_bytes());
        resources.extend([0, 0]);
        resources.extend((profile.len() as u32).to_be_bytes());
        resources.extend(profile);
        if profile.len() % 2 != 0 {
            resources.push(0);
        }
    }
    bytes.extend((resources.len() as u32).to_be_bytes());
    bytes.extend(resources);
    let mut layer_section = Vec::new();
    if channels == 4 {
        layer_section.extend(vec![0; if version == 2 { 8 } else { 4 }]); // empty layer info
        layer_section.extend(0u32.to_be_bytes()); // no global mask
        layer_section.extend(b"8BIMMt16");
        layer_section.extend(vec![0; if version == 2 { 8 } else { 4 }]); // empty marker
    }
    if version == 2 {
        bytes.extend((layer_section.len() as u64).to_be_bytes());
    } else {
        bytes.extend((layer_section.len() as u32).to_be_bytes());
    }
    bytes.extend(layer_section);
    bytes.extend(compression.to_be_bytes());
    let mut planar = Vec::new();
    for channel in 0..usize::from(channels) {
        let mut previous = 0u16;
        for pixel in samples {
            let value = if compression == 3 {
                pixel[channel].wrapping_sub(previous)
            } else {
                pixel[channel]
            };
            previous = pixel[channel];
            planar.extend(value.to_be_bytes());
        }
    }
    match compression {
        0 => bytes.extend(planar),
        1 => {
            let row_bytes = samples.len() * 2;
            assert!(row_bytes <= 128);
            for _ in 0..channels {
                if version == 2 {
                    bytes.extend(((row_bytes + 1) as u32).to_be_bytes());
                } else {
                    bytes.extend(((row_bytes + 1) as u16).to_be_bytes());
                }
            }
            for row in planar.chunks(row_bytes) {
                bytes.push((row_bytes - 1) as u8);
                bytes.extend(row);
            }
        }
        2 | 3 => {
            let mut zip = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            zip.write_all(&planar).unwrap();
            bytes.extend(zip.finish().unwrap());
        }
        _ => unreachable!(),
    }
    bytes
}

#[test]
fn independently_authored_rgb16_samples_stay_exact_for_every_compression() {
    let pixels = [
        [1, 255, 256],
        [257, 32768, 65534],
        [65535, 0, 12345],
        [513, 258, 32767],
    ];
    for compression in 0..=3 {
        let doc = open_bytes(&independent_rgb16(&pixels, compression, None));
        assert_eq!(doc.layers.len(), 1);
        let source = doc.layers[0].advanced.as_ref().unwrap().source.to_rgba16();
        for (actual, expected) in source.pixels().zip(pixels) {
            assert_eq!(
                actual.0,
                [expected[0], expected[1], expected[2], 65535],
                "compression {compression}"
            );
        }
        assert!(
            import_report::conversion_notes(&doc)
                .iter()
                .any(|x| x.contains("Original layers, masks, text, adjustments"))
        );
        assert_eq!(doc.metadata["sourceBitDepth"], 16);
    }
}

#[test]
fn psb_layered_sources_use_the_declared_merged_alpha_and_keep_precision_after_native_save() {
    let samples = vec![1001, 32001, 65534, 12345, 60001, 27001, 42, 65535];
    // Layer pixels remain straight. The merged preview is independently
    // white-matted: round(channel * alpha / 65535 + 65535 - alpha).
    // The inverse loses low-alpha precision; do not promise the original RGB.
    let matted = vec![53379, 59218, 65535, 12345, 60001, 27001, 42, 65535];
    let interpreted = vec![1003, 32000, 65535, 12345, 60001, 27001, 42, 65535];
    for version in [Version::Psd, Version::Psb] {
        for compression in Compression::ALL {
            let mut builder = PsdBuilder::new(2, 1)
                .depth(16)
                .version(version)
                .compression(compression);
            builder.push_layer(LayerSpec::new(
                "Original layer",
                0,
                0,
                2,
                1,
                PixelData::Rgba16(samples.clone()),
            ));
            builder.composite(PixelData::Rgba16(matted.clone()));
            let bytes = builder.to_bytes().unwrap();
            let doc = open_bytes(&bytes);
            assert_eq!(
                doc.layers[0]
                    .advanced
                    .as_ref()
                    .unwrap()
                    .source
                    .to_rgba16()
                    .into_raw(),
                interpreted
            );
            let temp = tempfile::tempdir().unwrap();
            let project = temp.path().join("Retained.omuse");
            document::save(&doc, &project).unwrap();
            let native = document::open(&project).unwrap();
            assert_eq!(
                native.layers[0]
                    .advanced
                    .as_ref()
                    .unwrap()
                    .source
                    .to_rgba16()
                    .into_raw(),
                interpreted
            );
        }
    }
}

#[test]
fn independent_merged_white_matte_is_unblended_at_full_precision_for_every_codec_and_container() {
    // Hard-coded source-space stored samples and exact rational inverse values.
    // Include zero-alpha hidden RGB, tiny/partial/full alpha, non-u8-aligned
    // values, and a malformed matte below its white floor (clamp, not wrap).
    let stored = [
        [1001, 32001, 65534, 0],
        [65534, 65535, 65534, 1],
        [44924, 48011, 50690, 21845],
        [38940, 48767, 65535, 32768],
        [60001, 27001, 42, 65535],
        [0, 50000, 65535, 12345],
    ];
    let interpreted = [
        [1001, 32001, 65534, 0],
        [0, 65535, 0, 1],
        [3702, 12963, 21000, 21845],
        [12346, 32000, 65535, 32768],
        [60001, 27001, 42, 65535],
        [0, 0, 65535, 12345],
    ];
    for version in [1, 2] {
        for compression in 0..=3 {
            let bytes = independent_merged16(&stored, 4, version, compression, None);
            let doc = open_bytes(&bytes);
            let exact = doc.layers[0].advanced.as_ref().unwrap().source.to_rgba16();
            for (i, (actual, expected)) in exact.pixels().zip(interpreted).enumerate() {
                assert_eq!(
                    actual.0, expected,
                    "version {version}, codec {compression}, pixel {i}"
                );
            }
            let notes = import_report::conversion_notes(&doc);
            assert!(
                notes
                    .iter()
                    .any(|x| x.contains("white matte before colour conversion"))
            );
            assert!(notes.iter().any(|x| x.contains("quantization")));
            let temp = tempfile::tempdir().unwrap();
            let project = temp.path().join("Interpreted.omuse");
            document::save(&doc, &project).unwrap();
            let reopened = document::open(&project).unwrap();
            assert_eq!(
                reopened.layers[0]
                    .advanced
                    .as_ref()
                    .unwrap()
                    .source
                    .to_rgba16(),
                exact
            );
        }
    }
}

#[test]
fn declared_white_matte_is_removed_in_source_space_before_nonlinear_icc_conversion() {
    // Alpha is exactly 1/3 or 2/3, making these inverse source values exact.
    // Converting the matte first and unblending afterwards yields very
    // different values and must not pass this analytic sRGB-transfer test.
    let stored = [
        [47786, 51882, 60074, 21845],
        [25941, 46421, 54613, 43690],
        [12288, 24576, 49152, 0],
        [6144, 36864, 49152, 65535],
    ];
    let linear = [
        [12288u16, 24576, 49152, 21845],
        [6144, 36864, 49152, 43690],
        [12288, 24576, 49152, 0],
        [6144, 36864, 49152, 65535],
    ];
    for version in [1, 2] {
        let bytes = independent_merged16(
            &stored,
            4,
            version,
            3,
            Some(include_bytes!("fixtures/photoshop/linear-srgb16.icc")),
        );
        let doc = open_bytes(&bytes);
        let converted = doc.layers[0].advanced.as_ref().unwrap().source.to_rgba16();
        for (actual, source) in converted.pixels().zip(linear) {
            for channel in 0..3 {
                let value = f64::from(source[channel]) / 65535.;
                let srgb = if value <= 0.0031308 {
                    12.92 * value
                } else {
                    1.055 * value.powf(1. / 2.4) - 0.055
                };
                let expected = (srgb * 65535.).round() as u16;
                assert!(
                    actual[channel].abs_diff(expected) <= 64,
                    "version {version}, channel {channel}: {} != {expected}",
                    actual[channel]
                );
            }
            assert_eq!(actual[3], source[3], "ICC must preserve alpha");
        }
    }
}

#[test]
fn embedded_linear_profile_is_converted_against_an_independent_srgb_transfer_reference() {
    let values = [[8192, 16384, 32768], [4096, 24576, 49152]];
    let doc = open_bytes(&independent_rgb16(
        &values,
        3,
        Some(include_bytes!("fixtures/photoshop/linear-srgb16.icc")),
    ));
    let converted = doc.layers[0].advanced.as_ref().unwrap().source.to_rgba16();
    for (actual, original) in converted.pixels().zip(values) {
        for channel in 0..3 {
            let linear = f64::from(original[channel]) / 65535.;
            let srgb = if linear <= 0.0031308 {
                12.92 * linear
            } else {
                1.055 * linear.powf(1. / 2.4) - 0.055
            };
            let expected = (srgb * 65535.).round() as u16;
            assert!(
                actual[channel].abs_diff(expected) <= 64,
                "{} != {expected}",
                actual[channel]
            );
            assert_ne!(actual[channel], original[channel]);
        }
        assert_eq!(actual[3], 65535);
    }
    assert_eq!(
        doc.layers[0].metadata["sourceColorProfile"]["iccBytes"],
        include_bytes!("fixtures/photoshop/linear-srgb16.icc").len()
    );
    assert!(
        import_report::conversion_notes(&doc)
            .iter()
            .any(|x| x.contains("colour-managed"))
    );
}

#[test]
fn real_independent_psd_and_psb_keep_their_icc_and_are_explicitly_merged() {
    for bytes in [
        include_bytes!("fixtures/photoshop/psd-tools-16bit5x5.psd").as_slice(),
        include_bytes!("fixtures/photoshop/psd-tools-16bit5x5.psb").as_slice(),
    ] {
        let doc = open_bytes(bytes);
        assert_eq!((doc.width, doc.height), (5, 5));
        assert_eq!(doc.layers.len(), 1);
        assert_eq!(
            doc.layers[0].metadata["sourceColorProfile"]["iccBytes"],
            3144
        );
        assert!(doc.layers[0].advanced.is_some());
        assert!(!import_report::conversion_notes(&doc).is_empty());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Source.psd");
        std::fs::write(&path, bytes).unwrap();
        let placed = document::import_image(&path).unwrap();
        assert!(placed.children[0].advanced.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn current_8bit_icc_refusal_and_unsupported_depth_modes_profiles_and_spot_channels_remain_explicit()
{
    let valid = independent_rgb16(&[[10, 20, 30]], 0, None);
    for (offset, value) in [(22, 32u16), (24, 4), (12, 5)] {
        let mut bytes = valid.clone();
        bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        assert!(!rejects(&bytes).is_empty());
    }
    let mut huge = valid.clone();
    huge[14..18].copy_from_slice(&5000u32.to_be_bytes());
    huge[18..22].copy_from_slice(&5000u32.to_be_bytes());
    assert!(rejects(&huge).contains("16 megapixel"));
    let mut spot = valid.clone();
    spot[12..14].copy_from_slice(&4u16.to_be_bytes());
    spot.extend([0, 0]);
    assert!(rejects(&spot).contains("spot channel"));
    let invalid = independent_rgb16(&[[10, 20, 30]], 0, Some(b"broken profile"));
    assert!(rejects(&invalid).contains("ICC profile"));
    let mut builder =
        PsdBuilder::new(1, 1).icc_profile(omuse::color_management::srgb_profile().unwrap());
    builder.composite(PixelData::Rgba8(vec![10, 20, 30, 255]));
    assert!(rejects(&builder.to_bytes().unwrap()).contains("embedded ICC profile"));
    let mut absent = PsdBuilder::new(1, 1).depth(16);
    absent.push_layer(LayerSpec::new(
        "No preview",
        0,
        0,
        1,
        1,
        PixelData::Rgba16(vec![1, 2, 3, 65535]),
    ));
    assert!(rejects(&absent.to_bytes().unwrap()).contains("no real merged preview"));
}

#[test]
fn declared_srgb_keeps_exact_samples_and_truncation_never_fabricates_pixels() {
    let samples = [[1, 255, 256], [12345, 32768, 65534]];
    let profile = omuse::color_management::srgb_profile().unwrap();
    let doc = open_bytes(&independent_rgb16(&samples, 0, Some(&profile)));
    let exact = doc.layers[0].advanced.as_ref().unwrap().source.to_rgba16();
    for (p, s) in exact.pixels().zip(samples) {
        assert_eq!(p.0, [s[0], s[1], s[2], 65535]);
    }
    let bytes = independent_rgb16(&samples, 0, None);
    for end in [0, 12, 25, bytes.len() - 1] {
        assert!(!rejects(&bytes[..end]).is_empty());
    }
    let expected = RgbaImage::from_fn(2, 1, |x, _| {
        Rgba(
            [
                samples[x as usize][0],
                samples[x as usize][1],
                samples[x as usize][2],
                65535,
            ]
            .map(|v| ((u32::from(v) * 255 + 32767) / 65535) as u8),
        )
    });
    assert_eq!(doc.layers[0].image.as_deref().unwrap(), &expected);
}

#[test]
fn high_depth_file_and_decode_budgets_are_checked_before_large_work() {
    let bytes = independent_rgb16(&[[1, 2, 3]], 0, None);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Too-large.psd");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&bytes).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    drop(file);
    assert!(format!("{:#}", psd::open(&path).unwrap_err()).contains("64 MiB"));
    for compression in 0..=3 {
        let mut excess = independent_rgb16(&[[1, 2, 3], [4, 5, 6]], compression, None);
        excess[18..22].copy_from_slice(&1u32.to_be_bytes());
        assert!(!rejects(&excess).is_empty(), "compression {compression}");
    }
}
