//! View-only canvas navigation. Zoom never changes artwork or history.
pub const ZOOM_STOPS: &[f32] = &[
    0.02,
    0.03125,
    0.05,
    0.0625,
    0.083333336,
    0.125,
    0.16666667,
    0.25,
    0.33333334,
    0.5,
    0.6666667,
    1.,
    1.5,
    2.,
    3.,
    4.,
    6.,
    8.,
    12.,
    16.,
];

pub fn step_zoom(current: f32, inward: bool) -> f32 {
    if !current.is_finite() || current <= 0. {
        return 1.;
    }
    if inward {
        ZOOM_STOPS
            .iter()
            .copied()
            .find(|z| *z > current + 0.000001)
            .unwrap_or(16.)
    } else {
        ZOOM_STOPS
            .iter()
            .rev()
            .copied()
            .find(|z| *z < current - 0.000001)
            .unwrap_or(0.02)
    }
}

/// `anchor` is relative to the viewport centre, in screen points.
pub fn zoom_about(zoom: f32, pan: (f32, f32), next: f32, anchor: (f32, f32)) -> (f32, (f32, f32)) {
    if ![zoom, pan.0, pan.1, next, anchor.0, anchor.1]
        .iter()
        .all(|v| v.is_finite())
        || zoom <= 0.
    {
        return (zoom, pan);
    }
    let next = next.clamp(0.02, 16.);
    let ratio = next / zoom;
    (
        next,
        (
            anchor.0 - (anchor.0 - pan.0) * ratio,
            anchor.1 - (anchor.1 - pan.1) * ratio,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_stop_roundtrips_without_moving_the_inspected_point() {
        for &z in &ZOOM_STOPS[..ZOOM_STOPS.len() - 1] {
            for anchor in [(0., 0.), (231., -109.)] {
                let pan = (-63.5, 147.25);
                let next = step_zoom(z, true);
                let (new_z, new_pan) = zoom_about(z, pan, next, anchor);
                assert!(((anchor.0 - pan.0) / z - (anchor.0 - new_pan.0) / new_z).abs() < 0.002);
                assert!(((anchor.1 - pan.1) / z - (anchor.1 - new_pan.1) / new_z).abs() < 0.002);
                let (restored, p) = zoom_about(new_z, new_pan, step_zoom(next, false), anchor);
                assert_eq!(restored, z);
                assert!((p.0 - pan.0).abs() < 0.001 && (p.1 - pan.1).abs() < 0.001);
            }
        }
    }
    #[test]
    fn arbitrary_fit_levels_limits_and_invalid_events_are_bounded() {
        assert_eq!(step_zoom(0.65, true), 0.6666667);
        assert_eq!(step_zoom(0.65, false), 0.5);
        assert_eq!(step_zoom(16., true), 16.);
        assert_eq!(step_zoom(0.02, false), 0.02);
        assert_eq!(zoom_about(1., (2., 3.), f32::NAN, (0., 0.)), (1., (2., 3.)));
    }
}
