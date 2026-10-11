//! A reproducible brief -> editable carousel -> targeted revision -> export journey.
//! Usage: cargo run --release --locked --example content_workflow -- <new-directory>
//! Uses authored plans, not a live provider. Never overwrites an output directory.
use anyhow::{Context, Result, ensure};
use omuse::{
    content_export::{
        ExportManifest, ExportPackageWriter, PackageOptions, PageExportMetadata, RasterExportFormat,
    },
    create,
    create_history::documents_match,
    create_project::Project,
    creative_commands::{CreativeOperation as Op, CreativePlan, LayoutFit},
    creative_context::project_text_context,
    model::{Document, Layer},
    objects, raster,
};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf, sync::atomic::AtomicBool};

const BRIEF: &str = "Create a six-page, 1080 × 1350 editable carousel called Find your files. Use warm cream, plum and orange, clear headings and practical copy. Cover: Find your files. Four steps: give each project a home; name files clearly; identify the current version; archive completed work. Close with a ten-minute action. Preserve native text, add captions and descriptive alt text, and export all six pages. This is original companion content, not an excerpt or a claim that the book has been released.";
const REVISION: &str = "While keeping the cover selected, revise only page three. Change its heading to ‘Name it for your future self’ and add a concrete filename example. Leave the other five pages unchanged.";
const REVISED_HEADLINE: &str = "Name it for\nyour future self";
const REVISED_BODY: &str = "Use a clear subject, a useful date and a status when needed.\n\nKitchen-quote_2026-10-05_approved.pdf";
const SLIDES: [(&str, &str, &str); 6] = [
    (
        "editorial-quote",
        "Find your\nfiles",
        "A small system for finding the right document when you need it.",
    ),
    (
        "lesson-step",
        "Give each\nproject a home",
        "Choose one main folder for each active project. Keep its working files together so you know where to start.",
    ),
    (
        "lesson-step",
        "Use names\nthat explain",
        "Replace vague filenames with words that describe what the file contains.",
    ),
    (
        "lesson-step",
        "Know which\ncopy is current",
        "Choose one current working copy. Mark the version you send, and keep earlier drafts in a separate folder.",
    ),
    (
        "lesson-step",
        "Archive\nfinished work",
        "Move completed projects out of your daily workspace. Keep the folder names and structure you already understand.",
    ),
    (
        "action-card",
        "Fix one\nfolder today",
        "Take ten minutes. Choose one active project, gather its files and name the next document so it is easy to find.",
    ),
];

fn fields(index: usize) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "eyebrow".into(),
            format!("FIND YOUR FILES  /  {:02}", index + 1),
        ),
        ("headline".into(), SLIDES[index].1.into()),
        ("body".into(), SLIDES[index].2.into()),
    ])
}
fn text_layer<'a>(layers: &'a [Layer], field: &str) -> Option<&'a Layer> {
    for layer in layers {
        if layer
            .metadata
            .pointer("/omuseCreate/field")
            .and_then(|v| v.as_str())
            == Some(field)
        {
            return Some(layer);
        }
        if let Some(found) = text_layer(&layer.children, field) {
            return Some(found);
        }
    }
    None
}
fn content(index: usize, revised: bool) -> (String, String) {
    let (headline, body) = if index == 2 && revised {
        (REVISED_HEADLINE, REVISED_BODY)
    } else {
        (SLIDES[index].1, SLIDES[index].2)
    };
    let caption = format!(
        "{} — {}",
        headline.replace('\n', " "),
        body.replace('\n', " ")
    );
    let alt = format!(
        "Slide {} of 6. Cream, plum and orange typographic card. {}",
        index + 1,
        caption
    );
    (caption, alt)
}
fn check_text(doc: &Document, index: usize, revised: bool) -> Result<usize> {
    let mut checked = 0;
    for (field, expected) in fields(index) {
        let expected = if index == 2 && revised {
            match field.as_str() {
                "headline" => REVISED_HEADLINE.into(),
                "body" => REVISED_BODY.into(),
                _ => expected,
            }
        } else {
            expected
        };
        let layer = text_layer(&doc.layers, &field).context("Missing native template field")?;
        let style = objects::live_text(layer)?.context("Text was rasterized")?;
        ensure!(
            style.content == expected,
            "Wrong copy in page {} / {field}",
            index + 1
        );
        ensure!(
            !objects::text_layout_report(&style)?.overflows(),
            "Overflow in page {} / {field}",
            index + 1
        );
        ensure!(
            style.font_size >= if field == "body" { 38. } else { 16. },
            "Unreadable type in page {} / {field}",
            index + 1
        );
        checked += 1;
    }
    ensure!((doc.width, doc.height) == (1080, 1350), "Page size changed");
    let (caption, alt) = content(index, revised);
    ensure!(doc.metadata["omuseContent"]["caption"] == caption);
    ensure!(doc.metadata["omuseContent"]["altText"] == alt);
    Ok(checked)
}
fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("Supply a new output directory")?;
    ensure!(!output.exists(), "Output already exists");
    fs::create_dir_all(&output)?;
    fs::write(
        output.join("brief.txt"),
        format!("{BRIEF}\n\nRevision:\n{REVISION}\n"),
    )?;
    let brand = create::sugata_brand_kit();
    let source = create::instantiate_template(SLIDES[0].0, Some(&brand))?;
    let mut operations = vec![Op::ResizePage {
        width: 1080,
        height: 1350,
        strategy: LayoutFit::Adapt,
    }];
    for (field, value) in fields(0) {
        operations.push(Op::SetText {
            layer_id: text_layer(&source.layers, &field)
                .context("Cover field absent")?
                .id
                .clone(),
            content: value,
        });
    }
    let (caption, alt_text) = content(0, false);
    operations.push(Op::SetContent { caption, alt_text });
    let mut seed = Project::new("Find your files", source);
    let cover_id = seed.active_page_id().to_owned();
    seed.set_page_name(&cover_id, "01 Find your files")?;
    seed.set_page_template(&cover_id, Some(SLIDES[0].0))?;
    seed.add_brand(brand)?;
    for index in 1..6 {
        let (caption, alt_text) = content(index, false);
        operations.push(Op::AddTemplatePage {
            template_id: SLIDES[index].0.into(),
            name: format!("{:02} {}", index + 1, SLIDES[index].1.replace('\n', " ")),
            fields: fields(index),
            caption,
            alt_text,
        });
        operations.push(Op::ResizePage {
            width: 1080,
            height: 1350,
            strategy: LayoutFit::Adapt,
        });
    }
    operations.push(Op::SelectPage {
        page_id: cover_id.clone(),
    });
    let plan = CreativePlan {
        summary: "Six editable file-organisation cards".into(),
        operations,
    };
    let mut initial =
        CreativePlan::parse(&serde_json::to_string(&plan)?)?.prepare_project(&seed)?;
    // A visual review step: larger mobile copy, dark bookends and page counters.
    let mut layout_ops = Vec::new();
    for (index, id) in initial.page_ids().iter().enumerate() {
        let doc = initial.page_document(id)?;
        layout_ops.push(Op::SelectPage {
            page_id: id.clone(),
        });
        let dark = index == 0 || index == 5;
        if dark {
            layout_ops.push(Op::SetBackground {
                color: [43, 22, 32, 255],
            });
        }
        for (field, font_size, leading, height, y) in [
            (
                "headline",
                if index == 0 { 150. } else { 100. },
                if index == 0 { 160. } else { 108. },
                400.,
                280.,
            ),
            ("body", 42., 54., 320., 760.),
        ] {
            let layer = text_layer(&doc.layers, field).context("Missing layout field")?;
            let mut style = objects::live_text(layer)?.context("Expected live text")?;
            style.font_size = font_size;
            style.leading = leading;
            style.box_size = Some(objects::ObjectSize {
                width: 820.,
                height,
            });
            if dark {
                style.red = 247. / 255.;
                style.green = 235. / 255.;
                style.blue = 214. / 255.;
            }
            layout_ops.push(Op::StyleText {
                layer_id: layer.id.clone(),
                style,
            });
            layout_ops.push(Op::PlaceLayer {
                layer_id: layer.id.clone(),
                x: 140.,
                y,
                width: 820.,
                height,
                rotation: 0.,
            });
        }
        layout_ops.push(Op::AddText {
            name: "Page number".into(),
            x: 784.,
            y: 1192.,
            style: objects::LiveTextStyle {
                content: format!("{:02} / 06", index + 1),
                font_name: "Outfit".into(),
                font_size: 32.,
                leading: 38.,
                red: 1.,
                green: 1.,
                blue: 1.,
                box_size: Some(objects::ObjectSize {
                    width: 180.,
                    height: 48.,
                }),
                ..Default::default()
            },
        });
    }
    layout_ops.push(Op::SelectPage {
        page_id: cover_id.clone(),
    });
    let layout = CreativePlan {
        summary: "Improve mobile reading and page orientation".into(),
        operations: layout_ops,
    };
    initial = layout.prepare_project(&initial)?;
    fs::write(
        output.join("layout-plan.json"),
        serde_json::to_vec_pretty(&layout)?,
    )?;
    initial.save(&output.join("before-revision.omuse"))?;
    let ids = initial.page_ids();
    ensure!(ids.len() == 6);
    let context = project_text_context(&initial);
    let target = context["pages"]
        .as_array()
        .context("No inactive-page context")?
        .iter()
        .find(|page| page["pageID"] == ids[2])
        .context("Page three not exposed")?;
    let mut revision_ops = vec![Op::SelectPage {
        page_id: ids[2].clone(),
    }];
    for (old, new) in [(SLIDES[2].1, REVISED_HEADLINE), (SLIDES[2].2, REVISED_BODY)] {
        let layer = target["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|layer| layer["text"] == old)
            .context("Cannot address page-three copy from assistant context")?;
        revision_ops.push(Op::SetText {
            layer_id: layer["id"].as_str().unwrap().into(),
            content: new.into(),
        });
    }
    let (caption, alt_text) = content(2, true);
    revision_ops.push(Op::SetContent { caption, alt_text });
    let revision = CreativePlan {
        summary: REVISION.into(),
        operations: revision_ops,
    };
    let mut final_project =
        CreativePlan::parse(&serde_json::to_string(&revision)?)?.prepare_project(&initial)?;
    for (index, id) in ids.iter().enumerate() {
        ensure!(
            documents_match(initial.page_document(id)?, final_project.page_document(id)?)
                == (index != 2),
            "Revision affected wrong page"
        );
    }
    final_project.set_active_page(&cover_id)?;
    let project_path = output.join("Find your files.omuse");
    final_project.save(&project_path)?;
    let mut reopened = Project::open(&project_path)?;
    let cancel = AtomicBool::new(false);
    let mut writer = ExportPackageWriter::begin(
        output.join("export"),
        6,
        PackageOptions {
            formats: vec![
                RasterExportFormat::Png,
                RasterExportFormat::Jpeg,
                RasterExportFormat::WebP,
            ],
            ..Default::default()
        },
        &cancel,
    )?;
    let mut text_fields = 0;
    reopened.for_each_page_document(|page, doc| {
        let index = ids.iter().position(|id| id == &page.id).unwrap();
        text_fields += check_text(doc, index, true)?;
        // File loading retains the complete serialized manifest in metadata;
        // an in-memory history identity comparison is not a persistence test.
        let expected = final_project.page_document(&page.id)?;
        ensure!(
            omuse::creative_commands::document_context(expected)
                == omuse::creative_commands::document_context(doc),
            "Reopen changed native text, layer placement or export copy"
        );
        ensure!(
            raster::composite(expected) == raster::composite(doc),
            "Reopen changed rendered pixels"
        );
        writer.write_page(
            doc,
            PageExportMetadata {
                id: page.id.clone(),
                name: page.name.clone(),
                caption: doc.metadata["omuseContent"]["caption"]
                    .as_str()
                    .unwrap()
                    .into(),
                alt_text: doc.metadata["omuseContent"]["altText"]
                    .as_str()
                    .unwrap()
                    .into(),
            },
            &cancel,
            |_| {},
        )
    })?;
    let exported = writer.finish(&cancel)?;
    let manifest: ExportManifest =
        serde_json::from_slice(&fs::read(exported.path.join("manifest.json"))?)?;
    ensure!(manifest.pages.len() == 6);
    for (index, page) in manifest.pages.iter().enumerate() {
        ensure!(page.id == ids[index] && page.order == index + 1);
        let (caption, alt) = content(index, true);
        ensure!(page.caption == caption && page.alt_text == alt);
        let document = reopened.page_document(&ids[index])?;
        let expected = raster::composite(document);
        for file in &page.files {
            let pixels = image::open(exported.path.join(file))?.to_rgba8();
            ensure!(pixels.dimensions() == expected.dimensions());
            if !file.ends_with(".jpg") {
                ensure!(
                    pixels == expected,
                    "Lossless export differs from saved artwork"
                );
            }
        }
    }
    fs::write(
        output.join("creation-plan.json"),
        serde_json::to_vec_pretty(&plan)?,
    )?;
    fs::write(
        output.join("revision-plan.json"),
        serde_json::to_vec_pretty(&revision)?,
    )?;
    fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(
            &json!({"pages":6,"nativeTextFieldsChecked":text_fields,"unchangedPages":5,"targetedRevisionPage":3,"saveReopenNativeTextAndPixelsExact":true,"losslessExportsPixelExact":true,"rasterExports":18,"pdf":"export/pages.pdf","provider":"authored deterministic plans; no live provider request","project":project_path}),
        )?,
    )?;
    println!("{}", output.join("results.json").display());
    Ok(())
}
