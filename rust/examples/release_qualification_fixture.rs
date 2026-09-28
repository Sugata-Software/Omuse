use anyhow::{Context, Result, bail, ensure};
use image::Rgba;
use omuse::{
    document,
    model::{Document, Layer},
};
use serde_json::json;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn synthetic(
    revision: u64,
    width: u32,
    height: u32,
    layers: usize,
    advanced: bool,
) -> Result<Document> {
    let mut doc = Document::new(width, height);
    doc.metadata["qualificationRevision"] = json!(revision);
    doc.metadata["qualificationAdvanced"] = json!(advanced);
    while doc.layers.len() < layers {
        doc.layers.push(Layer::paint(
            format!("Synthetic {}", doc.layers.len()),
            width,
            height,
        ));
    }
    for (layer_index, layer) in doc.layers.iter_mut().enumerate() {
        layer.name = format!("Revision {revision} layer {layer_index}");
        let image = layer.image.as_mut().expect("synthetic paint layer");
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let value = (u64::from(x) * 73_856_093
                ^ u64::from(y) * 19_349_663
                ^ revision * 83_492_791
                ^ layer_index as u64 * 2_654_435_761) as u32;
            *pixel = Rgba([
                value as u8,
                value.rotate_left(9) as u8,
                value.rotate_left(19) as u8,
                96 + (value % 160) as u8,
            ]);
        }
        if advanced {
            use omuse::{
                advanced::LayerState,
                advanced_ops::{AdvancedOperation, FilterNode},
                filters::Filter,
                precision::{TiledImage16, WorkingSpace},
            };
            let mut state = LayerState::from_image(image, &layer.name)?;
            let exact = image::ImageBuffer::from_fn(width, height, |x, y| {
                let pixel = image.get_pixel(x, y);
                Rgba(std::array::from_fn(|channel| {
                    (u16::from(pixel[channel]) << 8)
                        | ((x + y + revision as u32 + channel as u32 * 7) & 255) as u16
                }))
            });
            state.source = Arc::new(TiledImage16::from_rgba16_in(&exact, WorkingSpace::Srgb)?);
            state.result = state.source.clone();
            state.recipe.source_id = uuid::Uuid::from_u128(layer_index as u128 + 1).to_string();
            state.recipe.nodes.push(FilterNode {
                id: "qualification-exposure".into(),
                name: "Exposure".into(),
                enabled: true,
                opacity: 1.,
                soft_mask: None,
                operation: AdvancedOperation::Filter(Filter::Exposure { stops: 0.25 }),
            });
            state = state.evaluate(&AtomicBool::new(false))?;
            layer.image = Some(state.proxy()?.into());
            layer.advanced = Some(Arc::new(state));
        }
    }
    Ok(doc)
}

fn fingerprint(doc: &Document) -> u64 {
    fn visit(layers: &[Layer], hash: &mut u64) {
        for layer in layers {
            for byte in layer.name.as_bytes() {
                *hash = (*hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
            }
            if let Some(image) = &layer.image {
                for byte in image.as_raw() {
                    *hash = (*hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
                }
            }
            if let Some(state) = &layer.advanced {
                // Include exact 16-bit masters and recipes, not just byte proxies.
                for image in [&state.source, &state.result] {
                    for word in image.to_rgba16().as_raw() {
                        for byte in word.to_le_bytes() {
                            *hash = (*hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211);
                        }
                    }
                }
                for byte in serde_json::to_vec(&state.recipe).expect("validated recipe") {
                    *hash = (*hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211);
                }
            }
            visit(&layer.children, hash);
        }
    }
    let mut hash = 14_695_981_039_346_656_037;
    visit(&doc.layers, &mut hash);
    hash
}

fn marker(root: &Path, value: serde_json::Value) -> Result<()> {
    let temporary = root.join("phase.tmp");
    fs::write(&temporary, serde_json::to_vec(&value)?)?;
    FileSync::sync(&temporary)?;
    fs::rename(temporary, root.join("phase.json"))?;
    Ok(())
}

struct FileSync;
impl FileSync {
    fn sync(path: &Path) -> Result<()> {
        fs::File::open(path)?.sync_all()?;
        Ok(())
    }
}

fn save_and_verify(doc: &Document, path: &Path) -> Result<u64> {
    document::save(doc, path).with_context(|| format!("save {}", path.display()))?;
    let reopened = document::open(path).with_context(|| format!("reopen {}", path.display()))?;
    ensure!(
        reopened.width == doc.width && reopened.height == doc.height,
        "dimension mismatch"
    );
    ensure!(
        reopened.metadata["qualificationRevision"] == doc.metadata["qualificationRevision"],
        "revision mismatch"
    );
    let actual = fingerprint(&reopened);
    ensure!(actual == fingerprint(doc), "pixel fingerprint mismatch");
    Ok(actual)
}

fn run(root: &Path, revisions: u64, width: u32, height: u32, layers: usize) -> Result<()> {
    ensure!(
        (1..=1000).contains(&revisions) && (1..=64).contains(&layers),
        "invalid qualification revision/layer bounds"
    );
    ensure!(
        width > 0
            && height > 0
            && u64::from(width) * u64::from(height) * layers as u64 <= 16_000_000,
        "qualification exceeds 16 million total pixels"
    );
    fs::create_dir_all(root)?;
    let project = root.join("Sustained.comp");
    let recovery = root.join("recovery/session-11111111-1111-4111-8111-111111111111.comp");
    fs::create_dir_all(recovery.parent().unwrap())?;
    let mut journal = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("journal.jsonl"))?;
    let advanced = omuse::identity::env_var("OMUSE_QUALIFICATION_ADVANCED").as_deref() == Ok("1");
    for revision in 0..revisions {
        let doc = synthetic(revision, width, height, layers, advanced)?;
        marker(root, json!({"phase":"recovery-saving","revision":revision}))?;
        let recovery_hash = save_and_verify(&doc, &recovery)?;
        marker(root, json!({"phase":"project-saving","revision":revision}))?;
        let project_hash = save_and_verify(&doc, &project)?;
        writeln!(
            journal,
            "{}",
            json!({"revision":revision,"projectFingerprint":project_hash,"recoveryFingerprint":recovery_hash})
        )?;
        journal.sync_data()?;
        marker(root, json!({"phase":"committed","revision":revision}))?;
    }
    Ok(())
}

fn verify(path: &Path) -> Result<()> {
    let doc = document::open(path).with_context(|| format!("open {}", path.display()))?;
    let revision = doc.metadata["qualificationRevision"]
        .as_u64()
        .context("missing qualification revision")?;
    let advanced = doc.metadata["qualificationAdvanced"]
        .as_bool()
        .unwrap_or(false);
    let expected = synthetic(revision, doc.width, doc.height, doc.layers.len(), advanced)?;
    ensure!(
        fingerprint(&doc) == fingerprint(&expected),
        "stored revision does not match all synthetic pixels"
    );
    println!(
        "{}",
        json!({"path":path,"revision":revision,"width":doc.width,"height":doc.height,"layers":doc.layers.len(),"advancedLayers":doc.layers.iter().filter(|layer|layer.advanced.is_some()).count(),"fingerprint":fingerprint(&doc)})
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [command, path] if command == "verify" => verify(Path::new(path)),
        [command, root, revisions, width, height, layers] if command == "run" => run(
            Path::new(root),
            revisions.to_string_lossy().parse()?,
            width.to_string_lossy().parse()?,
            height.to_string_lossy().parse()?,
            layers.to_string_lossy().parse()?,
        ),
        _ => bail!(
            "usage: release_qualification_fixture run ROOT REVISIONS WIDTH HEIGHT LAYERS | verify PROJECT.comp"
        ),
    }
}
