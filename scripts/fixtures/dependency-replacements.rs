#![cfg(test)]

use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};
use tendril::{StrTendril, TendrilSink};

fn text_content(handle: &Handle) -> String {
    let mut output = String::new();
    if let NodeData::Text { contents } = &handle.data {
        output.push_str(&contents.borrow());
    }
    for child in handle.children.borrow().iter() {
        output.push_str(&text_content(child));
    }
    output
}

#[test]
fn html_tree_building_recovers_nesting_and_preserves_entities_and_unicode() {
    let dom = html5ever::parse_document(RcDom::default(), Default::default())
        .one("<!doctype html><p>Omuse &amp; café<b> bold<i> nested</b> after</i><p>second");
    let mut bytes = Vec::new();
    html5ever::serialize(
        &mut bytes,
        &SerializableHandle::from(dom.document.clone()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap(),
        "<!DOCTYPE html><html><head></head><body><p>Omuse &amp; café<b> bold<i> nested</i></b><i> after</i></p><p>second</p></body></html>");
    assert_eq!(
        text_content(&dom.document),
        "Omuse & café bold nested aftersecond"
    );
}

#[test]
fn html_error_modes_preserve_dom_and_detailed_diagnostics() {
    let input = "<!doctype html><p>x&#x110000;z</p>";
    let plain = html5ever::parse_document(RcDom::default(), Default::default()).one(input);
    let mut detailed_opts = html5ever::ParseOpts::default();
    detailed_opts.tokenizer.exact_errors = true;
    detailed_opts.tree_builder.exact_errors = true;
    let detailed = html5ever::parse_document(RcDom::default(), detailed_opts).one(input);
    assert_eq!(text_content(&plain.document), "x\u{fffd}z");
    assert_eq!(
        text_content(&plain.document),
        text_content(&detailed.document)
    );
    assert!(!plain.errors.is_empty());
    assert_eq!(plain.errors.len(), detailed.errors.len());
    assert_ne!(plain.errors, detailed.errors);
}

#[test]
fn xml_namespaces_entities_and_diagnostic_paths_remain_usable() {
    let dom = xml5ever::driver::parse_document(RcDom::default(), Default::default()).one(
        "<?xml version=\"1.0\"?><root xmlns=\"urn:omuse\"><item>A &amp; Ω &#x1F3A8;</item></root>",
    );
    assert!(dom.errors.is_empty(), "{:?}", dom.errors);
    assert_eq!(text_content(&dom.document), "A & Ω 🎨");
    for exact in [false, true] {
        let mut opts = xml5ever::driver::XmlParseOpts::default();
        opts.tokenizer.exact_errors = exact;
        let invalid =
            xml5ever::driver::parse_document(RcDom::default(), opts).one("<root>&#x110000;</root>");
        assert!(!invalid.errors.is_empty());
    }
}

#[test]
fn html_streaming_handles_utf8_and_token_boundaries() {
    let input = "<!doctype html><p>café &amp; 🎨</p>";
    let mut parser = html5ever::parse_document(RcDom::default(), Default::default()).from_utf8();
    for chunk in input.as_bytes().chunks(1) {
        parser.process(tendril::ByteTendril::from_slice(chunk));
    }
    let dom = parser.finish();
    assert_eq!(text_content(&dom.document), "café & 🎨");
    assert!(dom.errors.is_empty());
}

#[test]
fn utf8_classification_rejects_invalid_sequences_and_preserves_character_runs() {
    assert!(futf::classify(&[], 0).is_none());
    assert!(futf::classify(&[0xff], 0).is_none());
    assert!(futf::classify(&[0xc0, 0x80], 0).is_none());
    assert!(matches!(
        futf::classify("🎨".as_bytes(), 2).unwrap().meaning,
        futf::Meaning::Whole('🎨')
    ));
    assert!(StrTendril::try_from_byte_slice(&[0xff]).is_err());
    let mut text = StrTendril::from_slice("abc123é");
    let (letters, classification) = text.pop_front_char_run(char::is_alphabetic).unwrap();
    assert_eq!(&*letters, "abc");
    assert!(classification);
    assert_eq!(&*text, "123é");
    assert!(StrTendril::new()
        .pop_front_char_run(char::is_alphabetic)
        .is_none());
}

#[test]
fn hexadecimal_float_extremes_and_rejection_contract_are_preserved() {
    assert_eq!(
        hexf_parse::parse_hexf32("-0x0p0", false).unwrap().to_bits(),
        (-0.0_f32).to_bits()
    );
    assert_eq!(
        hexf_parse::parse_hexf64("-0x0p0", false).unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        hexf_parse::parse_hexf32("0x1p-149", false)
            .unwrap()
            .to_bits(),
        1
    );
    assert_eq!(
        hexf_parse::parse_hexf64("0x1p-1074", false)
            .unwrap()
            .to_bits(),
        1
    );
    assert_eq!(
        hexf_parse::parse_hexf32("0x1p-126", false).unwrap(),
        f32::MIN_POSITIVE
    );
    assert_eq!(
        hexf_parse::parse_hexf64("0x1p-1022", false).unwrap(),
        f64::MIN_POSITIVE
    );
    assert_eq!(
        hexf_parse::parse_hexf32("0x1.fffffep127", false).unwrap(),
        f32::MAX
    );
    assert_eq!(
        hexf_parse::parse_hexf64("0x1.fffffffffffffp1023", false).unwrap(),
        f64::MAX
    );
    for value in [
        "0x1p128",
        "0x1p-150",
        "0x1.000001p0",
        "not a number",
        "0x1_0p0",
    ] {
        assert!(hexf_parse::parse_hexf32(value, false).is_err(), "{value}");
    }
    for value in ["0x1p1024", "0x1p-1075", "0x1.00000000000001p0"] {
        assert!(hexf_parse::parse_hexf64(value, false).is_err(), "{value}");
    }
    assert_eq!(hexf_parse::parse_hexf32("0x1_0p0", true).unwrap(), 16.0);
}

#[test]
fn hexadecimal_float_samples_round_trip_bit_exactly() {
    let mut state = 0x91e1_0da5_c79e_7b1d_u64;
    for _ in 0..20_000 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let bits32 = state as u32;
        let bits64 = state.rotate_left(29);
        let exponent32 = (bits32 >> 23) & 0xff;
        if exponent32 != 0xff {
            let mantissa = (bits32 & 0x7f_ffff) | if exponent32 == 0 { 0 } else { 1 << 23 };
            let power = if exponent32 == 0 {
                -149
            } else {
                exponent32 as i32 - 150
            };
            let sign = if bits32 >> 31 == 0 { "" } else { "-" };
            let literal = format!("{sign}0x{mantissa:x}p{power}");
            assert_eq!(
                hexf_parse::parse_hexf32(&literal, false).unwrap().to_bits(),
                bits32,
                "{literal}"
            );
        }
        let exponent64 = (bits64 >> 52) & 0x7ff;
        if exponent64 != 0x7ff {
            let mantissa =
                (bits64 & 0x000f_ffff_ffff_ffff) | if exponent64 == 0 { 0 } else { 1 << 52 };
            let power = if exponent64 == 0 {
                -1074
            } else {
                exponent64 as i32 - 1075
            };
            let sign = if bits64 >> 63 == 0 { "" } else { "-" };
            let literal = format!("{sign}0x{mantissa:x}p{power}");
            assert_eq!(
                hexf_parse::parse_hexf64(&literal, false).unwrap().to_bits(),
                bits64,
                "{literal}"
            );
        }
    }
}
