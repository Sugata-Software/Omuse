//! End-to-end visual QA for an editable Sugata carousel.
//!
//! Usage:
//!
//! ```text
//! cargo run --release --example create_acceptance -- /tmp/omuse-create-acceptance [--motion]
//! ```
//!
//! The destination must not exist. The example creates a six-slide Create
//! project, a resized editable story variant, and an export package containing
//! six PNGs and a PDF. It always renders one native motion preview. Passing
//! `--motion` also creates MP4 and GIF versions when FFmpeg is installed.

use anyhow::{Context, Result, ensure};
use image::{ImageFormat, Rgba, RgbaImage};
use omuse::{
    content_export::{
        CollisionPolicy, ExportPackageWriter, PackageOptions, PageExportMetadata,
        RasterExportFormat,
    },
    create::{
        self, BindingImageResource, BindingTable, CropPlacement, FrameBounds, FrameSpec,
        ResizeStrategy,
    },
    create_project::Project,
    document,
    model::{Document, Layer},
    motion::{
        self, LayerAnimation, LayerTrack, MotionExportOptions, MotionFormat, MotionTimeline,
        PageTimeline, PageTransition,
    },
    objects::{self, RichTextPatch},
};
use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

struct Slide<'a> {
    template: &'a str,
    name: &'a str,
    number: Option<&'a str>,
    eyebrow: &'a str,
    headline: &'a str,
    body: &'a str,
}

const SLIDES: [Slide<'static>; 6] = [
    Slide {
        template: "editorial-quote",
        name: "01 Make room for clarity",
        number: None,
        eyebrow: "SUGATA FIELD NOTES · 01",
        headline: "Make room for\nclarity",
        body: "A six-part field guide for turning thoughtful work into a calm, useful rhythm.",
    },
    Slide {
        template: "lesson-cover",
        name: "02 Notice the signal",
        number: Some("02"),
        eyebrow: "SUGATA FIELD NOTES · 02",
        headline: "Notice the\nsignal first",
        body: "Before adding another idea, find the one detail people need to understand.",
    },
    Slide {
        template: "lesson-step",
        name: "03 Give the idea space",
        number: None,
        eyebrow: "SUGATA FIELD NOTES · 03",
        headline: "Give the idea\nenough space",
        body: "A little room around a decision makes the next action easier to see.",
    },
    Slide {
        template: "before-after",
        name: "04 Shift the frame",
        number: None,
        eyebrow: "SUGATA FIELD NOTES · 04",
        headline: "Shift the frame,\nkeep the meaning",
        body: "The same material can feel more generous when hierarchy and pacing improve.",
    },
    Slide {
        template: "service-feature",
        name: "05 Make it repeatable",
        number: None,
        eyebrow: "SUGATA FIELD NOTES · 05",
        headline: "Make thoughtful\nwork repeatable",
        body: "A simple shared system lets the team spend more time on the work that matters.",
    },
    Slide {
        template: "action-card",
        name: "06 Start with one useful move",
        number: None,
        eyebrow: "SUGATA FIELD NOTES · 06",
        headline: "Start with one\nuseful move",
        body: "Save this carousel for the next time your work needs a little more room to breathe.",
    },
];

const BEFORE_CROP: CropPlacement = CropPlacement {
    focal_x: 0.34,
    focal_y: 0.56,
    zoom: 1.16,
};

const AFTER_CROP: CropPlacement = CropPlacement {
    focal_x: 0.68,
    focal_y: 0.42,
    zoom: 1.10,
};

/// Build compact, original abstract artwork rather than downloading or
/// embedding a third-party image. The geometric sources are deliberately
/// non-uniform so frame crop, focal point and source persistence are visible.
fn abstract_artwork(variant: u8) -> RgbaImage {
    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 480;
    let palette = if variant == 0 {
        ([247, 235, 214], [43, 22, 32], [226, 98, 42], [242, 179, 61])
    } else {
        (
            [43, 22, 32],
            [247, 235, 214],
            [240, 136, 90],
            [255, 46, 136],
        )
    };
    RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let diagonal = (x as i32 - y as i32 / 2 - 180).abs() < 42;
        let circle_x = if variant == 0 { 410 } else { 246 };
        let circle_y = if variant == 0 { 202 } else { 272 };
        let dx = x as i32 - circle_x;
        let dy = y as i32 - circle_y;
        let circle = dx * dx + dy * dy < 110 * 110;
        let grid = ((x / 44) + (y / 44) + u32::from(variant)) % 2 == 0;
        let color = if circle {
            palette.3
        } else if diagonal {
            palette.2
        } else if grid && y > 310 {
            palette.1
        } else {
            palette.0
        };
        Rgba([color[0], color[1], color[2], 255])
    })
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image.clone()).write_to(&mut bytes, ImageFormat::Png)?;
    Ok(bytes.into_inner())
}

fn layer_field(layer: &Layer) -> Option<&str> {
    layer
        .metadata
        .pointer("/omuseCreate/field")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            layer
                .metadata
                .pointer("/omuseCreate/frame/contentField")
                .and_then(serde_json::Value::as_str)
        })
}

fn field_layer<'a>(layers: &'a [Layer], field: &str) -> Option<&'a Layer> {
    for layer in layers {
        if layer_field(layer) == Some(field) {
            return Some(layer);
        }
        if let Some(layer) = field_layer(&layer.children, field) {
            return Some(layer);
        }
    }
    None
}

fn field_layer_mut<'a>(layers: &'a mut [Layer], field: &str) -> Option<&'a mut Layer> {
    for layer in layers {
        if layer_field(layer) == Some(field) {
            return Some(layer);
        }
        if let Some(layer) = field_layer_mut(&mut layer.children, field) {
            return Some(layer);
        }
    }
    None
}

fn replace_placeholder_with_frame(
    document: &mut Document,
    field: &str,
    name: &str,
    source: RgbaImage,
    crop: CropPlacement,
    alt_text: &str,
) -> Result<()> {
    let index = document
        .layers
        .iter()
        .position(|layer| layer_field(layer) == Some(field))
        .with_context(|| format!("Template does not contain the '{field}' image placeholder"))?;
    let placeholder = document.layers.remove(index);
    let pixels = placeholder
        .image
        .as_ref()
        .context("Image placeholder has no geometry cache")?;
    let mut frame = FrameSpec::new(FrameBounds {
        x: placeholder.offset_x,
        y: placeholder.offset_y,
        width: pixels.width() as f32 * placeholder.scale_x.abs(),
        height: pixels.height() as f32 * placeholder.scale_y.abs(),
    });
    frame.crop = crop;
    frame.content_field = Some(field.into());
    frame.alt_text = Some(alt_text.into());
    let frame_id = create::add_image_frame(document, name, source, frame)?;
    let frame_layer = document
        .layers
        .pop()
        .context("Image-frame insertion did not create a layer")?;
    ensure!(
        frame_layer.id == frame_id,
        "Image-frame insertion reordered layers"
    );
    document.layers.insert(index, frame_layer);
    Ok(())
}

fn bind_packaged_comparison_frames(
    document: &Document,
    before_resource: &str,
    after_resource: &str,
    resources: &[BindingImageResource],
) -> Result<Document> {
    let table = BindingTable::from_csv(&format!(
        "image:before_image,image:after_image,alt:before_image,alt:after_image\n{before_resource},{after_resource},Abstract before composition,Abstract after composition\n"
    ))?;
    let schema = create::auto_map_bindings(document, &table);
    ensure!(
        schema.bindings.len() == 4,
        "Comparison frame binding schema is incomplete"
    );
    let mut pages = create::prepare_binding_pages_with_resources(
        document,
        &schema,
        &table,
        &[0],
        resources,
        || false,
    )?;
    Ok(pages
        .pop()
        .context("Comparison frame binding did not produce a page")?
        .document)
}

fn verify_packaged_frame(
    document: &Document,
    field: &str,
    expected_source: &RgbaImage,
    expected_resource: &str,
    expected_crop: CropPlacement,
) -> Result<()> {
    let layer = field_layer(&document.layers, field)
        .with_context(|| format!("Missing packaged '{field}' frame"))?;
    let frame = create::frame_spec(layer)?.context("Bound image is no longer an editable frame")?;
    ensure!(frame.content_field.as_deref() == Some(field));
    ensure!(
        frame.crop == expected_crop,
        "Frame crop changed after reopen"
    );
    ensure!(
        layer
            .metadata
            .pointer("/omuseCreate/boundImageResource")
            .and_then(serde_json::Value::as_str)
            == Some(expected_resource),
        "Frame no longer references its packaged project resource"
    );
    ensure!(
        layer
            .image
            .as_ref()
            .context("Frame source pixels are missing")?
            .as_raw()
            == expected_source.as_raw(),
        "Frame source pixels changed after reopen"
    );
    ensure!(layer.mask.is_some(), "Frame mask did not survive reopen");
    Ok(())
}

fn set_field_text(document: &mut Document, field: &str, content: &str) -> Result<()> {
    let layer = field_layer_mut(&mut document.layers, field)
        .with_context(|| format!("Template does not contain the '{field}' text field"))?;
    let mut style = objects::live_text(layer)?.context("Template field is not native live text")?;
    objects::set_text_content(&mut style, content);
    let fit = objects::fit_text_to_box(&style, 16.0)?;
    ensure!(
        !fit.report.overflows(),
        "The '{field}' copy does not fit the template above the 16 px readable minimum"
    );
    objects::set_live_text(layer, fit.style)?;
    Ok(())
}

fn emphasize_clarity(document: &mut Document) -> Result<()> {
    let layer = field_layer_mut(&mut document.layers, "headline")
        .context("Template does not contain a headline")?;
    let mut style = objects::live_text(layer)?.context("Headline is not native live text")?;
    let marker = "clarity";
    let byte_start = style
        .content
        .find(marker)
        .context("Headline has no clarity marker")?;
    let character_start = style.content[..byte_start].chars().count();
    objects::apply_rich_text_patch_characters(
        &mut style,
        character_start,
        character_start + marker.chars().count(),
        RichTextPatch {
            weight: Some(800),
            color: Some([
                0xF0 as f32 / 255.0,
                0x88 as f32 / 255.0,
                0x5A as f32 / 255.0,
                1.0,
            ]),
            ..Default::default()
        },
    )?;
    objects::set_live_text(layer, style)?;
    Ok(())
}

fn make_slide(slide: &Slide<'_>, brand: &omuse::create_project::BrandKit) -> Result<Document> {
    let source = create::instantiate_template(slide.template, Some(brand))?;
    let mut document = create::resize_layout(&source, 1080, 1350, ResizeStrategy::Adapt)?;
    if field_layer(&document.layers, "eyebrow").is_some() {
        set_field_text(&mut document, "eyebrow", slide.eyebrow)?;
    }
    if let Some(number) = slide.number {
        set_field_text(&mut document, "number", number)?;
    }
    set_field_text(&mut document, "headline", slide.headline)?;
    set_field_text(&mut document, "body", slide.body)?;
    if slide.template == "before-after" {
        replace_placeholder_with_frame(
            &mut document,
            "before_image",
            "Before abstract artwork",
            abstract_artwork(0),
            BEFORE_CROP,
            "Warm geometric composition before the reframing",
        )?;
        replace_placeholder_with_frame(
            &mut document,
            "after_image",
            "After abstract artwork",
            abstract_artwork(1),
            AFTER_CROP,
            "Plum geometric composition after the reframing",
        )?;
    }
    Ok(document)
}

fn text_content(document: &Document, field: &str) -> Result<String> {
    let layer = field_layer(&document.layers, field)
        .with_context(|| format!("Missing '{field}' text field"))?;
    Ok(objects::live_text(layer)?
        .context("Expected native live text")?
        .content)
}

fn headline_id(document: &Document) -> Result<String> {
    Ok(field_layer(&document.layers, "headline")
        .context("Missing headline layer")?
        .id
        .clone())
}

fn output_arguments() -> Result<(PathBuf, bool)> {
    let mut values = env::args_os().skip(1);
    let output = values
        .next()
        .map(PathBuf::from)
        .context("Usage: create_acceptance <new-output-directory> [--motion]")?;
    let motion = match values.next() {
        None => false,
        Some(value) if value.to_str() == Some("--motion") => true,
        Some(_) => anyhow::bail!("Usage: create_acceptance <new-output-directory> [--motion]"),
    };
    ensure!(values.next().is_none(), "Too many arguments");
    ensure!(
        !output.exists(),
        "Output directory already exists: {}",
        output.display()
    );
    Ok((output, motion))
}

fn export_motion_if_requested(
    requested: bool,
    output: &Path,
    pages: &[Document],
    timeline: &MotionTimeline,
    cancel: &AtomicBool,
) -> Result<()> {
    if !requested {
        return Ok(());
    }
    let encoder = motion::Ffmpeg::discover().context("--motion needs a working FFmpeg encoder")?;
    let options = MotionExportOptions {
        frames_per_second: 6,
        max_frames: 64,
        ..Default::default()
    };
    motion::export_motion_with_ffmpeg(
        &encoder,
        pages,
        timeline,
        output.join("sugata-carousel.mp4"),
        MotionFormat::Mp4,
        options.clone(),
        cancel,
        |_| {},
    )?;
    motion::export_motion_with_ffmpeg(
        &encoder,
        pages,
        timeline,
        output.join("sugata-carousel.gif"),
        MotionFormat::Gif,
        options,
        cancel,
        |_| {},
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let (output, export_motion) = output_arguments()?;
    fs::create_dir(&output).with_context(|| format!("Creating {}", output.display()))?;
    let cancel = AtomicBool::new(false);
    let brand = create::sugata_brand_kit();

    let mut first = make_slide(&SLIDES[0], &brand)?;
    emphasize_clarity(&mut first)?;
    let headline_before_save = text_content(&first, "headline")?;
    let mut project = Project::new("Sugata field notes", first);
    let first_id = project.active_page_id().to_owned();
    project.set_page_name(&first_id, SLIDES[0].name)?;
    project.set_page_template(&first_id, Some(SLIDES[0].template))?;
    let brand_id = project.add_brand(brand)?;
    let before_artwork = abstract_artwork(0);
    let after_artwork = abstract_artwork(1);
    let before_artwork_png = encode_png(&before_artwork)?;
    let after_artwork_png = encode_png(&after_artwork)?;
    let before_resource = project.add_resource(
        "Sugata abstract before artwork",
        "image/png",
        before_artwork_png.clone(),
    )?;
    let after_resource = project.add_resource(
        "Sugata abstract after artwork",
        "image/png",
        after_artwork_png.clone(),
    )?;
    let mut before_after_page_id = None;
    for slide in &SLIDES[1..] {
        let document = make_slide(
            slide,
            project.active_brand().context("Missing Sugata brand")?,
        )?;
        let page_id = project.add_page(slide.name, document)?;
        project.set_page_template(&page_id, Some(slide.template))?;
        if slide.template == "before-after" {
            before_after_page_id = Some(page_id);
        }
    }
    create::apply_brand_to_project(&mut project, &brand_id)?;
    let before_after_page_id = before_after_page_id.context("Missing before-after slide")?;
    let binding_resources = vec![
        BindingImageResource {
            id: before_resource.clone(),
            bytes: project.resource_bytes(&before_resource)?.to_vec(),
        },
        BindingImageResource {
            id: after_resource.clone(),
            bytes: project.resource_bytes(&after_resource)?.to_vec(),
        },
    ];
    let comparison_source = project.page_document(&before_after_page_id)?.clone();
    let comparison_bound = bind_packaged_comparison_frames(
        &comparison_source,
        &before_resource,
        &after_resource,
        &binding_resources,
    )?;
    project.replace_page_document(&before_after_page_id, comparison_bound)?;
    ensure!(
        project.page_ids().len() == 6,
        "Expected six campaign slides"
    );

    let project_path = output.join("sugata-field-notes.omuse");
    project.save(&project_path)?;
    let mut reopened = Project::open(&project_path)?;
    ensure!(
        reopened.page_ids().len() == 6,
        "Reopen lost a campaign slide"
    );
    let page_ids = reopened.page_ids();
    let documents = page_ids
        .iter()
        .map(|id| reopened.page_document(id).cloned())
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        text_content(&documents[0], "headline")? == headline_before_save,
        "Headline changed after project reopen"
    );
    ensure!(
        text_content(&documents[1], "number")? == "02",
        "Lesson-cover number did not retain its intended slide number"
    );
    let reopened_before_artwork = reopened.resource_bytes(&before_resource)?.to_vec();
    let reopened_after_artwork = reopened.resource_bytes(&after_resource)?.to_vec();
    ensure!(
        reopened_before_artwork == before_artwork_png
            && reopened_after_artwork == after_artwork_png,
        "Packaged artwork bytes changed after project reopen"
    );
    verify_packaged_frame(
        &documents[3],
        "before_image",
        &before_artwork,
        &before_resource,
        BEFORE_CROP,
    )?;
    verify_packaged_frame(
        &documents[3],
        "after_image",
        &after_artwork,
        &after_resource,
        AFTER_CROP,
    )?;
    let preserved = objects::live_text(
        field_layer(&documents[0].layers, "headline").context("Missing first headline")?,
    )?
    .context("Headline was rasterized")?;
    ensure!(
        !preserved.runs.is_empty(),
        "Rich text run did not survive save/reopen"
    );

    let story_variant = create::resize_layout(&documents[0], 1080, 1920, ResizeStrategy::Adapt)?;
    ensure!(
        text_content(&story_variant, "headline")? == headline_before_save,
        "Layout resize did not preserve editable headline text"
    );
    let story_path = output.join("sugata-story-variant.comp");
    document::save(&story_variant, &story_path)?;
    let reopened_story = document::open(&story_path)?;
    ensure!(
        text_content(&reopened_story, "headline")? == headline_before_save,
        "Saved story variant lost native text"
    );
    omuse::raster::composite(&reopened_story).save(output.join("sugata-story-variant.png"))?;

    let mut writer = ExportPackageWriter::begin(
        output.join("social-export"),
        documents.len(),
        PackageOptions {
            formats: vec![RasterExportFormat::Png],
            include_pdf: true,
            collision: CollisionPolicy::Reject,
            ..Default::default()
        },
        &cancel,
    )?;
    for (index, document) in documents.iter().enumerate() {
        writer.write_page(
            document,
            PageExportMetadata {
                id: page_ids[index].clone(),
                name: SLIDES[index].name.into(),
                caption: SLIDES[index].headline.replace('\n', " "),
                alt_text: format!(
                    "Sugata Field Notes slide {}: {}",
                    index + 1,
                    SLIDES[index].headline
                ),
            },
            &cancel,
            |_| {},
        )?;
    }
    let export = writer.finish(&cancel)?;
    ensure!(export.pages == 6, "Export did not retain all six slides");
    ensure!(
        export.path.join("pages.pdf").is_file(),
        "Export PDF is missing"
    );
    ensure!(
        fs::read_dir(export.path.join("images"))?.count() == 6,
        "PNG export set is incomplete"
    );

    let timeline = MotionTimeline {
        pages: documents
            .iter()
            .enumerate()
            .map(|(index, document)| -> Result<PageTimeline> {
                let headline = headline_id(document)?;
                Ok(PageTimeline {
                    page_id: page_ids[index].clone(),
                    duration_ms: 850,
                    transition: if index + 1 == documents.len() {
                        PageTransition::None
                    } else {
                        PageTransition::CrossFade { duration_ms: 150 }
                    },
                    tracks: vec![LayerTrack {
                        layer_id: headline,
                        animations: LayerAnimation::rise_in(0, 320, 24.0).into(),
                    }],
                })
            })
            .collect::<Result<Vec<_>>>()?,
    };
    let frame = motion::render_frame(&documents, &timeline, 0)?;
    frame.save(output.join("motion-preview.png"))?;
    export_motion_if_requested(export_motion, &output, &documents, &timeline, &cancel)?;

    println!("Project: {}", project_path.display());
    println!("Social PNG/PDF package: {}", export.path.display());
    println!("Editable story variant: {}", story_path.display());
    println!(
        "Native motion preview: {}",
        output.join("motion-preview.png").display()
    );
    if export_motion {
        println!("Motion: {}", output.join("sugata-carousel.mp4").display());
        println!("Motion: {}", output.join("sugata-carousel.gif").display());
    }
    Ok(())
}
