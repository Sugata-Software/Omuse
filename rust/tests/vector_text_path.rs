use omuse::vector_path::{Anchor, FillRule, Point, Subpath, VectorPath};
use omuse::vector_scene::{TextOnPath, TextPathAlignment, VectorObject, VectorScene, text};
use std::sync::atomic::AtomicBool;

fn recipe(text: &str) -> TextOnPath {
    TextOnPath {
        text: text.into(),
        font_family: "Outfit".into(),
        font_size: 28.,
        letter_spacing: 0.,
        start_offset: 0.5,
        alignment: TextPathAlignment::Center,
        guide: VectorPath {
            subpaths: vec![Subpath {
                closed: false,
                anchors: vec![
                    Anchor {
                        position: Point { x: 20., y: 140. },
                        incoming: None,
                        outgoing: Some(Point { x: 130., y: 30. }),
                    },
                    Anchor {
                        position: Point { x: 480., y: 140. },
                        incoming: Some(Point { x: 370., y: 30. }),
                        outgoing: None,
                    },
                ],
            }],
            fill_rule: FillRule::NonZero,
        },
        transform: [1., 0., 0., 1., 0., 0.],
        resolved_fonts: Vec::new(),
    }
}
fn object(recipe: &TextOnPath) -> VectorObject {
    text::update_object(
        &VectorObject::new(
            "Curved text",
            VectorPath::default(),
            Some([240, 140, 65, 255]),
            None,
        ),
        recipe,
        &AtomicBool::new(false),
    )
    .unwrap()
}

#[test]
fn text_retains_recipe_and_generates_portable_curved_outlines() {
    let recipe = recipe("Omuse curves");
    let object = object(&recipe);
    assert!(object.path.subpaths.len() > 8);
    assert_eq!(object.text_path.as_ref().unwrap().text, "Omuse curves");
    assert!(
        object
            .text_path
            .as_ref()
            .unwrap()
            .resolved_fonts
            .iter()
            .any(|font| font == "Outfit")
    );
    let scene = VectorScene {
        version: 4,
        width: 500,
        height: 180,
        objects: vec![object],
    };
    let pixels = scene.render(&AtomicBool::new(false)).unwrap();
    assert!(pixels.pixels().filter(|p| p[3] > 0).count() > 100);
    let json = serde_json::to_vec(&scene).unwrap();
    let restored: VectorScene = serde_json::from_slice(&json).unwrap();
    assert_eq!(restored, scene);
    assert_eq!(restored.render(&AtomicBool::new(false)).unwrap(), pixels);
    let svg = omuse::vector_svg_scene::encode_scene(&scene).unwrap();
    let outline = omuse::vector_svg_scene::decode_scene(svg.as_bytes()).unwrap();
    assert!(outline.objects[0].text_path.is_none());
    assert_eq!(outline.render(&AtomicBool::new(false)).unwrap(), pixels);
}

#[test]
fn text_limits_clipping_cancellation_and_old_readers_fail_closed() {
    let mut text = recipe("This sentence is far too long for a short curve");
    text.font_size = 180.;
    assert!(
        text::shape(&text, &AtomicBool::new(false))
            .unwrap_err()
            .to_string()
            .contains("does not fit")
    );
    assert!(text::shape(&recipe("cancel"), &AtomicBool::new(true)).is_err());
    assert!(recipe(&"x".repeat(513)).validate().is_err());
    let object = object(&recipe("Versioned"));
    for version in [1, 2, 3, 5] {
        assert!(
            VectorScene {
                version,
                width: 500,
                height: 180,
                objects: vec![object.clone()]
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        omuse::vector_boolean::combine(
            &[object],
            omuse::vector_boolean::BooleanOperation::Union,
            &AtomicBool::new(false)
        )
        .is_err()
    );
}

#[test]
fn missing_family_reports_bundled_fallback_and_xml_text_is_safe() {
    let mut recipe = recipe("<&\" café ﬁ");
    recipe.font_family = "Omuse missing font 12345".into();
    let (result, path) = text::shape(&recipe, &AtomicBool::new(false)).unwrap();
    assert!(result.resolved_fonts.iter().any(|font| font == "Outfit"));
    assert_eq!(result.font_family, "Omuse missing font 12345");
    assert!(!path.subpaths.is_empty());
}

#[test]
fn font_size_spacing_position_and_retained_transform_change_layout() {
    let original = recipe("Omuse");
    let (_, first) = text::shape(&original, &AtomicBool::new(false)).unwrap();
    let mut changed = original.clone();
    changed.font_size = 42.;
    changed.letter_spacing = 3.;
    let (_, large) = text::shape(&changed, &AtomicBool::new(false)).unwrap();
    let (a, b) = first.bounds().unwrap();
    let (c, d) = large.bounds().unwrap();
    assert!(d.x - c.x > b.x - a.x);
    changed = original.clone();
    changed.start_offset = 0.7;
    let (_, moved) = text::shape(&changed, &AtomicBool::new(false)).unwrap();
    assert!(moved.bounds().unwrap().0.x > a.x);
    changed = original;
    changed.bake_transform([1., 0., 0., 1., 12., 8.]);
    let (_, shifted) = text::shape(&changed, &AtomicBool::new(false)).unwrap();
    for (a, b) in first
        .subpaths
        .iter()
        .flat_map(|s| &s.anchors)
        .zip(shifted.subpaths.iter().flat_map(|s| &s.anchors))
    {
        assert!((a.position.x + 12. - b.position.x).abs() < 0.001);
        assert!((a.position.y + 8. - b.position.y).abs() < 0.001);
    }
}

fn transformed(path: &VectorPath, matrix: [f32; 6]) -> VectorPath {
    let mut result = path.clone();
    for anchor in result.subpaths.iter_mut().flat_map(|s| &mut s.anchors) {
        for point in [
            Some(&mut anchor.position),
            anchor.incoming.as_mut(),
            anchor.outgoing.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            let (x, y) = (point.x, point.y);
            point.x = matrix[0] * x + matrix[2] * y + matrix[4];
            point.y = matrix[1] * x + matrix[3] * y + matrix[5];
        }
    }
    result
}

fn assert_paths_near(actual: &VectorPath, expected: &VectorPath) {
    assert_eq!(actual.subpaths.len(), expected.subpaths.len());
    for (a, b) in actual.subpaths.iter().zip(&expected.subpaths) {
        assert_eq!((a.closed, a.anchors.len()), (b.closed, b.anchors.len()));
        for (a, b) in a.anchors.iter().zip(&b.anchors) {
            for (a, b) in [
                (Some(a.position), Some(b.position)),
                (a.incoming, b.incoming),
                (a.outgoing, b.outgoing),
            ] {
                match (a, b) {
                    (Some(a), Some(b)) => {
                        assert!((a.x - b.x).abs() < 0.002, "x: {} vs {}", a.x, b.x);
                        assert!((a.y - b.y).abs() < 0.002, "y: {} vs {}", a.y, b.y);
                    }
                    (None, None) => {}
                    _ => panic!("Curve handles changed"),
                }
            }
        }
    }
}

#[test]
fn editing_transformed_text_keeps_rotation_scale_and_position() {
    let cancel = AtomicBool::new(false);
    let source = object(&recipe("Before"));
    let scene = VectorScene {
        version: 4,
        width: 800,
        height: 600,
        objects: vec![source],
    };
    let scene =
        omuse::vector_scene_ops::rotate(&scene, &[0], 28., Point { x: 250., y: 90. }).unwrap();
    let scene =
        omuse::vector_scene_ops::scale(&scene, &[0], 1.4, 1.4, Point { x: 80., y: 20. }).unwrap();
    let scene = omuse::vector_scene_ops::translate(&scene, &[0], 30., 60.).unwrap();
    let source = &scene.objects[0];
    let mut updated_recipe = source.text_path.clone().unwrap();
    updated_recipe.text = "After café ﬁ".into();
    updated_recipe.font_size = 30.;
    updated_recipe.letter_spacing = 1.;
    let unbaked = text::update_object(source, &updated_recipe, &cancel).unwrap();
    assert_eq!(unbaked.transform, source.transform);

    // The UI bakes object transforms before editing. Reflowing this retained
    // recipe must have the same outlines as reflowing, then transforming it.
    let mut baked = source.clone();
    baked.path = transformed(&baked.path, baked.transform);
    updated_recipe.bake_transform(baked.transform);
    baked.transform = [1., 0., 0., 1., 0., 0.];
    baked.text_path = Some(updated_recipe.clone());
    let edited = text::update_object(&baked, &updated_recipe, &cancel).unwrap();
    assert_paths_near(&edited.path, &transformed(&unbaked.path, unbaked.transform));
    let round_trip: VectorObject =
        serde_json::from_slice(&serde_json::to_vec(&edited).unwrap()).unwrap();
    assert_eq!(round_trip, edited);
}

#[test]
fn replacing_a_world_guide_preserves_layout_and_rejects_invalid_changes_atomically() {
    let cancel = AtomicBool::new(false);
    let mut recipe = recipe("Along the new curve");
    recipe.bake_transform([1.2, 0.5, -0.5, 1.2, 43., 28.]);
    let mut new_local = recipe.guide.clone();
    new_local.subpaths[0].anchors[0]
        .outgoing
        .as_mut()
        .unwrap()
        .y += 30.;
    new_local.subpaths[0].anchors[1]
        .incoming
        .as_mut()
        .unwrap()
        .y += 30.;
    let new_world = transformed(&new_local, recipe.transform);
    recipe.set_world_guide(&new_world).unwrap();
    assert_paths_near(&recipe.guide, &new_local);
    let (_, actual) = text::shape(&recipe, &cancel).unwrap();
    let mut expected_recipe = recipe.clone();
    expected_recipe.guide = new_local;
    expected_recipe.transform = [1., 0., 0., 1., 0., 0.];
    let (_, expected) = text::shape(&expected_recipe, &cancel).unwrap();
    assert_paths_near(&actual, &transformed(&expected, recipe.transform));

    let before = recipe.clone();
    let mut invalid = new_world;
    invalid.subpaths.push(invalid.subpaths[0].clone());
    assert!(recipe.set_world_guide(&invalid).is_err());
    assert_eq!(recipe, before, "A rejected guide must not change live text");
    assert!(recipe.set_world_guide(&VectorPath::default()).is_err());
    assert_eq!(recipe, before);
}

#[test]
fn unicode_shaping_retains_combining_marks_and_closes_single_line_limits() {
    let cancel = AtomicBool::new(false);
    let original = recipe("café ﬁ e\u{301}");
    let (resolved, first) = text::shape(&original, &cancel).unwrap();
    let (_, again) = text::shape(&resolved, &cancel).unwrap();
    assert_eq!(first, again);
    assert_eq!(resolved.text, original.text);
    assert_eq!(resolved.resolved_fonts, ["Outfit"]);
    for content in [
        "two\nlines",
        "two\rlines",
        "two\u{2028}lines",
        "two\u{2029}lines",
    ] {
        assert!(recipe(content).validate().is_err(), "Accepted {content:?}");
    }
    // The fallback chain must report a missing glyph, not silently drop it.
    assert!(text::shape(&recipe("Missing \u{10FFFF}"), &cancel).is_err());
}
