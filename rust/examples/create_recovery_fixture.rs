//! Bounded persistence and interrupted-save qualification for `.omuse` Create
//! collections. The Python checker launches this executable directly after a
//! release build; it never invokes Cargo itself.
//!
//! ```text
//! create_recovery_fixture run ROOT REVISIONS
//! create_recovery_fixture verify PACKAGE.omuse
//! ```

use anyhow::{Context, Result, bail, ensure};
use image::{ImageFormat, Rgba, RgbaImage};
use omuse::{
    create,
    create_project::Project,
    model::{Document, Layer},
    objects::{self, LiveTextStyle, ObjectPoint, ObjectSize, TextAlignment},
    raster,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    path::Path,
    thread,
    time::Duration,
};

const PAGE_COUNT: usize = 6;
// Keep the recovery workload bounded, but use a real layout size so the
// branded fields can honour the same readable minimum as customer documents.
const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const MAX_REVISIONS: u64 = 32;

fn digest_bytes(bytes: &[u8]) -> u64 {
    bytes.iter().fold(14_695_981_039_346_656_037, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211)
    })
}

fn update_digest(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
    }
}

fn page_source(index: usize) -> Layer {
    let mut source = Layer::paint(format!("Original source {index}"), WIDTH, HEIGHT);
    let image = RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let seed = x
            .wrapping_mul(31)
            .wrapping_add(y.wrapping_mul(17))
            .wrapping_add((index as u32).wrapping_mul(47));
        Rgba([
            (seed & 0xff) as u8,
            (seed.rotate_left(7) & 0xff) as u8,
            (seed.rotate_left(13) & 0xff) as u8,
            112 + (seed % 120) as u8,
        ])
    });
    source.image = Some(image.into());
    source.metadata = json!({"qualificationSource": index});
    source
}

fn native_text(
    name: &str,
    field: &str,
    content: String,
    position: ObjectPoint,
    size: f32,
    role: &str,
) -> Result<Layer> {
    let mut layer = objects::live_text_layer(
        name,
        position,
        LiveTextStyle {
            content,
            font_name: "sans-serif".into(),
            font_size: size,
            red: 0.12,
            green: 0.12,
            blue: 0.14,
            alignment: TextAlignment::Left,
            tracking: 0.0,
            leading: size * 1.16,
            box_size: Some(ObjectSize {
                width: WIDTH as f32 - 48.0,
                height: 100.0,
            }),
            runs: vec![],
        },
    )?;
    layer.metadata["omuseCreate"] = json!({
        "field": field,
        "colorRole": "ink",
        "textRole": role,
    });
    Ok(layer)
}

fn text_values(revision: u64, page: usize) -> [(String, String); 3] {
    [
        (
            "page_label".into(),
            format!("CREATE RECOVERY · PAGE {:02}", page + 1),
        ),
        (
            "headline".into(),
            format!("Revision {revision} keeps native text"),
        ),
        (
            "body".into(),
            format!(
                "Page {} retains its original source pixels and packaged brand asset.",
                page + 1
            ),
        ),
    ]
}

fn make_page(index: usize, revision: u64) -> Result<Document> {
    let mut document = Document::new(WIDTH, HEIGHT);
    document.name = format!("Page {:02}", index + 1);
    document.layers.clear();
    document.layers.push(page_source(index));
    let values = text_values(revision, index);
    document.layers.push(native_text(
        "Page label",
        &values[0].0,
        values[0].1.clone(),
        ObjectPoint { x: 24.0, y: 24.0 },
        28.0,
        "label",
    )?);
    document.layers.push(native_text(
        "Headline",
        &values[1].0,
        values[1].1.clone(),
        ObjectPoint { x: 24.0, y: 132.0 },
        52.0,
        "heading",
    )?);
    document.layers.push(native_text(
        "Body",
        &values[2].0,
        values[2].1.clone(),
        ObjectPoint { x: 24.0, y: 248.0 },
        32.0,
        "body",
    )?);
    document.metadata["createRecoveryRevision"] = json!(revision);
    document.metadata["createRecoveryPage"] = json!(index);
    Ok(document)
}

fn packaged_logo_bytes() -> Result<Vec<u8>> {
    let logo = RgbaImage::from_fn(20, 12, |x, y| {
        let highlight = (x + y * 3) % 7 == 0;
        Rgba(if highlight {
            [242, 179, 61, 255]
        } else {
            [43, 22, 32, 255]
        })
    });
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(logo).write_to(&mut bytes, ImageFormat::Png)?;
    Ok(bytes.into_inner())
}

fn project_for_revision(revision: u64) -> Result<Project> {
    let first = make_page(0, revision)?;
    let mut project = Project::new("Create recovery qualification", first);
    project.title = format!("Create recovery qualification revision {revision}");
    let first_page_id = project.active_page_id().to_owned();
    let resource_id =
        project.add_resource("Qualification logo", "image/png", packaged_logo_bytes()?)?;

    let mut brand = create::sugata_brand_kit();
    brand.name = "Sugata recovery qualification".into();
    brand.logo_resource_ids.push(resource_id.clone());
    let brand_id = project.add_brand(brand)?;
    for index in 1..PAGE_COUNT {
        let page_id = project.add_page(
            format!("Page {:02}", index + 1),
            make_page(index, revision)?,
        )?;
        project.set_page_template(&page_id, Some("native-recovery"))?;
    }
    project.set_page_template(&first_page_id, Some("native-recovery"))?;
    create::apply_brand_to_project(&mut project, &brand_id)?;

    let source = project
        .active_document()?
        .layers
        .first()
        .cloned()
        .context("Qualification source layer disappeared")?;
    create::set_shared_background(&mut project, vec![source])?;
    create::insert_brand_logo(&mut project, &first_page_id, &brand_id, &resource_id)?;
    project.validate()?;
    Ok(project)
}

fn find_field_mut<'a>(layers: &'a mut [Layer], field: &str) -> Option<&'a mut Layer> {
    for layer in layers {
        if layer
            .metadata
            .pointer("/omuseCreate/field")
            .and_then(serde_json::Value::as_str)
            == Some(field)
            && objects::live_text(layer).ok().flatten().is_some()
        {
            return Some(layer);
        }
        if let Some(found) = find_field_mut(&mut layer.children, field) {
            return Some(found);
        }
    }
    None
}

fn set_field_text(document: &mut Document, field: &str, content: String) -> Result<()> {
    let layer = find_field_mut(&mut document.layers, field)
        .with_context(|| format!("Missing native '{field}' field"))?;
    let mut style = objects::live_text(layer)?.context("Qualification field is not native text")?;
    objects::set_text_content(&mut style, content);
    objects::set_live_text(layer, style)?;
    Ok(())
}

fn update_revision(project: &mut Project, revision: u64) -> Result<()> {
    project.title = format!("Create recovery qualification revision {revision}");
    for (index, page_id) in project.page_ids().iter().enumerate() {
        let document = project.page_document_mut(page_id)?;
        for (field, value) in text_values(revision, index) {
            set_field_text(document, &field, value)?;
        }
        document.metadata["createRecoveryRevision"] = json!(revision);
    }
    project.validate()
}

fn collect_fields(layers: &[Layer], fields: &mut BTreeMap<String, String>) -> Result<()> {
    for layer in layers {
        if let Some(field) = layer
            .metadata
            .pointer("/omuseCreate/field")
            .and_then(serde_json::Value::as_str)
            && let Some(style) = objects::live_text(layer)?
        {
            ensure!(
                fields.insert(field.to_owned(), style.content).is_none(),
                "Duplicate qualification text field '{field}'"
            );
        }
        collect_fields(&layer.children, fields)?;
    }
    Ok(())
}

fn source_digest(layers: &[Layer], hash: &mut u64, count: &mut usize) {
    for layer in layers {
        if layer.metadata.get("qualificationSource").is_some() {
            *count += 1;
            if let Some(image) = &layer.image {
                update_digest(hash, image.as_raw());
            }
            if let Some(mask) = &layer.mask {
                update_digest(hash, mask.as_raw());
            }
        }
        source_digest(&layer.children, hash, count);
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceEvidence {
    name: String,
    media_type: String,
    byte_len: u64,
    byte_digest: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageEvidence {
    name: String,
    revision: u64,
    native_fields: BTreeMap<String, String>,
    visual_digest: u64,
    source_count: usize,
    source_digest: u64,
    shared_backgrounds: usize,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectEvidence {
    revision: u64,
    title: String,
    page_count: usize,
    component_count: usize,
    brand_count: usize,
    active_brand_name: String,
    has_shared_background: bool,
    resources: Vec<ResourceEvidence>,
    pages: Vec<PageEvidence>,
}

fn project_evidence(project: &mut Project) -> Result<ProjectEvidence> {
    project.validate()?;
    ensure!(project.page_ids().len() == PAGE_COUNT, "Expected six pages");
    ensure!(project.brand_kits.len() == 1, "Expected one brand kit");
    ensure!(
        project.component_summaries().len() == 2,
        "Expected shared and logo components"
    );
    let shared_id = project
        .metadata
        .shared_background_component_id
        .as_deref()
        .context("Missing shared background component")?;
    ensure!(
        project
            .component_summaries()
            .iter()
            .any(|component| component.id == shared_id),
        "Shared background component record is missing"
    );
    let (active_brand_name, logo_resource_ids) = {
        let active_brand = project.active_brand().context("Missing active brand")?;
        (
            active_brand.name.clone(),
            active_brand.logo_resource_ids.clone(),
        )
    };
    let resource_summaries = project.resource_summaries();
    ensure!(
        resource_summaries.len() == 1,
        "Expected one packaged resource"
    );
    let resource_id = resource_summaries[0].id.clone();
    ensure!(
        logo_resource_ids == vec![resource_id],
        "Brand no longer references the packaged resource"
    );
    let resources = resource_summaries
        .iter()
        .map(|summary| {
            let bytes = project.resource_bytes(&summary.id)?;
            Ok(ResourceEvidence {
                name: summary.name.clone(),
                media_type: summary.media_type.clone(),
                byte_len: summary.byte_len,
                byte_digest: digest_bytes(bytes),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let mut pages = Vec::with_capacity(PAGE_COUNT);
    project.for_each_page_document(|summary, document| {
        let revision = document
            .metadata
            .get("createRecoveryRevision")
            .and_then(serde_json::Value::as_u64)
            .context("Page has no qualification revision")?;
        let mut fields = BTreeMap::new();
        collect_fields(&document.layers, &mut fields)?;
        ensure!(fields.len() == 3, "Expected three native text fields");
        let mut original_hash = 14_695_981_039_346_656_037;
        let mut source_count = 0;
        source_digest(&document.layers, &mut original_hash, &mut source_count);
        let shared_backgrounds = document
            .layers
            .iter()
            .filter(|layer| {
                layer
                    .metadata
                    .pointer("/omuseCreate/sharedBackground")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                    && layer.locked
            })
            .count();
        ensure!(
            shared_backgrounds == 1,
            "Page has no protected shared background"
        );
        pages.push(PageEvidence {
            name: summary.name.clone(),
            revision,
            native_fields: fields,
            visual_digest: digest_bytes(raster::composite(document).as_raw()),
            source_count,
            source_digest: original_hash,
            shared_backgrounds,
        });
        Ok(())
    })?;
    let revision = pages
        .first()
        .map(|page| page.revision)
        .context("Qualification has no pages")?;
    ensure!(
        pages.iter().all(|page| page.revision == revision),
        "Page revisions disagree"
    );
    Ok(ProjectEvidence {
        revision,
        title: project.title.clone(),
        page_count: pages.len(),
        component_count: project.component_summaries().len(),
        brand_count: project.brand_kits.len(),
        active_brand_name,
        has_shared_background: true,
        resources,
        pages,
    })
}

fn verify_package(path: &Path) -> Result<ProjectEvidence> {
    let mut actual = Project::open(path).with_context(|| format!("Open {}", path.display()))?;
    let actual_evidence = project_evidence(&mut actual)?;
    ensure!(
        actual_evidence.revision < MAX_REVISIONS,
        "Saved qualification revision exceeds the bounded replay"
    );
    // Match the real editing journey: apply the brand once, then edit the
    // content while retaining its fitted typography. Creating a fresh branded
    // project at each revision fits that revision's different glyph widths
    // again, which is a different document even when the saved one is exact.
    let mut expected = project_for_revision(0)?;
    for revision in 0..=actual_evidence.revision {
        update_revision(&mut expected, revision)?;
    }
    let expected_evidence = project_evidence(&mut expected)?;
    ensure!(
        actual_evidence == expected_evidence,
        "Saved package differs from the native qualification snapshot for revision {}: actual={:?}, expected={:?}",
        actual_evidence.revision,
        actual_evidence,
        expected_evidence
    );
    Ok(actual_evidence)
}

fn write_marker(root: &Path, value: serde_json::Value) -> Result<()> {
    let temporary = root.join("phase.json.tmp");
    fs::write(&temporary, serde_json::to_vec(&value)?)?;
    File::open(&temporary)?.sync_all()?;
    fs::rename(temporary, root.join("phase.json"))?;
    File::open(root)?.sync_all()?;
    Ok(())
}

fn pause_revision() -> Result<Option<u64>> {
    env::var("OMUSE_CREATE_RECOVERY_PAUSE_REVISION")
        .ok()
        .map(|value| value.parse().context("Invalid pause revision"))
        .transpose()
}

fn save_destination(
    project: &mut Project,
    root: &Path,
    destination: &Path,
    revision: u64,
) -> Result<()> {
    let pause = pause_revision()?;
    project.save_checked(destination, || {
        write_marker(
            root,
            json!({"phase":"destination-staged", "revision":revision}),
        )?;
        if pause == Some(revision) {
            loop {
                thread::sleep(Duration::from_millis(100));
            }
        }
        Ok(())
    })
}

fn run(root: &Path, revisions: u64) -> Result<()> {
    ensure!(
        (2..=MAX_REVISIONS).contains(&revisions),
        "Use 2–{MAX_REVISIONS} revisions"
    );
    ensure!(!root.exists(), "Use a fresh qualification directory");
    fs::create_dir_all(root)?;
    let destination = root.join("collection.omuse");
    let recovery = root.join("recovery.omuse");
    let mut journal = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join("journal.jsonl"))?;
    let mut project = project_for_revision(0)?;
    for revision in 0..revisions {
        update_revision(&mut project, revision)?;
        let before_save = project_evidence(&mut project)?;
        write_marker(
            root,
            json!({"phase":"recovery-saving", "revision":revision}),
        )?;
        project.save(&recovery)?;
        let recovery_evidence = verify_package(&recovery)?;
        ensure!(
            recovery_evidence == before_save,
            "Recovery changed the actual pre-save native snapshot"
        );
        ensure!(
            recovery_evidence.revision == revision,
            "Recovery revision changed"
        );

        write_marker(
            root,
            json!({"phase":"destination-saving", "revision":revision}),
        )?;
        save_destination(&mut project, root, &destination, revision)?;
        let destination_evidence = verify_package(&destination)?;
        ensure!(
            destination_evidence == before_save,
            "Destination changed the actual pre-save native snapshot"
        );
        ensure!(
            destination_evidence.revision == revision,
            "Destination revision changed"
        );
        writeln!(
            journal,
            "{}",
            json!({
                "revision": revision,
                "destinationVisualDigests": destination_evidence.pages.iter().map(|page| page.visual_digest).collect::<Vec<_>>(),
                "recoveryVisualDigests": recovery_evidence.pages.iter().map(|page| page.visual_digest).collect::<Vec<_>>(),
                "resourceDigest": destination_evidence.resources[0].byte_digest,
            })
        )?;
        journal.sync_data()?;
        write_marker(root, json!({"phase":"committed", "revision":revision}))?;
    }
    println!(
        "{}",
        serde_json::to_string(&json!({"root":root,"revisions":revisions,"pageCount":PAGE_COUNT}))?
    );
    Ok(())
}

fn main() -> Result<()> {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    match arguments.as_slice() {
        [command, package] if command == "verify" => {
            let evidence = verify_package(Path::new(package))?;
            println!("{}", serde_json::to_string(&evidence)?);
            Ok(())
        }
        [command, root, revisions] if command == "run" => {
            run(Path::new(root), revisions.to_string_lossy().parse()?)
        }
        _ => bail!("usage: create_recovery_fixture run ROOT REVISIONS | verify PACKAGE.omuse"),
    }
}
