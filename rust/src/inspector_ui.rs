//! Shared inspector presentation, derived entirely from the active Omarchy theme.
//! These builders retain the native controls' focus, activation and disabled states.
use super::*;
use gpui_kit::{Div, ElementId, FontWeight};

pub(super) fn panel_width(window: &Window) -> Pixels {
    px(if f32::from(window.viewport_size().width) < 1000. {
        320.
    } else {
        360.
    })
}

pub(super) fn panel_button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    variant: ButtonVariant,
    cx: &App,
) -> gpui_omarchy::Button {
    let t = cx.omarchy();
    let primary = variant == ButtonVariant::Primary;
    gpui_omarchy::button(id, text, variant, cx)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .h(px(32.))
        .px_3()
        .py_0()
        .gap_2()
        .rounded(px(6.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::MEDIUM)
        .bg(if primary { t.accent } else { t.normal_fill() })
        .border_color(if primary {
            t.accent
        } else {
            t.foreground.opacity(0.16)
        })
        .when(primary, |button| button.text_color(t.on_accent))
        .hover(|s| {
            s.bg(if primary {
                t.accent.opacity(0.88)
            } else {
                t.hover_fill()
            })
            .border_color(t.accent)
        })
        .active(|s| {
            s.bg(if primary {
                t.accent.opacity(0.76)
            } else {
                t.pressed_fill()
            })
        })
        .focus_visible(|s| {
            s.border_color(t.accent)
                .bg(if primary { t.accent } else { t.hover_fill() })
        })
        .styles(|s| {
            s.selected(|s| {
                s.bg(if primary { t.accent } else { t.selected_fill() })
                    .border_color(t.accent.opacity(0.7))
            })
            .disabled(|s| s.opacity(0.42))
        })
}

pub(super) fn panel_input(
    id: impl Into<ElementId>,
    state: &Entity<InputState>,
    window: &Window,
    cx: &mut App,
) -> gpui_omarchy::Input {
    let inset = cx.omarchy().inset;
    gpui_omarchy::input(id, state, window, cx)
        .h(px(32.))
        .py_0()
        .rounded(px(6.))
        .bg(inset)
}

pub(super) fn panel_note(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .min_w_0()
        .flex_shrink_0()
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(cx.omarchy().secondary)
        .child(text.into())
}

pub(super) fn panel_section(title: impl Into<SharedString>, cx: &App) -> Div {
    let t = cx.omarchy();
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w_0()
        .gap_2()
        .p_3()
        .rounded(px(8.))
        .border_1()
        .border_color(t.divider())
        .bg(t.background)
        .child(
            div()
                .text_size(px(10.))
                .line_height(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.secondary)
                .child(title.into()),
        )
}

pub(super) fn panel_header(
    title: &'static str,
    subtitle: &'static str,
    icon: &'static str,
    cx: &App,
) -> Div {
    let t = cx.omarchy();
    div()
        .debug_selector(move || format!("inspector-heading-{title}"))
        .flex()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .gap_2()
        .px_3()
        .py_3()
        .border_b_1()
        .border_color(t.divider())
        .child(
            div()
                .flex()
                .items_center()
                .flex_1()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .size(px(32.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .bg(t.accent.opacity(0.1))
                        .text_color(t.accent)
                        .child(crate::studio_icons::glyph(icon).size(px(17.))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(15.))
                                .line_height(px(20.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.bright)
                                .text_ellipsis()
                                .child(title),
                        )
                        .child(panel_note(subtitle, cx).text_ellipsis()),
                ),
        )
}
