//! Original synthetic mixed-media layout acceptance fixture, not a photo-quality
//! or performance benchmark. Usage: auto_layout_workflow <new-output-directory>.
use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use omuse::{
    asset_library::sha256_hex,
    auto_layout::{self, Align, FrameRule, FrameSize, ItemSize},
    create_history::documents_match,
    document,
    editor::Editor,
    model::{Document, Layer},
    objects::{self, LiveTextStyle, ObjectPoint, ObjectSize},
    raster,
    vector_scene::{VECTOR_SCENE_TEXT_VERSION, VectorObject, VectorScene},
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

const INK: [u8; 4] = [67, 47, 61, 255];
const CREAM: [u8; 4] = [245, 230, 207, 255];
fn text(
    name: &str,
    content: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    size: f32,
) -> Result<Layer> {
    objects::live_text_layer(
        name,
        ObjectPoint { x, y },
        LiveTextStyle {
            content: content.into(),
            font_name: "Outfit".into(),
            font_size: size,
            leading: size * 1.22,
            red: INK[0] as f32 / 255.,
            green: INK[1] as f32 / 255.,
            blue: INK[2] as f32 / 255.,
            box_size: Some(ObjectSize { width, height }),
            ..Default::default()
        },
    )
}
fn roundtrip(doc: &Document, out: &Path, name: &str) -> Result<serde_json::Value> {
    let file = out.join(format!("{name}.omuse"));
    let before = raster::composite(doc);
    document::save(doc, &file)?;
    let reopened = document::open(&file)?;
    auto_layout::validate_settled(&reopened)?;
    ensure!(
        before == raster::composite(&reopened),
        "{name}: reopened composite changed"
    );
    for source in &doc.layers {
        if auto_layout::frame_rule(source)?.is_some() {
            let restored = reopened.find_layer(&source.id).context("Frame missing")?;
            ensure!(
                auto_layout::frame_rule(source)? == auto_layout::frame_rule(restored)?,
                "Frame rules changed"
            );
            for child in &source.children {
                let other = reopened.find_layer(&child.id).context("Member missing")?;
                ensure!(
                    auto_layout::item_rule(child)? == auto_layout::item_rule(other)?,
                    "Item rules changed"
                );
                ensure!(
                    child.image == other.image && child.vector_scene == other.vector_scene,
                    "Editable source changed"
                );
                ensure!(
                    objects::live_text(child)? == objects::live_text(other)?,
                    "Live text changed"
                );
            }
        }
    }
    before.save(out.join(format!("{name}.png")))?;
    let manifest = fs::read(file.join("manifest.json"))?;
    let value: serde_json::Value = serde_json::from_slice(&manifest)?;
    Ok(
        json!({"project":format!("{name}.omuse"),"png":format!("{name}.png"),"format":value["version"],"rgbaSha256":sha256_hex(before.as_raw()),"manifestSha256":sha256_hex(&manifest),"saveReopenExact":true}),
    )
}
fn main() -> Result<()> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Supply a new output directory")?,
    );
    ensure!(!out.exists(), "Output directory must not exist");
    fs::create_dir_all(&out)?;
    let cancel = AtomicBool::new(false);
    let mut doc = Document::new(1000, 700);
    doc.name = "Omuse — room to create".into();
    doc.metadata["omuseEvidence"] = json!({"kind":"synthetic retained-layout acceptance fixture","provenance":"Original procedural raster, vector geometry and copy authored for Omuse; no photographic input","font":"Outfit resolved by native text renderer","scope":"No provider invocation, speed measurement, external-editor compatibility or photographic quality claim"});
    let mut paper = Layer::paint("Warm paper", 1000, 700);
    paper.image = Some(RgbaImage::from_pixel(1000, 700, Rgba(CREAM)).into());
    let mut photo = Layer::paint("Procedural landscape · raster", 220, 260);
    photo.image = Some(
        RgbaImage::from_fn(220, 260, |x, y| {
            let sky = [226., 171., 134.];
            let horizon = [177., 95., 91.];
            let t = y as f32 / 260.;
            let ridge = 175. + 19. * (x as f32 / 44.).sin();
            if (y as f32) > ridge {
                Rgba([88 + (x % 9) as u8, 68, 78, 255])
            } else if (x as f32 - 154.).hypot(y as f32 - 69.) < 33. {
                Rgba([249, 213, 151, 255])
            } else {
                Rgba([
                    (sky[0] * (1. - t) + horizon[0] * t) as u8,
                    (sky[1] * (1. - t) + horizon[1] * t) as u8,
                    (sky[2] * (1. - t) + horizon[2] * t) as u8,
                    255,
                ])
            }
        })
        .into(),
    );
    photo.offset_x = 80.;
    photo.offset_y = 234.;
    let copy = text(
        "Live copy",
        "Room to\ncreate.\n\nPixels, type\nand curves.\nOne canvas.",
        80.,
        234.,
        220.,
        260.,
        25.,
    )?;
    let scene = VectorScene {
        // This acceptance journey intentionally proves layout15 -> static14
        // compatibility. Its classic ellipses need no clipping/quality features;
        // pin the prior text-path scene generation so header normalization and
        // unrelated newer-format requirements cannot obscure source equality.
        version: VECTOR_SCENE_TEXT_VERSION,
        width: 220,
        height: 260,
        objects: vec![
            VectorObject::ellipse(
                "Warm ellipse",
                28.,
                31.,
                164.,
                190.,
                Some([206, 100, 68, 255]),
                None,
            )?,
            VectorObject::ellipse("Cream cutaway", 87., 17., 133., 193., Some(CREAM), None)?,
            VectorObject::ellipse("Plum companion", 43., 99., 100., 100., Some(INK), None)?,
        ],
    };
    let mut vector = Layer::paint("Editable vector motif", 220, 260);
    vector.image = Some(scene.render(&cancel)?.into());
    vector.vector_scene = Some(Arc::new(scene));
    vector.offset_x = 80.;
    vector.offset_y = 234.;
    let ids = vec![photo.id.clone(), copy.id.clone(), vector.id.clone()];
    doc.layers = vec![
        paper,
        photo,
        copy,
        vector,
        text(
            "Eyebrow",
            "OMUSE / LIVING LAYOUTS",
            64.,
            50.,
            850.,
            40.,
            18.,
        )?,
        text(
            "Headline",
            "Beautifully connected.",
            64.,
            104.,
            875.,
            100.,
            48.,
        )?,
        text(
            "Footer",
            "RASTER + LIVE TYPE + VECTOR     /     RETAINED RULES · ONE UNDO",
            64.,
            614.,
            875.,
            46.,
            16.,
        )?,
    ];
    let original = doc.clone();
    let mut editor = Editor::new(doc);
    let rule = FrameRule {
        width: FrameSize::Fixed(836.),
        padding: [24.; 4],
        gap: 24.,
        align: Align::Center,
        ..Default::default()
    };
    let frame = editor.create_layout_frame(&ids, rule, &cancel)?;
    ensure!(editor.undo_depth() == 1, "Framing must be one Undo");
    let framed = editor.document.clone();
    ensure!(
        editor.undo() && documents_match(&editor.document, &original),
        "Undo did not restore siblings"
    );
    ensure!(
        editor.redo() && documents_match(&editor.document, &framed),
        "Redo did not restore frame"
    );
    let first = roundtrip(&editor.document, &out, "retained-mixed-frame")?;
    // Fill gets exactly the remaining fixed frame width. Source pixels stay intact.
    editor.set_layout_item(&ids[0], ItemSize::Fill, ItemSize::Hug, &cancel)?;
    let fill = editor.document.clone();
    ensure!(editor.undo_depth() == 2, "Member sizing must be one Undo");
    ensure!(
        fill.find_layer(&ids[0]).unwrap().image == framed.find_layer(&ids[0]).unwrap().image,
        "Fill resampled source"
    );
    let second = roundtrip(&fill, &out, "retained-mixed-fill")?;
    ensure!(
        editor.undo() && documents_match(&editor.document, &framed),
        "Fill Undo changed source"
    );
    ensure!(
        editor.redo() && documents_match(&editor.document, &fill),
        "Fill Redo changed source"
    );
    let scaled = omuse::create::resize_layout(
        &editor.document,
        1500,
        1050,
        omuse::create::ResizeStrategy::ScaleToFit,
    )?;
    let third = roundtrip(&scaled, &out, "retained-mixed-scaled")?;
    let before_release = raster::composite(&editor.document);
    editor.remove_layout_frame(&frame, &cancel)?;
    ensure!(
        before_release == raster::composite(&editor.document),
        "Release changed appearance"
    );
    let released = roundtrip(&editor.document, &out, "released-static-layout")?;
    ensure!(
        released["format"].as_u64().unwrap() < 15,
        "Released layout still needs format15"
    );
    fs::write(
        out.join("results.json"),
        serde_json::to_vec_pretty(&json!({
            "kind":"synthetic acceptance journey","status":"passed","fixtures":[first,second,third,released],
            "checks":["one-Undo framing","exact Undo/Redo","retained Fill and source preservation","Create resize scales retained rules","save/reopen exact composite, rules and live sources","release rules keeps composition and older format"],
            "limits":["leaf-only frames","no actual photograph or provider run","no performance claim","old-version refusal must be checked separately with the actual older binary"]
        }))?,
    )?;
    println!("{}", out.join("results.json").display());
    Ok(())
}
