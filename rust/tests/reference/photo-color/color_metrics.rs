//! Test-only colour math, independent of Omuse production conversion/filter code.
//! sRGB primaries/transfer: W3C CSS Color 4, section 19; Lab uses native D65 here
//! (not CSS's D50-adapted lab()). CIEDE2000: Sharma/Wu/Dalal (2005), equations 2–22.

fn linear(byte: u8) -> f64 {
    let v = f64::from(byte) / 255.;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_rgb_to_lab(rgb: [f64; 3]) -> [f64; 3] {
    let matrix = [
        [506752. / 1228815., 87881. / 245763., 12673. / 70218.],
        [87098. / 409605., 175762. / 245763., 12673. / 175545.],
        [7918. / 409605., 87881. / 737289., 1001167. / 1053270.],
    ];
    let white = [0.3127 / 0.3290, 1., (1. - 0.3127 - 0.3290) / 0.3290];
    let mut f = [0.; 3];
    for axis in 0..3 {
        let ratio = matrix[axis]
            .iter()
            .zip(rgb)
            .map(|(m, c)| m * c)
            .sum::<f64>()
            / white[axis];
        f[axis] = if ratio > 216. / 24389. {
            ratio.cbrt()
        } else {
            ((24389. / 27.) * ratio + 16.) / 116.
        };
    }
    [
        116. * f[1] - 16.,
        500. * (f[0] - f[1]),
        200. * (f[1] - f[2]),
    ]
}

pub fn srgb_to_lab(rgb: [u8; 3]) -> [f64; 3] {
    linear_rgb_to_lab(rgb.map(linear))
}

/// Diagnostic appearance over a neutral matte, composited in linear-light sRGB.
/// This is a comparison surface, not a claim about the display/ICC renderer.
pub fn composite_lab(rgba: [u8; 4], matte: u8) -> [f64; 3] {
    let a = f64::from(rgba[3]) / 255.;
    let ground = linear(matte);
    linear_rgb_to_lab([0, 1, 2].map(|c| linear(rgba[c]) * a + ground * (1. - a)))
}

pub fn delta_e_2000(a: [f64; 3], b: [f64; 3]) -> f64 {
    let mean_c = (a[1].hypot(a[2]) + b[1].hypot(b[2])) * 0.5;
    let c7 = mean_c.powi(7);
    let g = 0.5 * (1. - (c7 / (c7 + 25f64.powi(7))).sqrt());
    let aa = a[1] * (1. + g);
    let ab = b[1] * (1. + g);
    let c1 = aa.hypot(a[2]);
    let c2 = ab.hypot(b[2]);
    let hue = |y: f64, x: f64| {
        if x == 0. && y == 0. {
            0.
        } else {
            y.atan2(x).to_degrees().rem_euclid(360.)
        }
    };
    let h1 = hue(a[2], aa);
    let h2 = hue(b[2], ab);
    let dl = b[0] - a[0];
    let dc = c2 - c1;
    let mut dh = h2 - h1;
    let achromatic = c1 * c2 == 0.;
    // Preserve the published exact-180-degree branch despite atan2 roundoff;
    // the supplementary near-boundary pairs are far outside this tolerance.
    if achromatic {
        dh = 0.;
    } else if dh > 180. + 1e-12 {
        dh -= 360.;
    } else if dh < -180. - 1e-12 {
        dh += 360.;
    }
    let delta_h = 2. * (c1 * c2).sqrt() * (dh * 0.5).to_radians().sin();
    let l = (a[0] + b[0]) * 0.5;
    let c = (c1 + c2) * 0.5;
    let h = if achromatic {
        h1 + h2
    } else if (h1 - h2).abs() <= 180. + 1e-12 {
        (h1 + h2) * 0.5
    } else if h1 + h2 < 360. {
        (h1 + h2 + 360.) * 0.5
    } else {
        (h1 + h2 - 360.) * 0.5
    };
    let cos = |angle: f64| angle.to_radians().cos();
    let t = 1. - 0.17 * cos(h - 30.) + 0.24 * cos(2. * h) + 0.32 * cos(3. * h + 6.)
        - 0.20 * cos(4. * h - 63.);
    let l50 = (l - 50.).powi(2);
    let sl = 1. + 0.015 * l50 / (20. + l50).sqrt();
    let sc = 1. + 0.045 * c;
    let sh = 1. + 0.015 * c * t;
    let theta = 30. * (-((h - 275.) / 25.).powi(2)).exp();
    let rc = 2. * (c.powi(7) / (c.powi(7) + 25f64.powi(7))).sqrt();
    let rt = -(2. * theta).to_radians().sin() * rc;
    let (dl, dc, dh) = (dl / sl, dc / sc, delta_h / sh);
    (dl * dl + dc * dc + dh * dh + rt * dc * dh).max(0.).sqrt()
}
