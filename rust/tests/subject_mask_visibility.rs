use image::{GrayImage, Luma, Rgba, RgbaImage};
use omuse::{
    editor::Editor,
    model::{Document, Layer},
    raster,
};

#[test]
fn replacing_a_disabled_black_mask_preserves_visible_pixels_and_undo_restores_disabled_data() {
    let mut doc = Document::new(8, 8);
    let layer = &mut doc.layers[0];
    layer.image = Some(RgbaImage::from_pixel(8, 8, Rgba([150, 100, 80, 255])).into());
    layer.mask = Some(RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255])).into());
    layer.metadata["maskEnabled"] = serde_json::json!(false);
    let source = layer.image.clone().unwrap();
    let id = layer.id.clone();
    let mut editor = Editor::new(doc);
    assert_eq!(raster::composite(&editor.document).get_pixel(4, 4)[3], 255);
    assert!(
        editor
            .apply_subject_mask(&id, &GrayImage::from_pixel(8, 8, Luma([255])), false)
            .unwrap()
    );
    assert_eq!(
        raster::composite(&editor.document).get_pixel(4, 4)[3],
        255,
        "disabled prior coverage must not hide the replacement"
    );
    assert!(
        editor.document.layers[0]
            .image
            .as_ref()
            .unwrap()
            .shares_pixels_with(&source)
    );
    assert!(editor.undo());
    assert_eq!(editor.document.layers[0].metadata["maskEnabled"], false);
    assert_eq!(
        editor.document.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .get_pixel(4, 4)[0],
        0
    );
    assert_eq!(raster::composite(&editor.document).get_pixel(4, 4)[3], 255);
}
#[test]
fn replacing_enabled_coverage_keeps_the_existing_intersection() {
    let mut doc = Document::new(8, 8);
    doc.layers[0].image = Some(RgbaImage::from_pixel(8, 8, Rgba([150, 100, 80, 255])).into());
    doc.layers[0].mask = Some(RgbaImage::from_pixel(8, 8, Rgba([64, 64, 64, 255])).into());
    let id = doc.layers[0].id.clone();
    let mut editor = Editor::new(doc);
    assert!(
        editor
            .apply_subject_mask(&id, &GrayImage::from_pixel(8, 8, Luma([128])), false)
            .unwrap()
    );
    assert_eq!(
        editor.document.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .get_pixel(4, 4)[0],
        32
    );
}
#[test]
fn new_subject_bitmap_preserves_live_clipping_even_when_the_old_bitmap_is_disabled() {
    for disabled_bitmap in [false, true] {
        let mut doc = Document::new(8, 8);
        let mut source = Layer::paint("Live half-image clip", 8, 8);
        source.image = Some(
            RgbaImage::from_fn(8, 8, |x, _| {
                Rgba([255, 255, 255, if x < 4 { 255 } else { 0 }])
            })
            .into(),
        );
        source.visible = false;
        let source_id = source.id.clone();
        let target = &mut doc.layers[0];
        target.image = Some(RgbaImage::from_pixel(8, 8, Rgba([150, 100, 80, 255])).into());
        let value = if disabled_bitmap { 0 } else { 128 };
        target.mask = Some(RgbaImage::from_pixel(8, 8, Rgba([value, value, value, 255])).into());
        target.metadata["maskEnabled"] = serde_json::json!(!disabled_bitmap);
        target.metadata["maskSourceID"] = serde_json::json!(source_id);
        let id = target.id.clone();
        let pixels = target.image.clone().unwrap();
        let old_mask = target.mask.clone().unwrap();
        let old_metadata = target.metadata.clone();
        doc.layers.push(source);
        let before = raster::composite(&doc);
        assert_eq!(
            before.get_pixel(1, 4)[3],
            if disabled_bitmap { 255 } else { 128 }
        );
        assert_eq!(
            before.get_pixel(6, 4)[3],
            0,
            "the live link must really clip half the image"
        );
        let mut editor = Editor::new(doc);
        assert!(
            editor
                .apply_subject_mask(&id, &GrayImage::from_pixel(8, 8, Luma([255])), false)
                .unwrap()
        );
        assert_eq!(raster::composite(&editor.document), before);
        assert_eq!(
            editor.document.layers[0].metadata["maskSourceID"],
            source_id
        );
        assert!(
            editor.document.layers[0]
                .image
                .as_ref()
                .unwrap()
                .shares_pixels_with(&pixels)
        );
        assert!(editor.undo());
        assert_eq!(editor.document.layers[0].metadata, old_metadata);
        assert_eq!(editor.document.layers[0].mask.as_ref().unwrap(), &old_mask);
        assert_eq!(raster::composite(&editor.document), before);
    }
}
