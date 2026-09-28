use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use omuse::{
    create_project::Project,
    document,
    editor::{Adjustment, Editor},
    raster::{self, ExportOptions},
};

fn fixture_photo() -> RgbaImage {
    RgbaImage::from_fn(24, 18, |x, y| {
        Rgba([
            30 + ((x * 5 + y * 3) % 120) as u8,
            40 + ((x * 2 + y * 7) % 120) as u8,
            70 + ((x * 3 + y * 2) % 120) as u8,
            255,
        ])
    })
}

fn flattened(pixel: [u8; 4], matte: [u8; 3]) -> [u8; 3] {
    let alpha = u32::from(pixel[3]);
    std::array::from_fn(|channel| {
        ((u32::from(pixel[channel]) * alpha + u32::from(matte[channel]) * (255 - alpha) + 127)
            / 255) as u8
    })
}

#[test]
fn imported_photo_edit_crop_resize_package_and_exports_roundtrip() {
    let directory = tempfile::tempdir().unwrap();
    let imported_path = directory.path().join("original.png");
    let source = fixture_photo();
    DynamicImage::ImageRgba8(source.clone())
        .save_with_format(&imported_path, ImageFormat::Png)
        .unwrap();

    // Image import must retain the source raster before any edit is applied.
    let imported = document::open(&imported_path).unwrap();
    assert_eq!((imported.width, imported.height), source.dimensions());
    assert_eq!(raster::composite(&imported), source);
    assert_eq!(
        imported.layers[0].image.as_ref().unwrap().to_image(),
        source
    );

    let mut editor = Editor::new(imported);
    let layer_id = editor.active_layer.clone();
    editor.select_rectangle(4.0, 3.0, 16.0, 12.0);
    assert!(editor.adjust(Adjustment::Brightness(0.1)));
    let adjusted = raster::composite(&editor.document);
    assert_eq!(adjusted.get_pixel(0, 0), source.get_pixel(0, 0));
    assert_eq!(
        adjusted.get_pixel(8, 7).0,
        [
            source.get_pixel(8, 7)[0] + 26,
            source.get_pixel(8, 7)[1] + 26,
            source.get_pixel(8, 7)[2] + 26,
            255,
        ]
    );

    // The same selection becomes an editable mask, then the nondestructive
    // crop and resize retain that source and mask through undo/redo.
    assert!(editor.add_mask(&layer_id, true));
    assert_eq!(raster::composite(&editor.document).get_pixel(0, 0)[3], 0);
    assert_eq!(raster::composite(&editor.document).get_pixel(8, 7)[3], 255);
    assert!(editor.crop_canvas(2, 1, 18, 14));
    assert!(editor.resize_canvas(16, 12));
    assert!(editor.resize_image(32, 24));
    let final_document = editor.document.clone();
    let final_composite = raster::composite(&final_document);
    let final_source = final_document.find_layer(&layer_id).unwrap().image.clone();
    let final_mask = final_document.find_layer(&layer_id).unwrap().mask.clone();
    assert_eq!(final_composite.dimensions(), (32, 24));
    assert_eq!(final_composite.get_pixel(12, 12)[3], 255);
    assert_eq!(editor.undo_depth(), 5);

    assert!(editor.undo());
    assert_eq!((editor.document.width, editor.document.height), (16, 12));
    assert!(editor.undo());
    assert_eq!((editor.document.width, editor.document.height), (18, 14));
    assert!(editor.undo());
    assert_eq!((editor.document.width, editor.document.height), (24, 18));
    assert!(editor.redo());
    assert!(editor.redo());
    assert!(editor.redo());
    assert_eq!(raster::composite(&editor.document), final_composite);
    assert_eq!(
        editor.document.find_layer(&layer_id).unwrap().image,
        final_source
    );
    assert_eq!(
        editor.document.find_layer(&layer_id).unwrap().mask,
        final_mask
    );

    let package = directory.path().join("photo-workflow.omuse");
    let mut project = Project::new("Photo workflow", final_document);
    project.save(&package).unwrap();
    let mut reopened = Project::open(&package).unwrap();
    let reopened_document = reopened.active_document().unwrap().clone();
    let reopened_layer = reopened_document.find_layer(&layer_id).unwrap();
    assert_eq!(raster::composite(&reopened_document), final_composite);
    assert_eq!(reopened_layer.image, final_source);
    assert_eq!(reopened_layer.mask, final_mask);

    let options = ExportOptions {
        jpeg_quality: 100,
        matte: [17, 23, 29],
    };
    for extension in ["png", "tiff"] {
        let path = directory.path().join(format!("final.{extension}"));
        raster::export_with_options(&reopened_document, &path, options).unwrap();
        assert_eq!(image::open(path).unwrap().to_rgba8(), final_composite);
    }

    let jpeg_path = directory.path().join("final.jpg");
    raster::export_with_options(&reopened_document, &jpeg_path, options).unwrap();
    let jpeg = image::open(jpeg_path).unwrap().to_rgba8();
    assert_eq!(jpeg.dimensions(), final_composite.dimensions());
    assert!(jpeg.pixels().all(|pixel| pixel[3] == 255));
    let mut maximum_error = 0u8;
    let mut total_error = 0u64;
    for (expected, actual) in final_composite.pixels().zip(jpeg.pixels()) {
        let expected = flattened(expected.0, options.matte);
        for channel in 0..3 {
            let error = expected[channel].abs_diff(actual[channel]);
            maximum_error = maximum_error.max(error);
            total_error += u64::from(error);
        }
    }
    let channels = u64::from(jpeg.width()) * u64::from(jpeg.height()) * 3;
    assert!(
        maximum_error <= 24,
        "JPEG maximum channel error: {maximum_error}"
    );
    assert!(
        total_error * 10 <= channels * 25,
        "JPEG mean channel error exceeds 2.5: {total_error}/{channels}"
    );
}
