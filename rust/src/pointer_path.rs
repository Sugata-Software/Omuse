//! Bounded pointer samples shared by deferred raster and selection gestures.
//! Mouse-up commonly repeats the final motion: it must not consume a new slot.
//! Actual overflow refuses the gesture instead of silently dropping its tail.
pub const MAX_POINTS: usize = 100_000;

pub fn append(points: &mut Vec<(f32, f32)>, point: (f32, f32)) -> Result<(), &'static str> {
    if !point.0.is_finite() || !point.1.is_finite() {
        return Err("Stroke cancelled: invalid pointer position");
    }
    if points.last() == Some(&point) {
        return Ok(());
    }
    if points.len() >= MAX_POINTS {
        return Err("Stroke cancelled: too many points; use a shorter stroke");
    }
    points
        .try_reserve(1)
        .map_err(|_| "Stroke cancelled: not enough memory to keep its path")?;
    points.push(point);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_retains_the_endpoint_at_both_capture_boundaries() {
        for count in [MAX_POINTS - 1, MAX_POINTS] {
            let mut points: Vec<_> = (0..count).map(|i| (i as f32, 0.)).collect();
            let endpoint = ((MAX_POINTS - 1) as f32, 0.);
            append(&mut points, endpoint).unwrap();
            append(&mut points, endpoint).unwrap(); // Duplicate release is inert.
            assert_eq!(points.len(), MAX_POINTS);
            assert_eq!(points.first(), Some(&(0., 0.)));
            assert_eq!(points.last(), Some(&endpoint));
        }
    }

    #[test]
    fn genuine_overflow_never_silently_changes_the_captured_path() {
        let mut points: Vec<_> = (0..MAX_POINTS).map(|i| (i as f32, 0.)).collect();
        let original = points.clone();
        assert!(append(&mut points, (MAX_POINTS as f32, 1.)).is_err());
        assert_eq!(points, original);
    }

    #[test]
    fn repeated_motion_is_free_but_corners_and_reversals_are_kept() {
        let mut points = Vec::new();
        for p in [(0., 0.), (0., 0.), (1., 0.), (1., 1.), (1., 0.), (0., 0.)] {
            append(&mut points, p).unwrap();
        }
        assert_eq!(points, [(0., 0.), (1., 0.), (1., 1.), (1., 0.), (0., 0.)]);
        assert!(append(&mut points, (f32::NAN, 0.)).is_err());
        assert!(append(&mut points, (0., f32::INFINITY)).is_err());
        assert_eq!(points.len(), 5);
    }
}
