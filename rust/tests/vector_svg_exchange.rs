use omuse::vector_svg_scene::{decode_scene, decode_scene_with_report, encode_scene};
use resvg::{tiny_skia, usvg};
use std::sync::Arc;

fn render(svg: &str) -> Vec<u8> {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_font_data(include_bytes!("../assets/fonts/Outfit.ttf").to_vec());
    fonts.set_sans_serif_family("Outfit");
    fonts.set_serif_family("Outfit");
    let options = usvg::Options {
        fontdb: Arc::new(fonts),
        font_family: "Outfit".into(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &options).unwrap();
    let mut pixels =
        tiny_skia::Pixmap::new(tree.size().width() as u32, tree.size().height() as u32).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
    );
    pixels.take()
}

fn assert_appearance(source: &str, exported: &str) {
    let reference = render(source);
    let actual = render(exported);
    assert_eq!(reference.len(), actual.len());
    let large = reference
        .iter()
        .zip(&actual)
        .filter(|(a, b)| a.abs_diff(**b) > 12)
        .count();
    let total: u64 = reference
        .iter()
        .zip(&actual)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    assert!(
        large * 1000 <= reference.len() * 3,
        "more than 0.3% of channels differ by >12: {large}"
    );
    assert!(
        total as f64 / reference.len() as f64 <= 0.35,
        "mean channel difference: {}",
        total as f64 / reference.len() as f64
    );
}

#[test]
fn shaped_svg_text_becomes_named_editable_outlines_with_visible_conversion_report() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="140">
      <g id="campaign" transform="translate(8 4) rotate(-3 150 70)">
        <text id="headline" x="150" y="42" font-family="Outfit" font-size="28" text-anchor="middle" fill="#23485e">Omuse café</text>
        <text id="caption" x="14" y="86" font-family="Outfit" font-size="19" xml:space="preserve">Create <tspan fill="#be633b" font-weight="600">beautiful</tspan><tspan x="14" dy="26" letter-spacing="1.5">work together.</tspan></text>
      </g>
    </svg>"##;
    let imported = decode_scene_with_report(svg.as_bytes()).unwrap();
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("not retained as live text"))
    );
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("resolved to Outfit"))
    );
    assert!(imported.scene.objects.iter().all(|o| o.text_path.is_none()));
    assert!(
        imported
            .scene
            .objects
            .iter()
            .any(|o| o.name == "headline · outlines")
    );
    assert!(
        imported
            .scene
            .objects
            .iter()
            .all(|o| !o.path.subpaths.is_empty())
    );
    let exported = encode_scene(&imported.scene).unwrap();
    assert!(!exported.contains("<text"));
    assert_appearance(svg, &exported);
    let reopened = decode_scene(exported.as_bytes()).unwrap();
    assert_appearance(svg, &encode_scene(&reopened).unwrap());
    let pdf = omuse::vector_pdf::encode_scene(&imported.scene, 72.).unwrap();
    assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
}

#[test]
fn missing_font_is_reported_and_portable_outline_does_not_depend_on_original_font() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="60"><text x="8" y="36" font-family="OmuseDefinitelyMissingFont" font-size="24">Hello Omuse</text></svg>"##;
    let imported = decode_scene_with_report(svg).unwrap();
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("OmuseDefinitelyMissingFont") && w.contains("resolved to Outfit"))
    );
    let exported = encode_scene(&imported.scene).unwrap();
    assert!(!exported.contains("font-family"));
    assert!(decode_scene(exported.as_bytes()).is_ok());
}

#[test]
fn transformed_gradient_and_dashed_strokes_expand_before_affine_transform() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="150">
      <defs><linearGradient id="ink" x1="0" y1="0" x2="100" y2="0" gradientUnits="userSpaceOnUse"><stop stop-color="#ed7d45"/><stop offset="1" stop-color="#347591"/></linearGradient></defs>
      <g transform="translate(12 9) matrix(1.8 .15 .32 .8 0 0)">
        <path d="M 4 22 C 35 -2 66 47 100 18" fill="none" stroke="url(#ink)" stroke-width="7" stroke-linecap="round" stroke-dasharray="12 5"/>
        <path d="M 10 68 L 95 68 L 74 116 Z" fill="#eed7af" stroke="#243e50" stroke-width="8" stroke-linejoin="round"/>
      </g>
    </svg>"##;
    let imported = decode_scene_with_report(svg.as_bytes()).unwrap();
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("stroke width and dash settings are no longer live"))
    );
    assert_eq!(imported.scene.objects.len(), 3);
    assert!(imported.scene.objects.iter().all(|o| o.stroke.is_none()));
    assert!(imported.scene.objects[0].fill_gradient.is_some());
    let exported = encode_scene(&imported.scene).unwrap();
    assert_appearance(svg, &exported);
    assert_appearance(
        svg,
        &encode_scene(&decode_scene(exported.as_bytes()).unwrap()).unwrap(),
    );
}

#[test]
fn a_single_painted_object_can_retain_nested_group_opacity() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><g id="outer" opacity=".5"><g id="inner" opacity=".4"><rect x="10" y="12" width="65" height="42" fill="#b95132" stroke="#284e59" stroke-width="8" opacity=".8"/></g></g></svg>"##;
    let scene = decode_scene(svg.as_bytes()).unwrap();
    assert_eq!(scene.objects.len(), 1);
    assert!((scene.objects[0].opacity - 0.16).abs() < 0.00001);
    assert_eq!(scene.objects[0].groups.len(), 2);
    assert_appearance(svg, &encode_scene(&scene).unwrap());
}

#[test]
fn unsupported_compositing_text_and_stroke_work_fail_without_partial_artwork() {
    for body in [
        r#"<g opacity=".5"><rect width="40" height="40"/><rect x="10" width="40" height="40"/></g>"#,
        r#"<text x="4" y="30" font-family="Outfit"><tspan opacity=".5">partial</tspan></text>"#,
        r#"<text x="4" y="30" font-family="Outfit" style="font-variation-settings:'wght' 800">ignored setting</text>"#,
        r#"<text><textPath href="https://example.invalid/path">remote</textPath></text>"#,
        r#"<g clip-path="url(#crop)"><rect width="30" height="40"/></g>"#,
        r#"<path d="M0 1 L10000 1" fill="none" stroke="red" stroke-dasharray=".1 .1" transform="scale(2 1)"/>"#,
        r#"<path d="M10 10 L40 10 L40 40 Z" fill="red" stroke="blue" opacity=".5" transform="scale(2 1)"/>"#,
    ] {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="5" height="5"/>{body}</svg>"#
        );
        assert!(
            decode_scene_with_report(svg.as_bytes()).is_err(),
            "unexpectedly imported: {body}"
        );
    }
    let too_long = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><text>{}</text></svg>"#,
        "a".repeat(4097)
    );
    assert!(
        decode_scene(too_long.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("4096")
    );
}
