use omuse::objects::{self, LiveTextStyle, RichTextPatch};

fn rich_style() -> LiveTextStyle {
    let mut style = LiveTextStyle {
        content: "A😀éZ".into(),
        ..Default::default()
    };
    objects::apply_rich_text_patch(
        &mut style,
        1..7,
        RichTextPatch {
            color: Some([0.8, 0.2, 0.3, 1.]),
            weight: Some(700),
            ..Default::default()
        },
    )
    .unwrap();
    style
}

#[test]
fn typing_maps_unicode_ranges_and_inherits_inserted_character_style() {
    let mut style = rich_style();
    assert_eq!(
        objects::edit_text_content(&mut style, "A😀東京éZ".into()).unwrap(),
        5..11
    );
    assert_eq!(style.runs.len(), 1);
    assert_eq!((style.runs[0].start, style.runs[0].end), (1, 13));
    assert_eq!(style.runs[0].weight, Some(700));
    objects::edit_text_content(&mut style, "A🙂Z".into()).unwrap();
    assert_eq!((style.runs[0].start, style.runs[0].end), (1, 5));
    assert_eq!(style.runs[0].color, Some([0.8, 0.2, 0.3, 1.]));
    objects::validate_text_style(&style).unwrap();
}

#[test]
fn typing_preserves_unaffected_runs_and_deletion_cannot_split_a_character() {
    let mut style = rich_style();
    objects::apply_rich_text_patch(
        &mut style,
        7..8,
        RichTextPatch {
            italic: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    objects::edit_text_content(&mut style, "前A😀éZ".into()).unwrap();
    assert_eq!((style.runs[0].start, style.runs[0].end), (4, 10));
    assert_eq!((style.runs[1].start, style.runs[1].end), (10, 11));
    objects::edit_text_content(&mut style, "前A😀éZ後".into()).unwrap();
    assert_eq!((style.runs[1].start, style.runs[1].end), (10, 14));
    assert_eq!(style.runs[1].italic, Some(true));
    let mut style = rich_style();
    objects::edit_text_content(&mut style, "AéZ".into()).unwrap();
    assert_eq!((style.runs[0].start, style.runs[0].end), (1, 3));
    objects::edit_text_content(&mut style, "AZ".into()).unwrap();
    assert!(style.runs.is_empty());
    objects::edit_text_content(&mut style, String::new()).unwrap();
    assert!(style.runs.is_empty());
}

#[test]
fn invalid_typing_does_not_partially_mutate_style() {
    let mut style = rich_style();
    let before = style.clone();
    assert!(objects::edit_text_content(&mut style, "x".repeat(1_000_001)).is_err());
    assert_eq!(style, before);
}
