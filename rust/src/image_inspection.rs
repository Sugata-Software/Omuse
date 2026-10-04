//! Pixel-accurate, bounded navigation for encoded-image inspection.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageInspection {
    pub actual_size: bool,
    /// Offset from the image centre in source pixels.
    pub pan: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InspectionLayout {
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub scale: f32,
}

impl ImageInspection {
    pub fn layout(
        self,
        image: [u32; 2],
        viewport: [f32; 2],
        device_scale: f32,
    ) -> InspectionLayout {
        let image = image.map(|axis| axis.max(1) as f32);
        let viewport = viewport.map(|axis| if axis.is_finite() { axis.max(1.) } else { 1. });
        let device_scale = if device_scale.is_finite() {
            device_scale.max(0.25)
        } else {
            1.
        };
        let scale = if self.actual_size {
            1. / device_scale
        } else {
            (viewport[0] / image[0])
                .min(viewport[1] / image[1])
                .min(1. / device_scale)
        };
        let size = image.map(|axis| axis * scale);
        let origin = [0, 1].map(|axis| {
            let limit = ((size[axis] - viewport[axis]) / (2. * scale)).max(0.);
            let pan = if self.actual_size && self.pan[axis].is_finite() {
                self.pan[axis].clamp(-limit, limit)
            } else {
                0.
            };
            (viewport[axis] - size[axis]) / 2. - pan * scale
        });
        InspectionLayout {
            origin,
            size,
            scale,
        }
    }

    pub fn drag(
        &mut self,
        delta: [f32; 2],
        image: [u32; 2],
        viewport: [f32; 2],
        device_scale: f32,
    ) {
        if !self.actual_size {
            return;
        }
        let layout = self.layout(image, viewport, device_scale);
        for axis in 0..2 {
            let limit = ((layout.size[axis] - viewport[axis]) / (2. * layout.scale)).max(0.);
            if delta[axis].is_finite() {
                self.pan[axis] = (self.pan[axis] - delta[axis] / layout.scale).clamp(-limit, limit);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_size_maps_one_source_pixel_to_one_device_pixel() {
        for device_scale in [1., 1.25, 1.5, 2.] {
            let layout = ImageInspection {
                actual_size: true,
                pan: [0., 0.],
            }
            .layout([1200, 800], [400., 240.], device_scale);
            assert!((layout.size[0] * device_scale - 1200.).abs() < 0.001);
            assert!((layout.size[1] * device_scale - 800.).abs() < 0.001);
        }
    }

    #[test]
    fn fit_centres_without_stretching_or_upscaling_small_images() {
        let fit = ImageInspection::default();
        assert_eq!(
            fit.layout([1200, 800], [600., 600.], 1.),
            InspectionLayout {
                origin: [0., 100.],
                size: [600., 400.],
                scale: 0.5,
            }
        );
        assert_eq!(
            fit.layout([100, 100], [600., 400.], 1.).origin,
            [250., 150.]
        );
    }

    #[test]
    fn pan_reaches_each_edge_without_exposing_empty_margins() {
        let mut view = ImageInspection {
            actual_size: true,
            pan: [0., 0.],
        };
        view.drag([10_000., 10_000.], [1200, 800], [400., 240.], 1.);
        assert_eq!(view.layout([1200, 800], [400., 240.], 1.).origin, [0., 0.]);
        view.drag([-20_000., -20_000.], [1200, 800], [400., 240.], 1.);
        assert_eq!(
            view.layout([1200, 800], [400., 240.], 1.).origin,
            [-800., -560.]
        );
        view.actual_size = false;
        let origin = view.layout([1200, 800], [400., 240.], 1.).origin;
        assert!((origin[0] - 20.).abs() < 0.001 && origin[1].abs() < 0.001);
    }

    #[test]
    fn resizing_and_invalid_motion_cannot_lose_the_image() {
        let mut view = ImageInspection {
            actual_size: true,
            pan: [500., 400.],
        };
        view.drag([f32::NAN, 0.], [100, 100], [400., 240.], 1.);
        assert_eq!(
            view.layout([100, 100], [400., 240.], 1.).origin,
            [150., 70.]
        );
    }
}
