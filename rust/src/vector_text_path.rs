//! Bounded text shaping along retained Bézier guides. Stored outlines keep
//! projects portable; an explicit text edit reflows with the available fonts.
use super::{VectorObject, style, transform_path_raw};
use crate::vector_path::{FillRule, VectorPath};
use anyhow::{Context, Result, bail, ensure};
use resvg::usvg;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextPathAlignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextOnPath {
    pub text: String,
    pub font_family: String,
    pub font_size: f32,
    pub letter_spacing: f32,
    /// Fraction of total guide length, before alignment is applied.
    pub start_offset: f32,
    pub alignment: TextPathAlignment,
    pub guide: VectorPath,
    /// The layout's affine transform, baked when scene objects are transformed.
    pub transform: [f32; 6],
    pub resolved_fonts: Vec<String>,
}

impl TextOnPath {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.text.trim().is_empty()
                && self.text.len() <= 4096
                && self.text.chars().count() <= 512
                && !self
                    .text
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')),
            "Text on a path needs a single line of 1–512 characters"
        );
        ensure!(
            !self.font_family.trim().is_empty()
                && self.font_family.len() <= 128
                && !self.font_family.chars().any(char::is_control),
            "Choose a font family of at most 128 bytes"
        );
        ensure!(
            self.font_size.is_finite() && (1. ..=1024.).contains(&self.font_size),
            "Text size must be 1–1024 px"
        );
        ensure!(
            self.letter_spacing.is_finite()
                && (-self.font_size * 0.5..=self.font_size * 4.).contains(&self.letter_spacing),
            "Tracking must be between −0.5 and 4 times the font size"
        );
        ensure!(
            self.start_offset.is_finite() && (0. ..=1.).contains(&self.start_offset),
            "Position along the curve must be 0–100%"
        );
        self.guide.validate()?;
        ensure!(
            self.guide.subpaths.len() == 1
                && (2..=512).contains(&self.guide.subpaths[0].anchors.len()),
            "Choose a single curve with 2–512 anchors for text"
        );
        ensure!(
            self.transform
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
                && style::transform(self.transform).invert().is_some(),
            "Text layout transform is invalid"
        );
        ensure!(
            self.resolved_fonts.len() <= 16
                && self
                    .resolved_fonts
                    .iter()
                    .all(|f| !f.is_empty() && f.len() <= 256),
            "Too many or invalid resolved font names"
        );
        Ok(())
    }

    pub fn bake_transform(&mut self, transform: [f32; 6]) {
        self.transform =
            style::matrix(style::transform(transform).pre_concat(style::transform(self.transform)));
    }

    pub fn set_world_guide(&mut self, guide: &VectorPath) -> Result<()> {
        let inverse = style::transform(self.transform)
            .invert()
            .context("Cannot invert text transform")?;
        let mut candidate = self.clone();
        candidate.guide = transform_path_raw(guide, style::matrix(inverse))?;
        candidate.validate()?;
        self.guide = candidate.guide;
        Ok(())
    }

    pub fn retained_bytes(&self) -> usize {
        self.text.capacity()
            + self.font_family.capacity()
            + self
                .resolved_fonts
                .iter()
                .map(String::capacity)
                .sum::<usize>()
            + self.resolved_fonts.capacity() * std::mem::size_of::<String>()
            + self.guide.subpaths.capacity() * std::mem::size_of::<crate::vector_path::Subpath>()
            + self
                .guide
                .subpaths
                .iter()
                .map(|s| s.anchors.capacity() * std::mem::size_of::<crate::vector_path::Anchor>())
                .sum::<usize>()
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Text shaping cancelled");
    Ok(())
}

fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            db.load_font_data(include_bytes!("../assets/fonts/Outfit.ttf").to_vec());
            Arc::new(db)
        })
        .clone()
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn build_svg(recipe: &TextOnPath, family: &str, on_curve: bool) -> Result<String> {
    let mut path = String::new();
    crate::vector_svg_scene::write_path(&mut path, &recipe.guide)?;
    let anchor = match recipe.alignment {
        TextPathAlignment::Start => "start",
        TextPathAlignment::Center => "middle",
        TextPathAlignment::End => "end",
    };
    let content = if on_curve {
        format!(
            "<textPath href=\"#guide\" startOffset=\"{}%\">{}</textPath>",
            recipe.start_offset * 100.,
            xml(&recipe.text)
        )
    } else {
        xml(&recipe.text)
    };
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000000\" height=\"1000000\"><defs><path id=\"guide\" d=\"{path}\"/></defs><text id=\"omuse-text\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" letter-spacing=\"{}\" text-anchor=\"{}\" fill=\"#FFFFFF\" xml:space=\"preserve\">{content}</text></svg>",
        if on_curve { 0 } else { 2000 },
        if on_curve { 0 } else { 2000 },
        xml(family),
        recipe.font_size,
        recipe.letter_spacing,
        if on_curve { anchor } else { "start" }
    ))
}

/// Shape entirely off the UI thread. All generated path data is bounded before
/// it can enter a scene, and a too-short guide is an error rather than lost text.
pub fn shape(recipe: &TextOnPath, cancel: &AtomicBool) -> Result<(TextOnPath, VectorPath)> {
    recipe.validate()?;
    check_cancel(cancel)?;
    let db = fonts();
    check_cancel(cancel)?;
    let family = match recipe.font_family.as_str() {
        "sans-serif" => usvg::fontdb::Family::SansSerif,
        "serif" => usvg::fontdb::Family::Serif,
        "monospace" => usvg::fontdb::Family::Monospace,
        name => usvg::fontdb::Family::Name(name),
    };
    let id = db
        .query(&usvg::fontdb::Query {
            families: &[family],
            ..Default::default()
        })
        .or_else(|| {
            db.query(&usvg::fontdb::Query {
                families: &[usvg::fontdb::Family::Name("Outfit")],
                ..Default::default()
            })
        })
        .context("No font is available for text on path")?;
    let family = db
        .face(id)
        .and_then(|face| face.families.first())
        .map(|f| f.0.clone())
        .context("Cannot identify resolved font")?;
    let options = usvg::Options {
        fontdb: db.clone(),
        font_family: family.clone(),
        ..Default::default()
    };
    let baseline = usvg::Tree::from_str(&build_svg(recipe, &family, false)?, &options)
        .context("Cannot shape text")?;
    check_cancel(cancel)?;
    let curved = usvg::Tree::from_str(&build_svg(recipe, &family, true)?, &options)
        .context("Cannot shape curved text")?;
    check_cancel(cancel)?;
    let baseline = text_node(&baseline)?;
    let curved_text = text_node(&curved)?;
    let expected: Vec<_> = baseline
        .layouted()
        .iter()
        .flat_map(|s| s.positioned_glyphs.iter())
        .collect();
    let glyphs: Vec<_> = curved_text
        .layouted()
        .iter()
        .flat_map(|s| s.positioned_glyphs.iter())
        .collect();
    ensure!(
        !glyphs.is_empty() && glyphs.len() <= 1024,
        "Text glyph count is outside supported bounds"
    );
    ensure!(
        expected.len() == glyphs.len()
            && expected
                .iter()
                .zip(&glyphs)
                .all(|(a, b)| a.id == b.id && a.text == b.text),
        "Text does not fit the curve; reduce size/tracking, move the start position, or lengthen the curve"
    );
    ensure!(
        glyphs.iter().all(|g| g.id.0 != 0),
        "A character is missing from the available fonts; choose a font that includes it"
    );
    let mut fonts = Vec::new();
    for glyph in &glyphs {
        if let Some(name) = curved
            .fontdb()
            .face(glyph.font)
            .and_then(|f| f.families.first())
            .map(|f| f.0.clone())
        {
            if !fonts.contains(&name) {
                fonts.push(name);
            }
        }
    }
    let mut path = VectorPath {
        subpaths: Vec::new(),
        fill_rule: FillRule::NonZero,
    };
    collect_outlines(curved_text.flattened(), &mut path, cancel)?;
    ensure!(
        !path.subpaths.is_empty(),
        "Text contains no editable outlines"
    );
    path = transform_path_raw(&path, recipe.transform)?;
    let mut result = recipe.clone();
    result.resolved_fonts = fonts;
    result.validate()?;
    check_cancel(cancel)?;
    Ok((result, path))
}

fn collect_outlines(group: &usvg::Group, path: &mut VectorPath, cancel: &AtomicBool) -> Result<()> {
    ensure!(
        group.clip_path().is_none()
            && group.mask().is_none()
            && group.filters().is_empty()
            && group.opacity().get() == 1.,
        "Color or bitmap glyphs are not supported as editable text outlines"
    );
    for node in group.children() {
        check_cancel(cancel)?;
        match node {
            usvg::Node::Group(group) => collect_outlines(group, path, cancel)?,
            usvg::Node::Path(shape) => {
                ensure!(shape.stroke().is_none() && shape.fill().is_some_and(|f|matches!(f.paint(),usvg::Paint::Color(c) if c.red==255 && c.green==255 && c.blue==255) && f.opacity().get()==1.),"Color glyphs are not supported as editable text outlines");
                let outline =
                    crate::vector_svg_scene::convert_path(shape.data(), shape.abs_transform())?;
                path.subpaths.extend(outline.subpaths);
                ensure!(
                    path.subpaths.len() <= 4096
                        && path.subpaths.iter().map(|s| s.anchors.len()).sum::<usize>() <= 100_000,
                    "Text outlines exceed vector geometry limits"
                );
            }
            _ => bail!("Bitmap or nested text glyphs are not supported as editable text outlines"),
        }
    }
    path.validate()
}

pub fn update_object(
    object: &VectorObject,
    recipe: &TextOnPath,
    cancel: &AtomicBool,
) -> Result<VectorObject> {
    let (recipe, path) = shape(recipe, cancel)?;
    let mut object = object.clone();
    object.path = path;
    object.text_path = Some(recipe);
    Ok(object)
}

fn text_node(tree: &usvg::Tree) -> Result<&usvg::Text> {
    match tree.node_by_id("omuse-text") {
        Some(usvg::Node::Text(text)) => Ok(text),
        _ => bail!(
            "Text does not fit the curve; reduce size/tracking, move the start position, or lengthen the curve"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector_path::{Anchor, Point, Subpath};
    #[test]
    fn curved_outlines_match_independent_svg_text_rendering() {
        let recipe = TextOnPath {
            text: "Omuse curves".into(),
            font_family: "Outfit".into(),
            font_size: 32.,
            letter_spacing: 0.,
            start_offset: 0.5,
            alignment: TextPathAlignment::Center,
            guide: VectorPath {
                fill_rule: FillRule::NonZero,
                subpaths: vec![Subpath {
                    closed: false,
                    anchors: vec![
                        Anchor {
                            position: Point { x: 20., y: 150. },
                            incoming: None,
                            outgoing: Some(Point { x: 120., y: 30. }),
                        },
                        Anchor {
                            position: Point { x: 480., y: 150. },
                            incoming: Some(Point { x: 380., y: 30. }),
                            outgoing: None,
                        },
                    ],
                }],
            },
            transform: [1., 0., 0., 1., 0., 0.],
            resolved_fonts: Vec::new(),
        };
        let (_, path) = shape(&recipe, &AtomicBool::new(false)).unwrap();
        let object = VectorObject::new("Text", path, Some([255; 4]), None);
        let scene = super::super::VectorScene {
            version: 4,
            width: 500,
            height: 180,
            objects: vec![object],
        };
        let ours = scene.render(&AtomicBool::new(false)).unwrap();
        let options = usvg::Options {
            fontdb: fonts(),
            font_family: "Outfit".into(),
            ..Default::default()
        };
        let tree =
            usvg::Tree::from_str(&build_svg(&recipe, "Outfit", true).unwrap(), &options).unwrap();
        let mut reference = resvg::tiny_skia::Pixmap::new(500, 180).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut reference.as_mut(),
        );
        let a: usize = ours.pixels().map(|p| usize::from(p[3])).sum();
        let b: usize = reference
            .pixels()
            .iter()
            .map(|p| usize::from(p.alpha()))
            .sum();
        assert!(
            a > 10_000 && a.abs_diff(b) < b / 20,
            "Outline coverage {a} differs from text coverage {b}"
        );
        let differing = ours
            .pixels()
            .zip(reference.pixels())
            .filter(|(a, b)| a[3].abs_diff(b.alpha()) > 40)
            .count();
        assert!(
            differing < 400,
            "Too many differing text pixels: {differing}"
        );
    }
}
