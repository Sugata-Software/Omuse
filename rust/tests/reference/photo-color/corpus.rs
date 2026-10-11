//! Original synthetic input, shared by the read-only regression test and the
//! explicit review-directory generator. No measured chart or Adobe data.
use image::{Rgba, RgbaImage};
use omuse::camera_raw::Settings;
use serde::Deserialize;

pub const WIDTH: u32 = 256;
pub const HEIGHT: u32 = 16;
pub const ROWS: [&str; 16] = [
    "neutral ramp",
    "near-black ramp",
    "near-white ramp",
    "red ramp",
    "green ramp",
    "blue ramp",
    "hue wheel",
    "pastel hue wheel",
    "warm skin-tone proxies",
    "neutral patches",
    "synthetic colour chart",
    "warm/cool neutral ramp",
    "skin proxy alpha ramp",
    "blue alpha ramp",
    "magenta alpha boundaries",
    "mixed colour alpha boundaries",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    pub purpose: String,
    pub settings: Settings,
}

pub fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("cases.json")).expect("valid committed case definitions")
}

pub fn fixture() -> RgbaImage {
    const SKIN: [[u8; 3]; 8] = [
        [67, 39, 29],
        [96, 56, 42],
        [125, 75, 54],
        [159, 99, 70],
        [189, 133, 103],
        [214, 164, 129],
        [235, 194, 160],
        [247, 221, 193],
    ];
    const CHART: [[u8; 3]; 16] = [
        [200, 38, 35],
        [225, 102, 25],
        [226, 200, 37],
        [70, 145, 46],
        [28, 151, 125],
        [26, 151, 196],
        [31, 83, 186],
        [76, 52, 151],
        [142, 53, 157],
        [199, 53, 115],
        [237, 153, 165],
        [109, 71, 45],
        [91, 107, 56],
        [77, 100, 118],
        [193, 180, 153],
        [215, 224, 231],
    ];
    const ALPHAS: [u8; 16] = [
        0, 1, 2, 3, 7, 16, 31, 32, 63, 64, 127, 128, 191, 192, 254, 255,
    ];
    RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let byte = x as u8;
        let rgb = match y {
            0 => [byte; 3],
            1 => [(x / 8) as u8; 3],
            2 => [224 + (x / 8) as u8; 3],
            3 => [byte, 0, 0],
            4 => [0, byte, 0],
            5 => [0, 0, byte],
            6 | 7 => {
                let sector = x * 6 / 256;
                let t = ((x * 6 % 256) * 255 / 256) as u8;
                let rgb = match sector {
                    0 => [255, t, 0],
                    1 => [255 - t, 255, 0],
                    2 => [0, 255, t],
                    3 => [0, 255 - t, 255],
                    4 => [t, 0, 255],
                    _ => [255, 0, 255 - t],
                };
                if y == 7 {
                    rgb.map(|v| ((u16::from(v) + 510) / 3) as u8)
                } else {
                    rgb
                }
            }
            8 => SKIN[(x / 32) as usize],
            9 => [
                [16; 3], [32; 3], [64; 3], [96; 3], [128; 3], [160; 3], [208; 3], [240; 3],
            ][(x / 32) as usize],
            10 => CHART[(x / 16) as usize],
            11 => [byte.saturating_add(7), byte, byte.saturating_sub(7)],
            12 => [196, 131, 87],
            13 => [23, 97, 211],
            14 => [255, 33, 177],
            _ => [byte, 255 - byte, 91],
        };
        let alpha = match y {
            12 | 13 => byte,
            14 | 15 => ALPHAS[(x / 16) as usize],
            _ => 255,
        };
        Rgba([rgb[0], rgb[1], rgb[2], alpha])
    })
}
