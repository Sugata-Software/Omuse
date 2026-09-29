//! Explicit local finishing for protected-subject background edits.
//!
//! These settings are intentionally small, local, and opt-in. They never
//! alter a provider request; AI request code snapshots `product_presentation`
//! and only uses it when composing a returned background candidate.
use super::*;
use omuse::ai_edits::{
    ProductPresentation, ProductReflection, ProductShadow, remove_product_presentation_layers,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ShadowPreset {
    #[default]
    Off,
    Soft,
    Contact,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ReflectionPreset {
    #[default]
    Off,
    Subtle,
}

/// UI-only choices for deterministic native product finishing. The actual
/// settings travel with the AI request/proposal rather than being read again
/// when an asynchronous result arrives.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ProductUiState {
    shadow: ShadowPreset,
    reflection: ReflectionPreset,
    expanded: bool,
}

impl ProductUiState {
    pub(super) fn new(_window: &mut Window, _cx: &mut Context<EditorView>) -> Self {
        Self::default()
    }

    pub(super) fn presentation(&self) -> ProductPresentation {
        ProductPresentation {
            shadow: match self.shadow {
                ShadowPreset::Off => None,
                ShadowPreset::Soft => Some(ProductShadow::default()),
                ShadowPreset::Contact => Some(ProductShadow {
                    offset_x: 0,
                    offset_y: 6,
                    blur_px: 4,
                    opacity: 0.5,
                    color: [0, 0, 0],
                }),
            },
            reflection: match self.reflection {
                ReflectionPreset::Off => None,
                ReflectionPreset::Subtle => Some(ProductReflection::default()),
            },
        }
    }

    pub(super) fn summary(&self) -> String {
        let shadow = match self.shadow {
            ShadowPreset::Off => "shadow off",
            ShadowPreset::Soft => "soft shadow",
            ShadowPreset::Contact => "contact shadow",
        };
        let reflection = match self.reflection {
            ReflectionPreset::Off => None,
            ReflectionPreset::Subtle => Some("subtle reflection"),
        };
        match reflection {
            Some(reflection) => format!("{shadow} · {reflection}"),
            None => shadow.into(),
        }
    }

    /// Restore only settings representable by the visible preset controls.
    /// Unknown historical/custom values leave the current controls untouched
    /// instead of silently displaying a different finishing treatment.
    pub(super) fn restore_presentation(&mut self, presentation: &ProductPresentation) -> bool {
        let soft_shadow = ProductShadow::default();
        let contact_shadow = ProductShadow {
            offset_x: 0,
            offset_y: 6,
            blur_px: 4,
            opacity: 0.5,
            color: [0, 0, 0],
        };
        let shadow = match presentation.shadow.as_ref() {
            None => ShadowPreset::Off,
            Some(value) if value == &soft_shadow => ShadowPreset::Soft,
            Some(value) if value == &contact_shadow => ShadowPreset::Contact,
            Some(_) => return false,
        };
        let reflection = match presentation.reflection.as_ref() {
            None => ReflectionPreset::Off,
            Some(value) if value == &ProductReflection::default() => ReflectionPreset::Subtle,
            Some(_) => return false,
        };
        self.shadow = shadow;
        self.reflection = reflection;
        self.expanded = !presentation.is_empty();
        true
    }
}

impl EditorView {
    /// A plain value suitable for snapshotting with an AI request. It is
    /// deliberately independent of the provider and stays empty by default.
    pub(super) fn product_presentation(&self) -> ProductPresentation {
        self.product.presentation()
    }

    fn product_actions_busy(&self) -> bool {
        self.create.saving || self.create.job.is_some() || self.create.bulk_job.is_some()
    }

    /// Presentation controls are shown beside the background-replacement
    /// controls. The renderer only reads state; click handlers mutate the
    /// editor view later through GPUI's normal listener path.
    pub(super) fn render_product_controls(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.product_actions_busy();
        let mut panel = div()
            .id("product-presentation-controls")
            .debug_selector(|| "product-presentation-controls".into())
            .flex()
            .flex_col()
            .gap_1()
            .child(
                button(
                    "product-finish-disclosure",
                    format!("Product finishing · {}", self.product.summary()),
                    ButtonVariant::Secondary,
                    cx,
                )
                .selected(self.product.expanded)
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.product.expanded = !this.product.expanded;
                    cx.notify();
                })),
            );
        if !self.product.expanded {
            return panel.into_any_element();
        }

        let theme = cx.omarchy().clone();
        let mut details = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(theme.divider())
            .bg(theme.background)
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme.secondary)
                    .child("Optional local layers after a protected-subject background result. Nothing here is sent to a provider."),
            )
            .child(div().text_size(px(11.)).text_color(theme.secondary).child("Shadow"));
        let mut shadows = div().flex().gap_1();
        for (preset, id, name) in [
            (ShadowPreset::Off, "product-shadow-off", "Off"),
            (ShadowPreset::Soft, "product-shadow-soft", "Soft"),
            (ShadowPreset::Contact, "product-shadow-contact", "Contact"),
        ] {
            let selected = self.product.shadow == preset;
            shadows = shadows.child(
                button(id, name, ButtonVariant::Secondary, cx)
                    .debug_selector(move || id.into())
                    .flex_1()
                    .min_w_0()
                    .selected(selected)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.product.shadow = preset;
                        cx.notify();
                    })),
            );
        }
        details = details.child(shadows).child(
            div()
                .text_size(px(11.))
                .text_color(theme.secondary)
                .child("Reflection"),
        );
        let mut reflections = div().flex().gap_1();
        for (preset, id, name) in [
            (ReflectionPreset::Off, "product-reflection-off", "Off"),
            (
                ReflectionPreset::Subtle,
                "product-reflection-subtle",
                "Subtle",
            ),
        ] {
            let selected = self.product.reflection == preset;
            reflections = reflections.child(
                button(id, name, ButtonVariant::Secondary, cx)
                    .debug_selector(move || id.into())
                    .flex_1()
                    .min_w_0()
                    .selected(selected)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.product.reflection = preset;
                        cx.notify();
                    })),
            );
        }
        let presentation = self.product.presentation();
        details = details.child(reflections).child(
            button(
                "product-finish-apply",
                "Apply local finishing",
                ButtonVariant::Secondary,
                cx,
            )
            .disabled(busy || presentation.is_empty())
            .on_click(cx.listener(|this, _, _, cx| {
                let result = (|| -> anyhow::Result<()> {
                    anyhow::ensure!(
                        !this.product_actions_busy(),
                        "Wait for the current content job before changing product finishing"
                    );
                    this.finish_interaction(cx);
                    let subject = this.editor.selection.as_ref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "Select the product before applying local shadow or reflection"
                        )
                    })?;
                    let document = omuse::ai_edits::apply_product_presentation(
                        &this.editor.document,
                        subject,
                        &this.product_presentation(),
                    )?;
                    this.editor.replace_document_transaction(document)?;
                    this.changed(cx);
                    Ok(())
                })();
                this.status = match result {
                    Ok(()) => "Local product finishing applied. Undo restores the source.".into(),
                    Err(error) => error.to_string(),
                };
                cx.notify();
            })),
        );
        details = details.child(
            button(
                "product-finish-remove",
                "Remove local finishing",
                ButtonVariant::Secondary,
                cx,
            )
            .disabled(busy)
            .on_click(cx.listener(|this, _, _, cx| {
                let result = (|| -> anyhow::Result<()> {
                    anyhow::ensure!(
                        !this.product_actions_busy(),
                        "Wait for the current content job before changing product finishing"
                    );
                    this.finish_interaction(cx);
                    let mut document = this.editor.document.clone();
                    let removed = remove_product_presentation_layers(&mut document)?;
                    anyhow::ensure!(
                        removed > 0,
                        "There is no local product finishing on this canvas"
                    );
                    this.editor.replace_document_transaction(document)?;
                    this.changed(cx);
                    Ok(())
                })();
                this.status = match result {
                    Ok(()) => "Local product finishing removed. Undo restores it.".into(),
                    Err(error) => error.to_string(),
                };
                cx.notify();
            })),
        );
        panel = panel.child(details);
        panel.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_product_presentation_is_inert() {
        assert!(ProductUiState::default().presentation().is_empty());
    }

    #[test]
    fn presets_map_to_explicit_bounded_native_settings() {
        let state = ProductUiState {
            shadow: ShadowPreset::Contact,
            reflection: ReflectionPreset::Subtle,
            ..ProductUiState::default()
        };
        let presentation = state.presentation();
        assert_eq!(presentation.shadow.unwrap().offset_y, 6);
        assert_eq!(presentation.reflection.unwrap().height_ratio, 0.45);
    }
}
