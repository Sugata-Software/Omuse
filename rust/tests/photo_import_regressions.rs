use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use omuse::{
    advanced_ops::{AdvancedOperation, FilterNode},
    document,
    editor::{Adjustment, Editor},
    filters::Filter,
    raster,
};

fn oriented_tiff16(path: &std::path::Path) -> image::ImageBuffer<Rgba<u16>, Vec<u16>> {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
    let mut image = encoder
        .new_image::<tiff::encoder::colortype::RGBA16>(2, 3)
        .unwrap();
    image
        .encoder()
        .write_tag(tiff::tags::Tag::Orientation, 6u16)
        .unwrap();
    let source = (0..6)
        .flat_map(|index| {
            [
                1001 + index * 1000,
                12003 + index * 37,
                45005 - index * 101,
                40001 + index * 503,
            ]
        })
        .collect::<Vec<u16>>();
    image.write_data(&source).unwrap();
    drop(encoder);

    // TIFF orientation 6 rotates clockwise: source (0, 0) lands at (2, 0).
    image::ImageBuffer::from_fn(3, 2, |x, y| {
        let source_x = y;
        let source_y = 2 - x;
        let index = source_y * 2 + source_x;
        Rgba([
            1001 + index as u16 * 1000,
            12003 + index as u16 * 37,
            45005 - index as u16 * 101,
            40001 + index as u16 * 503,
        ])
    })
}

#[test]
fn ordinary_open_retains_oriented_tiff16_through_project_and_explicit_16_bit_export() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("oriented-master.tiff");
    let expected = oriented_tiff16(&source_path);

    let opened = document::open(&source_path).unwrap();
    assert_eq!((opened.width, opened.height), expected.dimensions());
    let state = opened.layers[0]
        .advanced
        .as_ref()
        .expect("16-bit import must retain an editable master");
    assert_eq!(state.source.to_rgba16(), expected);
    assert_eq!(raster::composite16(&opened).unwrap(), expected);
    assert!(
        expected
            .pixels()
            .any(|pixel| pixel[3] < u16::MAX && pixel.0.iter().any(|value| value % 257 != 0))
    );

    let package = directory.path().join("retained.omuse");
    document::save(&opened, &package).unwrap();
    let reopened = document::open(&package).unwrap();
    assert_eq!(
        reopened.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        expected
    );

    let exact_export = directory.path().join("exact.tiff");
    raster::export16(&reopened, &exact_export).unwrap();
    let exact_file = std::fs::File::open(&exact_export).unwrap();
    let mut exact_decoder =
        tiff::decoder::Decoder::new(std::io::BufReader::new(exact_file)).unwrap();
    assert_eq!(
        exact_decoder
            .get_tag_u16_vec(tiff::tags::Tag::ExtraSamples)
            .unwrap(),
        [2]
    );
    match exact_decoder.read_image().unwrap() {
        tiff::decoder::DecodingResult::U16(samples) => {
            assert_eq!(samples, expected.as_raw().as_slice())
        }
        other => panic!("expected RGBA16 TIFF samples, got {other:?}"),
    }
    let exact = image::open(&exact_export).unwrap();
    assert_eq!(exact.color(), image::ColorType::Rgba16);
    assert_eq!(exact.to_rgba16(), expected);

    // The standard export command intentionally remains the 8-bit output path.
    let standard_export = directory.path().join("standard.tiff");
    raster::export(&reopened, &standard_export).unwrap();
    let expected_standard = raster::composite(&reopened);
    let standard_file = std::fs::File::open(&standard_export).unwrap();
    let mut standard_decoder =
        tiff::decoder::Decoder::new(std::io::BufReader::new(standard_file)).unwrap();
    assert_eq!(
        standard_decoder
            .get_tag_u16_vec(tiff::tags::Tag::ExtraSamples)
            .unwrap(),
        [2]
    );
    match standard_decoder.read_image().unwrap() {
        tiff::decoder::DecodingResult::U8(samples) => {
            assert_eq!(samples, expected_standard.as_raw().as_slice())
        }
        other => panic!("expected RGBA8 TIFF samples, got {other:?}"),
    }
    let standard = image::open(&standard_export).unwrap();
    assert_eq!(standard.color(), image::ColorType::Rgba8);
    assert_eq!(standard.to_rgba8(), expected_standard);
}

#[test]
fn advanced_photo_rejects_destructive_byte_edits_until_explicit_rasterization() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("master.tiff");
    let expected = oriented_tiff16(&source_path);
    let opened = document::open(&source_path).unwrap();
    let mut editor = Editor::new(opened);
    let id = editor.active_layer.clone();
    let revision = editor.revision();

    assert!(!editor.adjust(Adjustment::Brightness(0.1)));
    assert_eq!(editor.revision(), revision);
    assert_eq!(
        editor.document.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        expected
    );

    assert!(editor.rasterize_layer(&id));
    assert!(editor.document.layers[0].advanced.is_none());
    let before = editor.document.layers[0].image.clone();
    assert!(editor.adjust(Adjustment::Brightness(0.1)));
    assert_ne!(editor.document.layers[0].image, before);
    let edited = raster::composite(&editor.document);

    let package = directory.path().join("rasterized-edit.omuse");
    document::save(&editor.document, &package).unwrap();
    let reopened = document::open(&package).unwrap();
    assert!(reopened.layers[0].advanced.is_none());
    assert_eq!(raster::composite(&reopened), edited);

    assert!(editor.undo());
    assert!(editor.document.layers[0].advanced.is_none());
    assert!(editor.undo());
    assert_eq!(
        editor.document.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .source
            .to_rgba16(),
        expected
    );
    assert!(editor.redo());
    assert!(editor.document.layers[0].advanced.is_none());
    assert!(editor.redo());
    assert_eq!(raster::composite(&editor.document), edited);
}

#[test]
fn editable_filter_path_changes_result_without_quantizing_the_imported_master() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("filter-master.tiff");
    let expected = oriented_tiff16(&source_path);
    let opened = document::open(&source_path).unwrap();
    let mut editor = Editor::new(opened);
    let id = editor.active_layer.clone();
    let mut state = editor.editable_state(&id).unwrap();
    state.recipe.nodes.push(FilterNode {
        id: uuid::Uuid::new_v4().to_string(),
        name: "precision exposure".into(),
        enabled: true,
        opacity: 1.0,
        operation: AdvancedOperation::Filter(Filter::Exposure { stops: 0.5 }),
        soft_mask: None,
    });
    let evaluated = state
        .evaluate(&std::sync::atomic::AtomicBool::new(false))
        .unwrap();
    assert_eq!(evaluated.source.to_rgba16(), expected);
    assert_ne!(evaluated.result.to_rgba16(), expected);
    assert!(
        editor
            .replace_editable_states(vec![(id.clone(), evaluated)])
            .unwrap()
    );
    let retained = editor
        .document
        .find_layer(&id)
        .unwrap()
        .advanced
        .as_ref()
        .unwrap();
    assert_eq!(retained.source.to_rgba16(), expected);
    assert!(
        retained
            .result
            .to_rgba16()
            .pixels()
            .any(|pixel| pixel.0[..3].iter().any(|value| value % 257 != 0))
    );
}

#[test]
fn ordinary_8_bit_photo_stays_on_the_lightweight_raster_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ordinary.png");
    let pixels = RgbaImage::from_fn(17, 13, |x, y| {
        Rgba([(x * 11) as u8, (y * 17) as u8, (x + y) as u8, 255])
    });
    DynamicImage::ImageRgba8(pixels.clone())
        .save_with_format(&path, ImageFormat::Png)
        .unwrap();

    let opened = document::open(&path).unwrap();
    assert!(opened.layers[0].advanced.is_none());
    assert_eq!(raster::composite(&opened), pixels);
}

#[test]
fn oversized_16_bit_photo_is_rejected_before_pixel_decode_instead_of_quantized() {
    fn entry(output: &mut Vec<u8>, tag: u16, field_type: u16, count: u32, value: u32) {
        output.extend_from_slice(&tag.to_le_bytes());
        output.extend_from_slice(&field_type.to_le_bytes());
        output.extend_from_slice(&count.to_le_bytes());
        output.extend_from_slice(&value.to_le_bytes());
    }

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("over-precision-limit.tiff");
    const WIDTH: u32 = 4097;
    const HEIGHT: u32 = 4097;
    const ENTRIES: u16 = 14;
    const IFD_OFFSET: u32 = 8;
    const IFD_END: u32 = IFD_OFFSET + 2 + ENTRIES as u32 * 12 + 4;
    const BITS_OFFSET: u32 = IFD_END;
    const X_RESOLUTION_OFFSET: u32 = BITS_OFFSET + 8;
    const Y_RESOLUTION_OFFSET: u32 = X_RESOLUTION_OFFSET + 8;
    const SAMPLE_FORMAT_OFFSET: u32 = Y_RESOLUTION_OFFSET + 8;
    const PIXEL_OFFSET: u32 = SAMPLE_FORMAT_OFFSET + 8;
    const PIXEL_BYTES: u32 = WIDTH * HEIGHT * 4 * 2;

    // A complete little-endian RGBA16 directory with one uncompressed strip.
    // Extending the file creates a sparse zero-filled payload, so the decoder
    // sees consistent offsets and sizes without allocating 128 MiB in the test.
    let mut header = Vec::with_capacity(PIXEL_OFFSET as usize);
    header.extend_from_slice(b"II");
    header.extend_from_slice(&42u16.to_le_bytes());
    header.extend_from_slice(&IFD_OFFSET.to_le_bytes());
    header.extend_from_slice(&ENTRIES.to_le_bytes());
    entry(&mut header, 256, 4, 1, WIDTH); // ImageWidth, LONG
    entry(&mut header, 257, 4, 1, HEIGHT); // ImageLength, LONG
    entry(&mut header, 258, 3, 4, BITS_OFFSET); // BitsPerSample, SHORT[4]
    entry(&mut header, 259, 3, 1, 1); // Compression = none
    entry(&mut header, 262, 3, 1, 2); // Photometric = RGB
    entry(&mut header, 273, 4, 1, PIXEL_OFFSET); // StripOffsets
    entry(&mut header, 277, 3, 1, 4); // SamplesPerPixel
    entry(&mut header, 278, 4, 1, HEIGHT); // RowsPerStrip
    entry(&mut header, 279, 4, 1, PIXEL_BYTES); // StripByteCounts
    entry(&mut header, 282, 5, 1, X_RESOLUTION_OFFSET); // XResolution
    entry(&mut header, 283, 5, 1, Y_RESOLUTION_OFFSET); // YResolution
    entry(&mut header, 296, 3, 1, 1); // ResolutionUnit = none
    entry(&mut header, 338, 3, 1, 2); // ExtraSamples = unassociated alpha
    entry(&mut header, 339, 3, 4, SAMPLE_FORMAT_OFFSET); // unsigned integer[4]
    header.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    for _ in 0..4 {
        header.extend_from_slice(&16u16.to_le_bytes());
    }
    for _ in 0..2 {
        header.extend_from_slice(&1u32.to_le_bytes());
        header.extend_from_slice(&1u32.to_le_bytes());
    }
    for _ in 0..4 {
        header.extend_from_slice(&1u16.to_le_bytes());
    }
    assert_eq!(header.len(), PIXEL_OFFSET as usize);
    use std::io::Write;
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&header).unwrap();
    file.set_len(u64::from(PIXEL_OFFSET) + u64::from(PIXEL_BYTES))
        .unwrap();
    drop(file);

    let error = document::open(&path).unwrap_err().to_string();
    assert!(error.contains("16 megapixel"), "{error}");
}

#[test]
fn ordinary_raw_open_uses_the_embedded_16_bit_master_when_fixture_is_available() {
    let Some(path) = omuse::identity::env_var_os("OMUSE_RAW_FIXTURE") else {
        return;
    };
    let opened = document::open(std::path::Path::new(&path)).unwrap();
    let state = opened.layers[0]
        .advanced
        .as_ref()
        .expect("RAW open must retain the developed master and original bytes");
    assert!(state.raw_bytes.is_some());
    assert!(
        state
            .source
            .to_rgba16()
            .pixels()
            .take(100_000)
            .any(|pixel| pixel.0[..3].iter().any(|value| value % 257 != 0)),
        "RAW open must not pass through an 8-bit surface"
    );
}
