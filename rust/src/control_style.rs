//! Omuse control geometry, shared by every workspace and dialog.
//! Omarchy still supplies colours, fonts, focus and disabled-state behaviour.
use gpui_kit::base::{
    ColorPicker, ColorPickerState, InputBase, Popup,
    input::{InputState, TextareaState},
};
use gpui_kit::{
    App, ElementId, Entity, Focusable, Pixels, SharedString, Window, div, prelude::*, px, rems,
};
use gpui_omarchy::{ActiveTheme, ButtonVariant, popover_surface, slider};

/// Match the tighter corners of the studio toolbar at every display scale.
pub(super) fn control_radius() -> Pixels {
    px(3.)
}

pub(super) fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    variant: ButtonVariant,
    cx: &App,
) -> gpui_omarchy::Button {
    gpui_omarchy::button(id, label, variant, cx).rounded(control_radius())
}

pub(super) fn input(
    id: impl Into<ElementId>,
    state: &Entity<InputState>,
    window: &Window,
    cx: &mut App,
) -> gpui_omarchy::Input {
    gpui_omarchy::input(id, state, window, cx).rounded(control_radius())
}

pub(super) fn textarea(
    id: impl Into<ElementId>,
    state: &Entity<TextareaState>,
    window: &Window,
    cx: &mut App,
) -> InputBase {
    gpui_omarchy::textarea(id, state, window, cx).rounded(control_radius())
}

// The picker presentation below is adapted from gpui-omarchy 3625c6c,
// Copyright (c) 2026 Jason Lee (huacnlee), MIT.
// See ../licenses/GPUI-OMARCHY-MIT.txt. Keep its editing/focus/cancel behaviour
// aligned with the pinned upstream; only constructors use Omuse geometry.
pub(super) fn color_picker(
    id: impl Into<ElementId>,
    state: &Entity<ColorPickerState>,
    window: &mut Window,
    cx: &mut App,
) -> ColorPicker {
    let id = id.into();
    state.update(cx, |state, cx| state.sync_pending_value(window, cx));
    let current = state.read(cx);
    let open = current.is_open();
    let color = current.displayed_color();
    let focus = current.focus_handle(cx);
    let hex = current.hex_input().clone();
    let channels = current.sliders().clone();
    let hex_text = hex.read(cx).value().to_string();
    let digits = hex_text.strip_prefix('#').unwrap_or(&hex_text);
    let valid_hex = matches!(digits.len(), 3 | 4 | 6 | 8)
        && digits.bytes().all(|byte| byte.is_ascii_hexdigit());
    // A successful Hex commit is owned by base and closes its state directly.
    // Return focus before removing the text field from the render tree.
    if !open && hex.read(cx).focus_handle(cx).is_focused(window) {
        focus.focus(window, cx);
    }
    let t = cx.omarchy().clone();
    let trigger = button("color-trigger", "Choose color", ButtonVariant::Outline, cx)
        .debug_selector(|| "color-picker-trigger".into())
        // The outer picker owns focus; registering this handle on the trigger
        // as well reports two focused accessibility nodes in the same frame.
        .focusable(false)
        .child(
            div()
                .size(rems(1.))
                .rounded(control_radius())
                .border_1()
                .border_color(t.border)
                .bg(color.unwrap_or(t.background)),
        )
        .on_click(|_, window, cx| {
            window.dispatch_action(
                Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                cx,
            );
        });
    let mut popup = Popup::new((id.clone(), "popup"), trigger);
    if open {
        let target = state.clone();
        let return_focus = focus.clone();
        let mut body = popover_surface(cx)
            .id("color-editor")
            .debug_selector(|| "color-picker-popup".into())
            .key_context("OmarchyPopoverContent")
            .w(rems(17.5))
            .mt(rems(0.25))
            .flex()
            .flex_col()
            .gap(rems(0.375))
            .on_mouse_down_out(move |_, window, cx| {
                target.update(cx, |state, cx| {
                    restore_committed_color(state, window, cx);
                    state.set_open(false, cx);
                });
                return_focus.focus(window, cx);
            })
            .child("Hex color")
            .child(input("color-hex", &hex, window, cx))
            .when(!hex_text.is_empty() && !valid_hex, |body| {
                body.child(
                    div()
                        .debug_selector(|| "color-hex-error".into())
                        .text_color(t.danger)
                        .child("Use 3, 4, 6 or 8 hexadecimal digits."),
                )
            });
        for (name, channel) in [
            ("Hue", channels.hue()),
            ("Saturation", channels.saturation()),
            ("Lightness", channels.lightness()),
            ("Opacity", channels.alpha()),
        ] {
            body = body.child(
                div().flex().flex_col().gap(rems(0.25)).child(name).child(
                    div()
                        .debug_selector(move || format!("color-channel-{name}"))
                        .child(slider(channel, false, window, cx).px(rems(0.))),
                ),
            );
        }
        let target = state.clone();
        let return_focus = focus.clone();
        body = body.child(
            button("color-apply", "Apply color", ButtonVariant::Outline, cx)
                .debug_selector(|| "color-apply".into())
                .disabled(!valid_hex)
                .on_click(move |_, window, cx| {
                    let committed = target.update(cx, |state, cx| {
                        let text = state.hex_input().read(cx).value().to_string();
                        state.commit_hex(&text, window, cx).is_some()
                    });
                    if committed {
                        return_focus.focus(window, cx);
                    }
                }),
        );
        popup = popup.content(body);
    }
    let target = state.clone();
    let return_focus = focus.clone();
    ColorPicker::new(id)
        .open(open)
        .track_focus(&focus)
        .accessibility_label("Choose color")
        .on_open_change(move |open, window, cx| {
            target.update(cx, |state, cx| {
                if !open {
                    restore_committed_color(state, window, cx);
                }
                state.set_open(open, cx);
            });
            return_focus.focus(window, cx);
        })
        .child(popup)
}

// Invalid Hex input leaves base's preview equal to its committed value, so
// clear_preview alone can retain an invalid draft. Re-sync all editing fields.
fn restore_committed_color(
    state: &mut ColorPickerState,
    window: &mut Window,
    cx: &mut gpui_kit::Context<ColorPickerState>,
) {
    if let Some(value) = state.value() {
        state.set_value(value, window, cx);
    } else {
        state.clear_value(window, cx);
    }
}
