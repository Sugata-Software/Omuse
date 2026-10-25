//! Reversible guided-mask review state. Does not mutate an editor or document.
use crate::guided_mask::{self, Alternative, Prediction, Prompt, SourceKey};
use anyhow::{Context, Result, ensure};
use image::{GrayImage, RgbaImage};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Positive,
    Box,
    AddCoverage,
    SubtractCoverage,
}
pub struct Review {
    pub source: SourceKey,
    pub pixels: Arc<RgbaImage>,
    pub prompts: Vec<Prompt>,
    pub generation: u64,
    pub visual_revision: u64,
    pub alternatives: Vec<Alternative>,
    pub selected_alternative: usize,
    pub tool: Tool,
    pub brush_radius: f32,
    pub manually_refined: bool,
    selected: Option<Arc<GrayImage>>,
}
impl Review {
    pub fn new(source: SourceKey, pixels: Arc<RgbaImage>) -> Result<Self> {
        ensure!(
            source.width == pixels.width() && source.height == pixels.height(),
            "The mask source dimensions changed"
        );
        ensure!(
            u64::from(source.width) * u64::from(source.height) <= guided_mask::MAX_SOURCE_PIXELS,
            "The mask source exceeds 16 megapixels"
        );
        Ok(Self {
            source,
            pixels,
            prompts: Vec::new(),
            generation: 0,
            visual_revision: 0,
            alternatives: Vec::new(),
            selected_alternative: 0,
            tool: Tool::Positive,
            brush_radius: 24.,
            manually_refined: false,
            selected: None,
        })
    }
    pub fn add_prompt(&mut self, prompt: Prompt) -> Result<()> {
        ensure!(
            !self.manually_refined,
            "Reset manual refinements before changing model prompts"
        );
        let mut next = self.prompts.clone();
        next.push(prompt);
        guided_mask::validate_prompts(&next, self.source.width, self.source.height)?;
        self.prompts = next;
        self.invalidate();
        Ok(())
    }
    pub fn undo_prompt(&mut self) -> Result<()> {
        ensure!(
            !self.manually_refined,
            "Reset manual refinements before changing model prompts"
        );
        ensure!(
            self.prompts.pop().is_some(),
            "There are no prompts to remove"
        );
        self.invalidate();
        Ok(())
    }
    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.visual_revision = self.visual_revision.wrapping_add(1);
        self.alternatives.clear();
        self.selected = None;
        self.selected_alternative = 0;
    }
    pub fn accept(&mut self, generation: u64, prediction: Prediction) -> Result<()> {
        ensure!(
            generation == self.generation && prediction.source == self.source,
            "Stale guided-mask result discarded"
        );
        ensure!(
            (1..=3).contains(&prediction.alternatives.len()),
            "Unexpected number of mask alternatives"
        );
        let expected = guided_mask::work_dimensions(self.source.width, self.source.height);
        ensure!(
            prediction
                .alternatives
                .iter()
                .all(|a| a.mask.dimensions() == expected && a.estimated_iou.is_finite()),
            "Invalid guided-mask alternative"
        );
        ensure!(
            prediction.initial_mask.dimensions() == (self.source.width, self.source.height),
            "Prepared mask has the wrong source dimensions"
        );
        self.alternatives = prediction.alternatives;
        self.select_prepared_alternative(0, prediction.initial_mask)
    }
    pub fn select_prepared_alternative(
        &mut self,
        index: usize,
        prepared: Arc<GrayImage>,
    ) -> Result<()> {
        ensure!(
            !self.manually_refined,
            "Reset manual refinements before choosing another alternative"
        );
        self.alternatives
            .get(index)
            .context("This mask alternative is unavailable")?;
        ensure!(
            prepared.dimensions() == (self.source.width, self.source.height),
            "Prepared alternative has wrong dimensions"
        );
        self.selected = Some(prepared);
        self.selected_alternative = index;
        self.visual_revision = self.visual_revision.wrapping_add(1);
        Ok(())
    }
    pub fn reset_manual(&mut self, prepared: Arc<GrayImage>) -> Result<()> {
        ensure!(
            prepared.dimensions() == (self.source.width, self.source.height),
            "Prepared reset has wrong dimensions"
        );
        self.manually_refined = false;
        self.select_prepared_alternative(self.selected_alternative, prepared)
    }
    pub fn mask(&self) -> Result<&GrayImage> {
        self.selected
            .as_deref()
            .context("Click a subject or draw a box, then update the mask")
    }
    pub fn mask_arc(&self) -> Result<Arc<GrayImage>> {
        self.selected.clone().context("Generate a mask first")
    }
    /// Manual brush is explicitly separate from model inference. Each call is a
    /// bounded local dab. Canvas events interpolate at most64 dabs, never an
    /// unbounded source-pixel path in one UI event.
    pub fn dab(&mut self, x: f32, y: f32, subtract: bool) -> Result<()> {
        ensure!(
            x.is_finite()
                && y.is_finite()
                && x >= 0.
                && y >= 0.
                && x < self.source.width as f32
                && y < self.source.height as f32,
            "Paint inside the source image"
        );
        ensure!(
            self.brush_radius.is_finite() && (0.5..=128.).contains(&self.brush_radius),
            "Brush diameter must be 1–256 source pixels"
        );
        let mask = self
            .selected
            .as_mut()
            .context("Generate a mask before refining it")?;
        let mask =
            Arc::get_mut(mask).context("Wait for the current preview before painting this mask")?;
        if self.brush_radius == 0.5 {
            let value = if subtract { 0 } else { 255 };
            let pixel = mask.get_pixel_mut(x.floor() as u32, y.floor() as u32);
            if pixel[0] != value {
                pixel[0] = value;
                self.visual_revision = self.visual_revision.wrapping_add(1);
                self.manually_refined = true;
            }
            return Ok(());
        }
        let r = self.brush_radius;
        let left = (x - r).floor().max(0.) as u32;
        let right = (x + r).ceil().min(self.source.width as f32) as u32;
        let top = (y - r).floor().max(0.) as u32;
        let bottom = (y + r).ceil().min(self.source.height as f32) as u32;
        for py in top..bottom {
            for px in left..right {
                let d = ((px as f32 + 0.5 - x).powi(2) + (py as f32 + 0.5 - y).powi(2)).sqrt() / r;
                if d < 1. {
                    let coverage = ((1. - d) * 4.).clamp(0., 1.);
                    let old = f32::from(mask.get_pixel(px, py)[0]);
                    let target = if subtract { 0. } else { 255. };
                    mask.put_pixel(
                        px,
                        py,
                        image::Luma([(old + (target - old) * coverage).round() as u8]),
                    );
                }
            }
        }
        self.manually_refined = true;
        self.visual_revision = self.visual_revision.wrapping_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn review() -> Review {
        let pixels = Arc::new(RgbaImage::from_pixel(
            40,
            30,
            image::Rgba([80, 120, 140, 255]),
        ));
        let key = SourceKey::capture(1, 2, 3, "layer".into(), &pixels).unwrap();
        Review::new(key, pixels).unwrap()
    }
    fn prediction(review: &Review) -> Prediction {
        Prediction {
            source: review.source.clone(),
            alternatives: vec![Alternative {
                mask: Arc::new(GrayImage::from_pixel(40, 30, image::Luma([200]))),
                estimated_iou: 0.9,
            }],
            embedding_reused: false,
            initial_mask: Arc::new(GrayImage::from_pixel(40, 30, image::Luma([200]))),
        }
    }
    #[test]
    fn source_revision_and_generation_are_checked_before_review() {
        let mut r = review();
        r.add_prompt(Prompt::Positive { x: 5., y: 5. }).unwrap();
        let mut p = prediction(&r);
        p.source.revision += 1;
        assert!(r.accept(r.generation, p).is_err());
        assert!(r.accept(r.generation + 1, prediction(&r)).is_err());
        assert!(r.mask().is_err());
    }
    #[test]
    fn manual_subtract_is_reversible_and_preserves_source_and_prompt_state() {
        let mut r = review();
        let original = r.pixels.clone();
        r.add_prompt(Prompt::Positive { x: 5., y: 5. }).unwrap();
        r.accept(r.generation, prediction(&r)).unwrap();
        r.brush_radius = 4.;
        r.dab(10.5, 10.5, true).unwrap();
        assert_eq!(r.mask().unwrap().get_pixel(10, 10)[0], 0);
        assert_eq!(r.mask().unwrap().get_pixel(30, 20)[0], 200);
        assert!(r.add_prompt(Prompt::Positive { x: 6., y: 6. }).is_err());
        r.reset_manual(Arc::new(GrayImage::from_pixel(40, 30, image::Luma([200]))))
            .unwrap();
        assert_eq!(r.mask().unwrap().get_pixel(10, 10)[0], 200);
        assert_eq!(r.pixels, original);
        assert_eq!(r.prompts.len(), 1);
    }
    #[test]
    fn replacing_prompts_never_keeps_an_old_alternative() {
        let mut r = review();
        r.add_prompt(Prompt::Positive { x: 5., y: 5. }).unwrap();
        r.accept(r.generation, prediction(&r)).unwrap();
        r.add_prompt(Prompt::Positive { x: 6., y: 6. }).unwrap();
        assert!(r.mask().is_err());
        assert!(r.alternatives.is_empty());
        r.undo_prompt().unwrap();
        assert_eq!(r.prompts.len(), 1);
    }
    #[test]
    fn every_manual_change_advances_visual_revision_and_one_pixel_brush_is_precise() {
        let mut r = review();
        r.add_prompt(Prompt::Positive { x: 5., y: 5. }).unwrap();
        r.accept(r.generation, prediction(&r)).unwrap();
        let prior = (r.generation, r.visual_revision);
        let original = r.mask().unwrap().clone();
        r.brush_radius = 0.5;
        r.dab(10., 11., true).unwrap();
        assert_ne!((r.generation, r.visual_revision), prior);
        assert_eq!(r.mask().unwrap().get_pixel(10, 11)[0], 0);
        assert_eq!(
            r.mask()
                .unwrap()
                .pixels()
                .zip(original.pixels())
                .filter(|(a, b)| a != b)
                .count(),
            1
        );
        let changed = r.visual_revision;
        r.reset_manual(Arc::new(GrayImage::from_pixel(40, 30, image::Luma([200]))))
            .unwrap();
        assert!(r.visual_revision > changed);
        assert_eq!(r.mask().unwrap(), &original);
    }
}
