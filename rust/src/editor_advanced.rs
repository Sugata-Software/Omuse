//! Atomic integration of prepared editable-source changes with editor history.
use super::*;
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

impl Editor {
    /// Insert a replacement above its source in the same folder and hide the
    /// source in the same undo transaction. Toggling visibility permits a
    /// before/after comparison without deleting the original artwork.
    pub fn insert_derived_layers(&mut self, source: &str, layers: Vec<Layer>) -> Result<bool> {
        ensure!(
            self.floating.is_none(),
            "Commit or cancel the floating selection first"
        );
        ensure!(
            !locked_in_tree(&self.document.layers, source, false),
            "Unlock the source and its parents first"
        );
        ensure!(!layers.is_empty(), "No derived layers");
        ensure!(
            tree_count(&self.document.layers).saturating_add(tree_count(&layers))
                <= crate::model::MAX_LAYERS,
            "Too many layers"
        );
        self.finish_stroke();
        let mut candidate = self.document.clone();
        candidate
            .find_layer_mut(source)
            .context("Source layer no longer exists")?
            .visible = false;
        let mut after = source.to_owned();
        for mut layer in layers {
            regenerate_ids(&mut layer);
            let id = layer.id.clone();
            ensure!(
                insert_after(&mut candidate.layers, &after, layer),
                "Source layer no longer exists"
            );
            after = id;
        }
        crate::advanced::validate_document_budget(&candidate)?;
        ensure!(
            crate::raster::validate(&candidate).is_empty(),
            "Derived result cannot be rendered"
        );
        let before = self.snapshot();
        self.document = candidate;
        self.active_layer = after;
        self.commit(before);
        Ok(true)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn selection_for_layer(&self, id: &str) -> Result<Option<crate::advanced_ops::SoftMask>> {
        if self.selection.is_none() {
            return Ok(None);
        }
        let layer = self.document.find_layer(id).context("Layer not found")?;
        let image = layer.image.as_ref().context("Choose a pixel layer")?;
        let transform =
            Transform::for_layer(&self.document, id).context("Invalid layer transform")?;
        ensure!(
            u64::from(image.width()) * u64::from(image.height())
                <= crate::advanced::MAX_ADVANCED_PIXELS,
            "Selection mask exceeds limit"
        );
        let mut data = Vec::with_capacity(image.width() as usize * image.height() as usize);
        for y in 0..image.height() {
            for x in 0..image.width() {
                let (wx, wy) = transform.world(x as f32 + 0.5, y as f32 + 0.5);
                data.push((selection_coverage(&self.selection, wx, wy) * 255.).round() as u8);
            }
        }
        Ok(Some(crate::advanced_ops::SoftMask::new(
            image.width(),
            image.height(),
            data,
        )?))
    }
    pub fn editable_state(&self, id: &str) -> Result<crate::advanced::LayerState> {
        let layer = self.document.find_layer(id).context("Layer not found")?;
        ensure!(!layer.is_group(), "Choose a pixel layer");
        if let Some(state) = &layer.advanced {
            let mut state = (**state).clone();
            if layer
                .metadata
                .get(crate::advanced::RASTER_BLEND_IF_KEY)
                .is_some()
            {
                state.recipe.blend_if = crate::advanced::layer_blend_if(layer)?;
            }
            return Ok(state);
        }
        ensure!(
            !is_live_object(layer),
            "Rasterize text or shapes before attaching an editable source"
        );
        let mut state = crate::advanced::LayerState::from_image(
            layer.image.as_deref().context("Layer has no pixels")?,
            &layer.name,
        )?;
        state.recipe.blend_if = crate::advanced::layer_blend_if(layer)?;
        Ok(state)
    }

    /// Commit fully prepared results in one undo step. Every target is checked
    /// before mutation, including ancestors and the rendered cache dimensions.
    pub fn replace_editable_states(
        &mut self,
        states: Vec<(String, crate::advanced::LayerState)>,
    ) -> Result<bool> {
        self.replace_editable_states_internal(states, false)
    }

    /// Explicitly apply a vector recipe, rebuilding its vector-derived mask.
    pub fn replace_vector_state(
        &mut self,
        id: &str,
        state: crate::advanced::LayerState,
    ) -> Result<bool> {
        ensure!(
            state.recipe.vector.is_some(),
            "Vector apply requires a vector recipe"
        );
        self.replace_editable_states_internal(vec![(id.to_owned(), state)], true)
    }

    fn replace_editable_states_internal(
        &mut self,
        states: Vec<(String, crate::advanced::LayerState)>,
        force_vector_mask_rebuild: bool,
    ) -> Result<bool> {
        ensure!(
            self.floating.is_none(),
            "Commit or cancel the floating selection first"
        );
        if states.is_empty() {
            return Ok(false);
        }
        self.finish_stroke();
        let mut seen = std::collections::HashSet::new();
        let mut replacements = vec![];
        for (id, state) in states {
            ensure!(seen.insert(id.clone()), "Duplicate editable target");
            ensure!(
                !locked_in_tree(&self.document.layers, &id, false),
                "Unlock the target layer and its parents first"
            );
            let mut layer = self
                .document
                .find_layer(&id)
                .context("Target layer no longer exists")?
                .clone();
            ensure!(
                !layer.is_group() && layer.image.is_some(),
                "Choose a pixel layer"
            );
            state.validate()?;
            let old = layer.image.as_ref().unwrap().dimensions();
            let proxy = state.proxy()?;
            let vector_recipe_changed = layer.advanced.as_ref().is_none_or(|previous| {
                previous.recipe.vector != state.recipe.vector
                    || previous.recipe.vector_is_mask != state.recipe.vector_is_mask
            });
            let result_dimensions_changed = layer
                .advanced
                .as_ref()
                .is_some_and(|previous| previous.result.dimensions() != state.result.dimensions());
            let rebuild_vector_mask = force_vector_mask_rebuild
                || layer.advanced.is_none()
                || vector_recipe_changed
                || result_dimensions_changed;
            // Keep displayed dimensions when a linked asset changes resolution.
            layer.scale_x *= old.0 as f32 / proxy.width() as f32;
            layer.scale_y *= old.1 as f32 / proxy.height() as f32;
            if rebuild_vector_mask
                && let Some(path) = &state.recipe.vector
                && state.recipe.vector_is_mask
            {
                let mask = crate::vector_path::rasterize_mask(
                    path,
                    proxy.width(),
                    proxy.height(),
                    0.25,
                    || false,
                )?;
                layer.mask = Some(
                    image::RgbaImage::from_fn(mask.width(), mask.height(), |x, y| {
                        let v = mask.get_pixel(x, y)[0];
                        image::Rgba([v, v, v, 255])
                    })
                    .into(),
                );
                layer.metadata["maskEnabled"] = serde_json::json!(true);
                layer.metadata["maskLinked"] = serde_json::json!(true);
                if let Some(metadata) = layer.metadata.as_object_mut() {
                    metadata.remove("maskSourceID");
                    metadata.remove("maskPlacement");
                }
            }
            layer.image = Some(proxy.into());
            layer.advanced = Some(Arc::new(state));
            if let Some(metadata) = layer.metadata.as_object_mut() {
                metadata.remove(crate::advanced::RASTER_BLEND_IF_KEY);
            }
            replacements.push((id, layer));
        }
        let mut candidate = self.document.clone();
        for (id, layer) in replacements {
            *candidate.find_layer_mut(&id).unwrap() = layer;
        }
        crate::advanced::validate_document_budget(&candidate)?;
        ensure!(
            crate::raster::validate(&candidate).is_empty(),
            "Prepared edit cannot be rendered"
        );
        let before = self.snapshot();
        self.document = candidate;
        self.commit(before);
        Ok(true)
    }

    pub fn insert_prepared_layers(&mut self, layers: Vec<Layer>) -> Result<bool> {
        self.insert_prepared_layers_sized(layers, None)
    }

    pub fn insert_prepared_layers_sized(
        &mut self,
        layers: Vec<Layer>,
        dimensions: Option<(u32, u32)>,
    ) -> Result<bool> {
        ensure!(
            self.floating.is_none(),
            "Commit or cancel the floating selection first"
        );
        ensure!(
            !layers.is_empty() && layers.len() <= 256,
            "Invalid result layer count"
        );
        self.finish_stroke();
        let mut candidate = self.document.clone();
        ensure!(
            tree_count(&candidate.layers).saturating_add(tree_count(&layers))
                <= crate::model::MAX_LAYERS,
            "Too many layers"
        );
        if let Some((width, height)) = dimensions {
            ensure!(
                crate::model::valid_dimensions(width, height),
                "Invalid output canvas dimensions"
            );
            candidate.width = width;
            candidate.height = height;
        }
        let mut last = None;
        for mut layer in layers {
            regenerate_ids(&mut layer);
            last = Some(layer.id.clone());
            candidate.layers.push(layer);
        }
        crate::advanced::validate_document_budget(&candidate)?;
        ensure!(
            crate::raster::validate(&candidate).is_empty(),
            "Result cannot be rendered"
        );
        let before = self.snapshot();
        self.document = candidate;
        self.active_layer = last.unwrap();
        self.commit(before);
        Ok(true)
    }

    pub fn set_brush_dynamics(
        &mut self,
        settings: Option<crate::brush_dynamics::Settings>,
    ) -> Result<()> {
        if let Some(settings) = &settings {
            settings.validate()?;
        }
        self.brush_dynamics = settings;
        Ok(())
    }
}

#[cfg(test)]
mod blend_if_raster_tests {
    use super::*;
    use crate::advanced_ops::{BlendIf, BlendIfRange};

    fn document_with_blend_if() -> (Document, String) {
        let mut document = Document::new(8, 4);
        document.layers[0].image =
            Some(image::RgbaImage::from_pixel(8, 4, image::Rgba([20, 40, 180, 255])).into());
        let pixels = image::RgbaImage::from_fn(8, 4, |x, _| {
            let value = (x * 255 / 7) as u8;
            image::Rgba([value, 30, 30, 255])
        });
        let mut top = Layer::paint("Blend If", 8, 4);
        top.image = Some(pixels.clone().into());
        let mut state = crate::advanced::LayerState::from_image(&pixels, "Blend If").unwrap();
        state.recipe.blend_if = Some(BlendIf {
            source: BlendIfRange {
                black: 0.35,
                black_split: 0.55,
                ..Default::default()
            },
            ..Default::default()
        });
        top.advanced = Some(Arc::new(state));
        let id = top.id.clone();
        document.layers.push(top);
        (document, id)
    }

    #[test]
    fn rasterize_blend_if_paints_round_trips_and_undoes() {
        let (document, id) = document_with_blend_if();
        let before = crate::raster::composite(&document);
        let mut editor = Editor::new(document);
        editor.active_layer = id.clone();
        assert!(editor.rasterize_layer(&id));
        let raster = editor.document.find_layer(&id).unwrap();
        assert!(raster.advanced.is_none());
        assert!(
            raster
                .metadata
                .get(crate::advanced::RASTER_BLEND_IF_KEY)
                .is_some()
        );
        assert_eq!(crate::raster::composite(&editor.document), before);
        assert!(editor.begin_stroke(7., 3., 1., PaintTool::Brush));
        assert!(editor.finish_stroke());
        let painted = crate::raster::composite(&editor.document);
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("blend-if.omuse");
        crate::document::save(&editor.document, &path).unwrap();
        let reopened = crate::document::open(&path).unwrap();
        assert_eq!(crate::raster::composite(&reopened), painted);
        assert!(editor.undo());
        assert_eq!(crate::raster::composite(&editor.document), before);
    }

    #[test]
    fn metadata_restores_editable_state_and_invalid_values_fail_closed() {
        let (document, id) = document_with_blend_if();
        let mut editor = Editor::new(document);
        editor.active_layer = id.clone();
        assert!(editor.rasterize_layer(&id));
        let restored = editor.editable_state(&id).unwrap();
        assert!(restored.recipe.blend_if.is_some());
        editor
            .replace_editable_states(vec![(id.clone(), restored)])
            .unwrap();
        let layer = editor.document.find_layer(&id).unwrap();
        assert!(
            layer
                .metadata
                .get(crate::advanced::RASTER_BLEND_IF_KEY)
                .is_none()
        );
        assert!(layer.advanced.as_ref().unwrap().recipe.blend_if.is_some());

        let mut invalid = editor.document.clone();
        invalid.find_layer_mut(&id).unwrap().metadata[crate::advanced::RASTER_BLEND_IF_KEY] =
            serde_json::json!({"sourceChannel":"not-a-channel"});
        assert!(
            crate::raster::validate(&invalid)
                .iter()
                .any(|error| error.contains("rustBlendIf"))
        );
    }

    #[test]
    fn applying_mask_is_rejected_for_editable_source() {
        let (mut document, id) = document_with_blend_if();
        document.find_layer_mut(&id).unwrap().mask =
            Some(image::RgbaImage::from_pixel(8, 4, image::Rgba([128, 128, 128, 255])).into());
        let mut editor = Editor::new(document);
        let source = editor.document.find_layer(&id).unwrap().advanced.clone();
        assert!(!editor.remove_mask(&id, true));
        let layer = editor.document.find_layer(&id).unwrap();
        assert!(layer.mask.is_some());
        assert!(Arc::ptr_eq(
            layer.advanced.as_ref().unwrap(),
            source.as_ref().unwrap()
        ));
        assert_eq!(editor.undo_depth(), 0);
    }
}
