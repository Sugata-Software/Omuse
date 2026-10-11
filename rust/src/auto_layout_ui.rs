//! Retained layout controls on the existing canvas; no secondary workspace.
use super::*;
use anyhow::{Context as _, Result, ensure};
use omuse::auto_layout::{self as layout, Align, Flow, FrameRule, FrameSize, ItemSize};
use std::sync::atomic::AtomicBool;

#[derive(Clone)]
enum Target {
    Create(Vec<String>),
    Frame(String),
    Item(String),
}
pub(super) struct AutoLayoutDraft {
    target: Target,
    instance: u64,
    revision: u64,
    selection_revision: u64,
    epoch: u64,
    selected: Vec<String>,
    message: String,
}

impl EditorView {
    pub(super) fn auto_layout_key(
        &mut self,
        key: &gpui_kit::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.dialog != Dialog::AutoLayout
            || !self.modal_focus.contains_focused(window, cx)
            || key.modifiers.control
            || key.modifiers.shift
            || key.modifiers.alt
            || key.modifiers.platform
        {
            return false;
        }
        match key.key.as_str() {
            "escape" => self.close_auto_layout(window, cx),
            "enter" => self.apply_auto_layout(window, cx),
            _ => return false,
        }
        true
    }
    fn close_auto_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.auto_layout_draft = None;
        self.dialog = Dialog::None;
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn open_auto_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = (|| -> Result<_> {
            let ids = self.selected_layer_ids();
            ensure!(!ids.is_empty(), "Select layers or a folder for auto-layout");
            let mut rule = FrameRule::default();
            let mut item = None;
            let target = if ids.len() == 1 {
                let layer = self
                    .editor
                    .document
                    .find_layer(&ids[0])
                    .context("Selected layer disappeared")?;
                if layer.is_group() {
                    rule = layout::frame_rule(layer)?.unwrap_or_default();
                    Target::Frame(layer.id.clone())
                } else if let Some(value) = layout::item_rule(layer)? {
                    item = Some(value);
                    Target::Item(layer.id.clone())
                } else {
                    Target::Create(ids.clone())
                }
            } else {
                Target::Create(ids.clone())
            };
            let encode = |size: FrameSize| match size {
                FrameSize::Hug => ("hug", "320".to_string()),
                FrameSize::Fixed(n) => ("fixed", n.to_string()),
            };
            let (wm, w) = encode(rule.width);
            let (hm, h) = encode(rule.height);
            let mut values = vec![
                if rule.flow == Flow::Horizontal {
                    "horizontal".into()
                } else {
                    "vertical".into()
                },
                wm.into(),
                w,
                hm.into(),
                h,
                rule.gap.to_string(),
                rule.padding[0].to_string(),
                rule.padding[1].to_string(),
                rule.padding[2].to_string(),
                rule.padding[3].to_string(),
                if rule.wrap { "on".into() } else { "off".into() },
                match rule.align {
                    Align::Start => "start",
                    Align::Center => "center",
                    Align::End => "end",
                }
                .into(),
            ];
            if let Some(item) = item {
                for size in [item.width, item.height] {
                    let (mode, value) = match size {
                        ItemSize::Hug => ("hug", "100".into()),
                        ItemSize::Fill => ("fill", "100".into()),
                        ItemSize::Fixed(v) => ("fixed", v.to_string()),
                    };
                    values.push(mode.into());
                    values.push(value);
                }
            } else {
                values.extend(["hug", "100", "hug", "100"].map(str::to_owned));
            }
            Ok((target, ids, values))
        })();
        match result {
            Ok((target, selected, values)) => {
                for (i, value) in values.into_iter().enumerate() {
                    self.detail_inputs[i]
                        .update(cx, |input, cx| input.set_value(value, window, cx));
                }
                self.auto_layout_draft = Some(AutoLayoutDraft {
                    target,
                    instance: self.editor.instance_id(),
                    revision: self.editor.revision(),
                    selection_revision: self.editor.selection_revision(),
                    epoch: self.create.epoch,
                    selected,
                    message: String::new(),
                });
                self.dialog_generation = self.dialog_generation.wrapping_add(1);
                self.dialog = Dialog::AutoLayout;
                self.modal_focus.focus(window, cx);
                cx.notify();
            }
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
            }
        }
    }

    fn check_layout_draft(&self) -> Result<Target> {
        ensure!(
            !self.busy && self.dialog == Dialog::AutoLayout,
            "Auto-layout is no longer active"
        );
        let draft = self
            .auto_layout_draft
            .as_ref()
            .context("Reopen Auto-layout")?;
        ensure!(
            self.editor.instance_id() == draft.instance
                && self.editor.revision() == draft.revision
                && self.editor.selection_revision() == draft.selection_revision
                && self.create.epoch == draft.epoch
                && self.selected_layer_ids() == draft.selected,
            "Artwork or selection changed; reopen Auto-layout before applying"
        );
        Ok(draft.target.clone())
    }

    pub(super) fn apply_auto_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = (|| -> Result<bool> {
            let target = self.check_layout_draft()?;
            let value = |i: usize| self.detail_inputs[i].read(cx).value().to_string();
            let number = |i: usize| -> Result<f32> {
                let v = value(i)
                    .parse::<f32>()
                    .context("Enter a valid pixel size")?;
                ensure!(v.is_finite(), "Enter a finite pixel size");
                Ok(v)
            };
            let cancel = AtomicBool::new(false);
            if let Target::Item(id) = target {
                let size = |i: usize| -> Result<ItemSize> {
                    Ok(match value(i).as_str() {
                        "hug" => ItemSize::Hug,
                        "fill" => ItemSize::Fill,
                        "fixed" => ItemSize::Fixed(number(i + 1)?),
                        _ => anyhow::bail!("Choose an item size mode"),
                    })
                };
                return self
                    .editor
                    .set_layout_item(&id, size(12)?, size(14)?, &cancel);
            }
            let size = |i: usize| -> Result<FrameSize> {
                Ok(match value(i).as_str() {
                    "hug" => FrameSize::Hug,
                    "fixed" => FrameSize::Fixed(number(i + 1)?),
                    _ => anyhow::bail!("Choose a frame size mode"),
                })
            };
            let rule = FrameRule {
                version: 1,
                flow: match value(0).as_str() {
                    "horizontal" => Flow::Horizontal,
                    "vertical" => Flow::Vertical,
                    _ => anyhow::bail!("Choose a flow direction"),
                },
                width: size(1)?,
                height: size(3)?,
                gap: number(5)?,
                padding: [number(6)?, number(7)?, number(8)?, number(9)?],
                wrap: match value(10).as_str() {
                    "on" => true,
                    "off" => false,
                    _ => anyhow::bail!("Choose wrapping"),
                },
                align: match value(11).as_str() {
                    "start" => Align::Start,
                    "center" => Align::Center,
                    "end" => Align::End,
                    _ => anyhow::bail!("Choose alignment"),
                },
            };
            match target {
                Target::Frame(id) => self.editor.set_layout_frame(&id, rule, &cancel),
                Target::Create(ids) => {
                    let id = self.editor.create_layout_frame(&ids, rule, &cancel)?;
                    self.layer_selection.click(id, SelectionAction::Replace);
                    Ok(true)
                }
                Target::Item(_) => unreachable!(),
            }
        })();
        self.finish_layout_action(result, window, cx);
    }

    fn finish_layout_action(
        &mut self,
        result: Result<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(changed) => {
                self.auto_layout_draft = None;
                self.dialog = Dialog::None;
                self.dialog_generation = self.dialog_generation.wrapping_add(1);
                self.status = if changed {
                    "Retained layout updated · Undo restores the whole edit"
                } else {
                    "Layout is unchanged"
                }
                .into();
                if changed {
                    self.changed(cx);
                }
                self.focus.focus(window, cx);
                cx.notify();
            }
            Err(error) => {
                if let Some(draft) = &mut self.auto_layout_draft {
                    draft.message = error.to_string();
                }
                self.status = error.to_string();
                cx.notify();
            }
        }
    }

    fn layout_choices(
        &self,
        index: usize,
        choices: &[(&'static str, &'static str)],
        cx: &mut Context<Self>,
    ) -> gpui_kit::Div {
        let selected = self.detail_inputs[index].read(cx).value().to_string();
        let mut row = div().flex().flex_wrap().gap_1();
        for &(value, label) in choices {
            row = row.child(
                inspector_ui::panel_button(
                    SharedString::from(format!("layout-choice-{index}-{value}")),
                    label,
                    if selected == value {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Outline
                    },
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.detail_inputs[index]
                        .update(cx, |input, cx| input.set_value(value, window, cx));
                    cx.notify();
                })),
            );
        }
        row
    }
    fn layout_size_row(
        &self,
        label: &'static str,
        index: usize,
        member: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Div {
        let choices: &[(&str, &str)] = if member {
            &[
                ("hug", "Hug content"),
                ("fixed", "Fixed"),
                ("fill", "Fill frame"),
            ]
        } else {
            &[("hug", "Hug content"), ("fixed", "Fixed")]
        };
        let mut row = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(label)
            .child(self.layout_choices(index, choices, cx));
        if self.detail_inputs[index].read(cx).value().as_ref() == "fixed" {
            row = row.child(inspector_ui::panel_input(
                SharedString::from(format!("layout-size-{}", index + 1)),
                &self.detail_inputs[index + 1],
                window,
                cx,
            ));
        }
        row
    }
    pub(super) fn render_auto_layout(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.omarchy().clone();
        let member = self
            .auto_layout_draft
            .as_ref()
            .is_some_and(|d| matches!(d.target, Target::Item(_)));
        let release = self
            .auto_layout_draft
            .as_ref()
            .and_then(|d| match &d.target {
                Target::Frame(id) => Some(id.clone()),
                _ => None,
            })
            .filter(|id| {
                self.editor
                    .document
                    .find_layer(id)
                    .is_some_and(|l| l.metadata.get(layout::FRAME_KEY).is_some())
            });
        let message = self
            .auto_layout_draft
            .as_ref()
            .map_or("Reopen Auto-layout".into(), |d| d.message.clone());
        let mut body = div()
            .id("auto-layout-body")
            .min_h_0()
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_4();
        if member {
            body=body.child("Choose how this layer occupies its frame. Hug uses its source size; Fixed and Fill scale its placement without flattening.")
                .child(self.layout_size_row("Width",12,true,window,cx))
                .child(self.layout_size_row("Height",14,true,window,cx))
                .child(div().text_size(px(11.)).text_color(theme.secondary).child("Text remains live; Fill scales its existing text box. It does not introduce new text wrapping. Fill needs a fixed frame dimension and wrapping off."));
        } else {
            body=body.child(div().text_color(theme.secondary).child("Arrange mixed photo, text and vector layers. Rules stay with the folder and reflow when content, visibility or layer order changes."))
                .child("Flow").child(self.layout_choices(0,&[("horizontal","Horizontal"),("vertical","Vertical")],cx))
                .child(div().flex().gap_4()
                    .child(div().flex_1().min_w_0().child(self.layout_size_row("Frame width",1,false,window,cx)))
                    .child(div().flex_1().min_w_0().child(self.layout_size_row("Frame height",3,false,window,cx))))
                .child(div().child("Gap · px").child(inspector_ui::panel_input("layout-gap",&self.detail_inputs[5],window,cx)));
            let mut padding = div().flex().gap_2();
            for (i, label) in [(6, "Top"), (7, "Right"), (8, "Bottom"), (9, "Left")] {
                padding = padding.child(div().flex_1().min_w_0().child(label).child(
                    inspector_ui::panel_input(
                        SharedString::from(format!("layout-padding-{i}")),
                        &self.detail_inputs[i],
                        window,
                        cx,
                    ),
                ));
            }
            body=body.child("Padding · px").child(padding).child("Cross-axis alignment")
                .child(self.layout_choices(11,&[("start","Start"),("center","Center"),("end","End")],cx))
                .child("Wrapping").child(self.layout_choices(10,&[("off","Single line"),("on","Wrap")],cx))
                .child(div().text_size(px(11.)).text_color(theme.secondary).child("Wrap needs a fixed width for horizontal flow, or fixed height for vertical flow. Fixed frames do not clip overflow. Direct unrotated layers only; nested folders are not supported. Select a member later to set Hug, Fixed or Fill."));
        }
        if !message.is_empty() {
            body = body.child(div().text_color(theme.danger).child(message));
        }
        let mut footer = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .items_center()
            .pt_2()
            .border_t_1()
            .border_color(theme.divider());
        if let Some(id) = release {
            footer = footer.child(
                inspector_ui::panel_button(
                    "layout-release",
                    "Release rules",
                    ButtonVariant::Outline,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    let result = this.check_layout_draft().and_then(|_| {
                        this.editor
                            .remove_layout_frame(&id, &AtomicBool::new(false))
                    });
                    this.finish_layout_action(result, window, cx);
                })),
            );
        }
        footer = footer
            .child(div().flex_1())
            .child(
                inspector_ui::panel_button("layout-cancel", "Cancel", ButtonVariant::Outline, cx)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.close_auto_layout(window, cx)),
                    ),
            )
            .child(
                inspector_ui::panel_button(
                    "layout-apply",
                    "Apply layout",
                    ButtonVariant::Primary,
                    cx,
                )
                .on_click(cx.listener(|this, _, window, cx| this.apply_auto_layout(window, cx))),
            );
        let panel = div()
            .w(px(
                (f32::from(window.viewport_size().width) - 32.).clamp(280., 620.)
            ))
            .h(px(
                (f32::from(window.viewport_size().height) - 48.).clamp(240., 760.)
            ))
            .bg(theme.background)
            .border_1()
            .border_color(theme.divider())
            .rounded(control_radius())
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .text_size(px(12.))
            .text_color(theme.foreground)
            .child(div().text_size(px(20.)).child(if member {
                "Layout member"
            } else {
                "Auto-layout frame"
            }))
            .child(body)
            .child(footer);
        div()
            .id("modal-shield")
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .child(panel.focus_trap("modal-focus", &self.modal_focus))
            .into_any_element()
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Focusable, TestAppContext, VisualTestContext};

    fn setup(
        cx: &mut TestAppContext,
    ) -> (
        Entity<EditorView>,
        &mut VisualTestContext,
        tempfile::TempDir,
    ) {
        cx.update(|cx| {
            crate::init_test_theme(cx);
            install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx);
        });
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.dialog = Dialog::None;
            view.recovery = Recovery::at(temp.path().join("recovery"));
            let mut doc = Document::new(160, 120);
            doc.layers = vec![
                Layer::paint("Photo", 24, 18),
                Layer::paint("Detail", 16, 24),
            ];
            let ids = doc.layers.iter().map(|l| l.id.clone()).collect();
            view.editor = Editor::new(doc);
            view.select_layer_ids(ids);
            view.refresh(cx);
            view.focus.focus(window, cx);
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        draw(cx);
        (view, cx, temp)
    }
    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    #[gpui_kit::test]
    fn escape_from_numeric_input_discards_draft_without_mutation(cx: &mut TestAppContext) {
        let (view, cx, _temp) = setup(cx);
        view.update_in(cx, |v, w, cx| v.open_auto_layout(w, cx));
        draw(cx);
        view.update_in(cx, |v, w, cx| {
            v.detail_inputs[5].update(cx, |input, cx| input.set_value("123", w, cx));
            v.detail_inputs[5].read(cx).focus_handle(cx).focus(w, cx);
        });
        draw(cx);
        cx.simulate_keystrokes("escape");
        view.update_in(cx, |v, w, _| {
            assert_eq!(v.dialog, Dialog::None);
            assert!(v.auto_layout_draft.is_none());
            assert_eq!(v.editor.undo_depth(), 0);
            assert_eq!(v.editor.document.layers.len(), 2);
            assert!(!layout::has_layout(&v.editor.document.layers));
            assert!(v.focus.is_focused(w));
        });
    }
    #[gpui_kit::test]
    fn enter_applies_once_selects_frame_and_undo_restores_siblings(cx: &mut TestAppContext) {
        let (view, cx, _temp) = setup(cx);
        view.update_in(cx, |v, w, cx| v.open_auto_layout(w, cx));
        draw(cx);
        view.update_in(cx, |v, w, cx| {
            v.detail_inputs[5].read(cx).focus_handle(cx).focus(w, cx)
        });
        cx.simulate_keystrokes("enter");
        draw(cx);
        view.update_in(cx, |v, w, cx| {
            assert_eq!(v.dialog, Dialog::None);
            assert_eq!(v.editor.undo_depth(), 1);
            assert_eq!(v.editor.document.layers.len(), 1);
            assert_eq!(
                v.selected_layer_ids(),
                vec![v.editor.document.layers[0].id.clone()]
            );
            layout::validate_settled(&v.editor.document).unwrap();
            v.command("undo", w, cx);
            assert_eq!(v.editor.undo_depth(), 0);
            assert_eq!(v.editor.document.layers.len(), 2);
            assert!(!layout::has_layout(&v.editor.document.layers));
        });
    }
    #[gpui_kit::test]
    fn stale_editor_and_selection_cannot_apply_old_layout_draft(cx: &mut TestAppContext) {
        let (view, cx, _temp) = setup(cx);
        view.update_in(cx, |v, w, cx| {
            v.open_auto_layout(w, cx);
            let source = v.editor.document.clone();
            v.editor = Editor::new(source);
            v.apply_auto_layout(w, cx);
            assert_eq!(v.dialog, Dialog::AutoLayout);
            assert!(
                v.auto_layout_draft
                    .as_ref()
                    .unwrap()
                    .message
                    .contains("changed")
            );
            assert_eq!(v.editor.undo_depth(), 0);
            assert!(!layout::has_layout(&v.editor.document.layers));
            v.close_auto_layout(w, cx);
            v.open_auto_layout(w, cx);
            let id = v.editor.document.layers[0].id.clone();
            v.select_layer_ids(vec![id]);
            v.apply_auto_layout(w, cx);
            assert!(
                v.auto_layout_draft
                    .as_ref()
                    .unwrap()
                    .message
                    .contains("selection")
            );
            assert_eq!(v.editor.undo_depth(), 0);
        });
    }
    #[gpui_kit::test]
    fn invalid_fill_cycle_keeps_dialog_and_document_unchanged(cx: &mut TestAppContext) {
        let (view, cx, _temp) = setup(cx);
        view.update_in(cx, |v, w, cx| {
            v.open_auto_layout(w, cx);
            v.apply_auto_layout(w, cx);
            let id = v.editor.document.layers[0].children[0].id.clone();
            v.select_layer_ids(vec![id]);
            v.open_auto_layout(w, cx);
            v.detail_inputs[12].update(cx, |input, cx| input.set_value("fill", w, cx));
            let depth = v.editor.undo_depth();
            v.apply_auto_layout(w, cx);
            assert_eq!(v.dialog, Dialog::AutoLayout);
            assert!(
                v.auto_layout_draft
                    .as_ref()
                    .unwrap()
                    .message
                    .contains("cycle")
            );
            assert_eq!(v.editor.undo_depth(), depth);
            layout::validate_settled(&v.editor.document).unwrap();
        });
    }
}
