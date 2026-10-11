//! Synthetic, reproducible native composition and exchange journey.
//! Usage: cargo run --release --locked --example photo_vector_workflow -- <new-directory>
//! The output directory must not exist. No provider, network or performance claims.
use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use omuse::{
    asset_library::sha256_hex,
    create_history::documents_match,
    document,
    editor::Editor,
    model::{Document, Layer},
    objects::{self, LiveTextStyle, ObjectPoint, ObjectSize},
    psd, psd_export, raster,
    vector_commands::{self, VectorCommand},
    vector_path::{Anchor, Point, StrokeStyle, Subpath, VectorPath},
    vector_pdf,
    vector_repeat::{self, RepeatSpec},
    vector_scene::{VECTOR_SCENE_VERSION, VectorObject, VectorScene},
    vector_svg_scene,
};
use serde_json::{Value, json};
use std::{fs, path::Path, path::PathBuf, sync::atomic::AtomicBool};

const WIDTH: u32 = 1000;
const HEIGHT: u32 = 700;
const INK: [u8; 4] = [65, 45, 58, 255];
const ORANGE: [u8; 4] = [210, 96, 61, 255];
const PEACH: [u8; 4] = [222, 176, 136, 255];
const CREAM: [u8; 4] = [242, 231, 210, 255];

fn scene(width: u32, height: u32, objects: Vec<VectorObject>) -> VectorScene {
    VectorScene {
        version: VECTOR_SCENE_VERSION,
        width,
        height,
        objects,
    }
}

fn text(name: &str, content: &str, x: f32, y: f32, size: f32, width: f32) -> Result<Layer> {
    objects::live_text_layer(
        name,
        ObjectPoint { x, y },
        LiveTextStyle {
            content: content.into(),
            font_name: "Outfit".into(),
            font_size: size,
            red: INK[0] as f32 / 255.,
            green: INK[1] as f32 / 255.,
            blue: INK[2] as f32 / 255.,
            leading: size * 1.1,
            box_size: Some(ObjectSize {
                width,
                height: size * 1.1 * content.lines().count() as f32 + 24.,
            }),
            ..Default::default()
        },
    )
}

fn card_document() -> Result<Document> {
    let mut doc = Document::new(WIDTH, HEIGHT);
    doc.name = "Omuse — Shape your next idea".into();
    doc.metadata["resolution"] = json!(144);
    doc.metadata["omuseEvidence"] = json!({
        "kind":"synthetic cross-feature fixture",
        "provenance":"Original geometric artwork and procedural paper authored for Omuse",
        "font":"Outfit, resolved by Omuse's native text renderer",
        "scope":"No real photograph, provider session, performance benchmark or external-editor qualification"
    });
    let mut paper = Layer::paint("Warm paper · synthetic raster", WIDTH, HEIGHT);
    paper.image = Some(
        RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
            let grain = ((x * 73 + y * 151 + x * y * 3) % 5) as i16 - 2;
            Rgba([
                (CREAM[0] as i16 + grain) as u8,
                (CREAM[1] as i16 + grain) as u8,
                (CREAM[2] as i16 + grain) as u8,
                255,
            ])
        })
        .into(),
    );
    doc.layers = vec![
        paper,
        text("Masthead", "OMUSE", 52., 35., 38., 300.)?,
        text("Edition", "CREATIVE TOOLS / 01", 664., 45., 16., 280.)?,
        text("Eyebrow", "MADE TO MAKE.", 54., 175., 16., 350.)?,
        text("Headline", "Shape your\nnext idea.", 48., 220., 66., 500.)?,
        text(
            "Introduction",
            "Pixels. Curves. Possibility.\nOne canvas, entirely yours.",
            54.,
            405.,
            22.,
            465.,
        )?,
        text(
            "Footer left",
            "CURVES / REPEAT / LAYERS",
            54.,
            612.,
            15.,
            500.,
        )?,
        text(
            "Footer right",
            "CREATE WITH INTENTION",
            646.,
            612.,
            15.,
            300.,
        )?,
    ];
    Ok(doc)
}

fn make_artwork(cancel: &AtomicBool) -> Result<(VectorScene, VectorScene, Value)> {
    let outer = VectorObject::ellipse("Crescent", 584., 175., 292., 330., Some(ORANGE), None)?;
    let cutter = VectorObject::ellipse("Cutaway", 680., 143., 222., 330., Some(INK), None)?;
    let input = scene(WIDTH, HEIGHT, vec![outer, cutter]);
    let ids = input
        .objects
        .iter()
        .map(|o| o.id.clone())
        .collect::<Vec<_>>();
    let cut = vector_commands::apply(&input, &ids, &VectorCommand::Subtract, cancel)?;
    ensure!(
        cut.scene
            .objects
            .iter()
            .flat_map(|o| &o.path.subpaths)
            .flat_map(|s| &s.anchors)
            .any(|a| a.incoming.is_some() || a.outgoing.is_some()),
        "The boolean fixture must retain editable curved handles"
    );
    let ids = cut
        .scene
        .objects
        .iter()
        .map(|o| o.id.clone())
        .collect::<Vec<_>>();
    let simplified = vector_commands::apply(
        &cut.scene,
        &ids,
        &VectorCommand::Simplify { tolerance: 0.2 },
        cancel,
    )?;
    let mut objects = vec![VectorObject::ellipse(
        "Plum sun",
        691.,
        248.,
        171.,
        171.,
        Some(INK),
        None,
    )?];
    objects.extend(simplified.scene.objects);

    let ring = VectorObject::ellipse(
        "Orbit · outlined stroke",
        569.,
        163.,
        337.,
        353.,
        None,
        Some(StrokeStyle {
            color: PEACH,
            width: 2.,
        }),
    )?;
    let ring_id = ring.id.clone();
    let outlined = vector_commands::apply(
        &scene(WIDTH, HEIGHT, vec![ring]),
        &[ring_id],
        &VectorCommand::OutlineStroke,
        cancel,
    )?;
    ensure!(
        outlined.scene.objects.iter().all(|o| o.stroke.is_none()),
        "Stroke was not outlined"
    );
    objects.extend(outlined.scene.objects);

    // A separate open cubic remains editable alongside the outlined ellipse.
    let curve = VectorPath {
        subpaths: vec![Subpath {
            closed: false,
            anchors: vec![
                Anchor {
                    position: Point { x: 585., y: 466. },
                    incoming: None,
                    outgoing: Some(Point { x: 634., y: 526. }),
                },
                Anchor {
                    position: Point { x: 933., y: 334. },
                    incoming: Some(Point { x: 875., y: 540. }),
                    outgoing: None,
                },
            ],
        }],
        ..Default::default()
    };
    objects.push(VectorObject::new(
        "Free orbit · cubic",
        curve,
        None,
        Some(StrokeStyle {
            color: INK,
            width: 2.,
        }),
    ));
    objects.push(VectorObject::rectangle(
        "Top rule",
        54.,
        116.,
        890.,
        1.,
        Some(PEACH),
        None,
    )?);
    objects.push(VectorObject::rectangle(
        "Bottom rule",
        54.,
        591.,
        890.,
        1.,
        Some(PEACH),
        None,
    )?);
    let initial = scene(WIDTH, HEIGHT, objects);
    initial.validate()?;

    let dot = VectorObject::ellipse("Dot motif", 592., 547., 6., 6., Some(INK), None)?;
    let grid_spec = RepeatSpec::Grid {
        columns: 8,
        rows: 2,
        column_step: Point { x: 42., y: 0. },
        row_step: Point { x: 0., y: 17. },
    };
    let grid = vector_repeat::generate(
        &[dot],
        &grid_spec,
        "d4186c6b-8226-4bec-80d4-c9861f20fb64",
        cancel,
    )?;
    let ray = VectorObject::rectangle("Sun ray", 799., 181., 4., 15., Some(ORANGE), None)?;
    let radial_spec = RepeatSpec::Radial {
        count: 12,
        center: Point { x: 801., y: 222. },
        angle_step_degrees: 30.,
        rotate_copies: true,
    };
    let radial = vector_repeat::generate(
        &[ray],
        &radial_spec,
        "9d0d2b0c-5365-4d30-a1eb-7c58059fa2c1",
        cancel,
    )?;
    let mut final_scene = initial.clone();
    final_scene.objects.extend(grid.objects);
    final_scene.objects.extend(radial.objects);
    final_scene.validate()?;
    Ok((
        initial,
        final_scene,
        json!({
            "boolean":"Subtract two opaque cubic ellipses; curved handles retained",
            "booleanAnchorsBefore":cut.anchors_before,"booleanAnchorsAfter":cut.anchors_after,
            "simplifyTolerancePixels":0.2,"simplifyAnchorsBefore":simplified.anchors_before,
            "simplifyAnchorsAfter":simplified.anchors_after,
            "outlinedStrokeAnchors":outlined.anchors_after,
            "grid":grid_spec,"radial":radial_spec,
            "repeatSemantics":"Editable snapshot copies, not a persistent live-repeat effect"
        }),
    ))
}

fn check_saved(
    path: &Path,
    source: &Document,
    vector_id: &str,
    expected: &VectorScene,
) -> Result<RgbaImage> {
    document::save(source, path)?;
    let reopened = document::open(path)?;
    ensure!(
        reopened
            .find_layer(vector_id)
            .and_then(|l| l.vector_scene.as_deref())
            == Some(expected),
        "Saved vector scene changed in {}",
        path.display()
    );
    let before = raster::composite(source);
    let after = raster::composite(&reopened);
    ensure!(
        after == before,
        "Saved composition changed in {}",
        path.display()
    );
    for layer in &source.layers {
        if let Some(style) = objects::live_text(layer)? {
            let restored = reopened
                .find_layer(&layer.id)
                .context("Saved text layer missing")?;
            ensure!(
                objects::live_text(restored)?.as_ref() == Some(&style),
                "Native text changed"
            );
        }
    }
    Ok(after)
}

fn write_builder_fixture(out: &Path, cancel: &AtomicBool) -> Result<Value> {
    let artwork = scene(
        480,
        320,
        vec![
            VectorObject::ellipse("Orange ellipse", 70., 65., 210., 190., Some(ORANGE), None)?,
            VectorObject::ellipse("Plum ellipse", 195., 65., 210., 190., Some(INK), None)?,
        ],
    );
    let mut doc = Document::new(480, 320);
    doc.name = "Omuse — Shape Builder practice".into();
    doc.layers.clear();
    let mut editor = Editor::new(doc);
    let id = editor.insert_vector_scene(
        "Two editable ellipses",
        0,
        artwork.clone(),
        artwork.render(cancel)?,
    )?;
    ensure!(
        editor.undo_depth() == 1,
        "Builder fixture insertion must be one Undo"
    );
    let pixels = check_saved(
        &out.join("shape-builder.omuse"),
        &editor.document,
        &id,
        &artwork,
    )?;
    ensure!(
        pixels.get_pixel(0, 0)[3] == 0,
        "Builder original must remain transparent"
    );
    pixels.save(out.join("shape-builder.png"))?;
    Ok(
        json!({"project":"shape-builder.omuse","preview":"shape-builder.png",
        "dimensions":[480,320],"objects":2,"transparentOriginal":true,
        "manualJourney":"Open the project; edit the vector layer; Select All; start Shape Builder (Alt+M); drag across regions to merge or Alt-drag to erase; Ctrl+Z compares; Done commits one document Undo.",
        "pixelSha256":sha256_hex(pixels.as_raw())}),
    )
}

fn hashes(path: &Path, relative: &Path, result: &mut Vec<Value>) -> Result<()> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = relative.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            hashes(&entry.path(), &name, result)?;
        } else {
            let bytes = fs::read(entry.path())?;
            result.push(json!({"path":name,"bytes":bytes.len(),"sha256":sha256_hex(&bytes)}));
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let out = PathBuf::from(
        args.next()
            .context("Usage: photo_vector_workflow <new-directory>")?,
    );
    ensure!(args.next().is_none(), "Expected one output directory");
    fs::create_dir(&out)
        .context("Output directory must be new; existing evidence is never overwritten")?;
    let cancel = AtomicBool::new(false);
    let (initial, final_scene, operations) = make_artwork(&cancel)?;
    let base = card_document()?;
    let mut editor = Editor::new(base.clone());
    let id = editor.insert_vector_scene(
        "Celestial forms · editable vectors",
        editor.revision(),
        initial.clone(),
        initial.render(&cancel)?,
    )?;
    ensure!(editor.undo_depth() == 1, "Vector insert must be one Undo");
    let inserted = editor.document.clone();
    ensure!(
        editor.undo() && documents_match(&editor.document, &base),
        "Insert Undo lost source"
    );
    ensure!(
        editor.redo() && documents_match(&editor.document, &inserted),
        "Insert Redo changed source"
    );
    ensure!(
        editor.replace_vector_scene(
            &id,
            editor.revision(),
            final_scene.clone(),
            final_scene.render(&cancel)?
        )?,
        "Repeat edit did not change artwork"
    );
    ensure!(
        editor.undo_depth() == 2,
        "Vector replacement must add exactly one Undo"
    );
    let final_document = editor.document.clone();
    ensure!(
        editor.undo() && documents_match(&editor.document, &inserted),
        "Replace Undo lost originals"
    );
    ensure!(
        editor.redo() && documents_match(&editor.document, &final_document),
        "Replace Redo changed artwork"
    );

    let pixels = check_saved(
        &out.join("omuse-card.omuse"),
        &editor.document,
        &id,
        &final_scene,
    )?;
    pixels.save(out.join("omuse-card.png"))?;
    final_scene
        .render(&cancel)?
        .save(out.join("vector-artwork.png"))?;
    vector_svg_scene::prepare_scene_export(&out.join("vector-artwork.svg"), &final_scene)?
        .publish()?
        .finish()?;
    vector_pdf::prepare_scene_export(&out.join("vector-artwork.pdf"), &final_scene, 144.)?
        .publish()?
        .finish()?;
    // SVG/PDF intentionally contain the vector layer only. The complete card
    // and editable native text are in .omuse; PSD carries the converted card.
    let svg_reopened = vector_svg_scene::import_scene(&out.join("vector-artwork.svg"))?;
    svg_reopened.validate()?;
    let svg_pixels = svg_reopened.render(&cancel)?;
    svg_pixels.save(out.join("vector-svg-reopened.png"))?;
    let psd_report = psd_export::export(&editor.document, &out.join("omuse-card.psd"))?;
    let psd_reopened = psd::open(&out.join("omuse-card.psd"))?;
    ensure!(
        psd_reopened.layers.len() == editor.document.layers.len(),
        "PSD layer count changed"
    );
    ensure!(
        raster::composite(&psd_reopened) == pixels,
        "Converted PSD composite changed"
    );
    ensure!(
        documents_match(&editor.document, &final_document) && editor.undo_depth() == 2,
        "Save or exchange altered the source document/history"
    );
    let builder = write_builder_fixture(&out, &cancel)?;
    let mut artifacts = vec![];
    hashes(&out, Path::new(""), &mut artifacts)?;
    let receipt = json!({
        "scope":"Synthetic native composition and exchange journey; not real-photo qualification, live-provider QA, a speed benchmark, Photoshop QA or PDF visual validation",
        "source":"rust/examples/photo_vector_workflow.rs",
        "sourceSha256":sha256_hex(include_bytes!("photo_vector_workflow.rs")),
        "executableSha256":sha256_hex(&fs::read(std::env::current_exe()?)?),
        "dimensions":[WIDTH,HEIGHT],"project":"omuse-card.omuse","preview":"omuse-card.png",
        "vectorLayerId":id,"vectorObjects":final_scene.objects.len(),"operations":operations,
        "checks":{"insertOneUndo":true,"replaceOneUndo":true,"undoRedoExact":true,
            "savedSceneExact":true,"savedCompositeExact":true,"nativeTextRetained":true,
            "psdReopenedCompositeExact":true,"sourceAndHistoryUnchangedByExchange":true,
            "svgReopenedAndRendered":true},
        "cardPixelSha256":sha256_hex(pixels.as_raw()),
        "svgReopenedPixelSha256":sha256_hex(svg_pixels.as_raw()),
        "psd":{"layers":psd_report.layer_count,"warnings":psd_report.warnings},
        "exchangeScope":"SVG/PDF contain vector artwork only; PSD contains converted 8-bit layers of the entire card; .omuse retains native text and original editable scene",
        "manualReview":"Inspect composition, text fit and exports visually; use shape-builder.omuse for native interaction. No visual-approval claim is made merely by running this helper.",
        "shapeBuilder":builder,"artifacts":artifacts
    });
    fs::write(
        out.join("receipt.json"),
        serde_json::to_string_pretty(&receipt)? + "\n",
    )?;
    println!("{}", out.join("receipt.json").display());
    Ok(())
}
