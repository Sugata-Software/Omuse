use super::*;

const MIN_BRUSH_SIZE: f32 = 1.0;
const MAX_BRUSH_SIZE: f32 = 1024.0;
const BRUSH_SIZE_STEP: f32 = 1.25;
const BRUSH_HARDNESS_STEP: f32 = 0.1;

impl EditorView {
    /// Handle editor commands whose behavior is primarily keyboard-oriented.
    ///
    /// `EditorView::command` owns the global dialog, text-input and floating-selection
    /// guards. This method still finishes paint before changing a brush or tool so it
    /// remains safe when called directly by a native control or a focused UI test.
    pub(super) fn handle_keyboard_command(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match name {
            "brush-smaller" => {
                self.finish_keyboard_paint(cx);
                let size = (self.editor.brush.size / BRUSH_SIZE_STEP).max(MIN_BRUSH_SIZE);
                self.set_keyboard_brush_size(size, window, cx);
            }
            "brush-larger" => {
                self.finish_keyboard_paint(cx);
                let size = (self.editor.brush.size * BRUSH_SIZE_STEP).min(MAX_BRUSH_SIZE);
                self.set_keyboard_brush_size(size, window, cx);
            }
            "brush-softer" => {
                self.finish_keyboard_paint(cx);
                let hardness = (self.editor.brush.hardness - BRUSH_HARDNESS_STEP).max(0.0);
                self.set_keyboard_brush_hardness(hardness, window, cx);
            }
            "brush-harder" => {
                self.finish_keyboard_paint(cx);
                let hardness = (self.editor.brush.hardness + BRUSH_HARDNESS_STEP).min(1.0);
                self.set_keyboard_brush_hardness(hardness, window, cx);
            }
            "nudge-left-large" | "nudge-right-large" | "nudge-up-large" | "nudge-down-large" => {
                let (dx, dy) = match name {
                    "nudge-left-large" => (-10.0, 0.0),
                    "nudge-right-large" => (10.0, 0.0),
                    "nudge-up-large" => (0.0, -10.0),
                    _ => (0.0, 10.0),
                };
                let ids = self.selected_layer_ids();
                if self.editor.move_layers(&ids, dx, dy) {
                    self.changed(cx);
                } else {
                    self.status =
                        "Nudge needs an unlocked selected layer and no floating selection".into();
                    cx.notify();
                }
            }
            "tool-hand" => {
                if self.inline_text.is_some() {
                    self.status = "Finish text with Ctrl+Enter, or cancel with Escape".into();
                    cx.notify();
                    return true;
                }
                self.finish_keyboard_paint(cx);
                self.tool = Tool::Hand;
                self.status = "Hand tool — drag to pan the canvas".into();
                self.focus.focus(window, cx);
                cx.notify();
            }
            "toggle-panels" => {
                self.inspector_visible = !self.inspector_visible;
                self.status = if self.inspector_visible {
                    "Inspector shown"
                } else {
                    "Inspector hidden"
                }
                .into();
                self.focus.focus(window, cx);
                cx.notify();
            }
            "select-layer-above" | "select-layer-below" => {
                self.select_adjacent_layer(name == "select-layer-above", window, cx);
            }
            "reorder-layer-up" | "reorder-layer-down" => {
                // Reuse the layer panel transaction, including sibling checks,
                // clipping reconciliation, selection retention and one-step undo.
                self.command(
                    if name == "reorder-layer-up" {
                        "up"
                    } else {
                        "down"
                    },
                    window,
                    cx,
                );
            }
            "delete-forward" => self.command("delete-content", window, cx),
            "zoom-in-plus" => self.command("zoom-in", window, cx),
            "filter-levels" | "filter-curves" | "filter-hsl" | "filter-color-balance" => {
                self.editing_object = None;
                let kind = match name {
                    "filter-levels" => 1,
                    "filter-curves" => 2,
                    "filter-hsl" => 3,
                    _ => 4,
                };
                self.open_filter(kind, window, cx);
            }
            _ => {
                let Some(percent) = name
                    .strip_prefix("brush-opacity-")
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|value| *value >= 10 && *value <= 100 && value % 10 == 0)
                else {
                    return false;
                };
                self.finish_keyboard_paint(cx);
                self.editor.brush.opacity = f32::from(percent) / 100.0;
                self.status = format!("Brush opacity: {percent}%");
                self.focus.focus(window, cx);
                cx.notify();
            }
        }
        true
    }

    fn finish_keyboard_paint(&mut self, cx: &mut Context<Self>) {
        let changed = self.editor.finish_stroke() | self.editor.finish_clone_stroke();
        if changed {
            self.changed(cx);
        }
    }

    fn set_keyboard_brush_size(&mut self, size: f32, window: &mut Window, cx: &mut Context<Self>) {
        let size = size.clamp(MIN_BRUSH_SIZE, MAX_BRUSH_SIZE);
        self.editor.brush.size = size;
        if let Some(settings) = &mut self.editor.brush_dynamics {
            settings.size = size;
        }
        self.status = format!("Brush size: {size:.0} px");
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn set_keyboard_brush_hardness(
        &mut self,
        hardness: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hardness = hardness.clamp(0.0, 1.0);
        self.editor.brush.hardness = hardness;
        if let Some(settings) = &mut self.editor.brush_dynamics {
            settings.hardness = hardness;
        }
        self.status = format!("Brush hardness: {:.0}%", hardness * 100.0);
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn select_adjacent_layer(&mut self, above: bool, window: &mut Window, cx: &mut Context<Self>) {
        fn panel_order(
            layers: &[Layer],
            collapsed: &std::collections::HashSet<String>,
            ids: &mut Vec<String>,
        ) {
            for layer in layers.iter().rev() {
                ids.push(layer.id.clone());
                if !collapsed.contains(&layer.id) {
                    panel_order(&layer.children, collapsed, ids);
                }
            }
        }

        let mut ids = Vec::new();
        panel_order(
            &self.editor.document.layers,
            &self.collapsed_groups,
            &mut ids,
        );
        let Some(index) = ids.iter().position(|id| id == &self.editor.active_layer) else {
            self.status = "The active layer is not available in the layer panel".into();
            cx.notify();
            return;
        };
        let target = if above {
            index.checked_sub(1)
        } else {
            index.checked_add(1).filter(|next| *next < ids.len())
        };
        let Some(target) = target else {
            self.status = if above {
                "Already at the top visible layer"
            } else {
                "Already at the bottom visible layer"
            }
            .into();
            cx.notify();
            return;
        };
        let id = ids[target].clone();
        let label = self
            .editor
            .document
            .find_layer(&id)
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "Layer".into());
        self.select_layer_ids(vec![id]);
        self.paint_mask = false;
        self.status = format!("Selected layer: {label}");
        self.focus.focus(window, cx);
        cx.notify();
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{TestAppContext, VisualTestContext};

    fn editor_view(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
        cx.update(crate::init_test_theme);
        cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(64, 48));
            view.refresh(cx);
            view
        })
    }

    #[gpui_kit::test]
    fn brush_keys_keep_basic_and_advanced_settings_in_sync(cx: &mut TestAppContext) {
        let (view, cx) = editor_view(cx);
        view.update_in(cx, |view, window, cx| {
            view.editor.brush.size = 20.0;
            view.editor.brush.hardness = 0.5;
            view.editor.brush_dynamics = Some(omuse::brush_dynamics::Settings {
                size: 20.0,
                hardness: 0.5,
                ..Default::default()
            });

            assert!(view.handle_keyboard_command("brush-smaller", window, cx));
            assert_eq!(view.editor.brush.size, 16.0);
            assert_eq!(view.editor.brush_dynamics.as_ref().unwrap().size, 16.0);
            assert!(view.handle_keyboard_command("brush-harder", window, cx));
            assert!((view.editor.brush.hardness - 0.6).abs() < f32::EPSILON);
            assert!(
                (view.editor.brush_dynamics.as_ref().unwrap().hardness - 0.6).abs() < f32::EPSILON
            );
            assert!(view.handle_keyboard_command("brush-opacity-30", window, cx));
            assert!((view.editor.brush.opacity - 0.3).abs() < f32::EPSILON);

            view.editor.brush.size = MIN_BRUSH_SIZE;
            assert!(view.handle_keyboard_command("brush-smaller", window, cx));
            assert_eq!(view.editor.brush.size, MIN_BRUSH_SIZE);
            view.editor.brush.size = MAX_BRUSH_SIZE;
            assert!(view.handle_keyboard_command("brush-larger", window, cx));
            assert_eq!(view.editor.brush.size, MAX_BRUSH_SIZE);
            view.editor.brush.hardness = 0.0;
            assert!(view.handle_keyboard_command("brush-softer", window, cx));
            assert_eq!(view.editor.brush.hardness, 0.0);
        });
    }

    #[gpui_kit::test]
    fn brush_key_commits_a_pending_mask_stroke_without_extra_history(cx: &mut TestAppContext) {
        let (view, cx) = editor_view(cx);
        view.update_in(cx, |view, window, cx| {
            let id = view.editor.active_layer.clone();
            assert!(view.editor.add_mask(&id, true));
            view.paint_mask = true;
            view.editor.brush.size = 4.0;
            let depth = view.editor.undo_depth();

            assert!(
                view.editor
                    .begin_mask_stroke(8.0, 8.0, 1.0, PaintTool::Brush)
            );
            assert!(view.handle_keyboard_command("brush-larger", window, cx));
            assert!(view.paint_mask);
            assert_eq!(view.editor.brush.size, 5.0);
            assert_eq!(view.editor.undo_depth(), depth + 1);
        });
    }

    #[gpui_kit::test]
    fn large_nudge_preserves_rotated_flipped_placement_and_is_one_undo(cx: &mut TestAppContext) {
        let (view, cx) = editor_view(cx);
        view.update_in(cx, |view, window, cx| {
            let id = view.editor.active_layer.clone();
            let original = view.editor.layer_placement(&id).unwrap();
            assert!(view.editor.set_layer_placement(
                &id,
                omuse::editor::LayerPlacement {
                    rotation: 31.0,
                    flip_x: true,
                    flip_y: true,
                    ..original
                },
            ));
            let original = view.editor.layer_placement(&id).unwrap();
            let depth = view.editor.undo_depth();

            assert!(view.handle_keyboard_command("nudge-right-large", window, cx));
            assert_eq!(
                view.editor.layer_placement(&id),
                Some(omuse::editor::LayerPlacement {
                    x: original.x + 10.0,
                    ..original
                })
            );
            assert_eq!(view.editor.undo_depth(), depth + 1);
            assert!(view.editor.undo());
            assert_eq!(view.editor.layer_placement(&id), Some(original));

            assert!(view.editor.set_locked(&id, true));
            let locked = view.editor.layer_placement(&id).unwrap();
            let depth = view.editor.undo_depth();
            assert!(view.handle_keyboard_command("nudge-left-large", window, cx));
            assert_eq!(view.editor.layer_placement(&id), Some(locked));
            assert_eq!(view.editor.undo_depth(), depth);
        });
    }

    #[gpui_kit::test]
    fn layer_keys_follow_panel_order_and_reuse_reorder_transaction(cx: &mut TestAppContext) {
        let (view, cx) = editor_view(cx);
        view.update_in(cx, |view, window, cx| {
            let bottom = view.editor.active_layer.clone();
            let middle = view.editor.add_layer("Middle");
            let top = view.editor.add_layer("Top");
            view.select_layer_ids(vec![middle.clone()]);

            assert!(view.handle_keyboard_command("select-layer-above", window, cx));
            assert_eq!(view.editor.active_layer, top);
            assert!(view.handle_keyboard_command("select-layer-below", window, cx));
            assert_eq!(view.editor.active_layer, middle);

            let depth = view.editor.undo_depth();
            assert!(view.handle_keyboard_command("reorder-layer-up", window, cx));
            let order: Vec<_> = view
                .editor
                .document
                .layers
                .iter()
                .map(|layer| layer.id.as_str())
                .collect();
            assert_eq!(order, [bottom.as_str(), top.as_str(), middle.as_str()]);
            assert_eq!(view.editor.undo_depth(), depth + 1);
            assert_eq!(view.selected_layer_ids(), vec![middle]);
        });
    }

    #[gpui_kit::test]
    fn hand_tool_finishes_paint_and_does_not_interrupt_inline_text(cx: &mut TestAppContext) {
        let (view, cx) = editor_view(cx);
        view.update_in(cx, |view, window, cx| {
            view.editor.brush.size = 4.0;
            let depth = view.editor.undo_depth();
            assert!(view.editor.begin_stroke(8.0, 8.0, 1.0, PaintTool::Brush));
            assert!(view.handle_keyboard_command("tool-hand", window, cx));
            assert_eq!(view.tool, Tool::Hand);
            assert_eq!(view.editor.undo_depth(), depth + 1);

            view.begin_inline_text(None, (4.0, 4.0), None, window, cx);
            assert_eq!(view.tool, Tool::Text);
            assert!(view.handle_keyboard_command("tool-hand", window, cx));
            assert_eq!(view.tool, Tool::Text);
            assert!(view.inline_text.is_some());
        });
    }

    #[gpui_kit::test]
    fn forward_delete_and_shifted_plus_dispatch_to_canonical_commands(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        let (view, cx) = editor_view(cx);
        cx.simulate_resize(size(px(800.0), px(600.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let depth = view.update_in(cx, |view, window, cx| {
            view.editor.add_layer("Delete me");
            view.focus.focus(window, cx);
            view.editor.undo_depth()
        });
        cx.simulate_keystrokes("delete");
        view.update(cx, |view, _| {
            assert_eq!(view.editor.document.layers.len(), 1);
            assert_eq!(view.editor.undo_depth(), depth + 1);
        });
        view.update(cx, |view, _| {
            view.zoom = 1.0;
        });
        cx.simulate_keystrokes("ctrl-+");
        view.update(cx, |view, _| assert_eq!(view.zoom, 1.2));
    }

    #[gpui_kit::test]
    fn photo_adjustment_shortcuts_open_drafts_and_escape_preserves_artwork(
        cx: &mut TestAppContext,
    ) {
        cx.update(bind_keys);
        let (view, cx) = editor_view(cx);
        cx.simulate_resize(size(px(800.0), px(600.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.update_in(cx, |view, window, cx| {
            view.editor.document.layers[0].image =
                Some(image::RgbaImage::from_pixel(64, 48, image::Rgba([40, 90, 180, 255])).into());
            view.refresh(cx);
            view.focus.focus(window, cx);
        });

        for (shortcut, expected_kind) in
            [("ctrl-l", 1), ("ctrl-m", 2), ("ctrl-u", 3), ("ctrl-b", 4)]
        {
            let (before, depth) = view.update(cx, |view, _| {
                (
                    raster::composite(&view.editor.document),
                    view.editor.undo_depth(),
                )
            });
            cx.simulate_keystrokes(shortcut);
            view.update(cx, |view, _| {
                assert_eq!(view.dialog, Dialog::Filter);
                assert_eq!(view.filter_kind, expected_kind);
                assert_eq!(raster::composite(&view.editor.document), before);
                assert_eq!(view.editor.undo_depth(), depth);
            });
            cx.simulate_keystrokes("escape");
            view.update(cx, |view, _| {
                assert_eq!(view.dialog, Dialog::None);
                assert_eq!(raster::composite(&view.editor.document), before);
                assert_eq!(view.editor.undo_depth(), depth);
            });
        }
    }
}
