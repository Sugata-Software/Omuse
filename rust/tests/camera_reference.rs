//! Independently generated preserved-C fixtures. Regenerate with scripts/generate-rust-kernel-fixtures.py.
use omuse::camera_raw::{self, Settings};

fn assert_fixture(fixture: &serde_json::Value, include: impl Fn(&str) -> bool) {
    let width = fixture["width"].as_u64().unwrap() as u32;
    let height = fixture["height"].as_u64().unwrap() as u32;
    let data: Vec<u8> = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    let source = image::RgbaImage::from_raw(width, height, data).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if !include(name) {
            continue;
        }
        let settings: Settings = serde_json::from_value(case["settings"].clone()).unwrap();
        let output = camera_raw::apply(&source, &settings).unwrap();
        let expected = case["expectedPremultiplied"].as_array().unwrap();
        let mut worst = 0i16;
        let mut worst_at = (0usize, 0usize, 0i16, 0i16);
        for (index, pixel) in output.pixels().enumerate() {
            assert_eq!(
                pixel[3],
                expected[index * 4 + 3].as_u64().unwrap() as u8,
                "{} alpha differs at pixel {index}",
                case["name"]
            );
            for channel in 0..3 {
                let premultiplied = (u16::from(pixel[channel]) * u16::from(pixel[3]) + 127) / 255;
                let expected_level = expected[index * 4 + channel].as_i64().unwrap() as i16;
                let difference = (premultiplied as i16 - expected_level).abs();
                if difference > worst {
                    worst = difference;
                    worst_at = (index, channel, premultiplied as i16, expected_level);
                }
            }
        }
        assert!(
            worst <= 2,
            "{} differs by {worst} premultiplied levels at {:?}",
            case["name"],
            worst_at
        );
    }
}

#[test]
fn camera_basic_matches_preserved_c_across_colors_and_alpha() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-basic-reference.json")).unwrap();
    assert_fixture(&fixture, |_| true);
}

#[test]
fn camera_detail_optics_and_effects_match_preserved_c() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-advanced-reference.json")).unwrap();
    assert_eq!(
        fixture["stages"],
        serde_json::json!(["effects", "optics", "detail"])
    );
    assert_fixture(&fixture, |_| true);
}
