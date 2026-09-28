use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn prepared_image_keeps_pixels_and_never_becomes_a_project_save_target() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source.png");
    let pixels = image::RgbaImage::from_fn(19, 13, |x, y| {
        image::Rgba([(x * 11) as u8, (y * 17) as u8, 81, 255])
    });
    pixels.save(&source).unwrap();
    let prepared = PreparedEditor::load(Some(source), &AtomicBool::new(false)).unwrap();
    assert_eq!(prepared.pixels, pixels);
    assert_eq!(prepared.display.dimensions(), (19, 13));
    assert!(prepared.path.is_none());
    assert!(prepared.live_stamp.is_none());
    assert_eq!(prepared.status, "Document opened");
}

#[test]
fn prepared_project_keeps_render_save_target_and_external_change_stamp() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("source.comp");
    let mut doc = Document::new(19, 13);
    doc.layers[0].image =
        Some(image::RgbaImage::from_pixel(19, 13, image::Rgba([127, 42, 201, 255])).into());
    doc.layers[0].opacity = 0.6;
    document::save(&doc, &path).unwrap();
    let expected = raster::composite(&doc);
    let prepared = PreparedEditor::load(Some(path.clone()), &AtomicBool::new(false)).unwrap();
    assert_eq!(prepared.pixels, expected);
    assert_eq!(prepared.display.dimensions(), (19, 13));
    assert_eq!(prepared.path.as_ref(), Some(&path));
    assert_eq!(prepared.live_stamp, project_stamp(&path));
    assert!(prepared.live_stamp.is_some());
}

#[test]
fn failed_folder_open_falls_back_without_overwriting_the_failed_project() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("broken.comp");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("document.json"), b"invalid").unwrap();
    let prepared = PreparedEditor::load(Some(path.clone()), &AtomicBool::new(false)).unwrap();
    assert!(prepared.path.is_none());
    assert!(prepared.live_stamp.is_none());
    assert!(prepared.status.starts_with("Could not open document:"));
    assert_eq!(prepared.pixels.dimensions(), (1024, 768));
    assert_eq!(
        std::fs::read(path.join("document.json")).unwrap(),
        b"invalid"
    );
}

#[test]
fn cancelled_preparation_returns_no_document_or_display_buffers() {
    assert!(PreparedEditor::load(None, &AtomicBool::new(true)).is_none());
}
