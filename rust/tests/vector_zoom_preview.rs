use std::sync::atomic::AtomicBool;

use image::RgbaImage;
use omuse::{
    editor::Editor,
    model::Document,
    vector_path::Point,
    vector_scene::{VectorObject, VectorScene},
    vector_scene_preview::{composite, composite_at_scale},
};

fn artwork() -> VectorScene {
    VectorScene {
        version: omuse::vector_scene::VECTOR_SCENE_VERSION,
        width: 8,
        height: 8,
        objects: vec![
            VectorObject::ellipse("Curve", 1., 1., 6., 6., Some([240, 40, 80, 255]), None).unwrap(),
        ],
    }
}

fn scaled_scene(source: &VectorScene, scale: f32) -> VectorScene {
    let mut scene = source.clone();
    scene.width = (source.width as f32 * scale).round() as u32;
    scene.height = (source.height as f32 * scale).round() as u32;
    for object in &mut scene.objects {
        for subpath in &mut object.path.subpaths {
            for anchor in &mut subpath.anchors {
                let scale_point = |point: &mut Point| {
                    point.x *= scale;
                    point.y *= scale;
                };
                scale_point(&mut anchor.position);
                if let Some(point) = &mut anchor.incoming {
                    scale_point(point);
                }
                if let Some(point) = &mut anchor.outgoing {
                    scale_point(point);
                }
            }
        }
        if let Some(stroke) = &mut object.stroke {
            stroke.width *= scale;
        }
    }
    scene
}

#[test]
fn scale_one_is_the_settled_preview_and_larger_scale_rerenders() {
    let document = Document::new(8, 8);
    let scene = artwork();
    let cancel = AtomicBool::new(false);
    let settled = composite(&document, None, &scene, &cancel).unwrap();
    let scaled = composite_at_scale(&document, None, &scene, 2., &cancel).unwrap();
    let scale_one = composite_at_scale(&document, None, &scene, 1., &cancel).unwrap();
    assert_eq!(scale_one, settled);
    assert_eq!(scaled.dimensions(), (16, 16));
    let enlarged =
        image::imageops::resize(&settled, 16, 16, image::imageops::FilterType::CatmullRom);
    assert_ne!(
        scaled, enlarged,
        "scale-aware preview must rerender geometry instead of enlarging the old cache"
    );
}

#[test]
fn cancellation_and_pixel_budget_are_bounded() {
    let document = Document::new(8, 8);
    let scene = artwork();
    let cancel = AtomicBool::new(true);
    assert!(composite_at_scale(&document, None, &scene, 2., &cancel).is_err());
    let cancel = AtomicBool::new(false);
    let fallback =
        composite_at_scale(&Document::new(4096, 4096), None, &scene, 4., &cancel).unwrap();
    assert_eq!(fallback.dimensions(), (4096, 4096));
}

#[test]
fn unsupported_masks_fall_back_to_settled_dimensions() {
    let mut document = Document::new(8, 8);
    document.layers[0].mask = Some(RgbaImage::new(8, 8).into());
    let scene = artwork();
    let cancel = AtomicBool::new(false);
    let expected = composite(&document, None, &scene, &cancel).unwrap();
    let actual = composite_at_scale(&document, None, &scene, 2., &cancel).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn transformed_target_and_photo_match_an_independent_high_resolution_reference() {
    let scene = artwork();
    let cancel = AtomicBool::new(false);
    let mut source_editor = Editor::new(Document::new(16, 12));
    source_editor.document.layers[0].image =
        Some(image::RgbaImage::from_pixel(16, 12, image::Rgba([20, 30, 40, 255])).into());
    let id = source_editor
        .insert_vector_scene(
            "Curve",
            source_editor.revision(),
            scene.clone(),
            scene.render(&cancel).unwrap(),
        )
        .unwrap();
    let target = source_editor.document.find_layer_mut(&id).unwrap();
    target.offset_x = 3.25;
    target.offset_y = 2.5;
    target.rotation = 17.;
    target.scale_x = 1.2;
    target.scale_y = 0.8;
    let actual =
        composite_at_scale(&source_editor.document, Some(&id), &scene, 2., &cancel).unwrap();

    let mut reference = Document::new(32, 24);
    reference.layers[0].image =
        Some(image::RgbaImage::from_pixel(16, 12, image::Rgba([20, 30, 40, 255])).into());
    reference.layers[0].scale_x = 2.;
    reference.layers[0].scale_y = 2.;
    let scaled = scaled_scene(&scene, 2.);
    let ref_id = {
        let mut editor = Editor::new(reference);
        let id = editor
            .insert_vector_scene(
                "Curve",
                editor.revision(),
                scaled.clone(),
                scaled.render(&cancel).unwrap(),
            )
            .unwrap();
        let layer = editor.document.find_layer_mut(&id).unwrap();
        layer.offset_x = 6.5;
        layer.offset_y = 5.;
        layer.rotation = 17.;
        layer.scale_x = 1.2;
        layer.scale_y = 0.8;
        reference = editor.document;
        id
    };
    assert_eq!(actual, omuse::raster::composite(&reference));
    assert!(!ref_id.is_empty());
}
