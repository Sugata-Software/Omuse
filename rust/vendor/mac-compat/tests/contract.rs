use std::{borrow::Cow, cell::Cell};

use mac::{_tt_as_expr_hack, format_if, test_eq, unwrap_or_return};

#[test]
fn optional_value_and_fallback_are_evaluated_once_in_the_callers_scope() {
    fn extract(
        value: Option<String>,
        input_calls: &Cell<usize>,
        fallback_calls: &Cell<usize>,
    ) -> String {
        let text = unwrap_or_return!(
            {
                input_calls.set(input_calls.get() + 1);
                value
            },
            {
                fallback_calls.set(fallback_calls.get() + 1);
                String::from("missing")
            },
        );
        format!("{text} present")
    }
    let inputs = Cell::new(0);
    let fallbacks = Cell::new(0);
    assert_eq!(
        extract(Some(String::from("owned")), &inputs, &fallbacks),
        "owned present"
    );
    assert_eq!((inputs.get(), fallbacks.get()), (1, 0));
    assert_eq!(extract(None, &inputs, &fallbacks), "missing");
    assert_eq!((inputs.get(), fallbacks.get()), (2, 1));
}

#[test]
fn optional_unit_tuple_and_borrowed_values_work_in_parser_call_forms() {
    fn returns_unit(source: Option<u8>, called: &Cell<bool>) {
        let _byte = unwrap_or_return!(source, ());
        called.set(true);
    }
    fn tuple(source: Option<(usize, &str)>) -> Option<String> {
        let (index, text) = unwrap_or_return!(source, None);
        Some(format!("{index}:{text}"))
    }
    fn borrow(source: Option<&String>) -> String {
        unwrap_or_return!(source, String::from("absent")).clone()
    }
    let called = Cell::new(false);
    returns_unit(None, &called);
    assert!(!called.get());
    returns_unit(Some(7), &called);
    assert!(called.get());
    assert_eq!(tuple(Some((3, "<&>"))), Some(String::from("3:<&>")));
    assert_eq!(tuple(None), None);
    assert_eq!(borrow(Some(&String::from("borrowed"))), "borrowed");
    assert_eq!(borrow(None), "absent");
}

#[test]
fn diagnostic_fallback_is_borrowed_and_skips_formatting_work() {
    let condition_calls = Cell::new(0);
    let format_calls = Cell::new(0);
    let fallback = String::from("Bad character");
    let message: Cow<'_, str> = format_if!(
        {
            condition_calls.set(condition_calls.get() + 1);
            false
        },
        fallback.as_str(),
        "Saw {}",
        {
            format_calls.set(format_calls.get() + 1);
            'é'
        },
    );
    assert!(matches!(message, Cow::Borrowed(_)));
    assert_eq!(message, "Bad character");
    assert_eq!((condition_calls.get(), format_calls.get()), (1, 0));
}

#[test]
fn detailed_diagnostic_owns_formatted_text_and_skips_the_fallback() {
    let fallback_calls = Cell::new(0);
    let message: Cow<'_, str> = format_if!(
        true,
        {
            fallback_calls.set(fallback_calls.get() + 1);
            "fallback"
        },
        "Saw {} in state {:?}",
        'é',
        Some(2)
    );
    assert!(matches!(message, Cow::Owned(_)));
    assert_eq!(message, "Saw é in state Some(2)");
    assert_eq!(fallback_calls.get(), 0);
    let captured = 42;
    let message: Cow<'_, str> = format_if!(true, "fallback", "{captured}");
    assert_eq!(message, "42");
}

#[test]
fn expression_forwarding_keeps_match_and_operator_precedence() {
    macro_rules! forward {
        ($($tokens:tt)*) => { _tt_as_expr_hack!($($tokens)*) };
    }
    assert_eq!(3 * forward!(1 + 2), 9);
    assert!(forward!(match Some(3) {
        Some(3) => true,
        _ => false,
    }));
}

test_eq!(generated_equality_test, String::from("text"), "text");
test_eq!(
    generated_equality_test_trailing_comma,
    Some('A'.to_ascii_lowercase()),
    Some('a'),
);
