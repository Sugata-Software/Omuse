//! The editor shell: theme-derived surfaces, a compact tool dock and focused inspectors.
//! Artwork and editing commands remain owned by the editor, independently of presentation.
use super::*;
use crate::studio_icons::glyph;
use gpui_kit::{Div, FontWeight};
use gpui_omarchy::with_tooltip;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum InspectorTab {
    #[default]
    Layers,
    Develop,
    Selection,
    Canvas,
    Create,
    Assistant,
}

impl Tool {
    pub(super) fn studio_id(self) -> &'static str {
        match self {
            Self::Brush => "tool-brush",
            Self::Pencil => "tool-pencil",
            Self::Eraser => "tool-eraser",
            Self::Fill => "tool-fill",
            Self::Gradient => "tool-gradient",
            Self::Rectangle => "tool-rectangle",
            Self::Ellipse => "tool-ellipse",
            Self::Move => "tool-move",
            Self::Picker => "tool-picker",
            Self::Clone => "tool-clone",
            Self::Heal => "tool-heal",
            Self::SpotHeal => "tool-spot-heal",
            Self::Wand => "tool-wand",
            Self::Object => "tool-object",
            Self::ShapeRect => "tool-shape-rect",
            Self::ShapeEllipse => "tool-shape-ellipse",
            Self::Line => "tool-line",
            Self::Lasso => "tool-lasso",
            Self::Hand => "tool-hand",
            Self::BlurBrush => "tool-blur-brush",
            Self::Smudge => "tool-smudge",
            Self::Liquify => "tool-liquify",
            Self::Text => "tool-text",
        }
    }

    fn studio_icon(self) -> &'static str {
        match self {
            Self::Brush => "brush",
            Self::Pencil => "pencil",
            Self::Eraser => "eraser",
            Self::Fill => "paint-bucket",
            Self::Gradient => "blend",
            Self::Rectangle => "square-dashed",
            Self::Ellipse => "circle-dashed",
            Self::Move => "move",
            Self::Picker => "pipette",
            Self::Clone => "stamp",
            Self::Heal => "bandage",
            Self::SpotHeal => "sparkles",
            Self::Wand => "wand-sparkles",
            Self::Object => "scan",
            Self::ShapeRect => "square",
            Self::ShapeEllipse => "circle",
            Self::Line => "minus",
            Self::Lasso => "lasso",
            Self::Hand => "hand",
            Self::BlurBrush => "droplet",
            Self::Smudge => "waves",
            Self::Liquify => "swirl",
            Self::Text => "type",
        }
    }

    fn studio_label(self) -> &'static str {
        self.name().split("  ").next().unwrap_or(self.name())
    }

    fn uses_brush(self) -> bool {
        matches!(
            self,
            Self::Brush
                | Self::Pencil
                | Self::Eraser
                | Self::Clone
                | Self::Heal
                | Self::SpotHeal
                | Self::BlurBrush
                | Self::Smudge
                | Self::Liquify
        )
    }
}

fn rule(cx: &App) -> Div {
    div()
        .w(px(1.))
        .h(px(20.))
        .flex_shrink_0()
        .bg(cx.omarchy().divider())
}

fn section(label: &'static str, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .border_t_1()
        .border_color(cx.omarchy().divider())
        .child(
            div()
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.omarchy().secondary)
                .child(label),
        )
}

fn note(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(11.))
        .text_color(cx.omarchy().secondary)
        .child(text.into())
}

fn context_hint(text: &'static str, cx: &App) -> Div {
    note(text, cx)
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
}

pub(super) fn layer_identity(layer: &Layer, thumbnail: Option<Arc<RenderImage>>, cx: &App) -> Div {
    let t = cx.omarchy();
    let (icon, kind) = if layer.is_group() {
        ("folder-open", "Group")
    } else if layer.metadata.get("text").is_some_and(|v| !v.is_null()) {
        ("type", "Live text")
    } else if layer.metadata.get("shape").is_some_and(|v| !v.is_null()) {
        ("square", "Live shape")
    } else if layer
        .metadata
        .get("adjustment")
        .is_some_and(|v| !v.is_null())
    {
        ("sliders-horizontal", "Adjustment")
    } else {
        ("image", "Pixel layer")
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .size(px(30.))
                .flex_shrink_0()
                .rounded(px(3.))
                .border_1()
                .border_color(t.divider())
                .bg(t.inset)
                .text_color(t.secondary)
                .flex()
                .items_center()
                .justify_center()
                .child(match thumbnail {
                    Some(image) => gpui_kit::img(image).size(px(28.)).into_any_element(),
                    None => glyph(icon).size(px(16.)).into_any_element(),
                }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .whitespace_nowrap()
                        .text_size(px(11.))
                        .text_color(t.bright)
                        .child(layer.name.clone()),
                )
                .child(div().text_size(px(9.)).text_color(t.secondary).child(kind)),
        )
        .when(layer.locked, |row| {
            row.child(glyph("lock-keyhole").size(px(12.)).text_color(t.secondary))
        })
}

impl EditorView {
    fn studio_icon_action(
        &self,
        id: &'static str,
        label: &'static str,
        name: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui_omarchy::Button {
        let chord = self.shortcuts.chord(id);
        let tooltip = if chord.is_empty() {
            label.into()
        } else {
            format!("{label} · {chord}")
        };
        with_tooltip(
            self.control(id, "", cx)
                .accessibility_label(label)
                .size(px(30.))
                .p_0()
                .rounded(px(3.))
                .child(glyph(name)),
            tooltip,
        )
    }

    fn studio_action_grid(
        &self,
        actions: &[(&'static str, &'static str)],
        cx: &mut Context<Self>,
    ) -> Div {
        let mut grid = div().flex().flex_col().gap_1();
        for pair in actions.chunks(2) {
            let mut row = div().flex().gap_1();
            for &(id, label) in pair {
                row = row.child(
                    self.control(id, label, cx)
                        .flex_1()
                        .min_w_0()
                        .h(px(30.))
                        .px_2()
                        .py_0()
                        .rounded(px(3.))
                        .text_size(px(11.))
                        .bg(cx.omarchy().normal_fill())
                        .justify_start(),
                );
            }
            grid = grid.child(row);
        }
        grid
    }

    fn studio_toggle(
        &self,
        id: &'static str,
        label: &'static str,
        on: bool,
        cx: &mut Context<Self>,
    ) -> gpui_omarchy::Button {
        let t = cx.omarchy().clone();
        self.control(id, "", cx)
            .accessibility_label(label)
            .selected(on)
            .w_full()
            .h(px(30.))
            .px_2()
            .py_0()
            .rounded(px(3.))
            .justify_between()
            .text_size(px(11.))
            .child(label)
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(if on { t.accent } else { t.secondary })
                    .child(if on { "ON" } else { "OFF" }),
            )
    }

    pub(super) fn studio_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let document = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".into());
        div()
            .id("studio-header")
            .debug_selector(|| "studio-header".into())
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(48.))
            .px_3()
            .gap_2()
            .bg(t.surface)
            .border_b_1()
            .border_color(t.divider())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pr_2()
                    .text_color(t.bright)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        div()
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(t.accent)
                            .bg(t.accent.opacity(0.10))
                            .rounded(px(4.))
                            .child(glyph("omuse")),
                    )
                    .child("Omuse"),
            )
            .child(rule(cx))
            .child(self.studio_icon_action("new", "New document", "file-plus-2", cx))
            .child(self.studio_icon_action("open", "Open document", "folder-open", cx))
            .child(self.studio_icon_action("import", "Import image", "image", cx))
            .child(
                button("workspace-create", "Create", ButtonVariant::Secondary, cx)
                    .debug_selector(|| "workspace-create".into())
                    .selected(self.inspector_tab == InspectorTab::Create)
                    .h(px(30.))
                    .px_2()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.finish_interaction(cx);
                        this.inspector_tab = InspectorTab::Create;
                        this.inspector_visible = true;
                        cx.notify();
                    })),
            )
            .child(
                button(
                    "workspace-assistant",
                    "Ask Omuse",
                    ButtonVariant::Secondary,
                    cx,
                )
                .debug_selector(|| "workspace-assistant".into())
                .selected(self.inspector_tab == InspectorTab::Assistant)
                .h(px(30.))
                .px_2()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.finish_interaction(cx);
                    this.inspector_tab = InspectorTab::Assistant;
                    this.inspector_visible = true;
                    this.discover_ai_connections(cx);
                    cx.notify();
                })),
            )
            .child(rule(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .child(
                        div()
                            .size(px(5.))
                            .rounded_full()
                            .bg(if self.has_unsaved_work() {
                                t.warning
                            } else {
                                t.success
                            }),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(12.))
                            .child(document),
                    ),
            )
            .child(
                self.studio_icon_action("undo", "Undo", "undo-2", cx)
                    .disabled(!self.can_undo_or_collection()),
            )
            .child(
                self.studio_icon_action("redo", "Redo", "redo-2", cx)
                    .disabled(!self.can_redo_or_collection()),
            )
            .child(rule(cx))
            .child(self.studio_icon_action("save", "Save project", "save", cx))
            .child(self.studio_icon_action("save-as", "Save project as", "copy", cx))
            .child(
                button("export", "", ButtonVariant::Primary, cx)
                    .debug_selector(|| "export".into())
                    .accessibility_label("Export image")
                    .h(px(30.))
                    .px_3()
                    .py_0()
                    .rounded(px(3.))
                    .bg(t.accent)
                    .text_color(t.on_accent)
                    .hover(|s| {
                        s.bg(t.accent)
                            .border_color(t.bright)
                            .text_color(t.on_accent)
                    })
                    .focus_visible(|s| {
                        s.bg(t.accent)
                            .border_color(t.bright)
                            .text_color(t.on_accent)
                    })
                    .active(|s| s.bg(t.accent).text_color(t.on_accent))
                    .child(glyph("upload").size(px(14.)))
                    .child("Export")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.command("export", window, cx)),
                    ),
            )
            .child(with_tooltip(
                button("inspector-toggle", "", ButtonVariant::Secondary, cx)
                    .debug_selector(|| "inspector-toggle".into())
                    .accessibility_label(if self.inspector_visible {
                        "Hide inspector"
                    } else {
                        "Show inspector"
                    })
                    .size(px(30.))
                    .p_0()
                    .rounded(px(3.))
                    .child(glyph(if self.inspector_visible {
                        "panel-right-close"
                    } else {
                        "panel-right-open"
                    }))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.finish_interaction(cx);
                        this.inspector_visible = !this.inspector_visible;
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
                "Show or hide inspector",
            ))
            .into_any_element()
    }

    pub(super) fn studio_tools(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let groups: &[&[Tool]] = &[
            &[Tool::Move, Tool::Hand],
            &[
                Tool::Rectangle,
                Tool::Ellipse,
                Tool::Lasso,
                Tool::Wand,
                Tool::Object,
                Tool::Picker,
            ],
            &[
                Tool::Brush,
                Tool::Pencil,
                Tool::Eraser,
                Tool::Fill,
                Tool::Gradient,
                Tool::BlurBrush,
            ],
            &[
                Tool::Clone,
                Tool::Heal,
                Tool::SpotHeal,
                Tool::Smudge,
                Tool::Liquify,
            ],
            &[Tool::ShapeRect, Tool::ShapeEllipse, Tool::Line, Tool::Text],
        ];
        let mut dock = div()
            .id("tools")
            .debug_selector(|| "tools".into())
            .w(px(88.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_r_1()
            .border_color(t.divider());
        for (index, group) in groups.iter().enumerate() {
            if index > 0 {
                dock = dock.child(
                    div()
                        .h(px(1.))
                        .flex_shrink_0()
                        .my(px(6.))
                        .mx_1()
                        .bg(t.divider()),
                );
            }
            let mut grid = div().flex().flex_col().gap(px(2.));
            for pair in group.chunks(2) {
                let mut row = div().flex().gap(px(4.));
                for &tool in pair {
                    let selected = self.tool == tool;
                    let key = match tool {
                        Tool::Text => "text",
                        _ => tool.studio_id(),
                    };
                    let chord = if tool == Tool::Hand {
                        "Space"
                    } else {
                        self.shortcuts.chord(key)
                    };
                    let label = if chord.is_empty() {
                        tool.studio_label().into()
                    } else {
                        format!("{} · {chord}", tool.studio_label())
                    };
                    row = row.child(with_tooltip(
                        button(tool.studio_id(), "", ButtonVariant::Secondary, cx)
                            .debug_selector(move || tool.studio_id().into())
                            .accessibility_label(tool.studio_label()).selected(selected)
                            .size(px(32.)).p_0().rounded(px(3.))
                            .border_color(if selected { t.accent } else { t.foreground.opacity(0.) })
                            .bg(if selected { t.accent.opacity(0.12) } else { t.surface })
                            .text_color(if selected { t.accent } else { t.foreground })
                            .child(glyph(tool.studio_icon()))
                            .disabled(self.busy || self.inline_text.is_some())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.editor.floating_selection_layer().is_some() {
                                    this.status = "Commit Selection or Cancel Selection before changing tools".into();
                                } else {
                                    this.finish_interaction(cx);
                                    this.tool = tool;
                                }
                                this.focus.focus(window, cx);
                                cx.notify();
                            })), label));
                }
                grid = grid.child(row);
            }
            dock = dock.child(grid);
        }
        dock.into_any_element()
    }

    pub(super) fn studio_context(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut bar = div()
            .id("studio-context")
            .debug_selector(|| "studio-context".into())
            .flex()
            .items_center()
            .gap(px(6.))
            .px_3()
            .h(px(44.))
            .flex_shrink_0()
            .overflow_x_scroll()
            .bg(t.background)
            .border_b_1()
            .border_color(t.divider())
            .child(
                div()
                    .min_w(px(104.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .text_color(t.bright)
                    .text_size(px(12.))
                    .child(glyph(self.tool.studio_icon()).size(px(16.)))
                    .child(self.tool.studio_label()),
            )
            .child(rule(cx));
        if self.tool == Tool::SpotHeal {
            bar = bar
                .child(note(format!("Size {:.0} px", self.editor.brush.size), cx))
                .child(
                    button(
                        "spot-healing-mode",
                        format!(
                            "Mode: {}",
                            match self.spot_healing_mode {
                                omuse::spot_heal::SpotHealingMode::ContentAware => "Content aware",
                                omuse::spot_heal::SpotHealingMode::CreateTexture =>
                                    "Create texture",
                                omuse::spot_heal::SpotHealingMode::ProximityMatch =>
                                    "Proximity match",
                            }
                        ),
                        ButtonVariant::Outline,
                        cx,
                    )
                    .debug_selector(|| "spot-healing-mode".into())
                    .h(px(28.))
                    .py_0()
                    .rounded(px(3.))
                    .on_click(cx.listener(|this, _, window, cx| {
                        use omuse::spot_heal::SpotHealingMode::*;
                        this.spot_healing_mode = match this.spot_healing_mode {
                            ContentAware => CreateTexture,
                            CreateTexture => ProximityMatch,
                            ProximityMatch => ContentAware,
                        };
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
                );
        } else if self.tool.uses_brush() {
            bar = bar
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .flex_shrink_0()
                        .child(note("Size", cx))
                        .child(
                            button("smaller", "−", ButtonVariant::Secondary, cx)
                                .size(px(26.))
                                .p_0()
                                .accessibility_label("Decrease brush size")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.editor.brush.size =
                                        (this.editor.brush.size / 1.25).max(1.);
                                    this.focus.focus(window, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .min_w(px(42.))
                                .text_center()
                                .text_size(px(11.))
                                .font_family(t.mono_font.clone())
                                .child(format!("{:.0} px", self.editor.brush.size)),
                        )
                        .child(
                            button("larger", "+", ButtonVariant::Secondary, cx)
                                .size(px(26.))
                                .p_0()
                                .accessibility_label("Increase brush size")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.editor.brush.size =
                                        (this.editor.brush.size * 1.25).min(1024.);
                                    this.focus.focus(window, cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(rule(cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .flex_shrink_0()
                        .child(note("Opacity", cx))
                        .child(
                            button("less-opacity", "−", ButtonVariant::Secondary, cx)
                                .size(px(26.))
                                .p_0()
                                .accessibility_label("Decrease brush opacity")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.editor.brush.opacity =
                                        (this.editor.brush.opacity - 0.1).max(0.05);
                                    this.focus.focus(window, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .min_w(px(34.))
                                .text_center()
                                .text_size(px(11.))
                                .font_family(t.mono_font)
                                .child(format!("{:.0}%", self.editor.brush.opacity * 100.)),
                        )
                        .child(
                            button("more-opacity", "+", ButtonVariant::Secondary, cx)
                                .size(px(26.))
                                .p_0()
                                .accessibility_label("Increase brush opacity")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.editor.brush.opacity =
                                        (this.editor.brush.opacity + 0.1).min(1.);
                                    this.focus.focus(window, cx);
                                    cx.notify();
                                })),
                        ),
                );
        } else if self.tool == Tool::Move {
            bar = bar
                .child(
                    self.control("transform", "Transform", cx)
                        .debug_selector(|| "context-transform".into()),
                )
                .child(context_hint(
                    "Drag to move · Ctrl-drag handles to distort",
                    cx,
                ));
        } else if matches!(
            self.tool,
            Tool::Rectangle | Tool::Ellipse | Tool::Lasso | Tool::Wand | Tool::Object
        ) {
            bar = bar
                .child(
                    self.control("deselect", "Deselect", cx)
                        .debug_selector(|| "context-deselect".into()),
                )
                .child(context_hint("Shift adds · Alt subtracts", cx));
        } else if self.tool == Tool::Text {
            bar = bar
                .child(
                    self.control("edit-object", "Text properties", cx)
                        .debug_selector(|| "context-edit-object".into()),
                )
                .child(context_hint("Click the canvas to start typing", cx));
        } else {
            bar = bar.child(context_hint(
                match self.tool {
                    Tool::Hand => "Drag to pan · Scroll to zoom",
                    Tool::Gradient => "Drag on the canvas to preview a gradient",
                    Tool::Picker => "Click the canvas to sample a colour",
                    Tool::Fill => "Click to fill connected pixels",
                    _ => "Drag on the canvas to draw a shape",
                },
                cx,
            ));
        }
        bar.child(div().flex_1().min_w(px(8.)))
            .child(self.studio_icon_action(
                "tool-settings",
                "Tool settings",
                "sliders-horizontal",
                cx,
            ))
            .child(rule(cx))
            .child(color_picker("brush-color", &self.color, window, cx))
            .child(self.studio_icon_action(
                "swap-colors",
                "Swap foreground and background",
                "rotate-cw",
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn studio_inspector(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        if self.inspector_tab != InspectorTab::Layers || !self.inspector_visible {
            self.layer_thumbnails
                .borrow_mut()
                .retain(&Default::default(), window, cx);
        }
        if !self.inspector_visible {
            return div().into_any_element();
        }
        if self.inspector_tab == InspectorTab::Create {
            return self.create_inspector(window, cx);
        }
        if self.inspector_tab == InspectorTab::Assistant {
            return self.ai_inspector(window, cx);
        }
        let mut tabs = div()
            .flex()
            .h(px(40.))
            .flex_shrink_0()
            .border_b_1()
            .border_color(t.divider());
        for (tab, id, label) in [
            (InspectorTab::Layers, "inspector-tab-layers", "Layers"),
            (InspectorTab::Develop, "inspector-tab-develop", "Develop"),
            (InspectorTab::Selection, "inspector-tab-selection", "Select"),
            (InspectorTab::Canvas, "inspector-tab-canvas", "Canvas"),
        ] {
            let active = self.inspector_tab == tab;
            tabs = tabs.child(
                button(id, label, ButtonVariant::Secondary, cx)
                    .debug_selector(move || id.into())
                    .selected(active)
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .p_0()
                    .border_0()
                    .border_b_2()
                    .border_color(if active {
                        t.accent
                    } else {
                        t.foreground.opacity(0.)
                    })
                    .text_size(px(11.))
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if active { t.bright } else { t.secondary })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.finish_interaction(cx);
                        this.inspector_tab = tab;
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
            );
        }
        let mut body = div()
            .id("inspector-content")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        match self.inspector_tab {
            InspectorTab::Create | InspectorTab::Assistant => unreachable!(),
            InspectorTab::Layers => {
                fn rows<'a>(
                    list: &'a [Layer],
                    depth: usize,
                    collapsed: &std::collections::HashSet<String>,
                    out: &mut Vec<(&'a Layer, usize)>,
                ) {
                    for layer in list.iter().rev() {
                        out.push((layer, depth));
                        if !collapsed.contains(&layer.id) {
                            rows(&layer.children, depth + 1, collapsed, out);
                        }
                    }
                }
                let mut list = Vec::new();
                rows(
                    &self.editor.document.layers,
                    0,
                    &self.collapsed_groups,
                    &mut list,
                );
                let thumbnail_ids = list
                    .iter()
                    .filter(|(layer, _)| layer.image.is_some())
                    .map(|(layer, _)| layer.id.clone())
                    .collect();
                self.layer_thumbnails
                    .borrow_mut()
                    .retain(&thumbnail_ids, window, cx);
                fn count_layers(layers: &[Layer]) -> usize {
                    layers
                        .iter()
                        .map(|layer| 1 + count_layers(&layer.children))
                        .sum()
                }
                let count = count_layers(&self.editor.document.layers);
                let mut layers = div()
                    .id("layers")
                    .debug_selector(|| "layers".into())
                    .h(px(
                        (f32::from(window.viewport_size().height) * 0.28).clamp(120., 280.)
                    ))
                    .min_h(px(100.))
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .p_1();
                for (layer, depth) in list {
                    layers = layers.child(self.layer_row(layer, depth, window, cx));
                }
                let selected = self.editor.document.find_layer(&self.editor.active_layer);
                let opacity = selected.map_or(1., |l| l.opacity);
                let blend = selected.map(|l| l.blend_mode.as_str()).unwrap_or("Normal");
                body = body
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_1()
                            .child(note(format!("{count} layers"), cx))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(self.studio_icon_action("add", "New layer", "plus", cx))
                                    .child(self.studio_icon_action(
                                        "group",
                                        "Group selected layers",
                                        "folder-open",
                                        cx,
                                    )),
                            ),
                    )
                    .child(layers)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_2()
                            .py_1()
                            .border_t_1()
                            .border_color(t.divider())
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(self.studio_icon_action(
                                        "duplicate",
                                        "Duplicate layers",
                                        "copy",
                                        cx,
                                    ))
                                    .child(self.studio_icon_action(
                                        "add-mask",
                                        "Add layer mask",
                                        "scan",
                                        cx,
                                    ))
                                    .child(self.studio_icon_action(
                                        "lock",
                                        "Toggle layer lock",
                                        "lock-keyhole",
                                        cx,
                                    ))
                                    .child(self.studio_icon_action(
                                        "effects",
                                        "Live layer effects",
                                        "sparkles",
                                        cx,
                                    )),
                            )
                            .child(self.studio_icon_action(
                                "delete",
                                "Delete selected layers",
                                "trash-2",
                                cx,
                            )),
                    )
                    .child(
                        section("COMPOSITING", cx)
                            .child(
                                self.control("blend", format!("{blend}   ›"), cx)
                                    .w_full()
                                    .justify_between()
                                    .bg(t.normal_fill())
                                    .rounded(px(3.)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(note("Opacity", cx))
                                    .child(div().flex_1())
                                    .child(
                                        button(
                                            "layer-opacity-less",
                                            "−",
                                            ButtonVariant::Secondary,
                                            cx,
                                        )
                                        .accessibility_label("Decrease layer opacity")
                                        .size(px(26.))
                                        .p_0()
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                let id = this.editor.active_layer.clone();
                                                this.editor
                                                    .set_opacity(&id, (opacity - 0.1).max(0.));
                                                this.focus.focus(window, cx);
                                                this.changed(cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .min_w(px(40.))
                                            .text_center()
                                            .font_family(t.mono_font.clone())
                                            .text_size(px(11.))
                                            .child(format!("{:.0}%", opacity * 100.)),
                                    )
                                    .child(
                                        button(
                                            "layer-opacity-more",
                                            "+",
                                            ButtonVariant::Secondary,
                                            cx,
                                        )
                                        .accessibility_label("Increase layer opacity")
                                        .size(px(26.))
                                        .p_0()
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                let id = this.editor.active_layer.clone();
                                                this.editor
                                                    .set_opacity(&id, (opacity + 0.1).min(1.));
                                                this.focus.focus(window, cx);
                                                this.changed(cx);
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(section("LAYER", cx).child(self.studio_action_grid(
                        &[
                            ("rename", "Rename"),
                            ("edit-object", "Edit object"),
                            ("up", "Move up"),
                            ("down", "Move down"),
                            ("nest", "Move into group"),
                            ("unnest", "Move out"),
                            ("merge", "Merge down"),
                            ("flatten", "Flatten"),
                            ("rasterize", "Rasterize"),
                            ("clipping", "Toggle clipping"),
                        ],
                        cx,
                    )))
                    .child(
                        section("MASK", cx)
                            .child(self.studio_toggle(
                                "mask-paint",
                                "Paint on mask",
                                self.paint_mask,
                                cx,
                            ))
                            .child(self.studio_action_grid(
                                &[
                                    ("invert-mask", "Invert"),
                                    ("apply-mask", "Apply mask"),
                                    ("mask-enable", "Enable / disable"),
                                    ("mask-link", "Link / unlink"),
                                    ("mask-transform", "Place mask"),
                                    ("live-mask", "Live source"),
                                    ("remove-mask", "Delete mask"),
                                ],
                                cx,
                            )),
                    )
                    .child(
                        section("TRANSFORM", cx)
                            .child(self.studio_action_grid(
                                &[
                                    ("transform", "Transform"),
                                    ("distort", "Distort corners"),
                                    ("rotate", "Rotate 90°"),
                                    ("flip", "Flip"),
                                    ("sampling", "Sampling quality"),
                                ],
                                cx,
                            ))
                            .child(note("Ctrl-drag a transform handle to distort.", cx)),
                    );
            }
            InspectorTab::Develop => {
                body = body
                    .child(
                        section("EDITABLE WORKFLOWS", cx).child(self.studio_action_grid(
                            &[
                                ("filter-stack", "Filter stack"),
                                ("blend-if", "Blend If"),
                                ("smart-source", "Smart source"),
                                ("editable-raw", "Embedded RAW"),
                                ("colour-management", "Precision & colour"),
                                ("advanced-retouch", "Advanced retouch"),
                                ("editable-warp", "Mesh & pin warp"),
                                ("brush-studio", "Brush studio"),
                            ],
                            cx,
                        )),
                    )
                    .child(
                        section("PATHS & AUTOMATION", cx).child(self.studio_action_grid(
                            &[
                                ("vector-path", "Vector paths"),
                                ("vector-mask", "Vector mask"),
                                ("automation", "Recipes & batch"),
                                ("multi-image", "Multi-image merge"),
                            ],
                            cx,
                        )),
                    )
                    .child(
                        section("DEVELOP", cx)
                            .child(note("Shape light, colour and detail.", cx))
                            .child(self.studio_action_grid(
                                &[("camera-raw", "Camera Raw"), ("filter", "Pixel filters")],
                                cx,
                            )),
                    )
                    .child(
                        section("EDITABLE ADJUSTMENTS", cx)
                            .child(self.studio_action_grid(
                                &[
                                    ("live-adjustment", "Add adjustment"),
                                    ("edit-adjustment", "Edit adjustment"),
                                ],
                                cx,
                            ))
                            .child(note("Adjustment layers stay editable in your project.", cx)),
                    )
                    .child(section("LAYER EFFECTS", cx).child(self.studio_action_grid(
                        &[
                            ("effects", "Live effects"),
                            ("clear-effects", "Clear effects"),
                        ],
                        cx,
                    )))
                    .child(
                        section("QUICK PIXEL ADJUSTMENTS", cx)
                            .child(self.studio_action_grid(
                                &[
                                    ("brighter", "Lighten"),
                                    ("darker", "Darken"),
                                    ("contrast", "Contrast"),
                                    ("saturation", "Saturate"),
                                    ("gray", "Grayscale"),
                                    ("invert", "Invert"),
                                    ("blur", "Blur"),
                                    ("sharpen", "Sharpen"),
                                ],
                                cx,
                            ))
                            .child(note("These change the selected layer's pixels.", cx)),
                    );
            }
            InspectorTab::Selection => {
                body = body
                    .child(
                        section("REFINE & REPAIR", cx).child(self.studio_action_grid(
                            &[
                                ("refine-workspace", "Refinement workspace"),
                                ("controlled-removal", "Controlled removal"),
                            ],
                            cx,
                        )),
                    )
                    .child(
                        section("COLOUR & TONE", cx)
                            .child(self.studio_action_grid(
                                &[
                                    ("luminosity-range", "Luminosity range"),
                                    ("color-range", "Colour range"),
                                ],
                                cx,
                            ))
                            .child(note("Build soft selections and layer masks.", cx)),
                    )
                    .child(
                        section("SUBJECT & BACKGROUND", cx).child(self.studio_action_grid(
                            &[
                                ("select-subject", "Select subject"),
                                ("remove-background", "Remove background"),
                            ],
                            cx,
                        )),
                    )
                    .child(section("SELECTION", cx).child(self.studio_action_grid(
                        &[
                            ("select-all", "Select all"),
                            ("deselect", "Deselect"),
                            ("invert-selection", "Invert selection"),
                            ("feather-selection", "Feather"),
                            ("grow-selection", "Expand"),
                            ("shrink-selection", "Contract"),
                        ],
                        cx,
                    )))
                    .child(
                        section("EDIT SELECTED PIXELS", cx).child(self.studio_action_grid(
                            &[
                                ("content-fill", "Content-aware fill"),
                                ("crop", "Crop to selection"),
                                ("transform-selection", "Transform selection"),
                            ],
                            cx,
                        )),
                    )
                    .child(
                        section("FLOATING SELECTION", cx)
                            .child(self.studio_action_grid(
                                &[
                                    ("commit-selection", "Commit"),
                                    ("cancel-selection", "Cancel"),
                                ],
                                cx,
                            ))
                            .child(note("Enter commits · Escape cancels", cx)),
                    );
            }
            InspectorTab::Canvas => {
                body = body
                    .child(
                        section("DOCUMENT", cx)
                            .child(div().text_size(px(20.)).text_color(t.bright).child(format!(
                                "{} × {}",
                                self.editor.document.width, self.editor.document.height
                            )))
                            .child(note("Pixels · RGB / 8-bit · sRGB", cx))
                            .child(self.studio_action_grid(
                                &[
                                    ("resize", "Canvas size"),
                                    ("resize-image", "Image size"),
                                    ("trim", "Trim canvas"),
                                    ("import-report", "Import report"),
                                ],
                                cx,
                            )),
                    )
                    .child(
                        section("VIEW & ALIGNMENT", cx)
                            .child(self.studio_toggle("grid", "Pixel grid", self.show_grid, cx))
                            .child(self.studio_toggle("guides", "Guides", self.show_guides, cx))
                            .child(self.studio_toggle(
                                "rulers",
                                "Rulers",
                                self.preferences.rulers,
                                cx,
                            ))
                            .child(self.studio_toggle(
                                "snapping",
                                "Snapping",
                                self.preferences.snapping,
                                cx,
                            ))
                            .child(self.studio_toggle(
                                "auto-select",
                                "Auto-select layers",
                                self.preferences.auto_select,
                                cx,
                            ))
                            .child(self.studio_toggle(
                                "transform-box",
                                "Transform handles",
                                self.preferences.transform_box,
                                cx,
                            ))
                            .child(self.control("add-guide", "Manage guides", cx).w_full()),
                    )
                    .child(
                        section("COLOURS", cx)
                            .child(note("Background", cx))
                            .child(color_picker(
                                "background-color",
                                &self.background,
                                window,
                                cx,
                            ))
                            .child(self.control("default-colors", "Reset to black & white", cx)),
                    );
            }
        }
        div()
            .id("inspector")
            .debug_selector(|| "inspector".into())
            .w(px(292.))
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_l_1()
            .border_color(t.divider())
            .child(tabs)
            .child(body)
            .into_any_element()
    }

    pub(super) fn studio_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        div()
            .id("studio-footer")
            .debug_selector(|| "studio-footer".into())
            .flex()
            .items_center()
            .h(px(30.))
            .flex_shrink_0()
            .px_3()
            .gap_2()
            .bg(t.surface)
            .border_t_1()
            .border_color(t.divider())
            .text_size(px(10.))
            .child(
                self.studio_icon_action("shortcuts", "Keyboard shortcuts", "keyboard", cx)
                    .size(px(24.)),
            )
            .child(div().size(px(4.)).rounded_full().bg(if self.busy {
                t.warning
            } else {
                t.accent
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(t.secondary)
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(t.mono_font.clone())
                    .child(format!(
                        "{} × {}",
                        self.editor.document.width, self.editor.document.height
                    )),
            )
            .child(rule(cx))
            .child(
                self.studio_icon_action("zoom-out", "Zoom out", "minus", cx)
                    .size(px(24.)),
            )
            .child(
                self.control("actual", format!("{:.0}%", self.zoom * 100.), cx)
                    .h(px(24.))
                    .min_w(px(48.))
                    .py_0()
                    .px_1()
                    .text_size(px(10.))
                    .font_family(t.mono_font),
            )
            .child(
                self.studio_icon_action("zoom-in", "Zoom in", "plus", cx)
                    .size(px(24.)),
            )
            .child(
                self.studio_icon_action("fit", "Fit to window", "maximize", cx)
                    .size(px(24.)),
            )
            .into_any_element()
    }
}
