//! Shared canvas grid geometry for drawing, snapping, and persisted settings.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MIN_SPACING: u32 = 1;
pub const MAX_SPACING: u32 = 10_000;
pub const MIN_SUBDIVISIONS: u8 = 1;
pub const MAX_SUBDIVISIONS: u8 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridSettings {
    pub spacing: u32,
    pub subdivisions: u8,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            spacing: 8,
            subdivisions: 1,
        }
    }
}

impl GridSettings {
    pub fn validate(self) -> Result<Self> {
        ensure!(
            (MIN_SPACING..=MAX_SPACING).contains(&self.spacing),
            "Grid spacing is outside 1..10000"
        );
        ensure!(
            (MIN_SUBDIVISIONS..=MAX_SUBDIVISIONS).contains(&self.subdivisions),
            "Grid subdivisions are outside 1..32"
        );
        Ok(self)
    }

    pub fn minor_spacing(self) -> f32 {
        self.spacing as f32 / f32::from(self.subdivisions)
    }

    /// Draw only visible, distinguishable lines, all on the exact snap lattice.
    /// Density changes never change the configured snapping increment.
    pub fn visible_lines(
        self,
        start: f32,
        end: f32,
        zoom: f32,
    ) -> impl Iterator<Item = (f32, bool)> {
        let minor = self.minor_spacing().max(f32::EPSILON);
        let major = self.spacing.max(1) as f32;
        let minimum = (4. / zoom.max(0.0001)).max((end - start).max(0.) / 2048.);
        let stride = if minor >= minimum {
            1
        } else {
            ((minimum / major).ceil().max(1.) as u32)
                .saturating_mul(u32::from(self.subdivisions.max(1)))
        };
        let step = minor * stride as f32;
        let first = (start.max(0.) / step).ceil() as u32;
        let last = (end.max(0.) / step).floor() as u32;
        let count = if end < start || last < first {
            0
        } else {
            (last - first).saturating_add(1).min(2050)
        };
        (0..count).map(move |n| {
            let index = (first + n) * stride;
            (
                index as f32 * minor,
                index % u32::from(self.subdivisions.max(1)) == 0,
            )
        })
    }

    pub fn lines(self, extent: (f32, f32)) -> GridLines {
        GridLines {
            width: extent.0.max(0.),
            height: extent.1.max(0.),
            settings: self,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLines {
    width: f32,
    height: f32,
    settings: GridSettings,
}

impl GridLines {
    pub fn vertical(self) -> impl Iterator<Item = (f32, bool)> {
        line_positions(self.width, self.settings)
    }

    pub fn horizontal(self) -> impl Iterator<Item = (f32, bool)> {
        line_positions(self.height, self.settings)
    }

    pub fn vertical_between(self, start: f32, end: f32) -> impl Iterator<Item = (f32, bool)> {
        line_positions_between(self.width, self.settings, start, end)
    }

    pub fn horizontal_between(self, start: f32, end: f32) -> impl Iterator<Item = (f32, bool)> {
        line_positions_between(self.height, self.settings, start, end)
    }
}

fn line_positions_between(
    extent: f32,
    settings: GridSettings,
    start: f32,
    end: f32,
) -> impl Iterator<Item = (f32, bool)> {
    let step = settings.minor_spacing().max(f32::EPSILON);
    let first = (start.max(0.) / step).floor() as u32;
    let last = (end.min(extent) / step).ceil().max(0.) as u32;
    let count = (extent / step).floor().min(100_000.) as u32;
    (first.min(count)..=last.min(count)).map(move |index| {
        let position = index as f32 * step;
        let major_line = (position / settings.spacing as f32)
            .round()
            .mul_add(-(settings.spacing as f32), position)
            .abs()
            < 0.001;
        (position, major_line)
    })
}

fn line_positions(extent: f32, settings: GridSettings) -> impl Iterator<Item = (f32, bool)> {
    let step = settings.minor_spacing().max(f32::EPSILON);
    let major = settings.spacing as f32;
    let count = (extent / step).floor().min(100_000.) as u32;
    (0..=count).map(move |index| {
        let position = index as f32 * step;
        let major_line = (position / major).round().mul_add(-major, position).abs() < 0.001;
        (position, major_line)
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GridPoint {
    pub x: f32,
    pub y: f32,
}

pub fn snap_point(point: GridPoint, settings: GridSettings, bypass: bool) -> GridPoint {
    if bypass {
        return point;
    }
    let spacing = settings.minor_spacing().max(f32::EPSILON);
    GridPoint {
        x: (point.x / spacing).round() * spacing,
        y: (point.y / spacing).round() * spacing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_lines_and_snapping_share_minor_spacing() {
        let settings = GridSettings {
            spacing: 12,
            subdivisions: 3,
        };
        assert_eq!(settings.validate().unwrap().minor_spacing(), 4.);
        assert_eq!(
            snap_point(GridPoint { x: 5.9, y: 10.1 }, settings, false),
            GridPoint { x: 4., y: 12. }
        );
        assert_eq!(
            snap_point(GridPoint { x: 5.9, y: 10.1 }, settings, true),
            GridPoint { x: 5.9, y: 10.1 }
        );
        let lines = settings.lines((13., 13.)).vertical().collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![(0., true), (4., false), (8., false), (12., true)]
        );
    }

    #[test]
    fn dense_grid_is_viewport_bounded_and_pan_stable() {
        let settings = GridSettings {
            spacing: 1,
            subdivisions: 32,
        };
        for zoom in [0.01, 1., 64.] {
            let a = settings
                .visible_lines(7000., 100000., zoom)
                .collect::<Vec<_>>();
            assert!(a.len() <= 2050);
            assert!(a.iter().all(|(x, _)| *x >= 7000. && *x <= 100000.));
            for (x, _) in &a {
                assert_eq!(
                    snap_point(GridPoint { x: *x, y: *x }, settings, false).x,
                    *x
                );
            }
            let b = settings
                .visible_lines(7001., 100001., zoom)
                .collect::<Vec<_>>();
            assert!(
                a.iter()
                    .skip(1)
                    .take(a.len().saturating_sub(2))
                    .all(|p| b.contains(p))
            );
        }
    }

    #[test]
    fn settings_reject_unbounded_values() {
        assert!(
            GridSettings {
                spacing: 0,
                subdivisions: 1
            }
            .validate()
            .is_err()
        );
        assert!(
            GridSettings {
                spacing: 8,
                subdivisions: 0
            }
            .validate()
            .is_err()
        );
        assert!(
            GridSettings {
                spacing: 8,
                subdivisions: 33
            }
            .validate()
            .is_err()
        );
    }
}
