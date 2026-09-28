//! Native, editable content-creation primitives.
//!
//! Templates in this module are compositions of the existing raster-backed
//! live text and shape objects. Frames retain their source pixels and crop
//! transform, layout resizing changes object placement rather than flattening,
//! components preserve named overrides, and bulk binding operates on semantic
//! field names.

use crate::{
    create_project::{BrandKit, BrandTextStyle, Project},
    model::{Document, Layer, valid_dimensions},
    objects::{
        self, LiveShapeKind, LiveShapeStyle, LiveTextStyle, ObjectPoint, ObjectSize, TextAlignment,
    },
};
use anyhow::{Context, Result, bail, ensure};
use image::{ImageDecoder, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::Cursor,
};

const CREATE_METADATA: &str = "omuseCreate";
const MAX_BINDING_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_BINDING_ROWS: usize = 10_000;
const MAX_BINDING_COLUMNS: usize = 256;
const MAX_BINDING_FIELD_BYTES: usize = 64 * 1024;
const MAX_BINDING_IMAGE_PIXELS: u64 = 16_777_216;
const MAX_BINDING_IMAGE_RESOURCE_BYTES_TOTAL: u64 = 256 * 1024 * 1024;
const MAX_BINDING_OUTPUT_IMAGE_PIXELS: u64 = 64_000_000;
const MAX_BINDING_OUTPUT_CANVAS_PIXELS: u64 = 64_000_000;

/// Return whether a layer is locked either directly or through one of its
/// containing groups. Create controls use this instead of inspecting only a
/// selected child, so protected backgrounds and logo components remain
/// protected in the Layers panel as well as on the canvas.
pub fn layer_is_effectively_locked(document: &Document, id: &str) -> bool {
    fn visit(layers: &[Layer], id: &str, parent_locked: bool) -> Option<bool> {
        for layer in layers {
            let locked = parent_locked || layer.locked;
            if layer.id == id {
                return Some(locked);
            }
            if let Some(value) = visit(&layer.children, id, locked) {
                return Some(value);
            }
        }
        None
    }
    visit(&document.layers, id, false).unwrap_or(false)
}

/// CSV columns bind ordinary text fields by their field name. Prefix a field
/// with `image:`, `visible:`, or `alt:` for an explicit image resource,
/// visibility, or alt-text binding. Image cells are project resource UUIDs;
/// CSV never accepts a local path or URL as an image source.
pub const BULK_BINDING_CONVENTION: &str = "text field, image:<field>, visible:<field>, alt:<field>; image values are packaged project resource IDs";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub width: u32,
    pub height: u32,
    pub description: &'static str,
}

pub const TEMPLATE_CATALOG: [TemplateDefinition; 20] = [
    TemplateDefinition {
        id: "editorial-quote",
        name: "Editorial Quote",
        category: "Testimonial",
        width: 1080,
        height: 1080,
        description: "Editorial quotation with an offset color rail.",
    },
    TemplateDefinition {
        id: "customer-voice",
        name: "Customer Voice",
        category: "Testimonial",
        width: 1080,
        height: 1350,
        description: "Portrait testimonial with a framed author image.",
    },
    TemplateDefinition {
        id: "product-arrival",
        name: "Product Arrival",
        category: "Announcement",
        width: 1080,
        height: 1080,
        description: "Product announcement with an asymmetric image field.",
    },
    TemplateDefinition {
        id: "bold-notice",
        name: "Bold Notice",
        category: "Announcement",
        width: 1080,
        height: 1350,
        description: "Large typographic notice and compact supporting copy.",
    },
    TemplateDefinition {
        id: "event-poster",
        name: "Event Poster",
        category: "Event",
        width: 1080,
        height: 1350,
        description: "Event title, date block and venue details.",
    },
    TemplateDefinition {
        id: "workshop-invite",
        name: "Workshop Invite",
        category: "Event",
        width: 1080,
        height: 1080,
        description: "Friendly workshop invitation with a modular agenda card.",
    },
    TemplateDefinition {
        id: "lesson-cover",
        name: "Lesson Cover",
        category: "Education",
        width: 1080,
        height: 1080,
        description: "Numbered educational carousel cover.",
    },
    TemplateDefinition {
        id: "lesson-step",
        name: "Lesson Step",
        category: "Education",
        width: 1080,
        height: 1080,
        description: "Educational step with a callout and progress marker.",
    },
    TemplateDefinition {
        id: "case-study-result",
        name: "Case Study Result",
        category: "Case study",
        width: 1080,
        height: 1350,
        description: "Outcome-led case study with a prominent metric.",
    },
    TemplateDefinition {
        id: "before-after",
        name: "Before and After",
        category: "Case study",
        width: 1080,
        height: 1080,
        description: "Balanced comparison layout with two replaceable frames.",
    },
    TemplateDefinition {
        id: "limited-offer",
        name: "Limited Offer",
        category: "Offer",
        width: 1080,
        height: 1350,
        description: "Offer card with exact native price and terms.",
    },
    TemplateDefinition {
        id: "service-feature",
        name: "Service Feature",
        category: "Offer",
        width: 1080,
        height: 1080,
        description: "Service feature with three editable benefits.",
    },
    TemplateDefinition {
        id: "team-spotlight",
        name: "Team Spotlight",
        category: "Profile",
        width: 1080,
        height: 1350,
        description: "Team profile with a tall portrait frame.",
    },
    TemplateDefinition {
        id: "milestone-marker",
        name: "Milestone Marker",
        category: "Announcement",
        width: 1080,
        height: 1080,
        description: "Celebratory milestone with a restrained badge.",
    },
    TemplateDefinition {
        id: "video-title",
        name: "Video Title",
        category: "Thumbnail",
        width: 1920,
        height: 1080,
        description: "Wide video thumbnail with safe-area title treatment.",
    },
    TemplateDefinition {
        id: "podcast-episode",
        name: "Podcast Episode",
        category: "Thumbnail",
        width: 1080,
        height: 1080,
        description: "Podcast cover with episode number and guest frame.",
    },
    TemplateDefinition {
        id: "article-card",
        name: "Article Card",
        category: "Editorial",
        width: 1200,
        height: 630,
        description: "Link-card composition with a wide image and headline.",
    },
    TemplateDefinition {
        id: "minimal-story",
        name: "Minimal Story",
        category: "Story",
        width: 1080,
        height: 1920,
        description: "Vertical story with generous readable spacing.",
    },
    TemplateDefinition {
        id: "photo-grid",
        name: "Photo Grid",
        category: "Collage",
        width: 1080,
        height: 1350,
        description: "Three-frame editorial photo grid.",
    },
    TemplateDefinition {
        id: "action-card",
        name: "Action Card",
        category: "Call to action",
        width: 1080,
        height: 1080,
        description: "Focused call to action with a native button object.",
    },
];

pub fn templates() -> &'static [TemplateDefinition] {
    &TEMPLATE_CATALOG
}

/// Return the named editable text fields provided by a template without
/// constructing or rendering a document. This keeps assistant hints and CSV
/// setup accurate while leaving template construction on the user action
/// path.
pub fn template_text_fields(template_id: &str) -> Result<&'static [&'static str]> {
    const STANDARD: &[&str] = &["eyebrow", "headline", "body"];
    const NUMBERED: &[&str] = &["eyebrow", "number", "headline", "body"];
    const COMPARISON: &[&str] = &["headline", "body"];

    match template_id {
        "lesson-cover" => Ok(NUMBERED),
        "before-after" | "photo-grid" => Ok(COMPARISON),
        id if templates().iter().any(|template| template.id == id) => Ok(STANDARD),
        _ => bail!("Template not found"),
    }
}

/// A built-in starter kit matching Omuse's Sugata retro reference. It remains
/// an ordinary editable BrandKit after insertion, so projects can rename or
/// adjust every token without a hidden theme dependency.
pub fn sugata_brand_kit() -> BrandKit {
    let mut brand = BrandKit::new("Sugata");
    brand.colors.extend([
        ("paper".into(), [0xF7, 0xEB, 0xD6, 0xFF]),
        ("background".into(), [0xF7, 0xEB, 0xD6, 0xFF]),
        ("ink".into(), [0x2B, 0x16, 0x20, 0xFF]),
        ("plum".into(), [0x2B, 0x16, 0x20, 0xFF]),
        ("gold".into(), [0xF2, 0xB3, 0x3D, 0xFF]),
        ("coral".into(), [0xF0, 0x88, 0x5A, 0xFF]),
        ("orange".into(), [0xE2, 0x62, 0x2A, 0xFF]),
        ("pink".into(), [0xFF, 0x2E, 0x88, 0xFF]),
        ("accent".into(), [0xE2, 0x62, 0x2A, 0xFF]),
        ("secondary".into(), [0xF2, 0xB3, 0x3D, 0xFF]),
    ]);
    brand.fonts.heading = "Outfit".into();
    brand.fonts.body = "Outfit".into();
    for (role, size, leading, tracking) in [
        ("display", 92.0, 94.0, -1.0),
        ("heading", 64.0, 68.0, -0.4),
        ("heading_on_dark", 64.0, 68.0, -0.4),
        ("body", 28.0, 36.0, 0.0),
        ("body_on_dark", 28.0, 36.0, 0.0),
        ("label", 22.0, 27.0, 2.0),
        ("label_on_dark", 22.0, 27.0, 2.0),
    ] {
        brand.text_styles.insert(
            role.into(),
            BrandTextStyle {
                font: "Outfit".into(),
                size,
                tracking,
                leading,
            },
        );
    }
    brand.spacing.extend([
        ("tight".into(), 12.0),
        ("standard".into(), 24.0),
        ("section".into(), 48.0),
    ]);
    brand
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAnchor {
    Left,
    Center,
    Right,
    Relative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAnchor {
    Top,
    Center,
    Bottom,
    Relative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutScale {
    Preserve,
    Uniform,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerLayoutRule {
    pub horizontal: HorizontalAnchor,
    pub vertical: VerticalAnchor,
    pub scale: LayoutScale,
}

impl Default for LayerLayoutRule {
    fn default() -> Self {
        Self {
            horizontal: HorizontalAnchor::Relative,
            vertical: VerticalAnchor::Relative,
            scale: LayoutScale::Uniform,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeStrategy {
    /// Respect each object's stored layout rule and keep live objects editable.
    Adapt,
    /// Apply a uniform fit to every object around the old canvas origin.
    ScaleToFit,
    /// Scale horizontal and vertical geometry independently.
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropPlacement {
    pub focal_x: f32,
    pub focal_y: f32,
    pub zoom: f32,
}

impl Default for CropPlacement {
    fn default() -> Self {
        Self {
            focal_x: 0.5,
            focal_y: 0.5,
            zoom: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSpec {
    pub id: String,
    pub bounds: FrameBounds,
    #[serde(default)]
    pub crop: CropPlacement,
    #[serde(default)]
    pub corner_radius: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
}

impl FrameSpec {
    pub fn new(bounds: FrameBounds) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string().to_uppercase(),
            bounds,
            crop: CropPlacement::default(),
            corner_radius: 0.0,
            content_field: None,
            alt_text: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        uuid::Uuid::parse_str(&self.id).context("Invalid frame ID")?;
        ensure!(
            [
                self.bounds.x,
                self.bounds.y,
                self.bounds.width,
                self.bounds.height,
                self.corner_radius
            ]
            .into_iter()
            .all(f32::is_finite),
            "Frame geometry must be finite"
        );
        ensure!(
            self.bounds.x.abs() <= 1_000_000.0 && self.bounds.y.abs() <= 1_000_000.0,
            "Frame origin is out of range"
        );
        ensure!(
            self.bounds.width >= 1.0 && self.bounds.height >= 1.0,
            "Frame dimensions must be positive"
        );
        ensure!(
            self.bounds.width <= 30_000.0 && self.bounds.height <= 30_000.0,
            "Frame is too large"
        );
        ensure!(
            self.corner_radius >= 0.0
                && self.corner_radius <= self.bounds.width.min(self.bounds.height) * 0.5,
            "Invalid frame corner radius"
        );
        ensure!(
            self.crop.focal_x.is_finite() && (0.0..=1.0).contains(&self.crop.focal_x),
            "Invalid horizontal focal point"
        );
        ensure!(
            self.crop.focal_y.is_finite() && (0.0..=1.0).contains(&self.crop.focal_y),
            "Invalid vertical focal point"
        );
        ensure!(
            self.crop.zoom.is_finite() && (1.0..=32.0).contains(&self.crop.zoom),
            "Frame zoom must be 1-32"
        );
        if let Some(field) = &self.content_field {
            ensure!(valid_field_name(field), "Invalid frame content field");
        }
        if let Some(alt) = &self.alt_text {
            ensure!(alt.len() <= 16_384, "Frame alt text is too long");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentOverrides {
    #[serde(default)]
    pub text: BTreeMap<String, String>,
    #[serde(default)]
    pub hidden_fields: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComponentPropagationReport {
    pub instances_updated: usize,
    pub overrides_reapplied: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrandApplicationReport {
    pub colors_updated: usize,
    pub text_styles_updated: usize,
    /// Fixed native text boxes whose branded token needed a readable-size fit.
    pub text_fitted: usize,
    pub spacing_updated: usize,
}

pub fn instantiate_template(template_id: &str, brand: Option<&BrandKit>) -> Result<Document> {
    let definition = templates()
        .iter()
        .find(|template| template.id == template_id)
        .context("Template not found")?;
    let mut document = Document::new(definition.width, definition.height);
    document.name = definition.name.into();
    document.layers.clear();
    document.metadata[CREATE_METADATA] = json!({
        "templateId": definition.id,
        "templateName": definition.name,
        "category": definition.category,
    });

    let accent = [211, 91, 72, 255];
    let dark = [29, 31, 34, 255];
    let paper = [246, 240, 230, 255];
    let secondary = [232, 181, 104, 255];
    add_shape(
        &mut document,
        "Background",
        (0.0, 0.0, 1.0, 1.0),
        paper,
        "background",
        None,
        fixed_rule(),
    )?;

    let index = templates()
        .iter()
        .position(|template| template.id == template_id)
        .unwrap();
    let title = [
        "Words worth keeping",
        "What our customers notice",
        "Something new has arrived",
        "A clear announcement",
        "Make room for better ideas",
        "A practical workshop",
        "Five ways to work with focus",
        "Start with the signal",
        "A result you can measure",
        "See what thoughtful change makes",
        "A considered offer",
        "Everything you need to begin",
        "Meet the people behind the work",
        "A meaningful milestone",
        "The design decision nobody explains",
        "A generous conversation",
        "Ideas for a more useful week",
        "A quiet moment for your next idea",
        "Three views, one story",
        "Ready to make something useful?",
    ][index];
    let eyebrow = [
        "PERSPECTIVE",
        "CUSTOMER STORY",
        "NOW AVAILABLE",
        "NEWS",
        "FRIDAY · 6:30 PM",
        "SMALL GROUP · 90 MIN",
        "FIELD NOTES 01",
        "STEP 02 OF 05",
        "CASE STUDY",
        "THEN / NOW",
        "THIS WEEK",
        "A SIMPLE SYSTEM",
        "TEAM SPOTLIGHT",
        "THANK YOU",
        "NEW VIDEO",
        "EPISODE 24",
        "READING LIST",
        "STORY NOTE",
        "COLLECTION 03",
        "START HERE",
    ][index];
    let body = [
        "A strong point of view becomes useful when there is room around it.",
        "“The process felt calm, clear and completely ours.”",
        "Built with care, ready for the everyday.",
        "The details are changing. The purpose stays clear.",
        "An evening of conversation, making and shared practice.",
        "Bring one challenge. Leave with a plan you can use.",
        "A short, practical carousel for doing better work.",
        "Remove what competes with the one thing people need to understand.",
        "Clarity improved completion by 38% without adding another step.",
        "The same material, framed with more space and stronger hierarchy.",
        "Save 20% on the complete collection while it is available.",
        "Strategy, design and a repeatable handover in one focused engagement.",
        "A thoughtful collaborator who makes complex work feel possible.",
        "Ten years of making useful things with good people.",
        "A practical look at the choices behind clear visual work.",
        "Notes on craft, confidence and leaving room for surprise.",
        "A compact guide to making space for work that matters.",
        "Keep this for later, or share it with someone who needs the reminder.",
        "Replace each frame while the rhythm and spacing stay intact.",
        "Tell us what you are making and we will help shape the next step.",
    ][index];

    match index {
        0 | 3 | 7 | 13 | 17 | 19 => {
            add_shape(
                &mut document,
                "Accent rail",
                (0.07, 0.09, 0.025, 0.75),
                accent,
                "accent",
                None,
                left_rule(),
            )?;
            add_text(
                &mut document,
                "Eyebrow",
                eyebrow,
                (0.13, 0.11, 0.72, 0.08),
                30.0,
                dark,
                "label",
                "eyebrow",
                left_rule(),
            )?;
            add_text(
                &mut document,
                "Headline",
                title,
                (0.13, 0.25, 0.72, 0.34),
                if definition.height > definition.width {
                    78.0
                } else {
                    72.0
                },
                dark,
                "heading",
                "headline",
                left_rule(),
            )?;
            add_text(
                &mut document,
                "Body",
                body,
                (0.13, 0.65, 0.66, 0.18),
                32.0,
                dark,
                "body",
                "body",
                left_rule(),
            )?;
            add_shape(
                &mut document,
                "Action",
                (0.67, 0.87, 0.24, 0.07),
                accent,
                "accent",
                Some("cta"),
                right_rule(),
            )?;
        }
        1 | 2 | 8 | 12 | 15 => {
            add_shape(
                &mut document,
                "Image field",
                (0.48, 0.0, 0.52, 1.0),
                secondary,
                "secondary",
                Some("image"),
                right_stretch_rule(),
            )?;
            add_text(
                &mut document,
                "Eyebrow",
                eyebrow,
                (0.08, 0.1, 0.34, 0.08),
                26.0,
                accent,
                "label",
                "eyebrow",
                left_rule(),
            )?;
            add_text(
                &mut document,
                "Headline",
                title,
                (0.08, 0.23, 0.36, 0.34),
                60.0,
                dark,
                "heading",
                "headline",
                left_rule(),
            )?;
            add_text(
                &mut document,
                "Body",
                body,
                (0.08, 0.65, 0.34, 0.18),
                27.0,
                dark,
                "body",
                "body",
                left_rule(),
            )?;
        }
        4 | 5 | 10 | 11 => {
            add_shape(
                &mut document,
                "Header field",
                (0.0, 0.0, 1.0, 0.33),
                dark,
                "ink",
                None,
                top_stretch_rule(),
            )?;
            add_text(
                &mut document,
                "Eyebrow",
                eyebrow,
                (0.08, 0.08, 0.7, 0.07),
                28.0,
                paper,
                "label_on_dark",
                "eyebrow",
                top_rule(),
            )?;
            add_text(
                &mut document,
                "Headline",
                title,
                (0.08, 0.16, 0.82, 0.2),
                66.0,
                paper,
                "heading_on_dark",
                "headline",
                top_rule(),
            )?;
            add_text(
                &mut document,
                "Body",
                body,
                (0.08, 0.47, 0.74, 0.18),
                30.0,
                dark,
                "body",
                "body",
                left_rule(),
            )?;
            add_shape(
                &mut document,
                "Detail card",
                (0.08, 0.72, 0.84, 0.16),
                secondary,
                "secondary",
                Some("detail"),
                bottom_stretch_rule(),
            )?;
        }
        6 => {
            add_text(
                &mut document,
                "Issue",
                eyebrow,
                (0.08, 0.08, 0.72, 0.08),
                30.0,
                accent,
                "label",
                "eyebrow",
                top_rule(),
            )?;
            add_text(
                &mut document,
                "Number",
                "05",
                (0.64, 0.14, 0.29, 0.35),
                250.0,
                secondary,
                "display",
                "number",
                right_rule(),
            )?;
            add_text(
                &mut document,
                "Headline",
                title,
                (0.08, 0.29, 0.61, 0.31),
                72.0,
                dark,
                "heading",
                "headline",
                left_rule(),
            )?;
            add_text(
                &mut document,
                "Body",
                body,
                (0.08, 0.69, 0.68, 0.15),
                30.0,
                dark,
                "body",
                "body",
                bottom_rule(),
            )?;
        }
        9 => {
            add_shape(
                &mut document,
                "Before frame",
                (0.06, 0.21, 0.41, 0.57),
                secondary,
                "secondary",
                Some("before_image"),
                left_stretch_rule(),
            )?;
            add_shape(
                &mut document,
                "After frame",
                (0.53, 0.21, 0.41, 0.57),
                accent,
                "accent",
                Some("after_image"),
                right_stretch_rule(),
            )?;
            add_text(
                &mut document,
                "Headline",
                title,
                // A two-line Sugata heading needs 136 px before the live
                // text inset. Its upper placement also keeps the scaled
                // wide layout clear of the comparison frames.
                (0.07, 0.03, 0.84, 0.145),
                52.0,
                dark,
                "heading",
                "headline",
                top_rule(),
            )?;
            add_text(
                &mut document,
                "Body",
                body,
                (0.08, 0.83, 0.8, 0.1),
                25.0,
                dark,
                "body",
                "body",
                bottom_rule(),
            )?;
        }
        14 | 16 => {
            // Video Title starts as a 16:9 composition. Its text safe area
            // must follow the right panel's proportional boundary when the
            // catalog adapts it to square or story canvases. Article Card
            // already starts near those target widths and retains its right
            // edge semantics.
            let text_layout = if index == 14 {
                template_relative_rule()
            } else {
                right_rule()
            };
            add_shape(
                &mut document,
                "Image field",
                (0.0, 0.0, 0.58, 1.0),
                secondary,
                "secondary",
                Some("image"),
                left_stretch_rule(),
            )?;
            add_shape(
                &mut document,
                "Text field",
                (0.58, 0.0, 0.42, 1.0),
                dark,
                "ink",
                None,
                right_stretch_rule(),
            )?;
            add_text(
                &mut document,
                "Eyebrow",
                eyebrow,
                (0.63, 0.1, 0.31, 0.08),
                26.0,
                secondary,
                "label_on_dark",
                "eyebrow",
                text_layout,
            )?;
            add_text(
                &mut document,
                "Headline",
                title,
                (0.63, 0.25, 0.31, 0.42),
                if index == 14 { 70.0 } else { 43.0 },
                paper,
                "heading_on_dark",
                "headline",
                text_layout,
            )?;
            add_text(
                &mut document,
                "Body",
                body,
                // The Sugata body token is 28 px / 36 px leading. Reserve
                // three full lines so the default article summary remains
                // complete before and after a layout adaptation.
                (0.63, 0.73, 0.3, 0.22),
                24.0,
                paper,
                "body_on_dark",
                "body",
                text_layout,
            )?;
        }
        18 => {
            add_shape(
                &mut document,
                "Photo one",
                (0.06, 0.07, 0.56, 0.51),
                secondary,
                "secondary",
                Some("image_1"),
                left_stretch_rule(),
            )?;
            add_shape(
                &mut document,
                "Photo two",
                (0.66, 0.07, 0.28, 0.24),
                accent,
                "accent",
                Some("image_2"),
                right_rule(),
            )?;
            add_shape(
                &mut document,
                "Photo three",
                (0.66, 0.34, 0.28, 0.24),
                dark,
                "ink",
                Some("image_3"),
                right_rule(),
            )?;
            add_text(
                &mut document,
                "Headline",
                title,
                (0.07, 0.66, 0.8, 0.12),
                58.0,
                dark,
                "heading",
                "headline",
                bottom_rule(),
            )?;
            add_text(
                &mut document,
                "Body",
                body,
                (0.07, 0.82, 0.8, 0.09),
                27.0,
                dark,
                "body",
                "body",
                bottom_rule(),
            )?;
        }
        _ => unreachable!(),
    }
    if let Some(brand) = brand {
        apply_brand(&mut document, brand)?;
    }
    Ok(document)
}

fn add_shape(
    document: &mut Document,
    name: &str,
    normalized: (f32, f32, f32, f32),
    color: [u8; 4],
    color_role: &str,
    field: Option<&str>,
    layout: LayerLayoutRule,
) -> Result<String> {
    let x = normalized.0 * document.width as f32;
    let y = normalized.1 * document.height as f32;
    let width = (normalized.2 * document.width as f32).round().max(1.0) as u32;
    let height = (normalized.3 * document.height as f32).round().max(1.0) as u32;
    let mut layer = objects::live_shape_layer(
        name,
        ObjectPoint { x, y },
        width,
        height,
        LiveShapeStyle {
            kind: LiveShapeKind::Rectangle,
            red: color[0] as f32 / 255.0,
            green: color[1] as f32 / 255.0,
            blue: color[2] as f32 / 255.0,
            corner_radius: if field == Some("cta") { 18.0 } else { 0.0 },
            line_width: None,
            start: None,
            end: None,
        },
    )?;
    tag_layer(&mut layer, field, Some(color_role), None, layout)?;
    let id = layer.id.clone();
    document.layers.push(layer);
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
fn add_text(
    document: &mut Document,
    name: &str,
    content: &str,
    normalized: (f32, f32, f32, f32),
    font_size: f32,
    color: [u8; 4],
    text_role: &str,
    field: &str,
    layout: LayerLayoutRule,
) -> Result<String> {
    let mut layer = objects::live_text_layer(
        name,
        ObjectPoint {
            x: normalized.0 * document.width as f32,
            y: normalized.1 * document.height as f32,
        },
        LiveTextStyle {
            content: content.into(),
            font_name: "sans-serif".into(),
            font_size,
            red: color[0] as f32 / 255.0,
            green: color[1] as f32 / 255.0,
            blue: color[2] as f32 / 255.0,
            alignment: TextAlignment::Left,
            tracking: if text_role.contains("label") {
                2.0
            } else {
                0.0
            },
            leading: font_size
                * if text_role.contains("heading") {
                    1.04
                } else {
                    1.24
                },
            box_size: Some(ObjectSize {
                width: (normalized.2 * document.width as f32).max(16.0),
                height: (normalized.3 * document.height as f32).max(16.0),
            }),
            runs: vec![],
        },
    )?;
    let color_role = if text_role.contains("on_dark") {
        "paper"
    } else if text_role == "label" {
        "accent"
    } else {
        "ink"
    };
    tag_layer(
        &mut layer,
        Some(field),
        Some(color_role),
        Some(text_role),
        layout,
    )?;
    let (spacing_role, default_spacing) = default_spacing_for_text_role(text_role);
    let spacing_base_y = layer.offset_y - default_spacing;
    let metadata = create_metadata_mut(&mut layer);
    metadata.insert("spacingRole".into(), json!(spacing_role));
    metadata.insert("spacingBaseY".into(), json!(spacing_base_y));
    let id = layer.id.clone();
    document.layers.push(layer);
    Ok(id)
}

fn default_spacing_for_text_role(role: &str) -> (&'static str, f32) {
    if role.contains("eyebrow") || role.contains("label") || role == "number" {
        ("tight", 12.0)
    } else if role.contains("body") {
        ("section", 48.0)
    } else {
        ("standard", 24.0)
    }
}

fn tag_layer(
    layer: &mut Layer,
    field: Option<&str>,
    color_role: Option<&str>,
    text_role: Option<&str>,
    layout: LayerLayoutRule,
) -> Result<()> {
    if let Some(field) = field {
        ensure!(valid_field_name(field), "Invalid content field name");
    }
    layer.metadata[CREATE_METADATA] = json!({
        "field": field,
        "colorRole": color_role,
        "textRole": text_role,
        "layout": layout,
    });
    Ok(())
}

fn fixed_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Left,
        vertical: VerticalAnchor::Top,
        scale: LayoutScale::Stretch,
    }
}
fn left_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Left,
        vertical: VerticalAnchor::Relative,
        scale: LayoutScale::Uniform,
    }
}
fn right_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Right,
        vertical: VerticalAnchor::Relative,
        scale: LayoutScale::Uniform,
    }
}
fn template_relative_rule() -> LayerLayoutRule {
    // This is only used by catalog construction where an authored fraction
    // must survive an aspect-ratio change. Public anchor values continue to
    // retain their documented resize behavior for user-authored layers.
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Relative,
        vertical: VerticalAnchor::Relative,
        scale: LayoutScale::Uniform,
    }
}
fn top_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Relative,
        // These private helpers tag catalog layers only. Template positions
        // must move with both canvas axes when their text raster is scaled
        // uniformly; public Top/Bottom anchor semantics stay unchanged for
        // user-authored layers.
        vertical: VerticalAnchor::Relative,
        scale: LayoutScale::Uniform,
    }
}
fn bottom_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Relative,
        vertical: VerticalAnchor::Relative,
        scale: LayoutScale::Uniform,
    }
}
fn top_stretch_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Relative,
        vertical: VerticalAnchor::Top,
        scale: LayoutScale::Stretch,
    }
}
fn bottom_stretch_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Relative,
        // Detail cards are authored at a fractional position, rather than
        // flush to the canvas edge. Keep that relationship through aspect
        // changes so they cannot rise into their body copy.
        vertical: VerticalAnchor::Relative,
        scale: LayoutScale::Stretch,
    }
}
fn left_stretch_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        horizontal: HorizontalAnchor::Left,
        vertical: VerticalAnchor::Relative,
        scale: LayoutScale::Stretch,
    }
}
fn right_stretch_rule() -> LayerLayoutRule {
    LayerLayoutRule {
        // A stretched width is already derived from the target width. Its
        // origin must therefore scale relatively, rather than also receiving
        // the right-anchor delta and leaving a visible strip at one edge.
        horizontal: HorizontalAnchor::Relative,
        vertical: VerticalAnchor::Relative,
        scale: LayoutScale::Stretch,
    }
}

pub fn set_layout_rule(layer: &mut Layer, rule: LayerLayoutRule) -> Result<()> {
    let metadata = create_metadata_mut(layer);
    metadata.insert("layout".into(), serde_json::to_value(rule)?);
    Ok(())
}

pub fn layout_rule(layer: &Layer) -> Result<Option<LayerLayoutRule>> {
    layer
        .metadata
        .get(CREATE_METADATA)
        .and_then(|value| value.get("layout"))
        .filter(|value| !value.is_null())
        .map(|value| serde_json::from_value(value.clone()).context("Invalid Create layout rule"))
        .transpose()
}

pub fn apply_brand(document: &mut Document, brand: &BrandKit) -> Result<BrandApplicationReport> {
    brand.validate()?;
    // The fixed-box fit check may reject a later native field. Apply to a
    // shallow document snapshot first so public callers never observe the
    // colors, fonts, or spacing from an earlier field on failure.
    let mut draft = document.clone();
    let mut report = BrandApplicationReport::default();
    apply_brand_layers(&mut draft.layers, brand, &mut report)?;
    draft.metadata[CREATE_METADATA]["brandId"] = json!(brand.id);
    *document = draft;
    Ok(report)
}

pub fn apply_brand_to_project(
    project: &mut Project,
    brand_id: &str,
) -> Result<BrandApplicationReport> {
    // UI callers already stage a draft, but this public helper is also used
    // by fixtures and integrations. Preserve the same all-pages-or-none
    // outcome when one later page cannot fit the selected brand.
    let mut draft = project.clone();
    let brand = draft
        .brand_kits
        .iter()
        .find(|brand| brand.id == brand_id)
        .cloned()
        .context("Brand kit not found")?;
    let mut total = BrandApplicationReport::default();
    for page_id in draft.page_ids() {
        let report = apply_brand(draft.page_document_mut(&page_id)?, &brand)?;
        total.colors_updated += report.colors_updated;
        total.text_styles_updated += report.text_styles_updated;
        total.text_fitted += report.text_fitted;
        total.spacing_updated += report.spacing_updated;
    }
    draft.active_brand_id = Some(brand_id.to_owned());
    *project = draft;
    Ok(total)
}

fn apply_brand_layers(
    layers: &mut [Layer],
    brand: &BrandKit,
    report: &mut BrandApplicationReport,
) -> Result<()> {
    for layer in layers {
        let color_role = create_value(layer, "colorRole")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let text_role = create_value(layer, "textRole")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let spacing_role = create_value(layer, "spacingRole")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let spacing_base_y = create_value(layer, "spacingBaseY")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .map(|value| value as f32);
        if let Some(spacing) = spacing_role
            .as_deref()
            .and_then(|role| brand.spacing.get(role))
            && let Some(base_y) = spacing_base_y
        {
            layer.offset_y = base_y + spacing;
            report.spacing_updated += 1;
        }
        if let Some(mut style) = objects::live_shape(layer)? {
            if let Some(color) = color_role
                .as_deref()
                .and_then(|role| brand.colors.get(role))
            {
                (style.red, style.green, style.blue) = rgb(*color);
                let (width, height) = layer
                    .image
                    .as_ref()
                    .context("Live shape cache is missing")?
                    .dimensions();
                objects::set_live_shape(layer, style, width, height)?;
                report.colors_updated += 1;
            }
        } else if let Some(mut style) = objects::live_text(layer)? {
            let mut changed_style = false;
            let mut style_token_updated = false;
            if let Some(color) = color_role
                .as_deref()
                .and_then(|role| brand.colors.get(role))
            {
                (style.red, style.green, style.blue) = rgb(*color);
                report.colors_updated += 1;
                changed_style = true;
            }
            if let Some(role) = text_role.as_deref() {
                if let Some(token) = brand.text_styles.get(role) {
                    style.font_name = token.font.clone();
                    style.font_size = token.size;
                    style.tracking = token.tracking;
                    style.leading = token.leading;
                } else if role.contains("heading") || role == "display" {
                    style.font_name = brand.fonts.heading.clone();
                } else {
                    style.font_name = brand.fonts.body.clone();
                }
                report.text_styles_updated += 1;
                changed_style = true;
                style_token_updated = true;
            }
            if changed_style {
                // Fitting is intentionally limited to the native fixed boxes
                // whose text token this brand just changed. Unboxed and
                // arbitrary user-authored text keeps its authored size.
                if style_token_updated
                    && style.box_size.is_some()
                    && objects::text_layout_report(&style)?.overflows()
                {
                    ensure!(
                        style.font_size >= 16.0,
                        "Branded text field '{}' overflows its fixed box and is already below the 16 px readable minimum",
                        layer_field(layer).unwrap_or("unnamed")
                    );
                    let fit = objects::fit_text_to_box(&style, 16.0)?;
                    ensure!(
                        !fit.report.overflows(),
                        "Branded text field '{}' cannot fit its fixed box at the 16 px readable minimum",
                        layer_field(layer).unwrap_or("unnamed")
                    );
                    if fit.fitted {
                        report.text_fitted += 1;
                    }
                    style = fit.style;
                }
                objects::set_live_text(layer, style)?;
            }
        }
        apply_brand_layers(&mut layer.children, brand, report)?;
    }
    Ok(())
}

fn rgb(color: [u8; 4]) -> (f32, f32, f32) {
    (
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
    )
}

pub fn add_image_frame(
    document: &mut Document,
    name: impl Into<String>,
    image: RgbaImage,
    spec: FrameSpec,
) -> Result<String> {
    ensure!(
        valid_dimensions(image.width(), image.height()),
        "Frame image exceeds supported bounds"
    );
    spec.validate()?;
    let mut layer = Layer::group(name);
    layer.metadata = json!({});
    layer.image = Some(image.into());
    place_frame_layer(&mut layer, &spec)?;
    let id = layer.id.clone();
    document.layers.push(layer);
    Ok(id)
}

pub fn frame_spec(layer: &Layer) -> Result<Option<FrameSpec>> {
    layer
        .metadata
        .get(CREATE_METADATA)
        .and_then(|value| value.get("frame"))
        .filter(|value| !value.is_null())
        .map(|value| {
            let frame: FrameSpec =
                serde_json::from_value(value.clone()).context("Invalid image frame metadata")?;
            frame.validate()?;
            Ok(frame)
        })
        .transpose()
}

pub fn replace_frame_image(layer: &mut Layer, image: RgbaImage) -> Result<()> {
    ensure!(
        valid_dimensions(image.width(), image.height()),
        "Frame image exceeds supported bounds"
    );
    let frame = frame_spec(layer)?.context("Layer is not an image frame")?;
    layer.image = Some(image.into());
    place_frame_layer(layer, &frame)
}

pub fn set_frame_crop(layer: &mut Layer, crop: CropPlacement) -> Result<()> {
    let mut frame = frame_spec(layer)?.context("Layer is not an image frame")?;
    frame.crop = crop;
    place_frame_layer(layer, &frame)
}

fn place_frame_layer(layer: &mut Layer, frame: &FrameSpec) -> Result<()> {
    frame.validate()?;
    let image = layer
        .image
        .as_ref()
        .context("Image frame has no source pixels")?;
    let source_width = image.width() as f32;
    let source_height = image.height() as f32;
    let scale = (frame.bounds.width / source_width).max(frame.bounds.height / source_height)
        * frame.crop.zoom;
    let placed_width = source_width * scale;
    let placed_height = source_height * scale;
    let wanted_x = frame.bounds.x + frame.bounds.width * 0.5 - frame.crop.focal_x * placed_width;
    let wanted_y = frame.bounds.y + frame.bounds.height * 0.5 - frame.crop.focal_y * placed_height;
    layer.offset_x = wanted_x.clamp(
        frame.bounds.x + frame.bounds.width - placed_width,
        frame.bounds.x,
    );
    layer.offset_y = wanted_y.clamp(
        frame.bounds.y + frame.bounds.height - placed_height,
        frame.bounds.y,
    );
    layer.scale_x = scale;
    layer.scale_y = scale;

    let padding = 2u32;
    let mask_width = frame.bounds.width.ceil() as u32 + padding * 2;
    let mask_height = frame.bounds.height.ceil() as u32 + padding * 2;
    ensure!(
        valid_dimensions(mask_width, mask_height),
        "Frame mask exceeds supported bounds"
    );
    let mut mask = RgbaImage::from_pixel(mask_width, mask_height, Rgba([0, 0, 0, 255]));
    let radius = frame.corner_radius;
    for y in padding..mask_height - padding {
        for x in padding..mask_width - padding {
            let local_x = x as f32 - padding as f32 + 0.5;
            let local_y = y as f32 - padding as f32 + 0.5;
            let dx = (radius - local_x.min(frame.bounds.width - local_x)).max(0.0);
            let dy = (radius - local_y.min(frame.bounds.height - local_y)).max(0.0);
            if radius == 0.0 || dx * dx + dy * dy <= radius * radius {
                mask.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }
    layer.mask = Some(mask.into());
    let metadata = create_metadata_mut(layer);
    metadata.insert("frame".into(), serde_json::to_value(frame)?);
    layer.metadata["maskEnabled"] = json!(true);
    layer.metadata["maskLinked"] = json!(false);
    layer.metadata["maskPlacement"] = json!({
        "origin": [frame.bounds.x - padding as f32, frame.bounds.y - padding as f32],
        "size": [mask_width as f32, mask_height as f32],
        "rotation": 0.0,
        "flipX": false,
        "flipY": false,
        "sampling": "Smooth",
    });
    Ok(())
}

pub fn resize_layout(
    document: &Document,
    width: u32,
    height: u32,
    strategy: ResizeStrategy,
) -> Result<Document> {
    let mut resized = document.clone();
    resize_layout_in_place(&mut resized, width, height, strategy)?;
    Ok(resized)
}

pub fn resize_layout_in_place(
    document: &mut Document,
    width: u32,
    height: u32,
    strategy: ResizeStrategy,
) -> Result<()> {
    ensure!(
        valid_dimensions(width, height),
        "Target layout exceeds supported bounds"
    );
    let old_width = document.width;
    let old_height = document.height;
    if old_width == width && old_height == height {
        return Ok(());
    }
    let scale_x = width as f32 / old_width as f32;
    let scale_y = height as f32 / old_height as f32;
    resize_layers(
        &mut document.layers,
        old_width,
        old_height,
        width,
        height,
        strategy,
        scale_x,
        scale_y,
    )?;
    document.width = width;
    document.height = height;
    document.metadata[CREATE_METADATA]["lastLayoutResize"] = json!({
        "from": [old_width, old_height],
        "to": [width, height],
        "strategy": format!("{strategy:?}"),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resize_layers(
    layers: &mut [Layer],
    old_width: u32,
    old_height: u32,
    new_width: u32,
    new_height: u32,
    strategy: ResizeStrategy,
    scale_x: f32,
    scale_y: f32,
) -> Result<()> {
    for layer in layers {
        if !layer.children.is_empty() {
            // Groups are pass-through folders: their children retain canvas
            // coordinates, while the group also records its own placement for
            // editor operations. Resize both positions but never scale the
            // container and its children as one transform.
            let rule = resize_rule(layer, strategy)?;
            resize_layer_position(
                layer, rule, old_width, old_height, new_width, new_height, scale_x, scale_y,
            );
            resize_layers(
                &mut layer.children,
                old_width,
                old_height,
                new_width,
                new_height,
                strategy,
                scale_x,
                scale_y,
            )?;
            continue;
        }
        if let Some(mut frame) = frame_spec(layer)? {
            frame.bounds.x *= scale_x;
            frame.bounds.y *= scale_y;
            frame.bounds.width *= scale_x;
            frame.bounds.height *= scale_y;
            frame.corner_radius *= scale_x.min(scale_y);
            place_frame_layer(layer, &frame)?;
            continue;
        }
        let rule = resize_rule(layer, strategy)?;
        resize_layer_position(
            layer, rule, old_width, old_height, new_width, new_height, scale_x, scale_y,
        );
        match rule.scale {
            LayoutScale::Preserve => {}
            LayoutScale::Uniform => {
                let factor = scale_x.min(scale_y);
                layer.scale_x *= factor;
                layer.scale_y *= factor;
            }
            LayoutScale::Stretch => {
                layer.scale_x *= scale_x;
                layer.scale_y *= scale_y;
            }
        }
    }
    Ok(())
}

fn resize_rule(layer: &Layer, strategy: ResizeStrategy) -> Result<LayerLayoutRule> {
    Ok(match strategy {
        ResizeStrategy::Adapt => layout_rule(layer)?.unwrap_or_default(),
        ResizeStrategy::ScaleToFit => LayerLayoutRule::default(),
        ResizeStrategy::Stretch => LayerLayoutRule {
            horizontal: HorizontalAnchor::Relative,
            vertical: VerticalAnchor::Relative,
            scale: LayoutScale::Stretch,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn resize_layer_position(
    layer: &mut Layer,
    rule: LayerLayoutRule,
    old_width: u32,
    old_height: u32,
    new_width: u32,
    new_height: u32,
    scale_x: f32,
    scale_y: f32,
) {
    let delta_x = new_width as f32 - old_width as f32;
    let delta_y = new_height as f32 - old_height as f32;
    layer.offset_x = resize_horizontal(layer.offset_x, rule.horizontal, delta_x, scale_x);
    layer.offset_y = resize_vertical(layer.offset_y, rule.vertical, delta_y, scale_y);
    let spacing_base_y = create_value(layer, "spacingBaseY")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value as f32);
    if let Some(base_y) = spacing_base_y {
        create_metadata_mut(layer).insert(
            "spacingBaseY".into(),
            json!(resize_vertical(base_y, rule.vertical, delta_y, scale_y)),
        );
    }
}

fn resize_horizontal(value: f32, anchor: HorizontalAnchor, delta: f32, scale: f32) -> f32 {
    match anchor {
        HorizontalAnchor::Left => value,
        HorizontalAnchor::Center => value + delta * 0.5,
        HorizontalAnchor::Right => value + delta,
        HorizontalAnchor::Relative => value * scale,
    }
}

fn resize_vertical(value: f32, anchor: VerticalAnchor, delta: f32, scale: f32) -> f32 {
    match anchor {
        VerticalAnchor::Top => value,
        VerticalAnchor::Center => value + delta * 0.5,
        VerticalAnchor::Bottom => value + delta,
        VerticalAnchor::Relative => value * scale,
    }
}

pub fn instantiate_component(
    document: &mut Document,
    component_id: &str,
    revision: u64,
    prototype_layers: &[Layer],
    overrides: &ComponentOverrides,
) -> Result<String> {
    let group = component_instance_layer(component_id, revision, prototype_layers, overrides)?;
    let id = group.id.clone();
    document.layers.push(group);
    Ok(id)
}

fn component_instance_layer(
    component_id: &str,
    revision: u64,
    prototype_layers: &[Layer],
    overrides: &ComponentOverrides,
) -> Result<Layer> {
    uuid::Uuid::parse_str(component_id).context("Invalid component ID")?;
    ensure!(revision > 0, "Invalid component revision");
    ensure!(!prototype_layers.is_empty(), "Component has no layers");
    let mut children = prototype_layers.to_vec();
    regenerate_layer_ids(&mut children);
    let overrides_applied = apply_component_overrides(&mut children, overrides)?;
    let mut group = Layer::group("Component");
    group.children = children;
    group.metadata[CREATE_METADATA] = json!({
        "componentInstance": {
            "componentId": component_id,
            "revision": revision,
            "overrides": overrides,
            "overridesApplied": overrides_applied,
        }
    });
    Ok(group)
}

pub fn insert_project_component(
    project: &mut Project,
    page_id: &str,
    component_id: &str,
    overrides: &ComponentOverrides,
) -> Result<String> {
    let (revision, layers) = project.component_snapshot(component_id)?;
    instantiate_component(
        project.page_document_mut(page_id)?,
        component_id,
        revision,
        &layers,
        overrides,
    )
}

/// Promote selected page-level layers into a collection-wide protected
/// background. The source page replaces the selected originals with the same
/// component instance, so it keeps its appearance without compositing a
/// second editable copy.
pub fn set_shared_background(
    project: &mut Project,
    prototype_layers: Vec<Layer>,
) -> Result<String> {
    // Work on a complete draft because defining a component and rebuilding
    // every page must be one collection operation. In particular, a failed
    // rebuild must not leave the source page without its selected artwork.
    let mut draft = project.clone();
    ensure!(
        !prototype_layers.is_empty(),
        "Select one or more layers for the shared background"
    );
    let source_page_id = draft.active_page_id().to_owned();
    let requested = prototype_layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect::<HashSet<_>>();
    ensure!(
        requested.len() == prototype_layers.len(),
        "Select each background layer only once"
    );
    let source_layers = {
        let source = draft.page_document(&source_page_id)?;
        for id in &requested {
            ensure!(
                !layer_is_effectively_locked(source, id),
                "Unlock selected background layers or their groups before sharing them"
            );
            ensure!(
                source.layers.iter().any(|layer| layer.id == *id),
                "Shared backgrounds require page-level layers; select the containing component or group instead"
            );
        }
        // Retain the source stacking order rather than HashSet iteration
        // order. The component must reproduce the source pixels exactly.
        source
            .layers
            .iter()
            .filter(|layer| requested.contains(&layer.id))
            .cloned()
            .collect::<Vec<_>>()
    };
    let component_id = draft.define_component("Shared collection background", source_layers)?;
    draft.metadata.shared_background_component_id = Some(component_id.clone());
    let source = draft.page_document_mut(&source_page_id)?;
    source.layers.retain(|layer| !requested.contains(&layer.id));
    apply_shared_background(&mut draft)?;
    *project = draft;
    Ok(component_id)
}

/// Rebuild every page's locked background instance from the configured shared
/// component. Newer page artwork is left in place above it.
pub fn apply_shared_background(project: &mut Project) -> Result<usize> {
    let page_ids = project.page_ids();
    for page_id in &page_ids {
        apply_shared_background_to_page(project, page_id)?;
    }
    Ok(page_ids.len())
}

/// Rebuild only one page's protected background instance. Add/duplicate page
/// paths use this instead of touching existing page instances, preserving
/// their layer IDs and any open editor snapshots.
pub fn apply_shared_background_to_page(project: &mut Project, page_id: &str) -> Result<()> {
    let brand = project.active_brand().cloned();
    let component_id = project
        .metadata
        .shared_background_component_id
        .clone()
        .context("No shared background is configured")?;
    let (revision, layers) = project.component_snapshot(&component_id)?;
    let document = project.page_document_mut(page_id)?;
    document.layers.retain(|layer| !is_shared_background(layer));
    hide_template_backgrounds_for_shared_component(document);
    let mut instance = component_instance_layer(
        &component_id,
        revision,
        &layers,
        &ComponentOverrides::default(),
    )?;
    instance.name = "Shared background".into();
    instance.locked = true;
    if let Some(brand) = brand.as_ref() {
        // A new shared instance may be inserted into a later blank page after
        // its brand was selected. Style that instance alone; rebranding the
        // entire page would overwrite deliberate local text and spacing edits.
        let mut report = BrandApplicationReport::default();
        apply_brand_layers(std::slice::from_mut(&mut instance), brand, &mut report)?;
    }
    create_metadata_mut(&mut instance).insert("sharedBackground".into(), json!(true));
    document.layers.insert(0, instance);
    Ok(())
}

/// Template backgrounds are deliberately tagged at construction time. Once a
/// collection background is present, hide only those explicit root layers so
/// they cannot cover the protected shared instance. The original remains in
/// the page for reversible project history and carries its prior visibility
/// in metadata; arbitrary raster artwork is never inferred or hidden.
fn hide_template_backgrounds_for_shared_component(document: &mut Document) {
    for layer in &mut document.layers {
        if is_shared_background(layer)
            || create_value(layer, "colorRole").and_then(Value::as_str) != Some("background")
        {
            continue;
        }
        if create_value(layer, "sharedBackgroundHidden").is_none() {
            let was_visible = layer.visible;
            create_metadata_mut(layer).insert(
                "sharedBackgroundHidden".into(),
                json!({"wasVisible": was_visible}),
            );
        }
        layer.visible = false;
    }
}

/// Build or reuse a protected reusable logo component from a packaged image
/// resource, then place its locked instance on a page. Resource bytes are
/// decoded only through the project package; no external path is accepted.
pub fn insert_brand_logo(
    project: &mut Project,
    page_id: &str,
    brand_id: &str,
    resource_id: &str,
) -> Result<String> {
    let brand = project
        .brand_kits
        .iter()
        .find(|brand| brand.id == brand_id)
        .cloned()
        .context("Brand kit not found")?;
    ensure!(
        brand.logo_resource_ids.iter().any(|id| id == resource_id),
        "Logo resource is not attached to this brand"
    );
    let component_name = format!("Brand logo {brand_id} {resource_id}");
    let component_id = project
        .component_summaries()
        .into_iter()
        .find(|component| component.name == component_name)
        .map(|component| component.id)
        .map(Ok)
        .unwrap_or_else(|| {
            let bytes = project.resource_bytes(resource_id)?.to_vec();
            let image = decode_project_image(&bytes, "Brand logo")?;
            let (page_width, page_height) = {
                let document = project.page_document(page_id)?;
                (document.width, document.height)
            };
            let mut layer = Layer::paint("Brand logo", image.width(), image.height());
            layer.image = Some(image.into());
            let max_width = page_width as f32 * 0.22;
            let max_height = page_height as f32 * 0.16;
            let scale = (max_width / layer.image.as_ref().unwrap().width() as f32)
                .min(max_height / layer.image.as_ref().unwrap().height() as f32)
                .min(1.0);
            layer.scale_x = scale;
            layer.scale_y = scale;
            layer.offset_x =
                page_width as f32 - layer.image.as_ref().unwrap().width() as f32 * scale - 48.0;
            layer.offset_y = 48.0;
            layer.locked = true;
            create_metadata_mut(&mut layer).insert(
                "brandLogo".into(),
                json!({"brandId": brand_id, "resourceId": resource_id}),
            );
            project.define_component(component_name, vec![layer])
        })?;
    let id = insert_project_component(
        project,
        page_id,
        &component_id,
        &ComponentOverrides::default(),
    )?;
    let layer = project
        .page_document_mut(page_id)?
        .find_layer_mut(&id)
        .context("Inserted logo instance disappeared")?;
    layer.name = "Protected brand logo".into();
    layer.locked = true;
    create_metadata_mut(layer).insert(
        "brandLogo".into(),
        json!({"brandId": brand_id, "resourceId": resource_id, "protected": true}),
    );
    Ok(id)
}

fn decode_project_image(bytes: &[u8], label: &str) -> Result<RgbaImage> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .with_context(|| format!("{label} is not a supported image"))?;
    let decoder = reader
        .into_decoder()
        .with_context(|| format!("{label} cannot be decoded"))?;
    let (width, height) = decoder.dimensions();
    ensure!(
        valid_dimensions(width, height)
            && u64::from(width) * u64::from(height) <= MAX_BINDING_IMAGE_PIXELS,
        "{label} exceeds the editable logo bounds"
    );
    image::DynamicImage::from_decoder(decoder)
        .with_context(|| format!("{label} cannot be decoded"))
        .map(|image| image.to_rgba8())
}

pub fn propagate_component(
    document: &mut Document,
    component_id: &str,
    revision: u64,
    prototype_layers: &[Layer],
) -> Result<ComponentPropagationReport> {
    let mut report = ComponentPropagationReport::default();
    propagate_component_layers(
        &mut document.layers,
        component_id,
        revision,
        prototype_layers,
        &mut report,
    )?;
    Ok(report)
}

pub fn propagate_project_component(
    project: &mut Project,
    component_id: &str,
) -> Result<ComponentPropagationReport> {
    let (revision, layers) = project.component_snapshot(component_id)?;
    let mut total = ComponentPropagationReport::default();
    for page_id in project.page_ids() {
        let report = propagate_component(
            project.page_document_mut(&page_id)?,
            component_id,
            revision,
            &layers,
        )?;
        total.instances_updated += report.instances_updated;
        total.overrides_reapplied += report.overrides_reapplied;
    }
    Ok(total)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentInstanceInfo {
    pub component_id: String,
    pub revision: u64,
    pub overrides: ComponentOverrides,
}

/// Locate the component group containing a selected layer. This lets Create
/// edit a real instance rather than creating an unrelated new component.
pub fn component_instance_info(
    document: &Document,
    layer_id: &str,
) -> Result<ComponentInstanceInfo> {
    let layer = find_component_instance(&document.layers, layer_id)
        .context("Select a layer inside a reusable component")?;
    component_instance_from_layer(layer)
}

/// Replace one selected instance from a new prototype while preserving its
/// explicit text and visibility overrides.
pub fn refresh_component_instance(
    document: &mut Document,
    selected_layer_id: &str,
    revision: u64,
    prototype_layers: &[Layer],
    overrides: &ComponentOverrides,
) -> Result<()> {
    let layer = find_component_instance_mut(&mut document.layers, selected_layer_id)
        .context("Select a layer inside a reusable component")?;
    let info = component_instance_from_layer(layer)?;
    let mut replacement =
        component_instance_layer(&info.component_id, revision, prototype_layers, overrides)?;
    replacement.id = layer.id.clone();
    replacement.name = layer.name.clone();
    replacement.offset_x = layer.offset_x;
    replacement.offset_y = layer.offset_y;
    replacement.scale_x = layer.scale_x;
    replacement.scale_y = layer.scale_y;
    replacement.rotation = layer.rotation;
    replacement.visible = layer.visible;
    replacement.locked = layer.locked;
    *layer = replacement;
    Ok(())
}

/// Return the current instance children for the selected component. The
/// caller can pass these to `Project::update_component` and then propagate.
pub fn component_instance_layers(document: &Document, layer_id: &str) -> Result<Vec<Layer>> {
    let layer = find_component_instance(&document.layers, layer_id)
        .context("Select a layer inside a reusable component")?;
    component_instance_from_layer(layer)?;
    Ok(layer.children.clone())
}

/// Extract a selected instance as a component-definition draft while keeping
/// its explicitly recorded text and visibility overrides local to that one
/// instance. Canvas edits to the other native layer properties become the new
/// definition; a one-off caption or hidden field does not silently become the
/// default for every page during propagation.
pub fn component_definition_layers(
    document: &Document,
    layer_id: &str,
    current_prototype: &[Layer],
) -> Result<Vec<Layer>> {
    let info = component_instance_info(document, layer_id)?;
    let mut layers = component_instance_layers(document, layer_id)?;
    for field in info.overrides.text.keys() {
        let source = find_field(current_prototype, field)
            .with_context(|| format!("Component text field '{field}' disappeared"))?;
        let content = objects::live_text(source)?
            .with_context(|| format!("Component text field '{field}' is no longer editable"))?
            .content;
        let target = find_field_mut(&mut layers, field)
            .with_context(|| format!("Selected instance text field '{field}' disappeared"))?;
        let mut style = objects::live_text(target)?.with_context(|| {
            format!("Selected instance text field '{field}' is no longer editable")
        })?;
        objects::set_text_content(&mut style, content);
        objects::set_live_text(target, style)?;
    }
    for field in &info.overrides.hidden_fields {
        let source = find_field(current_prototype, field)
            .with_context(|| format!("Component visibility field '{field}' disappeared"))?;
        let target = find_field_mut(&mut layers, field)
            .with_context(|| format!("Selected instance visibility field '{field}' disappeared"))?;
        target.visible = source.visible;
    }
    Ok(layers)
}

fn component_instance_from_layer(layer: &Layer) -> Result<ComponentInstanceInfo> {
    let instance =
        create_value(layer, "componentInstance").context("Invalid component instance")?;
    let component_id = instance
        .get("componentId")
        .and_then(Value::as_str)
        .context("Component instance has no component ID")?
        .to_owned();
    uuid::Uuid::parse_str(&component_id).context("Invalid component instance ID")?;
    let revision = instance
        .get("revision")
        .and_then(Value::as_u64)
        .context("Component instance has no revision")?;
    ensure!(revision > 0, "Invalid component instance revision");
    let overrides = instance
        .get("overrides")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .context("Invalid component overrides")?
        .unwrap_or_default();
    Ok(ComponentInstanceInfo {
        component_id,
        revision,
        overrides,
    })
}

fn contains_layer(layers: &[Layer], id: &str) -> bool {
    layers
        .iter()
        .any(|layer| layer.id == id || contains_layer(&layer.children, id))
}

fn find_component_instance<'a>(layers: &'a [Layer], selected: &str) -> Option<&'a Layer> {
    for layer in layers {
        if create_value(layer, "componentInstance").is_some()
            && contains_layer(std::slice::from_ref(layer), selected)
        {
            return Some(layer);
        }
        if let Some(found) = find_component_instance(&layer.children, selected) {
            return Some(found);
        }
    }
    None
}

fn find_component_instance_mut<'a>(
    layers: &'a mut [Layer],
    selected: &str,
) -> Option<&'a mut Layer> {
    for layer in layers {
        if create_value(layer, "componentInstance").is_some()
            && (layer.id == selected || contains_layer(&layer.children, selected))
        {
            return Some(layer);
        }
        if let Some(found) = find_component_instance_mut(&mut layer.children, selected) {
            return Some(found);
        }
    }
    None
}

fn is_shared_background(layer: &Layer) -> bool {
    create_value(layer, "sharedBackground").and_then(Value::as_bool) == Some(true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerArrangement {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
    DistributeHorizontally,
    DistributeVertically,
}

#[derive(Clone, Copy)]
struct LayerBounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl LayerBounds {
    fn right(self) -> f32 {
        self.x + self.width
    }

    fn bottom(self) -> f32 {
        self.y + self.height
    }
}

/// Align or distribute a bounded multi-layer selection while keeping each
/// layer native. Groups without source pixels are rejected rather than being
/// flattened to obtain an inferred visual bound.
pub fn arrange_layers(
    document: &mut Document,
    layer_ids: &[String],
    arrangement: LayerArrangement,
) -> Result<()> {
    ensure!(
        (2..=64).contains(&layer_ids.len()),
        "Select 2–64 raster or live-object layers"
    );
    let mut unique = HashSet::new();
    let mut entries = Vec::with_capacity(layer_ids.len());
    for id in layer_ids {
        ensure!(
            unique.insert(id.as_str()),
            "Layer selection contains duplicates"
        );
        ensure!(
            !layer_is_effectively_locked(document, id),
            "Unlock selected layers or their groups before arranging them"
        );
        let layer = document
            .find_layer(id)
            .context("Selected layer disappeared")?;
        let image = layer
            .image
            .as_ref()
            .context("Selected group has no direct pixels")?;
        let width = image.width() as f32 * layer.scale_x.abs();
        let height = image.height() as f32 * layer.scale_y.abs();
        ensure!(
            width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0,
            "Selected layer has invalid bounds"
        );
        entries.push((
            id.clone(),
            LayerBounds {
                x: layer.offset_x,
                y: layer.offset_y,
                width,
                height,
            },
        ));
    }
    let left = entries
        .iter()
        .map(|(_, bounds)| bounds.x)
        .fold(f32::INFINITY, f32::min);
    let right = entries
        .iter()
        .map(|(_, bounds)| bounds.right())
        .fold(f32::NEG_INFINITY, f32::max);
    let top = entries
        .iter()
        .map(|(_, bounds)| bounds.y)
        .fold(f32::INFINITY, f32::min);
    let bottom = entries
        .iter()
        .map(|(_, bounds)| bounds.bottom())
        .fold(f32::NEG_INFINITY, f32::max);
    let center_x = (left + right) * 0.5;
    let center_y = (top + bottom) * 0.5;

    match arrangement {
        LayerArrangement::DistributeHorizontally => {
            ensure!(
                entries.len() >= 3,
                "Select at least three layers to distribute"
            );
            entries.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
            let occupied = entries.iter().map(|(_, bounds)| bounds.width).sum::<f32>();
            let gap = ((right - left - occupied) / (entries.len() - 1) as f32).max(0.0);
            let mut x = left;
            for (id, bounds) in entries {
                document.find_layer_mut(&id).unwrap().offset_x = x;
                x += bounds.width + gap;
            }
        }
        LayerArrangement::DistributeVertically => {
            ensure!(
                entries.len() >= 3,
                "Select at least three layers to distribute"
            );
            entries.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
            let occupied = entries.iter().map(|(_, bounds)| bounds.height).sum::<f32>();
            let gap = ((bottom - top - occupied) / (entries.len() - 1) as f32).max(0.0);
            let mut y = top;
            for (id, bounds) in entries {
                document.find_layer_mut(&id).unwrap().offset_y = y;
                y += bounds.height + gap;
            }
        }
        arrangement => {
            for (id, bounds) in entries {
                let layer = document.find_layer_mut(&id).unwrap();
                match arrangement {
                    LayerArrangement::Left => layer.offset_x = left,
                    LayerArrangement::Center => layer.offset_x = center_x - bounds.width * 0.5,
                    LayerArrangement::Right => layer.offset_x = right - bounds.width,
                    LayerArrangement::Top => layer.offset_y = top,
                    LayerArrangement::Middle => layer.offset_y = center_y - bounds.height * 0.5,
                    LayerArrangement::Bottom => layer.offset_y = bottom - bounds.height,
                    LayerArrangement::DistributeHorizontally
                    | LayerArrangement::DistributeVertically => unreachable!(),
                }
            }
        }
    }
    Ok(())
}

/// Add or replace a native live-shape background for one editable text layer.
/// The text remains untouched and the new shape is placed immediately behind
/// it when the text is at the page root.
pub fn set_live_text_background(
    document: &mut Document,
    text_layer_id: &str,
    color: [u8; 4],
    padding: f32,
    corner_radius: f32,
) -> Result<String> {
    ensure!(
        padding.is_finite() && (0.0..=500.0).contains(&padding),
        "Text background padding must be 0–500 pixels"
    );
    ensure!(
        corner_radius.is_finite() && (0.0..=500.0).contains(&corner_radius),
        "Text background corner radius must be 0–500 pixels"
    );
    ensure!(
        !layer_is_effectively_locked(document, text_layer_id),
        "Unlock text or its group before adding a background"
    );
    let text = document
        .layers
        .iter()
        .find(|layer| layer.id == text_layer_id)
        .context(
            "Text backgrounds are supported for page-level live text; edit the component definition instead",
        )?;
    objects::live_text(text)?.context("Select editable text first")?;
    let image = text.image.as_ref().context("Live text cache is missing")?;
    let width = (image.width() as f32 * text.scale_x.abs() + padding * 2.0)
        .ceil()
        .max(1.0) as u32;
    let height = (image.height() as f32 * text.scale_y.abs() + padding * 2.0)
        .ceil()
        .max(1.0) as u32;
    ensure!(
        valid_dimensions(width, height),
        "Text background is too large"
    );
    let mut background = objects::live_shape_layer(
        "Text background",
        ObjectPoint {
            x: text.offset_x - padding,
            y: text.offset_y - padding,
        },
        width,
        height,
        LiveShapeStyle {
            kind: LiveShapeKind::Rectangle,
            red: color[0] as f32 / 255.0,
            green: color[1] as f32 / 255.0,
            blue: color[2] as f32 / 255.0,
            corner_radius: corner_radius.min(width.min(height) as f32 * 0.5),
            line_width: None,
            start: None,
            end: None,
        },
    )?;
    background.opacity = color[3] as f32 / 255.0;
    create_metadata_mut(&mut background).insert("textBackgroundFor".into(), json!(text_layer_id));
    document.layers.retain(|layer| {
        create_value(layer, "textBackgroundFor").and_then(Value::as_str) != Some(text_layer_id)
    });
    let index = document
        .layers
        .iter()
        .position(|layer| layer.id == text_layer_id)
        .unwrap_or(0);
    let id = background.id.clone();
    document.layers.insert(index, background);
    Ok(id)
}

fn propagate_component_layers(
    layers: &mut [Layer],
    component_id: &str,
    revision: u64,
    prototype_layers: &[Layer],
    report: &mut ComponentPropagationReport,
) -> Result<()> {
    for layer in layers {
        let instance = create_value(layer, "componentInstance").cloned();
        if instance
            .as_ref()
            .and_then(|value| value.get("componentId"))
            .and_then(Value::as_str)
            == Some(component_id)
        {
            let overrides: ComponentOverrides = instance
                .as_ref()
                .and_then(|value| value.get("overrides"))
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .context("Invalid component overrides")?
                .unwrap_or_default();
            let mut children = prototype_layers.to_vec();
            regenerate_layer_ids(&mut children);
            report.overrides_reapplied += apply_component_overrides(&mut children, &overrides)?;
            layer.children = children;
            layer.metadata[CREATE_METADATA]["componentInstance"] = json!({
                "componentId": component_id,
                "revision": revision,
                "overrides": overrides,
            });
            report.instances_updated += 1;
        } else {
            propagate_component_layers(
                &mut layer.children,
                component_id,
                revision,
                prototype_layers,
                report,
            )?;
        }
    }
    Ok(())
}

fn apply_component_overrides(
    layers: &mut [Layer],
    overrides: &ComponentOverrides,
) -> Result<usize> {
    let mut applied = 0;
    for layer in layers {
        if let Some(field) = layer_field(layer).map(str::to_owned) {
            if let Some(content) = overrides.text.get(&field) {
                if let Some(mut style) = objects::live_text(layer)? {
                    objects::set_text_content(&mut style, content.clone());
                    objects::set_live_text(layer, style)?;
                    applied += 1;
                }
            }
            if overrides.hidden_fields.contains(&field) {
                layer.visible = false;
                applied += 1;
            }
        }
        applied += apply_component_overrides(&mut layer.children, overrides)?;
    }
    Ok(applied)
}

fn regenerate_layer_ids(layers: &mut [Layer]) {
    fn collect(layers: &[Layer], replacements: &mut HashMap<String, String>) {
        for layer in layers {
            replacements.insert(
                layer.id.clone(),
                uuid::Uuid::new_v4().to_string().to_uppercase(),
            );
            collect(&layer.children, replacements);
        }
    }
    fn apply(layers: &mut [Layer], replacements: &HashMap<String, String>) {
        for layer in layers {
            layer.id = replacements[&layer.id].clone();
            if let Some(source) = layer.metadata.get("maskSourceID").and_then(Value::as_str)
                && let Some(replacement) = replacements.get(source)
            {
                layer.metadata["maskSourceID"] = json!(replacement);
            }
            apply(&mut layer.children, replacements);
        }
    }
    let mut replacements = HashMap::new();
    collect(layers, &mut replacements);
    apply(layers, &replacements);
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl BindingTable {
    pub fn from_csv(input: &str) -> Result<Self> {
        ensure!(
            input.len() <= MAX_BINDING_INPUT_BYTES,
            "Binding CSV is too large"
        );
        let mut records = parse_csv(input.strip_prefix('\u{feff}').unwrap_or(input))?;
        ensure!(!records.is_empty(), "Binding CSV is empty");
        let headers = records.remove(0);
        ensure!(
            !headers.is_empty() && headers.len() <= MAX_BINDING_COLUMNS,
            "Invalid binding column count"
        );
        let mut unique = HashSet::new();
        for header in &headers {
            ensure!(
                valid_binding_header(header) && unique.insert(header.clone()),
                "Binding headers must be non-empty and unique"
            );
        }
        ensure!(
            records.len() <= MAX_BINDING_ROWS,
            "Binding CSV has too many rows"
        );
        for (index, row) in records.iter().enumerate() {
            ensure!(
                row.len() == headers.len(),
                "Binding row {} has {} fields; expected {}",
                index + 2,
                row.len(),
                headers.len()
            );
        }
        Ok(Self {
            headers,
            rows: records,
        })
    }

    pub fn column_index(&self, column: &str) -> Option<usize> {
        self.headers.iter().position(|header| header == column)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingTarget {
    Text,
    Visibility,
    ImageResource,
    AltText,
}

/// A bounded, project-owned image source passed to the background binder.
/// The ID comes from `Project::resource_summaries`; its bytes are copied from
/// the project package before worker execution, never from a CSV path.
#[derive(Clone, Debug)]
pub struct BindingImageResource {
    pub id: String,
    pub bytes: Vec<u8>,
}

/// Parse a field selector used in the optional manual mapping control. Plain
/// field names bind text. `image:hero`, `visible:cta`, and `alt:hero` select
/// the other native targets.
pub fn binding_target_and_field(selector: &str) -> Result<(BindingTarget, String)> {
    let (target, field) =
        binding_header_target(selector).unwrap_or((BindingTarget::Text, selector));
    ensure!(valid_field_name(field), "Invalid binding field");
    Ok((target, field.to_owned()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldBinding {
    pub field: String,
    pub column: String,
    pub target: BindingTarget,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingSchema {
    pub bindings: Vec<FieldBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename_column: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingIssue {
    pub row: Option<usize>,
    pub field: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingRowResult {
    pub row: usize,
    pub valid: bool,
    pub output_name: String,
    pub issues: Vec<BindingIssue>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingValidation {
    pub global_issues: Vec<BindingIssue>,
    pub rows: Vec<BindingRowResult>,
}

#[derive(Clone, Debug)]
pub struct BoundPage {
    pub row: usize,
    pub name: String,
    pub document: Document,
}

/// Match CSV headers to native template fields. Plain headers auto-bind only
/// unique text fields. The explicit `image:`, `visible:`, and `alt:` prefixes
/// make non-text intent visible in the source CSV instead of guessing.
pub fn auto_map_bindings(document: &Document, table: &BindingTable) -> BindingSchema {
    fn normalized(value: &str) -> String {
        value
            .chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }
    let mut fields = HashMap::new();
    collect_field_targets(&document.layers, &mut fields);
    let mut bindings = vec![];
    for column in &table.headers {
        if let Some((target, field)) = binding_header_target(column) {
            bindings.push(FieldBinding {
                field: field.to_owned(),
                column: column.clone(),
                target,
                required: false,
            });
            continue;
        }
        let key = normalized(column);
        let matches = fields
            .iter()
            .filter(|(field, targets)| {
                targets.len() == 1 && targets[0] == BindingTarget::Text && normalized(field) == key
            })
            .map(|(field, _)| field.clone())
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            bindings.push(FieldBinding {
                field: matches[0].clone(),
                column: column.clone(),
                target: BindingTarget::Text,
                required: false,
            });
        }
    }
    BindingSchema {
        bindings,
        filename_column: table
            .headers
            .iter()
            .find(|header| matches!(normalized(header).as_str(), "filename" | "name" | "title"))
            .cloned(),
    }
}

pub fn validate_binding_rows(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
) -> BindingValidation {
    validate_binding_rows_with_resource_ids(document, schema, table, None)
}

/// Validate every row against the current project's packaged image resource
/// IDs. Callers that do not have a project may pass `None`; image resolution
/// then remains the responsibility of `apply_binding_row_with_images`.
pub fn validate_binding_rows_with_resource_ids(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
    resource_ids: Option<&HashSet<String>>,
) -> BindingValidation {
    let issues = validate_bindings_with_resource_ids(document, schema, table, resource_ids);
    let global_issues = issues
        .iter()
        .filter(|issue| issue.row.is_none())
        .cloned()
        .collect::<Vec<_>>();
    let rows = (0..table.rows.len())
        .map(|row| {
            let mut row_issues = issues
                .iter()
                .filter(|issue| issue.row == Some(row))
                .cloned()
                .collect::<Vec<_>>();
            row_issues.extend(global_issues.iter().cloned());
            BindingRowResult {
                row,
                valid: row_issues.is_empty(),
                output_name: binding_output_name(schema, table, row)
                    .unwrap_or_else(|_| format!("variant-{:04}", row + 1)),
                issues: row_issues,
            }
        })
        .collect();
    BindingValidation {
        global_issues,
        rows,
    }
}

/// Prepare a set of bound pages without mutating the source project. Callers
/// can cancel between rows and commit the returned vector as one transaction.
pub fn prepare_binding_pages(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
    rows: &[usize],
    mut cancelled: impl FnMut() -> bool,
) -> Result<Vec<BoundPage>> {
    ensure!(
        rows.len() <= MAX_BINDING_ROWS,
        "Too many binding rows requested"
    );
    ensure!(
        !schema
            .bindings
            .iter()
            .any(|binding| binding.target == BindingTarget::ImageResource),
        "Batch image bindings require a shared-resource resolver"
    );
    validate_binding_output_canvas(document, rows)?;
    let validation = validate_binding_rows(document, schema, table);
    ensure!(
        validation.global_issues.is_empty(),
        "Binding schema is invalid"
    );
    let mut unique = HashSet::new();
    let mut result = Vec::with_capacity(rows.len());
    for &row in rows {
        ensure!(unique.insert(row), "Duplicate binding row requested");
        ensure!(!cancelled(), "Bulk design creation was cancelled");
        let status = validation
            .rows
            .get(row)
            .context("Binding row is out of range")?;
        ensure!(status.valid, "Binding row {} is invalid", row + 1);
        result.push(BoundPage {
            row,
            name: status.output_name.clone(),
            document: apply_binding_row_unchecked(document, schema, table, row, |_| {
                bail!("Image binding needs a resource resolver")
            })?,
        });
    }
    Ok(result)
}

/// Prepare image-aware bound pages without mutating the source project. Image
/// bytes must have been read from explicitly selected project resources before
/// this worker starts; CSV values merely select one of those resource IDs.
pub fn prepare_binding_pages_with_resources(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
    rows: &[usize],
    resources: &[BindingImageResource],
    mut cancelled: impl FnMut() -> bool,
) -> Result<Vec<BoundPage>> {
    ensure!(
        rows.len() <= MAX_BINDING_ROWS,
        "Too many binding rows requested"
    );
    validate_binding_output_canvas(document, rows)?;
    let resource_ids = resources
        .iter()
        .map(|resource| resource.id.clone())
        .collect::<HashSet<_>>();
    ensure!(
        resource_ids.len() == resources.len(),
        "Duplicate binding image resource ID"
    );
    let validation =
        validate_binding_rows_with_resource_ids(document, schema, table, Some(&resource_ids));
    ensure!(
        validation.global_issues.is_empty(),
        "Binding schema is invalid"
    );
    let images = decode_binding_image_resources(resources, || cancelled())?;
    // Each page owns its resolved pixels. Bound the aggregate before creating
    // any drafts so a valid CSV cannot turn into an unbounded all-page copy.
    let mut requested = HashSet::new();
    let mut output_image_pixels = 0u64;
    for &row in rows {
        ensure!(requested.insert(row), "Duplicate binding row requested");
        let values = table.rows.get(row).context("Binding row is out of range")?;
        for binding in schema
            .bindings
            .iter()
            .filter(|binding| binding.target == BindingTarget::ImageResource)
        {
            let column = table
                .column_index(&binding.column)
                .context("Binding column disappeared")?;
            let resource = values[column].trim();
            if let Some(image) = images.get(resource) {
                output_image_pixels = output_image_pixels
                    .checked_add(u64::from(image.width()) * u64::from(image.height()))
                    .context("Bound image output is too large")?;
                ensure!(
                    output_image_pixels <= MAX_BINDING_OUTPUT_IMAGE_PIXELS,
                    "Bound image output exceeds the 64 megapixel worker limit"
                );
            }
        }
    }
    let mut unique = HashSet::new();
    let mut result = Vec::with_capacity(rows.len());
    for &row in rows {
        ensure!(unique.insert(row), "Duplicate binding row requested");
        ensure!(!cancelled(), "Bulk design creation was cancelled");
        let status = validation
            .rows
            .get(row)
            .context("Binding row is out of range")?;
        ensure!(status.valid, "Binding row {} is invalid", row + 1);
        result.push(BoundPage {
            row,
            name: status.output_name.clone(),
            document: apply_binding_row_unchecked(document, schema, table, row, |id| {
                images
                    .get(id)
                    .cloned()
                    .with_context(|| format!("Project resource '{id}' is unavailable"))
            })?,
        });
    }
    Ok(result)
}

/// A bulk transaction stays atomic by preparing all of its pages privately.
/// Bound their combined canvas area before decoding resources or allocating
/// page copies, so a valid but oversized CSV cannot exhaust the worker.
fn validate_binding_output_canvas(document: &Document, rows: &[usize]) -> Result<()> {
    let page_pixels = u64::from(document.width) * u64::from(document.height);
    let output_pixels = page_pixels
        .checked_mul(rows.len() as u64)
        .context("Bound page canvas count overflow")?;
    ensure!(
        output_pixels <= MAX_BINDING_OUTPUT_CANVAS_PIXELS,
        "Bound page output exceeds the 64 megapixel worker limit"
    );
    Ok(())
}

/// Decode packaged binding resources after dimensions have been inspected. It
/// is public for a one-row preview, while bulk callers use the same path above.
pub fn decode_binding_image_resources(
    resources: &[BindingImageResource],
    mut cancelled: impl FnMut() -> bool,
) -> Result<BTreeMap<String, RgbaImage>> {
    let mut images = BTreeMap::new();
    let mut total_bytes = 0u64;
    for resource in resources {
        ensure!(!cancelled(), "Bulk design creation was cancelled");
        total_bytes = total_bytes
            .checked_add(resource.bytes.len() as u64)
            .context("Binding image resources are too large")?;
        ensure!(
            total_bytes <= MAX_BINDING_IMAGE_RESOURCE_BYTES_TOTAL,
            "Binding image resources exceed the 256 MiB worker limit"
        );
        ensure!(
            resource.bytes.len() as u64 <= crate::create_project::MAX_SHARED_RESOURCE_BYTES,
            "Project resource '{}' exceeds the image binding size limit",
            resource.id
        );
        let reader = image::ImageReader::new(Cursor::new(resource.bytes.as_slice()))
            .with_guessed_format()
            .with_context(|| {
                format!(
                    "Project resource '{}' is not a supported image",
                    resource.id
                )
            })?;
        let decoder = reader
            .into_decoder()
            .with_context(|| format!("Project resource '{}' cannot be decoded", resource.id))?;
        let (width, height) = decoder.dimensions();
        ensure!(
            valid_dimensions(width, height)
                && u64::from(width) * u64::from(height) <= MAX_BINDING_IMAGE_PIXELS,
            "Project resource '{}' exceeds the image binding pixel limit",
            resource.id
        );
        let image = image::DynamicImage::from_decoder(decoder)
            .with_context(|| format!("Project resource '{}' cannot be decoded", resource.id))?
            .to_rgba8();
        ensure!(
            images.insert(resource.id.clone(), image).is_none(),
            "Duplicate binding image resource ID"
        );
    }
    Ok(images)
}

/// Return non-empty image resource IDs used by a selected set of rows. The UI
/// uses this before spawning work, so it copies only project-owned bytes that
/// the worker can actually consume.
pub fn binding_image_resource_ids(
    schema: &BindingSchema,
    table: &BindingTable,
    rows: &[usize],
) -> Result<BTreeSet<String>> {
    let mut resources = BTreeSet::new();
    for &row in rows {
        let values = table.rows.get(row).context("Binding row is out of range")?;
        for binding in schema
            .bindings
            .iter()
            .filter(|binding| binding.target == BindingTarget::ImageResource)
        {
            let column = table
                .column_index(&binding.column)
                .context("Binding column disappeared")?;
            let value = values[column].trim();
            if !value.is_empty() {
                resources.insert(value.to_owned());
            }
        }
    }
    Ok(resources)
}

pub fn validate_bindings(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
) -> Vec<BindingIssue> {
    validate_bindings_with_resource_ids(document, schema, table, None)
}

fn validate_bindings_with_resource_ids(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
    resource_ids: Option<&HashSet<String>>,
) -> Vec<BindingIssue> {
    let mut issues = vec![];
    let mut fields = HashMap::new();
    collect_fields(&document.layers, &mut fields);
    for (binding_index, binding) in schema.bindings.iter().enumerate() {
        // Applying two values to the same native target would make a CSV
        // result depend on schema order. Reject that ambiguous mapping before
        // workers decode resources or create any page drafts.
        if schema.bindings[..binding_index]
            .iter()
            .any(|previous| previous.field == binding.field && previous.target == binding.target)
        {
            issues.push(BindingIssue {
                row: None,
                field: Some(binding.field.clone()),
                message: "Template field is mapped more than once for this target".into(),
            });
        }
        let occurrences = fields
            .get(binding.field.as_str())
            .copied()
            .unwrap_or_default();
        if occurrences == 0 {
            issues.push(BindingIssue {
                row: None,
                field: Some(binding.field.clone()),
                message: "Template field does not exist".into(),
            });
        } else if occurrences > 1 {
            issues.push(BindingIssue {
                row: None,
                field: Some(binding.field.clone()),
                message: "Template field is ambiguous".into(),
            });
        } else if let Some(layer) = find_field(&document.layers, &binding.field)
            && !binding_target_supported(layer, binding.target)
        {
            issues.push(BindingIssue {
                row: None,
                field: Some(binding.field.clone()),
                message: match binding.target {
                    BindingTarget::Text => "Text binding targets a non-text field",
                    BindingTarget::ImageResource => {
                        "Image binding needs an image placeholder or frame"
                    }
                    BindingTarget::Visibility => "Visibility binding targets an invalid layer",
                    BindingTarget::AltText => "Alt-text binding targets an invalid layer",
                }
                .into(),
            });
        }
        let Some(column) = table.column_index(&binding.column) else {
            issues.push(BindingIssue {
                row: None,
                field: Some(binding.field.clone()),
                message: format!("CSV column '{}' does not exist", binding.column),
            });
            continue;
        };
        for (row_index, row) in table.rows.iter().enumerate() {
            let value = &row[column];
            if binding.required && value.trim().is_empty() {
                issues.push(BindingIssue {
                    row: Some(row_index),
                    field: Some(binding.field.clone()),
                    message: "Required value is empty".into(),
                });
            }
            if binding.target == BindingTarget::Visibility
                && !value.trim().is_empty()
                && parse_visibility(value).is_none()
            {
                issues.push(BindingIssue {
                    row: Some(row_index),
                    field: Some(binding.field.clone()),
                    message: "Visibility must be true/false, yes/no, 1/0 or empty".into(),
                });
            }
            if binding.target == BindingTarget::ImageResource {
                let resource = value.trim();
                if !resource.is_empty()
                    && resource_ids.is_some_and(|known| !known.contains(resource))
                {
                    issues.push(BindingIssue {
                        row: Some(row_index),
                        field: Some(binding.field.clone()),
                        message: format!(
                            "Image resource '{resource}' is not packaged with this project"
                        ),
                    });
                }
            }
            if binding.target == BindingTarget::AltText && value.len() > 16_384 {
                issues.push(BindingIssue {
                    row: Some(row_index),
                    field: Some(binding.field.clone()),
                    message: "Alt text is too long".into(),
                });
            }
        }
    }
    if let Some(column) = &schema.filename_column
        && table.column_index(column).is_none()
    {
        issues.push(BindingIssue {
            row: None,
            field: None,
            message: format!("Filename column '{column}' does not exist"),
        });
    }
    issues
}

pub fn apply_binding_row(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
    row: usize,
) -> Result<Document> {
    apply_binding_row_with_images(document, schema, table, row, |_| {
        bail!("Image binding needs a resource resolver")
    })
}

pub fn apply_binding_row_with_images(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
    row: usize,
    resolve_image: impl FnMut(&str) -> Result<RgbaImage>,
) -> Result<Document> {
    table.rows.get(row).context("Binding row is out of range")?;
    let global_issues = validate_bindings(document, schema, table)
        .into_iter()
        .filter(|issue| issue.row.is_none() || issue.row == Some(row))
        .collect::<Vec<_>>();
    ensure!(
        global_issues.is_empty(),
        "Binding row is invalid: {}",
        global_issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    apply_binding_row_unchecked(document, schema, table, row, resolve_image)
}

fn apply_binding_row_unchecked(
    document: &Document,
    schema: &BindingSchema,
    table: &BindingTable,
    row: usize,
    mut resolve_image: impl FnMut(&str) -> Result<RgbaImage>,
) -> Result<Document> {
    let values = table.rows.get(row).context("Binding row is out of range")?;
    let mut result = document.clone();
    for binding in &schema.bindings {
        let column = table
            .column_index(&binding.column)
            .context("Binding column disappeared")?;
        let value = &values[column];
        let layer = find_field_mut(&mut result.layers, &binding.field)
            .context("Template field disappeared")?;
        match binding.target {
            BindingTarget::Text => {
                let mut style =
                    objects::live_text(layer)?.context("Text binding targets a non-text field")?;
                objects::set_text_content(&mut style, value.clone());
                // CSV rows must not silently turn an editable text field into
                // clipped copy. Native templates have fixed live-text boxes
                // and fit no lower than 16 px; an already smaller custom
                // style keeps its authored lower bound. Unboxed live text has
                // no fit constraint and remains at its authored size.
                if style.box_size.is_some() {
                    let minimum_size = style.font_size.min(16.0);
                    let fit = objects::fit_text_to_box(&style, minimum_size)?;
                    ensure!(
                        !fit.report.overflows(),
                        "Text for '{}' does not fit its template box at the {:.0} px minimum",
                        binding.field,
                        minimum_size,
                    );
                    style = fit.style;
                }
                objects::set_live_text(layer, style)?;
            }
            BindingTarget::Visibility => {
                layer.visible = parse_visibility(value).unwrap_or(false);
            }
            BindingTarget::ImageResource => {
                let resource_id = value.trim();
                if resource_id.is_empty() {
                    continue;
                }
                let image = resolve_image(resource_id)
                    .with_context(|| format!("Cannot resolve image resource '{resource_id}'"))?;
                if frame_spec(layer)?.is_some() {
                    replace_frame_image(layer, image)?;
                } else {
                    // Image placeholders are often live shapes in a native
                    // template. Once a row supplies pixels, remove only that
                    // generator metadata; keep its layer, placement and mask.
                    objects::detach_live_object(layer);
                    layer.image = Some(image.into());
                }
                create_metadata_mut(layer).insert("boundImageResource".into(), json!(resource_id));
            }
            BindingTarget::AltText => {
                ensure!(value.len() <= 16_384, "Alt text is too long");
                if let Some(mut frame) = frame_spec(layer)? {
                    frame.alt_text = (!value.is_empty()).then(|| value.clone());
                    // Frame accessibility metadata is stored with the native
                    // frame specification, alongside its source crop and mask.
                    place_frame_layer(layer, &frame)?;
                } else {
                    create_metadata_mut(layer).insert("altText".into(), json!(value));
                }
            }
        }
    }
    result.metadata[CREATE_METADATA]["binding"] = json!({ "row": row });
    Ok(result)
}

pub fn binding_output_name(
    schema: &BindingSchema,
    table: &BindingTable,
    row: usize,
) -> Result<String> {
    let fallback = format!("variant-{:04}", row + 1);
    let Some(column) = &schema.filename_column else {
        return Ok(fallback);
    };
    let index = table
        .column_index(column)
        .context("Filename column does not exist")?;
    let raw = table.rows.get(row).context("Binding row is out of range")?[index].trim();
    let safe = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let safe = safe.trim_matches('-');
    Ok(if safe.is_empty() {
        fallback
    } else {
        safe.chars().take(120).collect()
    })
}

fn parse_csv(input: &str) -> Result<Vec<Vec<String>>> {
    let mut records = vec![];
    let mut record = vec![];
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    let mut after_quote = false;
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    after_quote = true;
                }
            } else {
                field.push(character);
            }
        } else if after_quote {
            match character {
                ',' => {
                    push_csv_field(&mut record, &mut field)?;
                    after_quote = false;
                }
                '\n' => {
                    push_csv_field(&mut record, &mut field)?;
                    records.push(std::mem::take(&mut record));
                    after_quote = false;
                }
                '\r' if chars.peek() == Some(&'\n') => {}
                ' ' | '\t' => {}
                _ => bail!("Unexpected character after a quoted CSV field"),
            }
        } else {
            match character {
                '"' if field.is_empty() => quoted = true,
                '"' => bail!("Quote inside an unquoted CSV field"),
                ',' => push_csv_field(&mut record, &mut field)?,
                '\n' => {
                    push_csv_field(&mut record, &mut field)?;
                    records.push(std::mem::take(&mut record));
                }
                '\r' if chars.peek() == Some(&'\n') => {}
                _ => field.push(character),
            }
        }
    }
    ensure!(!quoted, "Unterminated quoted CSV field");
    if after_quote || !field.is_empty() || !record.is_empty() {
        push_csv_field(&mut record, &mut field)?;
        records.push(record);
    }
    if records
        .last()
        .is_some_and(|record| record.len() == 1 && record[0].is_empty())
    {
        records.pop();
    }
    ensure!(
        records.len() <= MAX_BINDING_ROWS + 1,
        "Binding CSV has too many rows"
    );
    Ok(records)
}

fn push_csv_field(record: &mut Vec<String>, field: &mut String) -> Result<()> {
    ensure!(
        record.len() < MAX_BINDING_COLUMNS,
        "Binding CSV has too many columns"
    );
    ensure!(
        field.len() <= MAX_BINDING_FIELD_BYTES,
        "Binding CSV field is too large"
    );
    record.push(std::mem::take(field));
    Ok(())
}

fn parse_visibility(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" | "show" | "visible" => Some(true),
        "false" | "no" | "0" | "hide" | "hidden" | "" => Some(false),
        _ => None,
    }
}

fn collect_fields<'a>(layers: &'a [Layer], fields: &mut HashMap<&'a str, usize>) {
    for layer in layers {
        if let Some(field) = layer_field(layer) {
            *fields.entry(field).or_default() += 1;
        }
        collect_fields(&layer.children, fields);
    }
}

fn collect_field_targets(layers: &[Layer], fields: &mut HashMap<String, Vec<BindingTarget>>) {
    for layer in layers {
        if let Some(field) = layer_field(layer) {
            let target = if objects::live_text(layer).ok().flatten().is_some() {
                BindingTarget::Text
            } else if frame_spec(layer).ok().flatten().is_some() {
                BindingTarget::ImageResource
            } else {
                BindingTarget::Visibility
            };
            fields.entry(field.to_owned()).or_default().push(target);
        }
        collect_field_targets(&layer.children, fields);
    }
}

fn find_field<'a>(layers: &'a [Layer], field: &str) -> Option<&'a Layer> {
    for layer in layers {
        if layer_field(layer) == Some(field) {
            return Some(layer);
        }
        if let Some(found) = find_field(&layer.children, field) {
            return Some(found);
        }
    }
    None
}

fn binding_target_supported(layer: &Layer, target: BindingTarget) -> bool {
    match target {
        BindingTarget::Text => objects::live_text(layer).ok().flatten().is_some(),
        BindingTarget::ImageResource => {
            frame_spec(layer).ok().flatten().is_some()
                || (!layer.is_group()
                    && layer.image.is_some()
                    && objects::live_text(layer).ok().flatten().is_none())
        }
        BindingTarget::Visibility | BindingTarget::AltText => true,
    }
}

fn find_field_mut<'a>(layers: &'a mut [Layer], field: &str) -> Option<&'a mut Layer> {
    for layer in layers {
        if layer_field(layer) == Some(field) {
            return Some(layer);
        }
        if let Some(found) = find_field_mut(&mut layer.children, field) {
            return Some(found);
        }
    }
    None
}

fn layer_field(layer: &Layer) -> Option<&str> {
    create_value(layer, "field")
        .and_then(Value::as_str)
        .or_else(|| {
            create_value(layer, "frame")
                .and_then(|frame| frame.get("contentField"))
                .and_then(Value::as_str)
        })
}

fn create_value<'a>(layer: &'a Layer, key: &str) -> Option<&'a Value> {
    layer.metadata.get(CREATE_METADATA)?.get(key)
}

fn create_metadata_mut(layer: &mut Layer) -> &mut serde_json::Map<String, Value> {
    if !layer.metadata.is_object() {
        layer.metadata = json!({});
    }
    if !layer
        .metadata
        .get(CREATE_METADATA)
        .is_some_and(Value::is_object)
    {
        layer.metadata[CREATE_METADATA] = json!({});
    }
    layer.metadata[CREATE_METADATA].as_object_mut().unwrap()
}

fn valid_field_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 256
        && value.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, '_' | '-' | ' ' | '.')
        })
}

fn binding_header_target(value: &str) -> Option<(BindingTarget, &str)> {
    let (prefix, field) = value.split_once(':')?;
    let target = match prefix {
        "image" => BindingTarget::ImageResource,
        "visible" => BindingTarget::Visibility,
        "alt" => BindingTarget::AltText,
        _ => return None,
    };
    Some((target, field))
}

fn valid_binding_header(value: &str) -> bool {
    binding_header_target(value).map_or_else(
        || valid_field_name(value),
        |(_, field)| valid_field_name(field),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT_EPSILON: f32 = 1.1;

    fn assert_native_text_fits(layers: &[Layer], template_id: &str, format: &str) {
        for layer in layers {
            if let Some(style) = objects::live_text(layer).unwrap() {
                assert!(
                    !objects::text_layout_report(&style).unwrap().overflows(),
                    "{} {} text field {:?} must fit its fixed native box",
                    template_id,
                    format,
                    layer_field(layer)
                );
            }
            assert_native_text_fits(&layer.children, template_id, format);
        }
    }

    fn assert_layers_within_canvas(
        layers: &[Layer],
        width: u32,
        height: u32,
        template_id: &str,
        format: &str,
    ) {
        for layer in layers {
            if let Some(image) = &layer.image {
                let right = layer.offset_x + image.width() as f32 * layer.scale_x.abs();
                let bottom = layer.offset_y + image.height() as f32 * layer.scale_y.abs();
                assert!(
                    layer.offset_x >= -LAYOUT_EPSILON
                        && layer.offset_y >= -LAYOUT_EPSILON
                        && right <= width as f32 + LAYOUT_EPSILON
                        && bottom <= height as f32 + LAYOUT_EPSILON,
                    "{} {} layer {:?} escapes the canvas: ({}, {}) to ({}, {}) of {}x{}",
                    template_id,
                    format,
                    layer.name,
                    layer.offset_x,
                    layer.offset_y,
                    right,
                    bottom,
                    width,
                    height
                );
            }
            assert_layers_within_canvas(&layer.children, width, height, template_id, format);
        }
    }

    fn native_text_snapshot(
        layers: &[Layer],
        snapshot: &mut Vec<(String, LiveTextStyle, f32, f32)>,
    ) {
        for layer in layers {
            if let Some(style) = objects::live_text(layer).unwrap() {
                snapshot.push((layer.id.clone(), style, layer.offset_x, layer.offset_y));
            }
            native_text_snapshot(&layer.children, snapshot);
        }
    }

    #[test]
    fn catalog_defaults_fit_and_stay_on_canvas_in_standard_adaptations() {
        let brand = sugata_brand_kit();
        for template in templates() {
            let source = instantiate_template(template.id, Some(&brand)).unwrap();
            for (format, width, height) in [
                ("source", template.width, template.height),
                ("square", 1080, 1080),
                ("story", 1080, 1920),
                ("wide", 1200, 630),
            ] {
                let document =
                    resize_layout(&source, width, height, ResizeStrategy::Adapt).unwrap();
                assert_native_text_fits(&document.layers, template.id, format);
                assert_layers_within_canvas(&document.layers, width, height, template.id, format);

                if template.id == "before-after" {
                    let headline = find_field(&document.layers, "headline").unwrap();
                    let before = find_field(&document.layers, "before_image").unwrap();
                    let headline_bottom = headline.offset_y
                        + headline.image.as_ref().unwrap().height() as f32 * headline.scale_y;
                    assert!(
                        headline_bottom <= before.offset_y - 4.0,
                        "before-after {format} headline must clear its editable frames"
                    );
                    let body = find_field(&document.layers, "body").unwrap();
                    let before_bottom = before.offset_y
                        + before.image.as_ref().unwrap().height() as f32 * before.scale_y;
                    assert!(
                        body.offset_y >= before_bottom + 4.0,
                        "before-after {format} body must remain below its editable frames"
                    );
                }

                if template.id == "video-title" {
                    let text_panel = document
                        .layers
                        .iter()
                        .find(|layer| layer.name == "Text field")
                        .expect("video title has a dark text panel");
                    let panel_image = text_panel.image.as_ref().unwrap();
                    let panel_left = text_panel.offset_x;
                    let panel_right =
                        text_panel.offset_x + panel_image.width() as f32 * text_panel.scale_x.abs();
                    for field in ["eyebrow", "headline", "body"] {
                        let text = find_field(&document.layers, field).unwrap();
                        let image = text.image.as_ref().unwrap();
                        let text_right = text.offset_x + image.width() as f32 * text.scale_x.abs();
                        assert!(
                            text.offset_x >= panel_left - LAYOUT_EPSILON
                                && text_right <= panel_right + LAYOUT_EPSILON,
                            "video-title {format} {field} must stay within its dark text panel"
                        );
                    }
                }

                if let (Some(body), Some(detail)) = (
                    find_field(&document.layers, "body"),
                    find_field(&document.layers, "detail"),
                ) {
                    let body_bottom =
                        body.offset_y + body.image.as_ref().unwrap().height() as f32 * body.scale_y;
                    assert!(
                        body_bottom <= detail.offset_y - 4.0,
                        "{} {format} body must clear its detail card",
                        template.id
                    );
                }
            }
        }
    }

    #[test]
    fn sugata_brand_reapplication_keeps_default_native_text_fitted_and_stable() {
        let brand = sugata_brand_kit();
        for template in templates() {
            let mut document = instantiate_template(template.id, None).unwrap();
            apply_brand(&mut document, &brand).unwrap();
            assert_native_text_fits(&document.layers, template.id, "first Sugata application");
            let mut first = Vec::new();
            native_text_snapshot(&document.layers, &mut first);

            apply_brand(&mut document, &brand).unwrap();
            assert_native_text_fits(&document.layers, template.id, "second Sugata application");
            let mut second = Vec::new();
            native_text_snapshot(&document.layers, &mut second);
            assert_eq!(
                second, first,
                "{} native text must not drift when Sugata is reapplied",
                template.id
            );
        }
    }

    #[test]
    fn rejected_brand_fit_leaves_public_document_and_project_calls_unchanged() {
        fn overflowing_template() -> Document {
            let mut document = instantiate_template("editorial-quote", None).unwrap();
            let body = find_field_mut(&mut document.layers, "body").unwrap();
            let mut style = objects::live_text(body).unwrap().unwrap();
            objects::set_text_content(&mut style, "A readable sentence. ".repeat(80));
            objects::set_live_text(body, style).unwrap();
            document
        }

        let brand = sugata_brand_kit();
        let mut document = overflowing_template();
        let before_pixels = crate::raster::composite(&document);
        let before_headline = objects::live_text(find_field(&document.layers, "headline").unwrap())
            .unwrap()
            .unwrap();
        assert!(apply_brand(&mut document, &brand).is_err());
        assert_eq!(crate::raster::composite(&document), before_pixels);
        assert_eq!(
            objects::live_text(find_field(&document.layers, "headline").unwrap())
                .unwrap()
                .unwrap(),
            before_headline
        );

        let mut project = Project::new(
            "Atomic brand",
            instantiate_template("action-card", None).unwrap(),
        );
        let first_page = project.active_page_id().to_owned();
        project
            .add_page("Overflow", overflowing_template())
            .unwrap();
        let brand_id = project.add_brand(brand).unwrap();
        let before_active_brand = project.active_brand_id.clone();
        let before_first_page =
            crate::raster::composite(project.page_document(&first_page).unwrap());
        assert!(apply_brand_to_project(&mut project, &brand_id).is_err());
        assert_eq!(project.active_brand_id, before_active_brand);
        assert_eq!(
            crate::raster::composite(project.page_document(&first_page).unwrap()),
            before_first_page
        );
    }

    #[test]
    fn catalog_has_twenty_distinct_editable_templates() {
        assert_eq!(templates().len(), 20);
        let ids = templates()
            .iter()
            .map(|template| template.id)
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), 20);
        for template in templates() {
            let document = instantiate_template(template.id, None).unwrap();
            assert!(
                document
                    .layers
                    .iter()
                    .any(|layer| objects::live_text(layer).unwrap().is_some())
            );
            assert!(
                document
                    .layers
                    .iter()
                    .any(|layer| objects::live_shape(layer).unwrap().is_some())
            );
            let advertised = template_text_fields(template.id)
                .unwrap()
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            let actual = document
                .layers
                .iter()
                .filter(|layer| objects::live_text(layer).unwrap().is_some())
                .filter_map(layer_field)
                .collect::<BTreeSet<_>>();
            assert_eq!(advertised, actual, "{}", template.id);
        }
        assert!(template_text_fields("missing-template").is_err());
    }

    #[test]
    fn before_after_headline_box_holds_two_lines_and_csv_copy_fits() {
        let document = instantiate_template("before-after", None).unwrap();
        let headline = find_field(&document.layers, "headline").unwrap();
        let mut style = objects::live_text(headline).unwrap().unwrap();
        objects::set_text_content(&mut style, "Shift the frame,\nkeep the meaning");
        assert!(
            !objects::text_layout_report(&style).unwrap().overflows(),
            "The default before-after headline must hold its two-line copy"
        );

        let table = BindingTable::from_csv(
            "headline\nA practical change can make the important idea easier to notice\n",
        )
        .unwrap();
        let schema = auto_map_bindings(&document, &table);
        let bound = apply_binding_row(&document, &schema, &table, 0).unwrap();
        let style = objects::live_text(find_field(&bound.layers, "headline").unwrap())
            .unwrap()
            .unwrap();
        assert!(style.font_size >= 16.0);
        assert!(!objects::text_layout_report(&style).unwrap().overflows());
    }

    #[test]
    fn brand_application_changes_live_objects_without_detaching_them() {
        let mut document = instantiate_template("editorial-quote", None).unwrap();
        let mut brand = BrandKit::new("Test");
        brand.colors.insert("ink".into(), [12, 34, 56, 255]);
        brand.colors.insert("accent".into(), [100, 110, 120, 255]);
        brand.fonts.heading = "serif".into();
        let report = apply_brand(&mut document, &brand).unwrap();
        assert!(report.colors_updated > 0);
        let headline = document
            .layers
            .iter()
            .find(|layer| layer_field(layer) == Some("headline"))
            .unwrap();
        let style = objects::live_text(headline).unwrap().unwrap();
        assert_eq!((style.red, style.green, style.blue), rgb([12, 34, 56, 255]));
        assert_eq!(style.font_name, "serif");
    }

    #[test]
    fn brand_application_updates_every_project_page() {
        let first = instantiate_template("editorial-quote", None).unwrap();
        let second = instantiate_template("lesson-step", None).unwrap();
        let mut project = Project::new("Brand parity", first);
        project.add_page("Second", second).unwrap();
        let mut brand = BrandKit::new("Project brand");
        brand.colors.insert("ink".into(), [12, 34, 56, 255]);
        brand.colors.insert("accent".into(), [210, 133, 102, 255]);
        brand.fonts.heading = "Outfit".into();
        brand.fonts.body = "Outfit".into();
        let brand_id = project.add_brand(brand).unwrap();
        apply_brand_to_project(&mut project, &brand_id).unwrap();
        assert_eq!(project.active_brand_id.as_deref(), Some(brand_id.as_str()));
        for page_id in project.page_ids() {
            assert_eq!(
                project
                    .page_document(&page_id)
                    .unwrap()
                    .metadata
                    .pointer("/omuseCreate/brandId")
                    .and_then(Value::as_str),
                Some(brand_id.as_str())
            );
        }
    }

    #[test]
    fn frame_replacement_retains_crop_and_source_pixels() {
        let mut document = Document::new(500, 500);
        document.layers.clear();
        let mut spec = FrameSpec::new(FrameBounds {
            x: 40.0,
            y: 50.0,
            width: 200.0,
            height: 300.0,
        });
        spec.crop = CropPlacement {
            focal_x: 0.8,
            focal_y: 0.25,
            zoom: 1.4,
        };
        spec.corner_radius = 20.0;
        let id = add_image_frame(
            &mut document,
            "Photo",
            RgbaImage::from_pixel(600, 400, Rgba([1, 2, 3, 255])),
            spec.clone(),
        )
        .unwrap();
        let layer = document.find_layer_mut(&id).unwrap();
        replace_frame_image(layer, RgbaImage::from_pixel(300, 900, Rgba([4, 5, 6, 255]))).unwrap();
        assert_eq!(frame_spec(layer).unwrap().unwrap(), spec);
        assert_eq!(
            layer.image.as_ref().unwrap().get_pixel(0, 0).0,
            [4, 5, 6, 255]
        );
        assert!(layer.mask.is_some());
    }

    #[test]
    fn layout_resize_preserves_live_text_and_uses_anchor_rules() {
        let document = instantiate_template("editorial-quote", None).unwrap();
        let original = document
            .layers
            .iter()
            .find(|layer| layer_field(layer) == Some("headline"))
            .unwrap();
        let resized = resize_layout(&document, 1080, 1920, ResizeStrategy::Adapt).unwrap();
        let changed = resized
            .layers
            .iter()
            .find(|layer| layer_field(layer) == Some("headline"))
            .unwrap();
        assert!(objects::live_text(changed).unwrap().is_some());
        assert!(changed.offset_y > original.offset_y);
        assert_eq!(resized.width, 1080);
        assert_eq!(resized.height, 1920);
    }

    #[test]
    fn layout_resize_moves_component_groups_once_and_scales_children_locally() {
        let mut document = Document::new(100, 100);
        document.layers.clear();
        let mut child = Layer::paint("Child", 10, 10);
        child.offset_x = 10.0;
        child.offset_y = 5.0;
        set_layout_rule(
            &mut child,
            LayerLayoutRule {
                horizontal: HorizontalAnchor::Relative,
                vertical: VerticalAnchor::Relative,
                scale: LayoutScale::Stretch,
            },
        )
        .unwrap();
        let mut group = Layer::group("Component");
        group.offset_x = 20.0;
        group.offset_y = 30.0;
        group.children.push(child);
        set_layout_rule(
            &mut group,
            LayerLayoutRule {
                horizontal: HorizontalAnchor::Relative,
                vertical: VerticalAnchor::Relative,
                scale: LayoutScale::Stretch,
            },
        )
        .unwrap();
        document.layers.push(group);

        resize_layout_in_place(&mut document, 200, 200, ResizeStrategy::Adapt).unwrap();

        let group = &document.layers[0];
        let child = &group.children[0];
        assert_eq!((group.offset_x, group.offset_y), (40.0, 60.0));
        assert_eq!((group.scale_x, group.scale_y), (1.0, 1.0));
        assert_eq!((child.offset_x, child.offset_y), (20.0, 10.0));
        assert_eq!((child.scale_x, child.scale_y), (2.0, 2.0));
    }

    #[test]
    fn brand_spacing_roles_reposition_native_template_text() {
        let mut document = instantiate_template("editorial-quote", None).unwrap();
        let headline = document
            .layers
            .iter()
            .find(|layer| layer_field(layer) == Some("headline"))
            .unwrap();
        assert_eq!(
            create_value(headline, "spacingRole").and_then(Value::as_str),
            Some("standard")
        );
        let base_y = create_value(headline, "spacingBaseY")
            .and_then(Value::as_f64)
            .unwrap() as f32;
        let mut brand = BrandKit::new("Measured rhythm");
        brand.spacing.insert("standard".into(), 84.0);

        let report = apply_brand(&mut document, &brand).unwrap();
        let headline = document
            .layers
            .iter()
            .find(|layer| layer_field(layer) == Some("headline"))
            .unwrap();
        assert_eq!(headline.offset_y, base_y + 84.0);
        assert!(report.spacing_updated > 0);
        assert!(objects::live_text(headline).unwrap().is_some());
    }

    #[test]
    fn component_updates_keep_text_overrides() {
        let prototype = instantiate_template("action-card", None).unwrap();
        let mut target = Document::new(1080, 1080);
        target.layers.clear();
        let overrides = ComponentOverrides {
            text: BTreeMap::from([("headline".into(), "A specific action".into())]),
            hidden_fields: BTreeSet::new(),
        };
        let component_id = uuid::Uuid::new_v4().to_string();
        instantiate_component(&mut target, &component_id, 1, &prototype.layers, &overrides)
            .unwrap();
        let report = propagate_component(&mut target, &component_id, 2, &prototype.layers).unwrap();
        assert_eq!(report.instances_updated, 1);
        let headline = find_field_mut(&mut target.layers, "headline").unwrap();
        assert_eq!(
            objects::live_text(headline).unwrap().unwrap().content,
            "A specific action"
        );
    }

    #[test]
    fn component_definition_draft_keeps_instance_overrides_local() {
        let prototype = instantiate_template("action-card", None).unwrap();
        let mut target = Document::new(1080, 1080);
        target.layers.clear();
        let overrides = ComponentOverrides {
            text: BTreeMap::from([("headline".into(), "Only this page".into())]),
            hidden_fields: BTreeSet::new(),
        };
        let component_id = uuid::Uuid::new_v4().to_string();
        let instance_id =
            instantiate_component(&mut target, &component_id, 1, &prototype.layers, &overrides)
                .unwrap();
        let definition =
            component_definition_layers(&target, &instance_id, &prototype.layers).unwrap();
        assert_ne!(
            objects::live_text(find_field(&definition, "headline").unwrap())
                .unwrap()
                .unwrap()
                .content,
            "Only this page"
        );
    }

    #[test]
    fn shared_background_is_a_locked_component_on_each_page() {
        let first = instantiate_template("editorial-quote", None).unwrap();
        let mut project = Project::new("Collection", first);
        project
            .add_page("Second", instantiate_template("lesson-step", None).unwrap())
            .unwrap();
        let prototype = project.active_document().unwrap().layers[0].clone();
        let component_id = set_shared_background(&mut project, vec![prototype]).unwrap();

        assert_eq!(
            project.metadata.shared_background_component_id.as_deref(),
            Some(component_id.as_str())
        );
        for page_id in project.page_ids() {
            let layer_id = {
                let layer = &project.page_document(&page_id).unwrap().layers[0];
                assert!(layer.locked);
                assert!(is_shared_background(layer));
                layer.id.clone()
            };
            assert_eq!(
                component_instance_info(project.page_document(&page_id).unwrap(), &layer_id)
                    .unwrap()
                    .component_id,
                component_id
            );
        }
    }

    #[test]
    fn shared_background_promotion_replaces_the_source_across_save_reopen_and_propagation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shared-background.omuse");
        let mut first = Document::new(16, 12);
        first.layers.clear();
        let mut source = Layer::paint("Translucent base", 16, 12);
        source.image = Some(RgbaImage::from_pixel(16, 12, Rgba([40, 80, 120, 128])).into());
        let source_id = source.id.clone();
        let mut overlay = Layer::paint("Translucent overlay", 16, 12);
        overlay.image = Some(RgbaImage::from_pixel(16, 12, Rgba([210, 40, 30, 90])).into());
        let overlay_id = overlay.id.clone();
        first.layers.push(source.clone());
        first.layers.push(overlay.clone());
        let expected = crate::raster::composite(&first);
        let mut project = Project::new("Collection", first);
        let second_id = project.add_blank_page("Second", 16, 12).unwrap();

        // Deliberately reverse the selection: promotion must retain the page's
        // own stack order, not the selection or HashSet iteration order.
        let component_id = set_shared_background(&mut project, vec![overlay, source]).unwrap();
        let first_id = project.active_page_id().to_owned();
        assert_eq!(project.page_document(&first_id).unwrap().layers.len(), 1);
        let promoted = &project.page_document(&first_id).unwrap().layers[0];
        assert_ne!(promoted.id, source_id);
        assert_ne!(promoted.id, overlay_id);
        assert_eq!(promoted.children.len(), 2);
        assert_eq!(
            crate::raster::composite(project.page_document(&first_id).unwrap()),
            expected
        );
        assert_eq!(
            crate::raster::composite(project.page_document(&second_id).unwrap()),
            expected
        );

        project.save(&path).unwrap();
        let mut reopened = Project::open(&path).unwrap();
        let reopened_first = reopened.active_page_id().to_owned();
        propagate_project_component(&mut reopened, &component_id).unwrap();
        assert_eq!(
            reopened
                .page_document(&reopened_first)
                .unwrap()
                .layers
                .len(),
            1
        );
        assert_eq!(
            crate::raster::composite(reopened.page_document(&reopened_first).unwrap()),
            expected
        );
        assert_eq!(
            crate::raster::composite(reopened.page_document(&second_id).unwrap()),
            expected
        );
    }

    #[test]
    fn shared_background_is_visible_below_the_default_transparent_canvas_layer() {
        let mut first = Document::new(12, 8);
        first.layers.clear();
        let mut color = Layer::paint("Colour", 12, 8);
        color.image = Some(RgbaImage::from_pixel(12, 8, Rgba([32, 64, 96, 255])).into());
        first.layers.push(color.clone());
        let mut project = Project::new("Collection", first);
        let second = project.add_page("Blank", Document::new(12, 8)).unwrap();
        set_shared_background(&mut project, vec![color]).unwrap();

        assert_eq!(
            crate::raster::composite(project.page_document(&second).unwrap())
                .get_pixel(3, 2)
                .0,
            [32, 64, 96, 255]
        );
    }

    #[test]
    fn shared_background_replaces_tagged_template_backgrounds_without_rebranding_local_text() {
        let first = instantiate_template("editorial-quote", None).unwrap();
        let second = instantiate_template("customer-voice", None).unwrap();
        let mut project = Project::new("Collection", first);
        let second_id = project.add_page("Second", second).unwrap();
        let mut brand = BrandKit::new("Local edit guard");
        brand.text_styles.insert(
            "heading".into(),
            BrandTextStyle {
                font: "sans-serif".into(),
                size: 64.0,
                tracking: 0.0,
                leading: 72.0,
            },
        );
        let brand_id = project.add_brand(brand).unwrap();
        apply_brand_to_project(&mut project, &brand_id).unwrap();

        let custom_background = [17, 41, 83, 255];
        let source = {
            let document = project.active_document_mut().unwrap();
            let background = document
                .layers
                .iter_mut()
                .find(|layer| {
                    create_value(layer, "colorRole").and_then(Value::as_str) == Some("background")
                })
                .unwrap();
            let mut shape = objects::live_shape(background).unwrap().unwrap();
            shape.red = custom_background[0] as f32 / 255.0;
            shape.green = custom_background[1] as f32 / 255.0;
            shape.blue = custom_background[2] as f32 / 255.0;
            let (width, height) = background.image.as_ref().unwrap().dimensions();
            objects::set_live_shape(background, shape, width, height).unwrap();
            background.clone()
        };
        {
            let document = project.page_document_mut(&second_id).unwrap();
            let headline = find_field_mut(&mut document.layers, "headline").unwrap();
            let mut style = objects::live_text(headline).unwrap().unwrap();
            style.font_size = 37.0;
            objects::set_live_text(headline, style).unwrap();
        }

        set_shared_background(&mut project, vec![source]).unwrap();
        for refresh in 0..2 {
            if refresh == 1 {
                apply_shared_background_to_page(&mut project, &second_id).unwrap();
            }
            let document = project.page_document(&second_id).unwrap();
            let template_background = document
                .layers
                .iter()
                .find(|layer| {
                    create_value(layer, "colorRole").and_then(Value::as_str) == Some("background")
                })
                .unwrap();
            assert!(!template_background.visible);
            assert_eq!(
                template_background
                    .metadata
                    .pointer("/omuseCreate/sharedBackgroundHidden/wasVisible")
                    .and_then(Value::as_bool),
                Some(true)
            );
            assert_eq!(
                crate::raster::composite(document).get_pixel(2, 2).0,
                custom_background
            );
            assert_eq!(
                objects::live_text(find_field(&document.layers, "headline").unwrap())
                    .unwrap()
                    .unwrap()
                    .font_size,
                37.0
            );
        }
    }

    #[test]
    fn shared_background_on_a_new_page_uses_the_active_brand() {
        let first = instantiate_template("editorial-quote", None).unwrap();
        let prototype = first.layers[0].clone();
        let mut project = Project::new("Collection", first);
        set_shared_background(&mut project, vec![prototype]).unwrap();
        let second = project
            .add_page("New page", Document::new(1080, 1080))
            .unwrap();
        let mut brand = BrandKit::new("Warm paper");
        brand.colors.insert("background".into(), [18, 42, 66, 255]);
        project.add_brand(brand).unwrap();

        apply_shared_background_to_page(&mut project, &second).unwrap();

        assert_eq!(
            crate::raster::composite(project.page_document(&second).unwrap())
                .get_pixel(20, 20)
                .0,
            [18, 42, 66, 255]
        );
    }

    #[test]
    fn brand_logo_uses_only_a_packaged_resource_and_is_locked() {
        let mut project = Project::new(
            "Brand logo",
            instantiate_template("editorial-quote", None).unwrap(),
        );
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(12, 8, Rgba([5, 6, 7, 255])))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let resource = project
            .add_resource("Logo", "image/png", png.into_inner())
            .unwrap();
        let mut brand = BrandKit::new("Test brand");
        brand.logo_resource_ids.push(resource.clone());
        let brand_id = project.add_brand(brand).unwrap();
        let page_id = project.active_page_id().to_owned();
        let layer_id = insert_brand_logo(&mut project, &page_id, &brand_id, &resource).unwrap();

        let layer = project
            .page_document(&page_id)
            .unwrap()
            .find_layer(&layer_id)
            .unwrap();
        assert!(layer.locked);
        assert_eq!(
            layer
                .metadata
                .pointer("/omuseCreate/brandLogo/protected")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            layer
                .metadata
                .pointer("/omuseCreate/brandLogo/resourceId")
                .and_then(Value::as_str),
            Some(resource.as_str())
        );
    }

    #[test]
    fn text_background_preserves_editable_live_text() {
        let mut document = instantiate_template("editorial-quote", None).unwrap();
        let text_id = document
            .layers
            .iter()
            .find(|layer| objects::live_text(layer).unwrap().is_some())
            .unwrap()
            .id
            .clone();
        let background =
            set_live_text_background(&mut document, &text_id, [20, 30, 40, 180], 12.0, 10.0)
                .unwrap();
        assert!(
            objects::live_shape(document.find_layer(&background).unwrap())
                .unwrap()
                .is_some()
        );
        assert!(
            objects::live_text(document.find_layer(&text_id).unwrap())
                .unwrap()
                .is_some()
        );
        assert_eq!(
            document
                .find_layer(&background)
                .unwrap()
                .metadata
                .pointer("/omuseCreate/textBackgroundFor")
                .and_then(Value::as_str),
            Some(text_id.as_str())
        );
    }

    #[test]
    fn locked_ancestor_blocks_create_actions_without_changing_pixels_or_editor_history() {
        let mut document = Document::new(160, 100);
        document.layers.clear();
        let mut first = objects::live_text_layer(
            "First",
            ObjectPoint { x: 12.0, y: 16.0 },
            LiveTextStyle::default(),
        )
        .unwrap();
        let second = objects::live_text_layer(
            "Second",
            ObjectPoint { x: 72.0, y: 16.0 },
            LiveTextStyle::default(),
        )
        .unwrap();
        let first_id = first.id.clone();
        let second_id = second.id.clone();
        first.name = "First text".into();
        let mut protected = Layer::group("Protected component");
        protected.locked = true;
        protected.children = vec![first, second];
        document.layers.push(protected);
        let mut editor = crate::editor::Editor::new(document);
        let before = crate::raster::composite(&editor.document);

        assert!(layer_is_effectively_locked(&editor.document, &first_id));
        assert!(
            arrange_layers(
                &mut editor.document,
                &[first_id.clone(), second_id],
                LayerArrangement::Left,
            )
            .is_err()
        );
        assert!(
            set_live_text_background(&mut editor.document, &first_id, [20, 30, 40, 255], 8.0, 4.0)
                .is_err()
        );
        assert_eq!(crate::raster::composite(&editor.document), before);
        assert_eq!(editor.undo_depth(), 0);
        assert_eq!(editor.redo_depth(), 0);
    }

    #[test]
    fn arrange_layers_aligns_native_raster_layers() {
        let mut document = Document::new(200, 100);
        document.layers = vec![
            Layer::paint("First", 20, 20),
            Layer::paint("Second", 10, 20),
        ];
        document.layers[0].offset_x = 10.0;
        document.layers[1].offset_x = 150.0;
        let ids = document
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect::<Vec<_>>();
        arrange_layers(&mut document, &ids, LayerArrangement::Right).unwrap();
        assert_eq!(document.layers[0].offset_x + 20.0, 160.0);
        assert_eq!(document.layers[1].offset_x + 10.0, 160.0);
    }

    #[test]
    fn csv_bindings_apply_packaged_images_visibility_and_frame_alt_text() {
        let mut document = instantiate_template("editorial-quote", None).unwrap();
        let mut spec = FrameSpec::new(FrameBounds {
            x: 40.0,
            y: 40.0,
            width: 240.0,
            height: 180.0,
        });
        spec.content_field = Some("hero".into());
        let frame_id = add_image_frame(
            &mut document,
            "Hero",
            RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255])),
            spec,
        )
        .unwrap();
        let table = BindingTable::from_csv(
            "headline,image:hero,visible:hero,alt:hero\nBound title,hero-resource,no,Descriptive hero\n",
        )
        .unwrap();
        let schema = auto_map_bindings(&document, &table);
        assert_eq!(schema.bindings.len(), 4);

        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(7, 5, Rgba([4, 5, 6, 255])))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let resources = vec![BindingImageResource {
            id: "hero-resource".into(),
            bytes: bytes.into_inner(),
        }];
        let known = HashSet::from(["hero-resource".to_owned()]);
        let validation =
            validate_binding_rows_with_resource_ids(&document, &schema, &table, Some(&known));
        assert!(validation.global_issues.is_empty());
        assert!(validation.rows[0].valid);

        let pages = prepare_binding_pages_with_resources(
            &document,
            &schema,
            &table,
            &[0],
            &resources,
            || false,
        )
        .unwrap();
        let page = &pages[0].document;
        assert_eq!(
            objects::live_text(find_field(&page.layers, "headline").unwrap())
                .unwrap()
                .unwrap()
                .content,
            "Bound title"
        );
        let frame = page.find_layer(&frame_id).unwrap();
        assert!(!frame.visible);
        assert_eq!(
            frame.image.as_ref().unwrap().get_pixel(0, 0).0,
            [4, 5, 6, 255]
        );
        assert_eq!(
            frame_spec(frame).unwrap().unwrap().alt_text.as_deref(),
            Some("Descriptive hero")
        );

        let unknown =
            BindingTable::from_csv("headline,image:hero\nBound title,missing-resource\n").unwrap();
        let unknown_schema = auto_map_bindings(&document, &unknown);
        assert!(
            !validate_binding_rows_with_resource_ids(
                &document,
                &unknown_schema,
                &unknown,
                Some(&known),
            )
            .rows[0]
                .valid
        );
    }

    #[test]
    fn quoted_csv_and_binding_validation_are_deterministic() {
        let table = BindingTable::from_csv(
            "name,quote,show\nAda,\"Clear, calm\",yes\nLin,\"Two\nlines\",no\n",
        )
        .unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0][1], "Clear, calm");
        assert_eq!(table.rows[1][1], "Two\nlines");
        let document = instantiate_template("editorial-quote", None).unwrap();
        let schema = BindingSchema {
            bindings: vec![
                FieldBinding {
                    field: "headline".into(),
                    column: "quote".into(),
                    target: BindingTarget::Text,
                    required: true,
                },
                FieldBinding {
                    field: "body".into(),
                    column: "show".into(),
                    target: BindingTarget::Visibility,
                    required: false,
                },
            ],
            filename_column: Some("name".into()),
        };
        assert!(validate_bindings(&document, &schema, &table).is_empty());
        let variant = apply_binding_row(&document, &schema, &table, 0).unwrap();
        let headline = variant
            .layers
            .iter()
            .find(|layer| layer_field(layer) == Some("headline"))
            .unwrap();
        assert_eq!(
            objects::live_text(headline).unwrap().unwrap().content,
            "Clear, calm"
        );
        assert_eq!(binding_output_name(&schema, &table, 1).unwrap(), "Lin");
    }

    #[test]
    fn duplicate_csv_target_mapping_is_rejected_before_any_page_is_bound() {
        let document = instantiate_template("editorial-quote", None).unwrap();
        let table = BindingTable::from_csv("first,second\nOne,Two\n").unwrap();
        let schema = BindingSchema {
            bindings: vec![
                FieldBinding {
                    field: "headline".into(),
                    column: "first".into(),
                    target: BindingTarget::Text,
                    required: false,
                },
                FieldBinding {
                    field: "headline".into(),
                    column: "second".into(),
                    target: BindingTarget::Text,
                    required: false,
                },
            ],
            filename_column: None,
        };

        let validation = validate_binding_rows(&document, &schema, &table);
        assert!(validation.rows.iter().all(|row| !row.valid));
        assert!(validation.global_issues.iter().any(|issue| {
            issue.message == "Template field is mapped more than once for this target"
        }));
        assert!(apply_binding_row(&document, &schema, &table, 0).is_err());
    }

    #[test]
    fn bulk_binding_rejects_oversized_combined_canvas_before_preparing_pages() {
        let document = Document::new(1000, 1000);
        let table = BindingTable {
            headers: vec![],
            rows: vec![vec![]; 65],
        };
        let schema = BindingSchema::default();
        let rows = (0..65).collect::<Vec<_>>();

        let error = prepare_binding_pages(&document, &schema, &table, &rows, || false).unwrap_err();
        assert!(error.to_string().contains("64 megapixel"));
        let error =
            prepare_binding_pages_with_resources(&document, &schema, &table, &rows, &[], || false)
                .unwrap_err();
        assert!(error.to_string().contains("64 megapixel"));
    }
}
