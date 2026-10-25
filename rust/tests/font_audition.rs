use omuse::objects::{self, LiveTextStyle, ObjectSize};

#[test]
fn font_usage_reports_shaped_fallback_instead_of_echoing_an_unavailable_request() {
    let style = LiveTextStyle {
        content: "Hello Ω é".into(),
        font_name: "Omuse definitely unavailable font 620194".into(),
        font_size: 24.,
        box_size: Some(ObjectSize {
            width: 320.,
            height: 84.,
        }),
        ..Default::default()
    };
    let actual = objects::text_font_usage(&style).unwrap();
    assert!(!actual.is_empty());
    assert!(actual.iter().all(|f| f != &style.font_name));
    assert!(
        objects::text_layout_report(&style)
            .unwrap()
            .missing_fonts
            .contains(&style.font_name)
    );
}

#[test]
fn font_usage_sample_limit_counts_characters_and_does_not_mutate_artwork() {
    let mut style = LiveTextStyle {
        content: "é".repeat(160),
        font_name: "sans-serif".into(),
        font_size: 24.,
        ..Default::default()
    };
    let original = style.clone();
    assert!(!objects::text_font_usage(&style).unwrap().is_empty());
    assert_eq!(style, original);
    style.content.push('é');
    assert!(objects::text_font_usage(&style).is_err());
}
