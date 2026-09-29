//! Native design controls for the Create inspector.
use super::create_ui::{note, section};
use super::inspector_ui::panel_button as button;
use super::inspector_ui::panel_input as input;
use super::*;
use anyhow::Context as _;
use gpui_kit::FontWeight;
use image::{Rgba, RgbaImage};
use omuse::create::{
    self, BindingSchema, BindingTable, BindingTarget, BoundPage, ComponentOverrides, FieldBinding,
    FrameBounds, FrameSpec, ResizeStrategy,
};
use omuse::create_project::{BrandKit, BrandTextStyle};
use omuse::objects;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

/// Inputs owned by the layout and CSV inspector. Keeping these separate from
/// the shared Create fields prevents CSV controls from colliding with content
/// timing, audio, clip and rich-text state.
pub(super) struct CreateDesignState {
    pub(super) collage_count: Entity<InputState>,
    pub(super) collage_gap: Entity<InputState>,
    pub(super) csv_path: Entity<InputState>,
    pub(super) csv_field: Entity<InputState>,
    pub(super) csv_column: Entity<InputState>,
    pub(super) brand_body_font: Entity<InputState>,
    pub(super) brand_style_role: Entity<InputState>,
    pub(super) brand_style_font: Entity<InputState>,
    pub(super) brand_style_size: Entity<InputState>,
    pub(super) brand_style_tracking: Entity<InputState>,
    pub(super) brand_style_leading: Entity<InputState>,
    pub(super) brand_spacing_role: Entity<InputState>,
    pub(super) brand_spacing_value: Entity<InputState>,
    pub(super) brand_logo_resource_id: Entity<InputState>,
    pub(super) component_override_field: Entity<InputState>,
    pub(super) component_override_text: Entity<InputState>,
    pub(super) component_override_visible: Entity<InputState>,
    pub(super) text_background_color: Entity<InputState>,
}

impl CreateDesignState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        let mut input = |value| cx.new(|cx| InputState::new(window, cx).default_value(value));
        Self {
            collage_count: input("3"),
            collage_gap: input("30"),
            csv_path: input(""),
            csv_field: input(""),
            csv_column: input(""),
            brand_body_font: input("sans-serif"),
            brand_style_role: input(""),
            brand_style_font: input(""),
            brand_style_size: input(""),
            brand_style_tracking: input("0"),
            brand_style_leading: input("0"),
            brand_spacing_role: input(""),
            brand_spacing_value: input(""),
            brand_logo_resource_id: input(""),
            component_override_field: input(""),
            component_override_text: input(""),
            component_override_visible: input("true"),
            text_background_color: input("#FFFFFF"),
        }
    }

    fn value(input: &Entity<InputState>, cx: &App) -> String {
        input.read(cx).value().trim().to_owned()
    }
}

pub(super) struct BulkDesignJob {
    pub(super) cancel: Arc<AtomicBool>,
    receiver: std::sync::mpsc::Receiver<BulkDesignEvent>,
    detail: String,
    generation: u64,
    create_epoch: u64,
    editor_revision: u64,
    active_page_id: String,
}

impl Drop for BulkDesignJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct BulkDesignPrepared {
    pages: Vec<BoundPage>,
    source_template_id: Option<String>,
}

enum BulkDesignEvent {
    Finished(Result<BulkDesignPrepared, String>),
}

fn parse_hex_color(value: &str) -> anyhow::Result<[u8; 4]> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    anyhow::ensure!(value.len() == 6, "Use a six-digit colour such as #D98566");
    anyhow::ensure!(
        value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Use hexadecimal colour digits"
    );
    Ok([
        u8::from_str_radix(&value[0..2], 16)?,
        u8::from_str_radix(&value[2..4], 16)?,
        u8::from_str_radix(&value[4..6], 16)?,
        255,
    ])
}

impl EditorView {
    pub(super) fn create_templates_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = section("ORIGINAL EDITABLE TEMPLATES", cx).child(note(
            "Every template uses native text and shapes. Choosing one adds a page to the current collection.",
            cx,
        ));
        for template in create::templates() {
            let template_id = template.id;
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.omarchy().divider())
                    .child(self.create_template_preview(template_id, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(template.name),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(cx.omarchy().secondary)
                                    .child(template.category),
                            ),
                    )
                    .child(note(
                        format!(
                            "{} · {} × {}",
                            template.description, template.width, template.height
                        ),
                        cx,
                    ))
                    .child(
                        button(
                            SharedString::from(format!("create-template-{template_id}")),
                            "Add to collection",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .w_full()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let result = this.add_template_page(template_id, cx);
                            this.create_error(result, cx);
                        })),
                    ),
            );
        }
        body.into_any_element()
    }

    fn add_template_page(
        &mut self,
        template_id: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let brand = self
            .create
            .session
            .as_ref()
            .and_then(|session| session.project.active_brand())
            .cloned();
        let document = create::instantiate_template(template_id, brand.as_ref())?;
        let name = document.name.clone();
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let id = draft.add_page(name, document)?;
        draft.set_page_template(&id, Some(template_id))?;
        if draft.metadata.shared_background_component_id.is_some() {
            create::apply_shared_background_to_page(&mut draft, &id)?;
        }
        draft.set_active_page(&id)?;
        self.apply_creative_project(draft, cx)?;
        self.schedule_content_recovery();
        Ok(())
    }

    pub(super) fn create_brand_section(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self
            .create
            .session
            .as_ref()
            .and_then(|session| session.project.active_brand());
        let mut body = section("BRAND KIT", cx)
            .child(note(
                "Save reusable colour roles and font pairing, then apply them across every live page object.",
                cx,
            ))
            .child(
                button(
                    "create-brand-sugata",
                    "Use Sugata starter",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.install_and_apply_brand(create::sugata_brand_kit(), cx);
                    this.create_error(result, cx);
                })),
            )
            .child(note("Brand name", cx))
            .child(input("create-brand-name", &self.create.fields[4], window, cx))
            .child(note("Paper", cx))
            .child(input("create-brand-paper", &self.create.fields[5], window, cx))
            .child(note("Ink", cx))
            .child(input("create-brand-ink", &self.create.fields[6], window, cx))
            .child(note("Accent", cx))
            .child(input("create-brand-accent", &self.create.fields[7], window, cx))
            .child(note("Heading font", cx))
            .child(input(
                "create-brand-heading-font",
                &self.create.fields[8],
                window,
                cx,
            ))
            .child(note("Body font", cx))
            .child(input(
                "create-brand-body-font",
                &self.create.design.brand_body_font,
                window,
                cx,
            ))
            .child(
                button(
                    "create-save-brand",
                    "Save and apply brand",
                    ButtonVariant::Primary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.save_and_apply_brand(cx);
                    this.create_error(result, cx);
                })),
            );
        body = body
            .child(note("TEXT STYLE TOKEN", cx))
            .child(note("Role", cx))
            .child(input(
                "create-brand-style-role",
                &self.create.design.brand_style_role,
                window,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(note("Font family", cx))
                            .child(input(
                                "create-brand-style-font",
                                &self.create.design.brand_style_font,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(note("Size (px)", cx))
                            .child(input(
                                "create-brand-style-size",
                                &self.create.design.brand_style_size,
                                window,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(note("Tracking", cx))
                            .child(input(
                                "create-brand-style-tracking",
                                &self.create.design.brand_style_tracking,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(note("Line height", cx))
                            .child(input(
                                "create-brand-style-leading",
                                &self.create.design.brand_style_leading,
                                window,
                                cx,
                            )),
                    ),
            )
            .child(
                button(
                    "create-brand-save-style",
                    "Save text style token",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.save_brand_text_style(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(note("SPACING TOKEN", cx))
            .child(note(
                "Template roles: tight for labels, standard for headings, and section for body copy. Saving one updates matching editable objects on every page.",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(note("Role", cx))
                            .child(input(
                                "create-brand-spacing-role",
                                &self.create.design.brand_spacing_role,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(note("Spacing (px)", cx))
                            .child(input(
                                "create-brand-spacing-value",
                                &self.create.design.brand_spacing_value,
                                window,
                                cx,
                            )),
                    ),
            )
            .child(
                button(
                    "create-brand-save-spacing",
                    "Save spacing token",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.save_brand_spacing(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(note("PROTECTED LOGO", cx))
            .child(note("Packaged image resource ID", cx))
            .child(input(
                "create-brand-logo-resource",
                &self.create.design.brand_logo_resource_id,
                window,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button(
                            "create-brand-attach-logo",
                            "Attach resource",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.attach_brand_logo_resource(cx);
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button(
                            "create-brand-insert-logo",
                            "Insert locked logo",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.insert_protected_brand_logo(cx);
                            this.create_error(result, cx);
                        })),
                    ),
            );
        if let Some(brand) = active {
            body = body.child(note(
                format!(
                    "Active: {} · {} colours · {} text styles",
                    brand.name,
                    brand.colors.len(),
                    brand.text_styles.len()
                ),
                cx,
            ));
        }
        if let Some(session) = &self.create.session {
            let logos = session
                .project
                .resource_summaries()
                .into_iter()
                .filter(|resource| resource.media_type.starts_with("image/"))
                .collect::<Vec<_>>();
            if !logos.is_empty() {
                body = body.child(note("Packaged image IDs for logo attachment:", cx));
                for resource in logos {
                    body = body.child(note(format!("{} · {}", resource.name, resource.id), cx));
                }
            }
            for brand in &session.project.brand_kits {
                let brand_id = brand.id.clone();
                body = body.child(
                    button(
                        SharedString::from(format!("create-apply-brand-{brand_id}")),
                        SharedString::from(format!("Apply {}", brand.name)),
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .w_full()
                    .selected(session.project.active_brand_id.as_deref() == Some(brand.id.as_str()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let result = this.apply_saved_brand(&brand_id, cx);
                        this.create_error(result, cx);
                    })),
                );
            }
        }
        body.into_any_element()
    }

    fn brand_from_fields(&self, cx: &App) -> anyhow::Result<BrandKit> {
        let name = self.create.value(4, cx);
        anyhow::ensure!(!name.is_empty(), "Enter a brand name");
        let paper = parse_hex_color(&self.create.value(5, cx))?;
        let ink = parse_hex_color(&self.create.value(6, cx))?;
        let accent = parse_hex_color(&self.create.value(7, cx))?;
        let heading_font = self.create.value(8, cx);
        let body_font = CreateDesignState::value(&self.create.design.brand_body_font, cx);
        anyhow::ensure!(
            !heading_font.is_empty() && !body_font.is_empty(),
            "Enter installed heading and body font families"
        );
        let mut brand = self
            .create
            .session
            .as_ref()
            .and_then(|session| {
                session
                    .project
                    .brand_kits
                    .iter()
                    .find(|brand| brand.name.eq_ignore_ascii_case(&name))
            })
            .cloned()
            .unwrap_or_else(|| BrandKit::new(name.clone()));
        brand.name = name;
        brand.colors.extend([
            ("paper".into(), paper),
            ("background".into(), paper),
            ("ink".into(), ink),
            ("accent".into(), accent),
            ("secondary".into(), [accent[0], accent[1], accent[2], 180]),
        ]);
        brand.fonts.heading = heading_font;
        brand.fonts.body = body_font;
        Ok(brand)
    }

    fn active_brand_id(&self) -> anyhow::Result<String> {
        self.create
            .session
            .as_ref()
            .and_then(|session| session.project.active_brand_id.clone())
            .context("Save and apply a brand before editing its tokens or logo")
    }

    fn save_brand_text_style(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let brand_id = self.active_brand_id()?;
        let role = CreateDesignState::value(&self.create.design.brand_style_role, cx);
        let font = CreateDesignState::value(&self.create.design.brand_style_font, cx);
        let size =
            CreateDesignState::value(&self.create.design.brand_style_size, cx).parse::<f32>()?;
        let tracking = CreateDesignState::value(&self.create.design.brand_style_tracking, cx)
            .parse::<f32>()?;
        let leading =
            CreateDesignState::value(&self.create.design.brand_style_leading, cx).parse::<f32>()?;
        anyhow::ensure!(
            !role.is_empty() && !font.is_empty(),
            "Enter a style role and font"
        );
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let brand = draft
            .brand_kits
            .iter_mut()
            .find(|brand| brand.id == brand_id)
            .context("Active brand disappeared")?;
        brand.text_styles.insert(
            role.clone(),
            BrandTextStyle {
                font,
                size,
                tracking,
                leading,
            },
        );
        create::apply_brand_to_project(&mut draft, &brand_id)?;
        self.apply_creative_project(draft, cx)?;
        self.status = format!("Saved {role} text style");
        self.schedule_content_recovery();
        Ok(())
    }

    fn save_brand_spacing(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let brand_id = self.active_brand_id()?;
        let role = CreateDesignState::value(&self.create.design.brand_spacing_role, cx);
        let value =
            CreateDesignState::value(&self.create.design.brand_spacing_value, cx).parse::<f32>()?;
        anyhow::ensure!(!role.is_empty(), "Enter a spacing role");
        anyhow::ensure!(
            value.is_finite() && (0.0..=30_000.0).contains(&value),
            "Spacing must be 0–30000 pixels"
        );
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        draft
            .brand_kits
            .iter_mut()
            .find(|brand| brand.id == brand_id)
            .context("Active brand disappeared")?
            .spacing
            .insert(role.clone(), value);
        create::apply_brand_to_project(&mut draft, &brand_id)?;
        self.apply_creative_project(draft, cx)?;
        self.status = format!("Saved {role} spacing token");
        self.schedule_content_recovery();
        Ok(())
    }

    fn attach_brand_logo_resource(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let brand_id = self.active_brand_id()?;
        let resource_id = CreateDesignState::value(&self.create.design.brand_logo_resource_id, cx);
        anyhow::ensure!(
            !resource_id.is_empty(),
            "Enter a packaged image resource ID"
        );
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        anyhow::ensure!(
            draft
                .resource_summaries()
                .iter()
                .any(|resource| resource.id == resource_id
                    && resource.media_type.starts_with("image/")),
            "Logo must be an image packaged with this project"
        );
        let brand = draft
            .brand_kits
            .iter_mut()
            .find(|brand| brand.id == brand_id)
            .context("Active brand disappeared")?;
        if !brand.logo_resource_ids.contains(&resource_id) {
            brand.logo_resource_ids.push(resource_id.clone());
        }
        draft.validate()?;
        self.apply_creative_project(draft, cx)?;
        self.status = "Attached protected logo resource to active brand".into();
        self.schedule_content_recovery();
        Ok(())
    }

    fn insert_protected_brand_logo(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let brand_id = self.active_brand_id()?;
        let resource_id = CreateDesignState::value(&self.create.design.brand_logo_resource_id, cx);
        anyhow::ensure!(
            !resource_id.is_empty(),
            "Enter an attached logo resource ID"
        );
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let page_id = draft.active_page_id().to_owned();
        create::insert_brand_logo(&mut draft, &page_id, &brand_id, &resource_id)?;
        self.apply_creative_project(draft, cx)?;
        self.status = "Inserted a protected reusable brand logo".into();
        self.schedule_content_recovery();
        Ok(())
    }

    fn save_and_apply_brand(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let brand = self.brand_from_fields(cx)?;
        self.install_and_apply_brand(brand, cx)
    }

    fn install_and_apply_brand(
        &mut self,
        mut brand: BrandKit,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        if let Some(existing) = draft
            .brand_kits
            .iter_mut()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(&brand.name))
        {
            brand.id = existing.id.clone();
            *existing = brand.clone();
        } else {
            draft.add_brand(brand.clone())?;
        }
        let report = create::apply_brand_to_project(&mut draft, &brand.id)?;
        // Brand application changes every page. Replacing the project through
        // the shared helper clears inactive editor snapshots before they can
        // overwrite the freshly branded documents during the next sync.
        self.apply_creative_project(draft, cx)?;
        self.status = if report.text_fitted == 0 {
            format!("Applied brand: {}", brand.name)
        } else {
            format!(
                "Applied brand: {} (fitted {} native text field{})",
                brand.name,
                report.text_fitted,
                if report.text_fitted == 1 { "" } else { "s" }
            )
        };
        self.schedule_content_recovery();
        Ok(())
    }

    fn apply_saved_brand(&mut self, brand_id: &str, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let report = create::apply_brand_to_project(&mut draft, brand_id)?;
        self.apply_creative_project(draft, cx)?;
        if report.text_fitted > 0 {
            self.status = format!(
                "Applied saved brand (fitted {} native text field{})",
                report.text_fitted,
                if report.text_fitted == 1 { "" } else { "s" }
            );
        }
        self.schedule_content_recovery();
        Ok(())
    }

    pub(super) fn create_layout_section(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = section("ADAPT THIS PAGE", cx)
            .child(note(
                "Resize the composition while retaining live text, shapes, frames and their layout rules.",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(note("Width (px)", cx))
                            .child(input(
                                "create-resize-width",
                                &self.create.fields[2],
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(note("Height (px)", cx))
                            .child(input(
                                "create-resize-height",
                                &self.create.fields[3],
                                window,
                                cx,
                            )),
                    ),
            )
            .child(
                button(
                    "create-adapt-layout",
                    "Adapt live layout",
                    ButtonVariant::Primary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.adapt_current_layout(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(note("FRAMES AND COLLAGES", cx))
            .child(
                button(
                    "create-frame-active",
                    "Frame a copy of the active image",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.frame_active_image(false, cx);
                    this.create_error(result, cx);
                })),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(note("Frames", cx)).child(input(
                        "create-collage-count",
                        &self.create.design.collage_count,
                        window,
                        cx,
                    )))
                    .child(div().flex_1().child(note("Gap", cx)).child(input(
                        "create-collage-gap",
                        &self.create.design.collage_gap,
                        window,
                        cx,
                    ))),
            )
            .child(
                button(
                    "create-collage-active",
                    "Build collage from active image",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.frame_active_image(true, cx);
                    this.create_error(result, cx);
                })),
            );
        body = body
            .child(note("CAROUSEL CONTINUITY", cx))
            .child(
                button(
                    "create-shared-background",
                    "Use selected layers as shared protected background",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.set_shared_background_from_selection(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(
                button(
                    "create-adjacent-spread",
                    "Preview adjacent-page spread",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.preview_adjacent_page_spread(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(note("ARRANGE SELECTED LAYERS", cx))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button("create-align-left", "Left", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this
                                    .arrange_selected_layers(create::LayerArrangement::Left, cx);
                                this.create_error(result, cx);
                            })),
                    )
                    .child(
                        button(
                            "create-align-center",
                            "Center",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result =
                                this.arrange_selected_layers(create::LayerArrangement::Center, cx);
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button("create-align-right", "Right", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result = this
                                    .arrange_selected_layers(create::LayerArrangement::Right, cx);
                                this.create_error(result, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button("create-align-top", "Top", ButtonVariant::Secondary, cx)
                            .flex_1()
                            .on_click(cx.listener(|this, _, _, cx| {
                                let result =
                                    this.arrange_selected_layers(create::LayerArrangement::Top, cx);
                                this.create_error(result, cx);
                            })),
                    )
                    .child(
                        button(
                            "create-align-middle",
                            "Middle",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result =
                                this.arrange_selected_layers(create::LayerArrangement::Middle, cx);
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button(
                            "create-align-bottom",
                            "Bottom",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result =
                                this.arrange_selected_layers(create::LayerArrangement::Bottom, cx);
                            this.create_error(result, cx);
                        })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button(
                            "create-distribute-horizontal",
                            "Space across",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.arrange_selected_layers(
                                create::LayerArrangement::DistributeHorizontally,
                                cx,
                            );
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button(
                            "create-distribute-vertical",
                            "Space down",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.arrange_selected_layers(
                                create::LayerArrangement::DistributeVertically,
                                cx,
                            );
                            this.create_error(result, cx);
                        })),
                    ),
            );
        if self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .is_some_and(|layer| objects::live_text(layer).ok().flatten().is_some())
        {
            let active = self.editor.active_layer.as_str();
            let is_page_level = self
                .editor
                .document
                .layers
                .iter()
                .any(|layer| layer.id == active);
            if create::layer_is_effectively_locked(&self.editor.document, active) {
                body = body.child(note(
                    "Unlock this text layer or its group before adding a text background.",
                    cx,
                ));
            } else if !is_page_level {
                body = body.child(note(
                    "Text backgrounds are available for page-level live text. Select or ungroup its container; for a component, edit its definition to keep it reusable.",
                    cx,
                ));
            } else {
                body = body
                    .child(note("TEXT BACKGROUND COLOUR", cx))
                    .child(input(
                        "create-text-background-colour",
                        &self.create.design.text_background_color,
                        window,
                        cx,
                    ))
                    .child(
                        button(
                            "create-text-background",
                            "Add editable text background",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .w_full()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.add_active_text_background(cx);
                            this.create_error(result, cx);
                        })),
                    );
            }
        }
        if let Some(frame) = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .and_then(|layer| create::frame_spec(layer).ok().flatten())
        {
            body = body
                .child(note("FRAME CROP", cx))
                .child(note(
                    format!(
                        "Focal point {:.0}% × {:.0}% · zoom {:.2}×. These controls keep the source pixels and rebuild only the frame mask.",
                        frame.crop.focal_x * 100.0,
                        frame.crop.focal_y * 100.0,
                        frame.crop.zoom,
                    ),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            button("create-frame-crop-left", "←", ButtonVariant::Secondary, cx)
                                .flex_1()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let result = this.adjust_active_frame_crop(-0.1, 0.0, 1.0, cx);
                                    this.create_error(result, cx);
                                })),
                        )
                        .child(
                            button("create-frame-crop-up", "↑", ButtonVariant::Secondary, cx)
                                .flex_1()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let result = this.adjust_active_frame_crop(0.0, -0.1, 1.0, cx);
                                    this.create_error(result, cx);
                                })),
                        )
                        .child(
                            button("create-frame-crop-down", "↓", ButtonVariant::Secondary, cx)
                                .flex_1()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let result = this.adjust_active_frame_crop(0.0, 0.1, 1.0, cx);
                                    this.create_error(result, cx);
                                })),
                        )
                        .child(
                            button("create-frame-crop-right", "→", ButtonVariant::Secondary, cx)
                                .flex_1()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let result = this.adjust_active_frame_crop(0.1, 0.0, 1.0, cx);
                                    this.create_error(result, cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            button("create-frame-crop-zoom-out", "Zoom out", ButtonVariant::Secondary, cx)
                                .flex_1()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let result = this.adjust_active_frame_crop(0.0, 0.0, 1.0 / 1.15, cx);
                                    this.create_error(result, cx);
                                })),
                        )
                        .child(
                            button("create-frame-crop-zoom-in", "Zoom in", ButtonVariant::Secondary, cx)
                                .flex_1()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let result = this.adjust_active_frame_crop(0.0, 0.0, 1.15, cx);
                                    this.create_error(result, cx);
                                })),
                        ),
                );
        }
        body = body.child(note("REUSABLE COMPONENTS", cx)).child(
            button(
                "create-component-save",
                "Save selected layer as component",
                ButtonVariant::Secondary,
                cx,
            )
            .w_full()
            .on_click(cx.listener(|this, _, _, cx| {
                let result = this.save_current_component(cx);
                this.create_error(result, cx);
            })),
        );
        if let Ok(instance) =
            create::component_instance_info(&self.editor.document, &self.editor.active_layer)
        {
            body = body
                .child(note(
                    format!(
                        "Selected component revision {} · {} text overrides · {} hidden fields",
                        instance.revision,
                        instance.overrides.text.len(),
                        instance.overrides.hidden_fields.len(),
                    ),
                    cx,
                ))
                .child(
                    button(
                        "create-component-update-selected",
                        "Save selected instance as definition and update all",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .w_full()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = this.update_selected_component_definition(cx);
                        this.create_error(result, cx);
                    })),
                )
                .child(note("Instance field override", cx))
                .child(input(
                    "create-component-override-field",
                    &self.create.design.component_override_field,
                    window,
                    cx,
                ))
                .child(input(
                    "create-component-override-text",
                    &self.create.design.component_override_text,
                    window,
                    cx,
                ))
                .child(note("Visible: true or false", cx))
                .child(input(
                    "create-component-override-visible",
                    &self.create.design.component_override_visible,
                    window,
                    cx,
                ))
                .child(
                    button(
                        "create-component-save-override",
                        "Apply selected instance override",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .w_full()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = this.apply_selected_component_override(cx);
                        this.create_error(result, cx);
                    })),
                );
        }
        if let Some(session) = &self.create.session {
            for component in session.project.component_summaries() {
                let insert_id = component.id.clone();
                let update_id = component.id.clone();
                body = body
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                button(
                                    SharedString::from(format!(
                                        "create-component-insert-{insert_id}"
                                    )),
                                    SharedString::from(format!("Insert {}", component.name)),
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .flex_1()
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let result = this.insert_saved_component(&insert_id, cx);
                                        this.create_error(result, cx);
                                    },
                                )),
                            )
                            .child(
                                button(
                                    SharedString::from(format!(
                                        "create-component-update-{update_id}"
                                    )),
                                    "Update all",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let result = this.propagate_saved_component(&update_id, cx);
                                        this.create_error(result, cx);
                                    },
                                )),
                            ),
                    )
                    .child(note(format!("Revision {}", component.revision), cx));
            }
        }
        body = body
            .child(note("CSV BULK PREVIEW", cx))
            .child(note(
                "Plain headers map text. Use image:field for a packaged project resource ID, visible:field for true/false, and alt:field for accessibility text. CSV never reads a file path or URL.",
                cx,
            ))
            .child(note("CSV file path", cx))
            .child(input("create-csv-path", &self.create.design.csv_path, window, cx))
            .child(note("Field selector (for example headline or image:hero)", cx))
            .child(input("create-csv-field", &self.create.design.csv_field, window, cx))
            .child(note("CSV column", cx))
            .child(input("create-csv-column", &self.create.design.csv_column, window, cx))
            .child(
                button(
                    "create-csv-package-active-image",
                    "Package active image for CSV",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .disabled(self.create.bulk_job.is_some())
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.package_active_image_resource(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(
                button(
                    "create-csv-validate",
                    "Validate and show mapping",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .disabled(self.create.bulk_job.is_some())
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.validate_csv_rows(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(
                button(
                    "create-csv-preview",
                    "Preview first row",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .disabled(self.create.bulk_job.is_some())
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = this.preview_csv_row(cx);
                    this.create_error(result, cx);
                })),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        button(
                            "create-csv-first",
                            "Create first valid",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .flex_1()
                        .disabled(self.create.bulk_job.is_some())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.start_bulk_design(false, cx);
                            this.create_error(result, cx);
                        })),
                    )
                    .child(
                        button(
                            "create-csv-all",
                            "Create all valid",
                            ButtonVariant::Primary,
                            cx,
                        )
                        .flex_1()
                        .disabled(self.create.bulk_job.is_some())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.start_bulk_design(true, cx);
                            this.create_error(result, cx);
                        })),
                    ),
            );
        if let Some(session) = &self.create.session {
            let resources = session
                .project
                .resource_summaries()
                .into_iter()
                .filter(|resource| resource.media_type.starts_with("image/"))
                .collect::<Vec<_>>();
            if resources.is_empty() {
                body = body.child(note(
                    "No packaged image resources. Select an image and use Package active image for CSV before using image: fields.",
                    cx,
                ));
            } else {
                body = body.child(note("Packaged image resource IDs for image: cells:", cx));
                for resource in resources {
                    body = body.child(note(format!("{} · {}", resource.name, resource.id), cx));
                }
            }
        }
        if let Some(report) = &self.create.bulk_report {
            body = body.child(note(report.clone(), cx));
        }
        if let Some(job) = &self.create.bulk_job {
            body = body.child(note(job.detail.clone(), cx)).child(
                button(
                    "create-csv-cancel",
                    "Cancel bulk creation",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(job) = &this.create.bulk_job {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    cx.notify();
                })),
            );
        }
        body = body.child(self.create_rich_text_section(window, cx));
        body.into_any_element()
    }

    fn adapt_current_layout(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let width = self.create.value(2, cx).parse::<u32>()?;
        let height = self.create.value(3, cx).parse::<u32>()?;
        let document =
            create::resize_layout(&self.editor.document, width, height, ResizeStrategy::Adapt)?;
        self.editor.replace_document_transaction(document)?;
        self.changed(cx);
        self.sync_create()?;
        self.schedule_content_recovery();
        Ok(())
    }

    fn arrange_selected_layers(
        &mut self,
        arrangement: create::LayerArrangement,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let ids = self.selected_layer_ids();
        let mut document = self.editor.document.clone();
        create::arrange_layers(&mut document, &ids, arrangement)?;
        self.editor.replace_document_transaction(document)?;
        self.changed(cx);
        self.sync_create()?;
        self.schedule_content_recovery();
        Ok(())
    }

    fn add_active_text_background(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let color = parse_hex_color(&CreateDesignState::value(
            &self.create.design.text_background_color,
            cx,
        ))?;
        let active = self.editor.active_layer.clone();
        let mut document = self.editor.document.clone();
        create::set_live_text_background(&mut document, &active, color, 14.0, 14.0)?;
        self.editor.replace_document_transaction(document)?;
        self.changed(cx);
        self.sync_create()?;
        self.schedule_content_recovery();
        Ok(())
    }

    fn set_shared_background_from_selection(
        &mut self,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        self.sync_create()?;
        let selected = self.selected_layer_ids();
        let ids = self.editor.selected_layer_roots(&selected);
        anyhow::ensure!(!ids.is_empty(), "Select one or more background layers");
        anyhow::ensure!(ids.len() <= 32, "Select at most 32 background layers");
        let layers = ids
            .iter()
            .map(|id| {
                anyhow::ensure!(
                    !create::layer_is_effectively_locked(&self.editor.document, id),
                    "Unlock selected background layers or their groups before sharing them"
                );
                self.editor
                    .document
                    .layers
                    .iter()
                    .find(|layer| layer.id == *id)
                    .cloned()
                    .with_context(|| {
                        "Shared backgrounds require page-level layers; select the containing component or group instead"
                    })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let component_id = create::set_shared_background(&mut draft, layers)?;
        let pages = draft.page_ids().len();
        self.apply_creative_project(draft, cx)?;
        self.status =
            format!("Applied shared protected background to {pages} pages ({component_id})");
        self.schedule_content_recovery();
        Ok(())
    }

    fn preview_adjacent_page_spread(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let mut project = self.content_snapshot()?;
        let ids = project.page_ids();
        anyhow::ensure!(ids.len() >= 2, "Add a second page to preview a spread");
        let active = project.active_page_id().to_owned();
        let index = ids
            .iter()
            .position(|id| id == &active)
            .context("Active page disappeared")?;
        let pair = if index + 1 < ids.len() {
            [ids[index].clone(), ids[index + 1].clone()]
        } else {
            [ids[index - 1].clone(), ids[index].clone()]
        };
        let identity = (self.create.epoch, self.editor.revision());
        let task = cx.background_executor().spawn(async move {
            let mut previews = Vec::with_capacity(2);
            for id in pair {
                let document = project.page_document(&id)?.clone();
                anyhow::ensure!(
                    u64::from(document.width) * u64::from(document.height) <= 16_777_216,
                    "Page is too large for a spread preview"
                );
                previews.push(image::imageops::thumbnail(
                    &omuse::raster::composite(&document),
                    360,
                    360,
                ));
            }
            let width = previews.iter().map(RgbaImage::width).sum::<u32>() + 12;
            let height = previews.iter().map(RgbaImage::height).max().unwrap_or(1);
            let mut spread = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
            let mut x = 0;
            for preview in previews {
                image::imageops::overlay(&mut spread, &preview, i64::from(x), 0);
                x += preview.width() + 12;
            }
            Ok::<_, anyhow::Error>(spread)
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| match result {
                Ok(spread)
                    if this.create.epoch == identity.0 && this.editor.revision() == identity.1 =>
                {
                    this.display.replace(&spread);
                    this.status = "Adjacent-page spread preview — artwork is unchanged".into();
                    cx.notify();
                }
                Ok(_) => {
                    this.status = "Spread preview discarded because the page changed".into();
                    cx.notify();
                }
                Err(error) => {
                    this.status = format!("Spread preview: {error:#}");
                    cx.notify();
                }
            });
        })
        .detach();
        Ok(())
    }

    fn active_image_copy(&self) -> anyhow::Result<RgbaImage> {
        Ok(self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .and_then(|layer| layer.image.as_ref())
            .context("Select an image layer first")?
            .to_image())
    }

    fn package_active_image_resource(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        self.sync_create()?;
        let active_layer = self
            .editor
            .document
            .find_layer(&self.editor.active_layer)
            .context("Select an image layer first")?;
        let image = active_layer
            .image
            .as_ref()
            .context("Select an image layer first")?
            .to_image();
        anyhow::ensure!(
            u64::from(image.width()) * u64::from(image.height()) <= 14_000_000,
            "Active image is too large to package for CSV binding"
        );
        let name = format!("{} CSV image", active_layer.name);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut bytes, image::ImageFormat::Png)?;

        let mut draft = self
            .create
            .session
            .as_ref()
            .context("Create project closed")?
            .project
            .clone();
        let resource_id = draft.add_resource(name, "image/png", bytes.into_inner())?;
        self.apply_creative_project(draft, cx)?;
        self.status = format!("Packaged image resource {resource_id} for CSV");
        self.schedule_content_recovery();
        Ok(())
    }

    fn frame_active_image(&mut self, collage: bool, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        let source = self.active_image_copy()?;
        let mut document = self.editor.document.clone();
        let margin = document.width.min(document.height) as f32 * 0.07;
        if collage {
            let count =
                CreateDesignState::value(&self.create.design.collage_count, cx).parse::<usize>()?;
            let gap =
                CreateDesignState::value(&self.create.design.collage_gap, cx).parse::<f32>()?;
            anyhow::ensure!((2..=6).contains(&count), "A collage supports 2-6 frames");
            anyhow::ensure!(
                gap.is_finite() && (0.0..=500.0).contains(&gap),
                "Gap must be 0-500 pixels"
            );
            let columns = if count <= 3 { count } else { 2 };
            let rows = count.div_ceil(columns);
            let available_width = document.width as f32 - margin * 2.0 - gap * (columns - 1) as f32;
            let available_height = document.height as f32 - margin * 2.0 - gap * (rows - 1) as f32;
            let frame_width = available_width / columns as f32;
            let frame_height = available_height / rows as f32;
            anyhow::ensure!(
                frame_width >= 1.0 && frame_height >= 1.0,
                "Gap leaves no room for the collage"
            );
            for index in 0..count {
                let column = index % columns;
                let row = index / columns;
                let mut frame = FrameSpec::new(FrameBounds {
                    x: margin + column as f32 * (frame_width + gap),
                    y: margin + row as f32 * (frame_height + gap),
                    width: frame_width,
                    height: frame_height,
                });
                frame.corner_radius = 18.0_f32.min(frame_width.min(frame_height) * 0.1);
                frame.content_field = Some(format!("image_{}", index + 1));
                create::add_image_frame(
                    &mut document,
                    format!("Collage photo {}", index + 1),
                    source.clone(),
                    frame,
                )?;
            }
        } else {
            let mut frame = FrameSpec::new(FrameBounds {
                x: margin,
                y: margin,
                width: document.width as f32 - margin * 2.0,
                height: document.height as f32 - margin * 2.0,
            });
            frame.corner_radius = 24.0;
            frame.content_field = Some("image".into());
            create::add_image_frame(&mut document, "Image frame", source, frame)?;
        }
        self.editor.replace_document_transaction(document)?;
        self.changed(cx);
        self.schedule_content_recovery();
        Ok(())
    }

    fn adjust_active_frame_crop(
        &mut self,
        focal_x_delta: f32,
        focal_y_delta: f32,
        zoom_factor: f32,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        anyhow::ensure!(
            focal_x_delta.is_finite() && focal_y_delta.is_finite() && zoom_factor.is_finite(),
            "Frame crop adjustment must be finite"
        );
        anyhow::ensure!(zoom_factor > 0.0, "Frame crop zoom must be positive");
        let active_layer = self.editor.active_layer.clone();
        anyhow::ensure!(
            !create::layer_is_effectively_locked(&self.editor.document, &active_layer),
            "Unlock the selected frame or its group before adjusting its crop"
        );
        let mut document = self.editor.document.clone();
        let layer = document
            .find_layer_mut(&active_layer)
            .context("Select an image frame first")?;
        let mut frame = create::frame_spec(layer)?.context("Select an image frame first")?;
        frame.crop.focal_x = (frame.crop.focal_x + focal_x_delta).clamp(0.0, 1.0);
        frame.crop.focal_y = (frame.crop.focal_y + focal_y_delta).clamp(0.0, 1.0);
        frame.crop.zoom = (frame.crop.zoom * zoom_factor).clamp(1.0, 32.0);
        create::set_frame_crop(layer, frame.crop)?;
        self.editor.replace_document_transaction(document)?;
        self.changed(cx);
        self.sync_create()?;
        self.schedule_content_recovery();
        Ok(())
    }

    fn save_current_component(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let ids = if self.layer_selection.ids.is_empty() {
            vec![self.editor.active_layer.clone()]
        } else {
            self.layer_selection.ids.iter().cloned().collect()
        };
        for id in &ids {
            anyhow::ensure!(
                !create::layer_is_effectively_locked(&self.editor.document, id),
                "Unlock selected layers or their groups before saving a component"
            );
        }
        let layers = ids
            .iter()
            .filter_map(|id| self.editor.document.find_layer(id).cloned())
            .collect::<Vec<_>>();
        anyhow::ensure!(!layers.is_empty(), "Select at least one layer");
        let name = if layers.len() == 1 {
            layers[0].name.clone()
        } else {
            format!("{}-layer component", layers.len())
        };
        self.sync_create()?;
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        draft.define_component(name.clone(), layers)?;
        self.apply_creative_project(draft, cx)?;
        self.status = format!("Saved component: {name}");
        self.schedule_content_recovery();
        Ok(())
    }

    fn insert_saved_component(
        &mut self,
        component_id: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        self.sync_create()?;
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let page_id = draft.active_page_id().to_owned();
        create::insert_project_component(
            &mut draft,
            &page_id,
            component_id,
            &ComponentOverrides::default(),
        )?;
        self.apply_creative_project(draft, cx)?;
        self.schedule_content_recovery();
        Ok(())
    }

    fn update_selected_component_definition(
        &mut self,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        self.sync_create()?;
        let selected = self.editor.active_layer.clone();
        anyhow::ensure!(
            !create::layer_is_effectively_locked(&self.editor.document, &selected),
            "Unlock the selected component or its group before updating its definition"
        );
        let info = create::component_instance_info(&self.editor.document, &selected)?;
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let (_, current_prototype) = draft.component_snapshot(&info.component_id)?;
        let layers = create::component_definition_layers(
            &self.editor.document,
            &selected,
            &current_prototype,
        )?;
        let revision = draft.update_component(&info.component_id, layers)?;
        let report = create::propagate_project_component(&mut draft, &info.component_id)?;
        self.apply_creative_project(draft, cx)?;
        self.status = format!(
            "Saved component revision {revision}; updated {} instances",
            report.instances_updated
        );
        self.schedule_content_recovery();
        Ok(())
    }

    fn apply_selected_component_override(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        self.sync_create()?;
        let selected = self.editor.active_layer.clone();
        anyhow::ensure!(
            !create::layer_is_effectively_locked(&self.editor.document, &selected),
            "Unlock the selected component or its group before applying an override"
        );
        let mut instance = create::component_instance_info(&self.editor.document, &selected)?;
        let field = CreateDesignState::value(&self.create.design.component_override_field, cx);
        let text = CreateDesignState::value(&self.create.design.component_override_text, cx);
        let visible =
            match CreateDesignState::value(&self.create.design.component_override_visible, cx)
                .to_ascii_lowercase()
                .as_str()
            {
                "true" | "yes" | "1" => true,
                "false" | "no" | "0" => false,
                _ => anyhow::bail!("Visibility must be true or false"),
            };
        anyhow::ensure!(!field.is_empty(), "Enter a component field name");
        if text.is_empty() {
            instance.overrides.text.remove(&field);
        } else {
            instance.overrides.text.insert(field.clone(), text);
        }
        if visible {
            instance.overrides.hidden_fields.remove(&field);
        } else {
            instance.overrides.hidden_fields.insert(field.clone());
        }
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let (revision, prototype_layers) = draft.component_snapshot(&instance.component_id)?;
        create::refresh_component_instance(
            draft.active_document_mut()?,
            &selected,
            revision,
            &prototype_layers,
            &instance.overrides,
        )?;
        self.apply_creative_project(draft, cx)?;
        self.status = format!("Applied {field} override to selected component instance");
        self.schedule_content_recovery();
        Ok(())
    }

    fn propagate_saved_component(
        &mut self,
        component_id: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let mut draft = self.create.session.as_ref().unwrap().project.clone();
        let report = create::propagate_project_component(&mut draft, component_id)?;
        self.apply_creative_project(draft, cx)?;
        self.status = format!("Updated {} component instances", report.instances_updated);
        self.schedule_content_recovery();
        Ok(())
    }

    fn load_binding_work(
        &mut self,
        cx: &App,
    ) -> anyhow::Result<(BindingTable, BindingSchema, create::BindingValidation)> {
        let path = PathBuf::from(CreateDesignState::value(&self.create.design.csv_path, cx));
        anyhow::ensure!(!path.as_os_str().is_empty(), "Enter a CSV file path");
        let bytes = std::fs::read(&path)?;
        let input = std::str::from_utf8(&bytes).context("CSV must be UTF-8")?;
        let table = BindingTable::from_csv(input)?;
        let field = CreateDesignState::value(&self.create.design.csv_field, cx);
        let column = CreateDesignState::value(&self.create.design.csv_column, cx);
        anyhow::ensure!(
            field.is_empty() == column.is_empty(),
            "Enter both a template field and CSV column, or leave both empty for automatic mapping"
        );
        let schema = if field.is_empty() {
            create::auto_map_bindings(&self.editor.document, &table)
        } else {
            let (target, field) = create::binding_target_and_field(&field)?;
            BindingSchema {
                bindings: vec![FieldBinding {
                    field,
                    column,
                    target,
                    required: true,
                }],
                filename_column: table
                    .headers
                    .iter()
                    .find(|header| {
                        matches!(
                            header.to_ascii_lowercase().as_str(),
                            "filename" | "name" | "title"
                        )
                    })
                    .cloned(),
            }
        };
        anyhow::ensure!(
            !schema.bindings.is_empty(),
            "No CSV headers match native template fields; enter one field and column mapping"
        );
        let resource_ids = self
            .create
            .session
            .as_ref()
            .map(|session| {
                session
                    .project
                    .resource_summaries()
                    .into_iter()
                    .filter(|resource| resource.media_type.starts_with("image/"))
                    .map(|resource| resource.id)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let validation = create::validate_binding_rows_with_resource_ids(
            &self.editor.document,
            &schema,
            &table,
            Some(&resource_ids),
        );
        Ok((table, schema, validation))
    }

    fn binding_resources(
        &mut self,
        schema: &BindingSchema,
        table: &BindingTable,
        rows: &[usize],
    ) -> anyhow::Result<Vec<create::BindingImageResource>> {
        let ids = create::binding_image_resource_ids(schema, table, rows)?;
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let session = self
            .create
            .session
            .as_mut()
            .context("Create project closed")?;
        let summaries = session
            .project
            .resource_summaries()
            .into_iter()
            .filter(|resource| resource.media_type.starts_with("image/"))
            .map(|resource| (resource.id, resource.byte_len))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut total_bytes = 0u64;
        let mut resources = Vec::with_capacity(ids.len());
        for id in ids {
            let byte_len = *summaries.get(&id).with_context(|| {
                format!("Image resource '{id}' is not packaged with this project")
            })?;
            total_bytes = total_bytes
                .checked_add(byte_len)
                .context("Binding image resources are too large")?;
            anyhow::ensure!(
                total_bytes <= 256 * 1024 * 1024,
                "Binding image resources exceed the 256 MiB worker limit"
            );
            resources.push(create::BindingImageResource {
                bytes: session.project.resource_bytes(&id)?.to_vec(),
                id,
            });
        }
        Ok(resources)
    }

    fn validation_report(schema: &BindingSchema, validation: &create::BindingValidation) -> String {
        let valid = validation.rows.iter().filter(|row| row.valid).count();
        let invalid = validation.rows.len().saturating_sub(valid);
        let mappings = schema
            .bindings
            .iter()
            .map(|binding| {
                let target = match binding.target {
                    BindingTarget::Text => "text",
                    BindingTarget::ImageResource => "image",
                    BindingTarget::Visibility => "visible",
                    BindingTarget::AltText => "alt",
                };
                format!("{target}:{} ← {}", binding.field, binding.column)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut report = format!("Mapped {mappings}. {valid} valid rows; {invalid} invalid rows.");
        for row in validation.rows.iter().filter(|row| !row.valid).take(5) {
            let issues = row
                .issues
                .iter()
                .map(|issue| issue.message.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            report.push_str(&format!(" Row {}: {issues}.", row.row + 1));
        }
        if invalid > 5 {
            report.push_str(&format!(" {} more invalid rows.", invalid - 5));
        }
        report
    }

    fn validate_csv_rows(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let (_, schema, validation) = self.load_binding_work(cx)?;
        self.create.bulk_report = Some(Self::validation_report(&schema, &validation));
        cx.notify();
        Ok(())
    }

    fn start_bulk_design(&mut self, all_valid: bool, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.create.bulk_job.is_none(),
            "Bulk creation is already running"
        );
        self.finish_interaction(cx);
        self.ensure_create()?;
        self.sync_create()?;
        let (table, schema, validation) = self.load_binding_work(cx)?;
        self.create.bulk_report = Some(Self::validation_report(&schema, &validation));
        anyhow::ensure!(
            validation.global_issues.is_empty(),
            "Fix the CSV mapping before creating pages"
        );
        let mut rows = validation
            .rows
            .iter()
            .filter(|row| row.valid)
            .map(|row| row.row)
            .collect::<Vec<_>>();
        anyhow::ensure!(!rows.is_empty(), "There are no valid CSV rows to create");
        if !all_valid {
            rows.truncate(1);
        }
        let resources = self.binding_resources(&schema, &table, &rows)?;
        let session = self.create.session.as_ref().unwrap();
        session.project.validate()?;
        anyhow::ensure!(
            session.project.page_ids().len().saturating_add(rows.len())
                <= omuse::create_project::MAX_PAGES,
            "The valid rows would exceed the {}-page project limit",
            omuse::create_project::MAX_PAGES
        );
        let generation = session.generation;
        let create_epoch = self.create.epoch;
        let editor_revision = self.editor.revision();
        let active_page_id = session.project.active_page_id().to_owned();
        let source = self.editor.document.clone();
        let source_template_id = source
            .metadata
            .pointer("/omuseCreate/templateId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let requested = rows.len();
        std::thread::Builder::new()
            .name("omuse-bulk-design".into())
            .spawn(move || {
                let result = create::prepare_binding_pages_with_resources(
                    &source,
                    &schema,
                    &table,
                    &rows,
                    &resources,
                    || worker_cancel.load(Ordering::Relaxed),
                )
                .map(|pages| BulkDesignPrepared {
                    pages,
                    source_template_id,
                })
                .map_err(|error| format!("{error:#}"));
                let _ = sender.send(BulkDesignEvent::Finished(result));
            })?;
        self.create.bulk_job = Some(BulkDesignJob {
            cancel,
            receiver,
            detail: format!("Preparing {requested} pages…"),
            generation,
            create_epoch,
            editor_revision,
            active_page_id,
        });
        self.poll_bulk_design(cx);
        cx.notify();
        Ok(())
    }

    fn poll_bulk_design(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let keep = view
                    .update(cx, |this, cx| {
                        let finished = this
                            .create
                            .bulk_job
                            .as_mut()
                            .and_then(|job| match job.receiver.try_recv() {
                                Ok(BulkDesignEvent::Finished(result)) => Some(result),
                                Err(_) => None,
                            });
                        if let Some(result) = finished {
                            let job = this.create.bulk_job.take().unwrap();
                            let explicitly_cancelled = job.cancel.load(Ordering::Relaxed);
                            let current = this.create.session.as_ref().is_some_and(|session| {
                                session.generation == job.generation
                                    && session.project.active_page_id() == job.active_page_id
                            }) && this.create.epoch == job.create_epoch
                                && this.editor.revision() == job.editor_revision;
                            if explicitly_cancelled {
                                this.status = "Bulk creation cancelled; no pages were added".into();
                            } else if !current {
                                this.status = "Bulk creation was discarded because the project changed".into();
                            } else {
                                match result {
                                    Err(error) => this.status = format!("Bulk creation: {error}"),
                                    Ok(prepared) => {
                                        let commit = (|| -> anyhow::Result<(usize, omuse::create_project::Project)> {
                                            let mut draft = this
                                                .create
                                                .session
                                                .as_ref()
                                                .context("Create project closed")?
                                                .project
                                                .clone();
                                            let mut added = 0;
                                            for page in prepared.pages {
                                                let id = draft.add_page(page.name, page.document)?;
                                                draft.set_page_template(&id, prepared.source_template_id.as_deref())?;
                                                added += 1;
                                            }
                                            if draft.metadata.shared_background_component_id.is_some() {
                                                let first_new_page = draft.page_ids().len() - added;
                                                for page_id in draft.page_ids().into_iter().skip(first_new_page) {
                                                    create::apply_shared_background_to_page(&mut draft, &page_id)?;
                                                }
                                            }
                                            // No live state changes until every page has passed
                                            // the project limits and entered this complete draft.
                                            Ok((added, draft))
                                        })();
                                        match commit {
                                            Ok((added, draft)) => {
                                                if let Err(error) = this.apply_creative_project(draft, cx) {
                                                    this.status = format!("Bulk creation: {error:#}");
                                                } else {
                                                    this.status = format!("Created {added} pages from CSV");
                                                    this.create.bulk_report = Some(format!(
                                                        "Created {added} pages. The source page and CSV remain unchanged."
                                                    ));
                                                    this.schedule_content_recovery();
                                                }
                                            }
                                            Err(error) => {
                                                this.status = format!("Bulk creation: {error:#}");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        cx.notify();
                        this.create.bulk_job.is_some()
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    fn preview_csv_row(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        self.sync_create()?;
        let (table, schema, validation) = self.load_binding_work(cx)?;
        self.create.bulk_report = Some(Self::validation_report(&schema, &validation));
        let row = validation
            .rows
            .iter()
            .find(|row| row.valid)
            .map(|row| row.row)
            .context("CSV contains no valid data rows")?;
        let resources = self.binding_resources(&schema, &table, &[row])?;
        let images = create::decode_binding_image_resources(&resources, || false)?;
        let document = create::apply_binding_row_with_images(
            &self.editor.document,
            &schema,
            &table,
            row,
            |id| {
                images
                    .get(id)
                    .cloned()
                    .with_context(|| format!("Image resource '{id}' is unavailable"))
            },
        )?;
        // A CSV preview must never become the source for subsequent rows.
        // `changed`/page activation refresh the real canvas, and the Editor
        // close action explicitly refreshes it as well.
        self.display.replace(&omuse::raster::composite(&document));
        self.status = format!(
            "Previewing row {} of {} — source page is unchanged",
            row + 1,
            table.rows.len()
        );
        cx.notify();
        Ok(())
    }
}
