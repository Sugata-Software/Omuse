use omuse::vector_scene::{VECTOR_SCENE_VERSION, VectorObject, VectorScene};
use omuse::vector_svg_scene::{decode_scene, encode_scene, import_scene};
use resvg::{tiny_skia, usvg};
use std::sync::atomic::AtomicBool;
use tempfile::tempdir;

const SVG: &str = r###"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48" viewBox="0 0 64 48">
  <g id="background"><rect id="back" x="0" y="0" width="64" height="48" fill="#204060"/></g>
  <g id="foreground"><path id="front" d="M 8 8 L 40 8 L 40 32 L 8 32 Z" fill="#FF0000" fill-opacity="0.5" opacity="0.75"/></g>
</svg>"###;

fn render_svg(svg: &str, width: u32, height: u32) -> Vec<u8> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.take()
}

fn unpremultiply(mut pixels: Vec<u8>) -> Vec<u8> {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            pixel[..3].fill(0);
        } else {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    pixels
}

#[test]
fn imports_multiple_objects_and_round_trips_group_names_and_paint_order() {
    let scene = decode_scene(SVG.as_bytes()).unwrap();
    assert_eq!((scene.width, scene.height), (64, 48));
    assert_eq!(scene.objects.len(), 2);
    assert_eq!(scene.objects[0].name, "back");
    assert_eq!(scene.objects[1].name, "front");
    assert_eq!(scene.objects[0].groups[0].name, "background");
    assert_eq!(scene.objects[1].groups[0].name, "foreground");
    let encoded = encode_scene(&scene).unwrap();
    let reread = decode_scene(encoded.as_bytes()).unwrap();
    assert_eq!(reread.objects.len(), 2);
    assert_eq!(reread.objects[0].groups[0].name, "background");
    assert_eq!(reread.objects[1].groups[0].name, "foreground");
    assert_eq!(
        reread.objects[1].fill.unwrap()[3],
        scene.objects[1].fill.unwrap()[3]
    );
}

#[test]
fn scene_render_matches_independent_resvg_render() {
    let scene = decode_scene(SVG.as_bytes()).unwrap();
    let ours = scene.render(&AtomicBool::new(false)).unwrap().into_raw();
    let reference = unpremultiply(render_svg(SVG, 64, 48));
    let differing = ours
        .chunks_exact(4)
        .zip(reference.chunks_exact(4))
        .filter(|(left, right)| {
            left.iter()
                .zip(right.iter())
                .any(|(a, b)| a.abs_diff(*b) > 3)
        })
        .count();
    assert!(
        differing < 64 * 48 / 20,
        "too many differing pixels: {differing}"
    );
}

#[test]
fn imported_scene_matches_original_for_curves_circle_opacity_and_hidden_css() {
    let svg = r###"<svg xmlns='http://www.w3.org/2000/svg' width='64' height='48' viewBox='0 0 64 48'>
      <g><g>
        <circle cx='12' cy='12' r='8' fill='#204060'/>
        <path d='M 25 10 C 25 2 39 2 39 10 C 39 18 25 18 25 10 Z' fill='#FF0000' opacity='0.5'/>
      </g></g>
      <path d='M 5 30 L 25 30 L 25 44 L 5 44 Z' fill='#00FF00' style=' opacity : 0.5 ; '/>
      <g id='hidden-group' style=' display : none ; visibility : hidden '>
        <path d='M 40 30 L 60 30 L 60 44 L 40 44 Z' fill='#FFFFFF'/>
      </g>
    </svg>"###;
    let scene = decode_scene(svg.as_bytes()).unwrap();
    assert_eq!(scene.objects.len(), 4);
    assert!(scene.objects.iter().any(|object| !object.visible));
    assert!(scene.objects.iter().any(|object| object.groups.len() >= 2));
    let ours = scene.render(&AtomicBool::new(false)).unwrap().into_raw();
    let reference = unpremultiply(render_svg(svg, 64, 48));
    let differing = ours
        .chunks_exact(4)
        .zip(reference.chunks_exact(4))
        .filter(|(left, right)| {
            left.iter()
                .zip(right.iter())
                .any(|(a, b)| a.abs_diff(*b) > 5)
        })
        .count();
    assert!(
        differing < 64 * 48 / 10,
        "too many differing pixels: {differing}"
    );
}

#[test]
fn preserves_nested_unnamed_groups_opacity_stroke_and_hidden_geometry() {
    let svg = r###"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30">
      <g><g><path id="overlap" d="M 4 4 L 28 4 L 28 22 L 4 22 Z" fill="#FF0000" stroke="#0000FF" stroke-width="6" stroke-linecap="round" stroke-linejoin="round" opacity="0.5"/></g></g>
      <path id="hidden" d="M 1 1 L 3 1 L 3 3 L 1 3 Z" fill="#00FF00" visibility="hidden"/>
    </svg>"###;
    let scene = decode_scene(svg.as_bytes()).unwrap();
    assert_eq!(scene.objects.len(), 2);
    assert!(scene.objects[0].groups.len() >= 2);
    assert!((scene.objects[0].opacity - 0.5).abs() < 0.01);
    assert!(!scene.objects[1].visible);
    let reopened = decode_scene(encode_scene(&scene).unwrap().as_bytes()).unwrap();
    assert!(!reopened.objects[1].visible);
    assert!((reopened.objects[0].opacity - 0.5).abs() < 0.01);
}

#[test]
fn file_import_keeps_no_follow_and_size_bounds() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("scene.svg");
    std::fs::write(&path, SVG).unwrap();
    assert_eq!(import_scene(&path).unwrap().objects.len(), 2);
    #[cfg(unix)]
    {
        let link = dir.path().join("link.svg");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(import_scene(&link).is_err());
    }
    std::fs::write(&path, vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
    assert!(import_scene(&path).is_err());
}

#[test]
fn rejects_resources_and_opacity_over_multiple_objects() {
    let image = SVG.replace(
        "<g id=\"background\">",
        "<g id=\"background\"><image href=\"data:image/png;base64,AA==\"/>",
    );
    assert!(decode_scene(image.as_bytes()).is_err());
    let opacity = SVG.replace(
        "<g id=\"foreground\">",
        "<g id=\"foreground\" opacity=\"0.5\"><rect width=\"20\" height=\"20\"/>",
    );
    assert!(decode_scene(opacity.as_bytes()).is_err());
}

#[test]
fn emits_object_affine_transforms() {
    let mut object =
        VectorObject::rectangle("Moved", 0., 0., 10., 10., Some([0, 255, 0, 255]), None).unwrap();
    object.transform = [1., 0., 0., 1., 12., 7.];
    let scene = VectorScene {
        version: VECTOR_SCENE_VERSION,
        width: 64,
        height: 48,
        objects: vec![object],
    };
    let encoded = encode_scene(&scene).unwrap();
    assert!(encoded.contains("transform=\"matrix(1 0 0 1 12 7)\""));
    let reopened = decode_scene(encoded.as_bytes()).unwrap();
    assert_eq!(reopened.objects[0].transform, [1., 0., 0., 1., 0., 0.]);
    assert!((reopened.objects[0].path.bounds().unwrap().0.x - 12.).abs() < 1e-5);
}

#[test]
fn visibility_overrides_preserve_hidden_paint_and_transparent_objects() {
    let source = br##"<svg xmlns='http://www.w3.org/2000/svg' width='20' height='20'>
      <g visibility='hidden'><rect id='shown' visibility='visible' width='8' height='8' fill='#00ff00'/><rect id='hidden' x='8' width='8' height='8' style='fill:#123456;visibility:hidden'/></g>
      <rect id='transparent' x='2' y='12' width='5' height='5' fill='#ff0000' opacity='0'/>
    </svg>"##;
    let scene = decode_scene(source).unwrap();
    assert!(scene.objects[0].visible);
    assert!(!scene.objects[1].visible);
    assert_eq!(scene.objects[1].fill, Some([0x12, 0x34, 0x56, 255]));
    assert_eq!(scene.objects[2].opacity, 0.);
    let reopened = decode_scene(encode_scene(&scene).unwrap().as_bytes()).unwrap();
    assert_eq!(reopened.objects.len(), 3);
    assert!(reopened.objects[0].visible);
    assert!(!reopened.objects[1].visible);
    assert_eq!(reopened.objects[2].opacity, 0.);
}
