use super::*;
use std::sync::atomic::Ordering;

fn fixture() -> (RgbaImage, RgbaImage, ContentAwareReplace) {
    let expected = RgbaImage::from_fn(15, 15, |x, y| {
        let v = if (x + y) % 2 == 0 { 30 } else { 220 };
        Rgba([v, v, v, 255])
    });
    let mut source = expected.clone();
    let mut target = vec![0; 225];
    let mut allowed = vec![255; 225];
    for y in 5..10 {
        for x in 5..10 {
            source.put_pixel(x, y, Rgba([250, 0, 150, 255]));
            target[(y * 15 + x) as usize] = 255;
            allowed[(y * 15 + x) as usize] = 0;
        }
    }
    (
        source,
        expected,
        ContentAwareReplace {
            algorithm: ContentAwareAlgorithm::ContextualV1,
            target_mask: SoftMask::new(15, 15, target).unwrap(),
            allowed_source_mask: SoftMask::new(15, 15, allowed).unwrap(),
            search_radius: 12,
            patch_radius: 1,
            feather: 0.,
        },
    )
}

fn entry(settings: ContentAwareReplace) -> FilterNode {
    FilterNode {
        id: "content-regression".into(),
        name: "Removal".into(),
        enabled: true,
        opacity: 1.,
        operation: AdvancedOperation::ContentAwareReplace(settings),
        soft_mask: None,
    }
}

#[test]
fn missing_algorithm_keeps_legacy_result_and_unknown_algorithm_is_rejected() {
    let (source, expected, mut settings) = fixture();
    settings.algorithm = ContentAwareAlgorithm::Legacy;
    let legacy = evaluate(&source, &[entry(settings.clone())]).unwrap();
    assert_ne!(
        legacy, expected,
        "The fixture must exercise the legacy zero-context defect"
    );
    let mut old_recipe = serde_json::to_value(&settings).unwrap();
    old_recipe.as_object_mut().unwrap().remove("algorithm");
    let reopened: ContentAwareReplace = serde_json::from_value(old_recipe.clone()).unwrap();
    assert_eq!(reopened.algorithm, ContentAwareAlgorithm::Legacy);
    assert_eq!(serde_json::to_value(&reopened).unwrap(), old_recipe);
    assert_eq!(
        evaluate(&source, &[entry(reopened.clone())]).unwrap(),
        legacy
    );
    let roundtrip: ContentAwareReplace =
        serde_json::from_slice(&serde_json::to_vec(&reopened).unwrap()).unwrap();
    assert_eq!(evaluate(&source, &[entry(roundtrip)]).unwrap(), legacy);
    old_recipe["algorithm"] = serde_json::json!("futureUnsupportedAlgorithm");
    assert!(serde_json::from_value::<ContentAwareReplace>(old_recipe).is_err());
}

#[test]
fn contextual_interior_continues_known_texture_instead_of_first_scan_candidate() {
    let (source, expected, settings) = fixture();
    let output = evaluate(&source, &[entry(settings.clone())]).unwrap();
    assert_eq!(output, expected);
    assert_eq!(
        evaluate(&source, &[entry(settings)]).unwrap(),
        output,
        "Removal must be deterministic"
    );
}

#[test]
fn contextual_donors_and_every_comparison_sample_stay_inside_allowed_region() {
    let (source, _, settings) = fixture();
    let mut visits = 0;
    visit_content_samples(
        &source,
        &settings,
        &AtomicBool::new(false),
        |x, y, sx, sy, amount| {
            assert!(settings.target_mask.value(x, y) > 0.);
            assert!(settings.allowed_source_mask.value(sx, sy) > 0.);
            assert_eq!(settings.target_mask.value(sx, sy), 0.);
            assert_eq!(amount, 1.);
            visits += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(visits, 25);
    // A centre-only allowed donor cannot provide any radius-one patch context.
    // Neighbouring pixels outside this allowed source must not be consulted.
    let mut no_context = settings;
    no_context.allowed_source_mask.data.fill(0);
    no_context.allowed_source_mask.data[2 * 15 + 2] = 255;
    let error = evaluate(&source, &[entry(no_context)]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("source patch has enough context")
    );
}

#[test]
fn contextual_missing_donors_and_all_selected_images_refuse_without_changes() {
    let (source, _, mut settings) = fixture();
    settings.allowed_source_mask.data.fill(0);
    let error = evaluate(&source, &[entry(settings.clone())]).unwrap_err();
    assert!(error.to_string().contains("No allowed source"));
    settings.target_mask.data.fill(255);
    let error = evaluate(&source, &[entry(settings)]).unwrap_err();
    assert!(error.to_string().contains("unselected surrounding context"));
}

#[test]
fn contextual_cancellation_after_first_pixel_does_not_commit_editor_history() {
    let (source, _, settings) = fixture();
    let mut editor = crate::editor::Editor::new(crate::model::Document::new(15, 15));
    editor.document.layers[0].image = Some(source.clone().into());
    let depth = editor.undo_depth();
    let cancel = AtomicBool::new(false);
    let mut visits = 0;
    let error = editor
        .apply_image_operation(|image| {
            let mut candidate = image.clone();
            visit_content_samples(image, &settings, &cancel, |x, y, sx, sy, _| {
                candidate.put_pixel(x, y, *image.get_pixel(sx, sy));
                visits += 1;
                cancel.store(true, Ordering::Relaxed);
                Ok(())
            })?;
            Ok(candidate)
        })
        .unwrap_err();
    assert_eq!(visits, 1);
    assert!(error.to_string().contains("cancelled"));
    assert_eq!(editor.undo_depth(), depth);
    assert_eq!(
        editor.document.layers[0].image.as_ref().unwrap().as_raw(),
        source.as_raw()
    );
}

#[test]
fn contextual_partial_coverage_blends_once_and_protects_other_pixels() {
    let (_, expected, mut settings) = fixture();
    settings.target_mask.data.fill(0);
    settings.allowed_source_mask.data.fill(255);
    let (x, y) = (7, 7);
    settings.target_mask.data[(y * 15 + x) as usize] = 128;
    settings.allowed_source_mask.data[(y * 15 + x) as usize] = 0;
    // Only one source pixel is damaged, so every context sample is legitimate.
    let mut source = expected;
    source.put_pixel(x, y, Rgba([250, 0, 150, 255]));
    let output = evaluate(&source, &[entry(settings.clone())]).unwrap();
    let mut donor = None;
    visit_content_samples(
        &source,
        &settings,
        &AtomicBool::new(false),
        |_, _, sx, sy, amount| {
            assert_eq!(amount, 128. / 255.);
            donor = Some((sx, sy));
            Ok(())
        },
    )
    .unwrap();
    let (sx, sy) = donor.unwrap();
    let a = source.get_pixel(x, y).0;
    let b = source.get_pixel(sx, sy).0;
    assert_eq!(
        output.get_pixel(x, y).0,
        std::array::from_fn(|c| lerp_byte(a[c], b[c], 128. / 255.))
    );
    for (px, py, original) in source.enumerate_pixels() {
        if (px, py) != (x, y) {
            assert_eq!(output.get_pixel(px, py), original);
        }
    }
}

#[test]
fn contextual_work_budget_and_early_cancellation_remain_bounded() {
    let target = SoftMask::new(1_000, 1_000, vec![255; 1_000_000]).unwrap();
    let settings = ContentAwareReplace {
        algorithm: ContentAwareAlgorithm::ContextualV1,
        target_mask: target.clone(),
        allowed_source_mask: target,
        search_radius: 64,
        patch_radius: 4,
        feather: 0.,
    };
    assert!(content_candidate_budget(&settings).is_err());
    let (source, _, settings) = fixture();
    let mut visits = 0;
    let error = visit_content_samples(
        &source,
        &settings,
        &AtomicBool::new(true),
        |_, _, _, _, _| {
            visits += 1;
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(visits, 0);
    assert!(error.to_string().contains("cancelled"));
}

#[test]
fn contextual_fill_reaches_image_edges_and_separate_selected_islands() {
    let (_, expected, mut settings) = fixture();
    let mut source = expected.clone();
    settings.target_mask.data.fill(0);
    settings.allowed_source_mask.data.fill(255);
    for (left, top, width, height) in [(0, 0, 3, 3), (12, 12, 3, 3), (0, 6, 2, 3)] {
        for y in top..top + height {
            for x in left..left + width {
                let i = (y * 15 + x) as usize;
                source.put_pixel(x, y, Rgba([250, 0, 150, 255]));
                settings.target_mask.data[i] = 255;
                settings.allowed_source_mask.data[i] = 0;
            }
        }
    }
    assert_eq!(evaluate(&source, &[entry(settings)]).unwrap(), expected);
}

#[test]
fn contextual_exhausted_donors_after_partial_work_leave_editor_untouched() {
    let (source, _, mut settings) = fixture();
    settings.search_radius = 1;
    let mut editor = crate::editor::Editor::new(crate::model::Document::new(15, 15));
    editor.document.layers[0].image = Some(source.clone().into());
    let depth = editor.undo_depth();
    let mut visits = 0;
    let error = editor
        .apply_image_operation(|image| {
            let mut candidate = image.clone();
            visit_content_samples(
                image,
                &settings,
                &AtomicBool::new(false),
                |x, y, sx, sy, _| {
                    candidate.put_pixel(x, y, *image.get_pixel(sx, sy));
                    visits += 1;
                    Ok(())
                },
            )?;
            Ok(candidate)
        })
        .unwrap_err();
    assert!(
        visits > 0,
        "Failure must occur after some work to exercise atomicity"
    );
    assert!(error.to_string().contains("No allowed source"));
    assert_eq!(editor.undo_depth(), depth);
    assert_eq!(
        editor.document.layers[0].image.as_ref().unwrap().as_raw(),
        source.as_raw()
    );
}
