//! Bounded, metadata-preserving .omuse document package I/O.
//!
//! The compatible document manifest also opens legacy .comp packages.
//!
//! Saving stages a complete sibling package, fsyncs it, then atomically exchanges
//! it with the old directory on Linux. Errors before exchange leave the old file intact.
use crate::{
    model::{Document, Layer, MAX_DIMENSION, MAX_LAYERS, MAX_PIXELS, valid_dimensions},
    objects,
};
use anyhow::{Context, Result, ensure};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

const MAX_MANIFEST: u64 = 4 * 1024 * 1024;
const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
/// Current single-canvas format. Earlier formats remain readable.
// Explicit mask outside coverage cannot be represented losslessly by v9's
// inferred border rule. Old readers must reject rather than change artwork.
pub const PROJECT_WRITE_VERSION: u64 = 10;
pub const PROJECT_SCENE_VERSION: u64 = 11;
pub const PROJECT_MAX_READ_VERSION: u64 = PROJECT_SCENE_VERSION;
const VECTOR_SCENE_FORMAT_VERSION: u32 = 1;
const MAX_VECTOR_SCENE_COMPRESSED: u64 = 32 * 1024 * 1024;
const MAX_VECTOR_SCENE_JSON: u64 = 64 * 1024 * 1024;
const MAX_VECTOR_SCENE_DOCUMENT_BYTES: usize = 256 * 1024 * 1024;
const MAX_VECTOR_SCENE_DOCUMENT_JSON: u64 = 256 * 1024 * 1024;
// Imports become RGBA8 before they enter the ordinary document model. Keep the
// decoder's own allocation at that model boundary rather than relying on a
// codec-specific default for a high-bit-depth or malformed source.
const MAX_DECODE_BYTES: u64 = MAX_PIXELS * 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VectorSceneAsset {
    version: u32,
    asset: String,
    cache_width: u32,
    cache_height: u32,
    cache_sha256: String,
    scene_sha256: String,
}

fn vector_scene_asset(
    id: &str,
    scene: &crate::vector_scene::VectorScene,
    image: &crate::shared_image::SharedImage,
    scene_json: &[u8],
) -> VectorSceneAsset {
    VectorSceneAsset {
        version: VECTOR_SCENE_FORMAT_VERSION,
        asset: format!("{id}.vector-scene.json.z"),
        cache_width: scene.width,
        cache_height: scene.height,
        cache_sha256: crate::asset_library::sha256_hex(image.as_raw()),
        scene_sha256: crate::asset_library::sha256_hex(scene_json),
    }
}

fn valid_sha256(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn serialize_vector_scene(scene: &crate::vector_scene::VectorScene) -> Result<Vec<u8>> {
    scene.validate()?;
    let json = serde_json::to_vec(scene)?;
    ensure!(
        json.len() as u64 <= MAX_VECTOR_SCENE_JSON,
        "Vector scene geometry exceeds sidecar size limit"
    );
    Ok(json)
}

fn open_vector_scene_sidecar(path: &Path, root: &Path) -> Result<File> {
    regular_file(path, Some(root), MAX_VECTOR_SCENE_COMPRESSED)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Reject a final-component symlink and ensure a substituted FIFO cannot
        // block the project loader. O_CLOEXEC also keeps the asset private.
        options.custom_flags(0x20000 | 0x800 | 0x80000);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= MAX_VECTOR_SCENE_COMPRESSED,
        "Vector scene sidecar is not a bounded regular file"
    );
    Ok(file)
}

fn load_vector_scene(
    root: &Path,
    id: &str,
    value: &Value,
    image: Option<&RgbaImage>,
    remaining_expanded_bytes: u64,
) -> Result<(std::sync::Arc<crate::vector_scene::VectorScene>, u64)> {
    let asset: VectorSceneAsset =
        serde_json::from_value(value.clone()).context("Invalid vector scene asset descriptor")?;
    ensure!(
        asset.version == VECTOR_SCENE_FORMAT_VERSION
            && asset.asset == format!("{id}.vector-scene.json.z"),
        "Invalid vector scene asset version or name"
    );
    ensure!(
        valid_sha256(&asset.cache_sha256) && valid_sha256(&asset.scene_sha256),
        "Invalid vector scene digest"
    );
    let image = image.context("Vector scene requires cached layer pixels")?;
    ensure!(
        image.dimensions() == (asset.cache_width, asset.cache_height)
            && crate::asset_library::sha256_hex(image.as_raw()) == asset.cache_sha256,
        "Vector scene cache dimensions or digest differ from its descriptor"
    );
    let sidecar = root.join("images").join(&asset.asset);
    let mut compressed = Vec::new();
    open_vector_scene_sidecar(&sidecar, root)?
        .take(MAX_VECTOR_SCENE_COMPRESSED + 1)
        .read_to_end(&mut compressed)?;
    ensure!(
        compressed.len() as u64 <= MAX_VECTOR_SCENE_COMPRESSED,
        "Vector scene sidecar exceeds compressed size limit"
    );
    let expanded_limit = MAX_VECTOR_SCENE_JSON.min(remaining_expanded_bytes);
    let mut json = Vec::new();
    let mut decoder = flate2::read::ZlibDecoder::new(&compressed[..]);
    (&mut decoder)
        .take(expanded_limit + 1)
        .read_to_end(&mut json)?;
    ensure!(
        json.len() as u64 <= expanded_limit,
        "Vector scene sidecars exceed expanded document size limit"
    );
    ensure!(
        decoder.total_in() == compressed.len() as u64,
        "Vector scene sidecar has trailing compressed data"
    );
    ensure!(
        crate::asset_library::sha256_hex(&json) == asset.scene_sha256,
        "Vector scene geometry digest differs from its descriptor"
    );
    let scene: crate::vector_scene::VectorScene =
        serde_json::from_slice(&json).context("Invalid vector scene geometry")?;
    scene.validate()?;
    ensure!(
        (scene.width, scene.height) == image.dimensions(),
        "Vector scene geometry dimensions differ from cached pixels"
    );
    let expanded_bytes = json.len() as u64;
    Ok((std::sync::Arc::new(scene), expanded_bytes))
}

pub(crate) fn validate_vector_scene_budget(layers: &[Layer]) -> Result<()> {
    fn retained(layers: &[Layer], total: &mut usize) -> Result<()> {
        for layer in layers {
            if let Some(scene) = &layer.vector_scene {
                *total = total.saturating_add(scene.retained_bytes());
                ensure!(
                    *total <= MAX_VECTOR_SCENE_DOCUMENT_BYTES,
                    "Vector scenes exceed document memory limit"
                );
            }
            retained(&layer.children, total)?;
        }
        Ok(())
    }
    let mut total = 0usize;
    retained(layers, &mut total)
}

/// Whether a path has the native Omuse extension, regardless of ASCII case.
pub fn is_omuse_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.as_encoded_bytes().eq_ignore_ascii_case(b"omuse"))
}

/// Normalize a user-selected project save destination without touching disk.
/// Legacy suffixes are replaced; other suffixes remain part of the chosen name
/// so saving an imported `photo.png` produces `photo.png.omuse`.
///
/// Low-level package writers deliberately accept exact caller paths, including
/// legacy packages. Save dialogs and other user-facing callers use this helper.
pub fn project_save_path(path: &Path) -> Result<PathBuf> {
    let final_component = path
        .as_os_str()
        .as_encoded_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default();
    ensure!(
        !final_component.is_empty() && final_component != b"." && final_component != b"..",
        "Choose a project filename"
    );
    let filename = path.file_name().context("Choose a project filename")?;
    ensure!(
        !filename.to_string_lossy().trim().is_empty(),
        "Choose a nonblank project filename"
    );
    if is_omuse_path(path) {
        return Ok(path.to_owned());
    }
    if path.extension().is_none_or(|extension| {
        extension.is_empty() || extension.as_encoded_bytes().eq_ignore_ascii_case(b"comp")
    }) {
        return Ok(path.with_extension("omuse"));
    }
    let mut filename = filename.to_owned();
    filename.push(".omuse");
    Ok(path.with_file_name(filename))
}

fn canonical_id(id: &str) -> Result<String> {
    Ok(uuid::Uuid::parse_str(id)
        .context("Invalid UUID")?
        .to_string()
        .to_uppercase())
}

fn valid_blend(mode: &str) -> bool {
    matches!(
        mode,
        "Normal"
            | "Darken"
            | "Multiply"
            | "Color Burn"
            | "Linear Burn"
            | "Lighten"
            | "Screen"
            | "Color Dodge"
            | "Linear Dodge (Add)"
            | "Overlay"
            | "Soft Light"
            | "Hard Light"
            | "Vivid Light"
            | "Linear Light"
            | "Pin Light"
            | "Hard Mix"
            | "Difference"
            | "Exclusion"
            | "Subtract"
            | "Divide"
            | "Hue"
            | "Saturation"
            | "Color"
            | "Luminosity"
    )
}

fn validate_guides(manifest: &Value, version: u64) -> Result<()> {
    if let Some(resolution) = manifest.get("resolution").filter(|v| !v.is_null()) {
        ensure!(
            resolution
                .as_f64()
                .is_some_and(|n| (1.0..=9600.0).contains(&n)),
            "Invalid resolution"
        );
    }
    if let Some(guides) = manifest.get("guides").filter(|v| !v.is_null()) {
        let guides = guides.as_array().context("Invalid guides")?;
        ensure!(
            guides.len() <= 1_000 && (version >= 8 || guides.is_empty()),
            "Invalid guide count or version"
        );
        let mut ids = HashSet::new();
        for guide in guides {
            ensure!(
                ids.insert(canonical_id(string(guide, "id")?)?),
                "Duplicate guide ID"
            );
            ensure!(
                matches!(string(guide, "axis")?, "horizontal" | "vertical"),
                "Invalid guide axis"
            );
            ensure!(
                guide["position"]
                    .as_f64()
                    .is_some_and(|p| p.is_finite() && p.abs() <= 1_000_000.),
                "Invalid guide position"
            );
        }
    }
    Ok(())
}

fn object(value: &Value) -> Result<Map<String, Value>> {
    value.as_object().cloned().context("Expected a JSON object")
}
fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing or invalid {field}"))
}
fn number(value: &Value, field: &str, default: f32) -> Result<f32> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let n = v.as_f64().context("Invalid numeric value")? as f32;
            ensure!(n.is_finite(), "Non-finite {field}");
            Ok(n)
        }
    }
}
fn flag(value: &Value, field: &str, default: bool) -> Result<bool> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_bool()
            .with_context(|| format!("Invalid boolean {field}")),
    }
}
fn pair(value: &Value, field: &str) -> Result<(f32, f32)> {
    let values = value
        .get(field)
        .and_then(Value::as_array)
        .context("Invalid transform vector")?;
    ensure!(values.len() == 2, "Invalid transform vector length");
    let x = values[0].as_f64().context("Invalid transform coordinate")? as f32;
    let y = values[1].as_f64().context("Invalid transform coordinate")? as f32;
    ensure!(x.is_finite() && y.is_finite(), "Non-finite transform");
    Ok((x, y))
}

/// Preserve a scaled transform's precision until the scale has been derived.
///
/// Project transforms store a visible size rather than an explicit scale. A
/// `f32` multiply while saving followed by a `f32` divide while loading can
/// move the recovered scale by one ULP, which is enough to change a
/// high-quality resample. Origins and rotations remain `f32` model fields;
/// only this size-to-scale recovery needs the additional precision.
fn pair64(value: &Value, field: &str) -> Result<(f64, f64)> {
    let values = value
        .get(field)
        .and_then(Value::as_array)
        .context("Invalid transform vector")?;
    ensure!(values.len() == 2, "Invalid transform vector length");
    let x = values[0].as_f64().context("Invalid transform coordinate")?;
    let y = values[1].as_f64().context("Invalid transform coordinate")?;
    ensure!(x.is_finite() && y.is_finite(), "Non-finite transform");
    Ok((x, y))
}
fn regular_file(path: &Path, root: Option<&Path>, limit: u64) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("Cannot read {}", path.display()))?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= limit,
        "Not a regular file or exceeds size limit: {}",
        path.display()
    );
    if let Some(root) = root {
        ensure!(
            path.canonicalize()?.starts_with(root.canonicalize()?),
            "Asset escapes project directory"
        );
    }
    Ok(())
}
fn reject_animated_png(path: &Path) -> Result<()> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let mut signature = [0; 8];
    file.read_exact(&mut signature)?;
    ensure!(
        signature == *b"\x89PNG\r\n\x1a\n",
        "Project asset is not a PNG"
    );
    loop {
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as u64;
        ensure!(
            &header[4..] != b"acTL",
            "Animated PNG project assets are not supported"
        );
        let position = file.stream_position()?;
        ensure!(
            size + 4 <= length.saturating_sub(position),
            "Truncated PNG chunk"
        );
        if &header[4..] == b"IDAT" || &header[4..] == b"IEND" {
            return Ok(());
        }
        file.seek(SeekFrom::Current((size + 4) as i64))?;
    }
}

struct Decoded {
    image: RgbaImage,
    exact: Option<crate::precision::Rgba16Image>,
    profile: Option<crate::color_management::SourceProfile>,
}
fn decode(path: &Path, png_only: bool, used: &mut u64) -> Result<Decoded> {
    regular_file(path, None, MAX_FILE)?;
    if png_only {
        reject_animated_png(path)?;
    }
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let format = reader.format();
    if png_only {
        ensure!(
            reader.format() == Some(ImageFormat::Png),
            "Project assets must be PNG images"
        );
    }
    let mut decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    ensure!(
        valid_dimensions(width, height),
        "Image exceeds dimension or pixel limits"
    );
    let pixels = u64::from(width) * u64::from(height);
    ensure!(
        pixels <= MAX_PIXELS.saturating_sub(*used),
        "Project exceeds total image pixel limit"
    );
    if png_only {
        ensure!(
            matches!(
                decoder.color_type(),
                image::ColorType::L8
                    | image::ColorType::La8
                    | image::ColorType::Rgb8
                    | image::ColorType::Rgba8
            ),
            "Unsupported project image bit depth"
        );
    }
    let high_bit_depth = matches!(
        decoder.color_type(),
        image::ColorType::L16
            | image::ColorType::La16
            | image::ColorType::Rgb16
            | image::ColorType::Rgba16
    );
    if !png_only && high_bit_depth {
        ensure!(
            pixels <= crate::advanced::MAX_ADVANCED_PIXELS,
            "16-bit image exceeds the 16 megapixel editable-source limit"
        );
    }
    let orientation = decoder.orientation()?;
    let icc = if png_only {
        None
    } else if format == Some(ImageFormat::Tiff) {
        crate::color_management::tiff_icc_profile_from_path(path)?
    } else {
        decoder.icc_profile()?
    };
    let decoded = DynamicImage::from_decoder(decoder)?;
    let (image, exact, profile) = if !png_only && high_bit_depth {
        let decoded = decoded.into_rgba16();
        let converted = if icc.is_some() {
            crate::color_management::to_srgb16(&decoded, icc.as_deref())?
        } else {
            decoded
        };
        let profile = icc
            .as_deref()
            .map(crate::color_management::source_profile_metadata)
            .transpose()?;
        let mut oriented = DynamicImage::ImageRgba16(converted);
        oriented.apply_orientation(orientation);
        let exact = oriented.into_rgba16();
        let image = RgbaImage::from_fn(exact.width(), exact.height(), |x, y| {
            image::Rgba(
                exact
                    .get_pixel(x, y)
                    .0
                    .map(|value| ((u32::from(value) * 255 + 32_767) / 65_535) as u8),
            )
        });
        (image, Some(exact), profile)
    } else {
        let decoded = decoded.to_rgba8();
        let converted = if png_only {
            crate::color_management::ConvertedImage {
                image: decoded,
                source_profile: None,
            }
        } else {
            crate::color_management::to_srgb(&decoded, icc.as_deref())?
        };
        let mut oriented = DynamicImage::ImageRgba8(converted.image);
        if !png_only {
            oriented.apply_orientation(orientation);
        }
        (oriented.to_rgba8(), None, converted.source_profile)
    };
    *used += pixels;
    Ok(Decoded {
        image,
        exact,
        profile,
    })
}

pub fn import_image(path: &Path) -> Result<Layer> {
    if crate::psd::matches(path) {
        let doc = crate::psd::open(path)?;
        let mut group = Layer::group(&doc.name);
        group.children = doc.layers;
        return Ok(group);
    }
    if crate::svg_import::matches(path) {
        return crate::svg_import::import(path);
    }
    if crate::raw_import::matches(path) {
        let state = crate::smart_source::import(
            path,
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
        )?;
        let image = state.proxy()?;
        let mut layer = Layer::paint(
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("RAW image"),
            image.width(),
            image.height(),
        );
        layer.image = Some(image.into());
        layer.advanced = Some(std::sync::Arc::new(state));
        return Ok(layer);
    }
    let decoded = decode(path, false, &mut 0)?;
    let image = decoded.image;
    let source_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported image");
    let mut layer = Layer::group(
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported image"),
    );
    layer.metadata = json!({});
    if let Some(profile) = decoded.profile {
        let digest = profile
            .icc
            .iter()
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
        layer.metadata["sourceColorProfile"] = json!({"description":profile.description,"iccBytes":profile.icc.len(),"fnv1a64":format!("{digest:016x}")});
    }
    layer.image = Some(image.into());
    if let Some(exact) = decoded.exact {
        layer.advanced = Some(std::sync::Arc::new(
            crate::advanced::LayerState::from_rgba16(&exact, source_name)?,
        ));
    }
    Ok(layer)
}

pub fn open(path: &Path) -> Result<Document> {
    if crate::psd::matches(path) {
        return crate::psd::open(path);
    }
    if !path.is_dir() {
        let layer = import_image(path)?;
        let image = layer.image.as_ref().unwrap();
        return Ok(Document {
            width: image.width(),
            height: image.height(),
            name: layer.name.clone(),
            background: [0; 4],
            layers: vec![layer],
            metadata: json!({}),
        });
    }
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "Project directory must not be a symbolic link"
    );
    let manifest_path = path.join("manifest.json");
    regular_file(&manifest_path, Some(path), MAX_MANIFEST)?;
    let mut bytes = Vec::new();
    File::open(&manifest_path)?
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_MANIFEST, "Manifest too large");
    let manifest: Value =
        serde_json::from_slice(&bytes).context("Invalid project manifest JSON")?;
    ensure!(
        string(&manifest, "format")? == "com.compositor.project",
        "Not an Omuse-compatible project"
    );
    let version = manifest["version"]
        .as_u64()
        .context("Invalid project version")?;
    ensure!(
        (1..=PROJECT_MAX_READ_VERSION).contains(&version),
        "Unsupported project version {version}"
    );
    ensure!(
        string(&manifest, "colorSpace")? == "sRGB",
        "Unsupported project color space"
    );
    uuid::Uuid::parse_str(string(&manifest, "documentID")?).context("Invalid document ID")?;
    if let Some(resolution) = manifest.get("resolution").filter(|v| !v.is_null()) {
        let resolution = resolution.as_f64().context("Invalid resolution")?;
        ensure!((1.0..=9600.0).contains(&resolution), "Invalid resolution");
    }
    validate_guides(&manifest, version)?;
    let width = u32::try_from(manifest["width"].as_u64().context("Invalid canvas width")?)?;
    let height = u32::try_from(
        manifest["height"]
            .as_u64()
            .context("Invalid canvas height")?,
    )?;
    ensure!(
        valid_dimensions(width, height),
        "Canvas exceeds dimension or pixel limits"
    );
    let records = manifest["layers"]
        .as_array()
        .context("Invalid layer list")?;
    ensure!(records.len() <= MAX_LAYERS, "Too many layers");
    let mut nodes = HashMap::new();
    let mut parents: HashMap<String, Option<String>> = HashMap::new();
    let mut order = Vec::new();
    let (mut used, mut mask_used) = (0, 0);
    let mut advanced_used = 0usize;
    let mut vector_scene_count = 0usize;
    let mut vector_scene_used = 0usize;
    let mut vector_scene_expanded = 0u64;
    for record in records {
        let id = canonical_id(string(record, "id")?)?;
        ensure!(!nodes.contains_key(&id), "Duplicate layer ID");
        let name = string(record, "name")?;
        ensure!(
            !name.trim().is_empty() && name.len() <= 16_384,
            "Invalid layer name"
        );
        let is_group = flag(record, "isGroup", false)?;
        let opacity = number(record, "opacity", 1.)?;
        ensure!((0. ..=1.).contains(&opacity), "Invalid layer opacity");
        let blend = record
            .get("blendMode")
            .filter(|v| !v.is_null())
            .map(|v| v.as_str().context("Invalid blend mode"))
            .transpose()?
            .unwrap_or("Normal");
        ensure!(valid_blend(blend), "Unsupported layer blend mode: {blend}");
        ensure!(
            version >= 3 || (opacity == 1. && blend == "Normal"),
            "Layer appearance requires project version 3+"
        );
        ensure!(
            !is_group || (blend == "Normal" && (version >= 8 || opacity == 1.)),
            "Invalid group appearance"
        );
        let transform = record.get("transform").context("Missing layer transform")?;
        if let Some(sampling) = transform.get("sampling") {
            ensure!(
                matches!(
                    sampling.as_str(),
                    Some("Nearest" | "Smooth" | "High quality")
                ),
                "Unsupported image sampling"
            );
        }
        let (offset_x, offset_y) = pair(transform, "origin")?;
        let (size_x, size_y) = pair64(transform, "size")?;
        ensure!(
            offset_x.abs() <= 1_000_000.
                && offset_y.abs() <= 1_000_000.
                && (1. ..=300_000.).contains(&size_x)
                && (1. ..=300_000.).contains(&size_y),
            "Invalid layer transform bounds"
        );
        let rotation = number(transform, "rotation", 0.)?;
        let load_asset = |field: &str, suffix: &str, used: &mut u64| -> Result<Option<RgbaImage>> {
            let Some(filename) = record.get(field).filter(|v| !v.is_null()) else {
                return Ok(None);
            };
            let filename = filename.as_str().context("Invalid image filename")?;
            ensure!(
                filename == format!("{id}{suffix}"),
                "Invalid or unsafe project asset name"
            );
            let asset_path = path.join("images").join(filename);
            regular_file(&asset_path, Some(path), MAX_FILE)?;
            Ok(Some(decode(&asset_path, true, used)?.image))
        };
        let image = load_asset("imageFile", ".png", &mut used)?;
        let mask = load_asset("maskFile", ".mask.png", &mut mask_used)?;
        let vector_scene = if let Some(asset) = record
            .get("rustVectorScene")
            .filter(|value| !value.is_null())
        {
            ensure!(
                version == PROJECT_SCENE_VERSION,
                "Vector scenes require project format 11"
            );
            ensure!(
                !is_group
                    && record.get("rustEditableAsset").is_none_or(Value::is_null)
                    && record.get("text").is_none_or(Value::is_null)
                    && record.get("shape").is_none_or(Value::is_null),
                "Vector scene must be the layer's only editable source"
            );
            let (scene, expanded_bytes) = load_vector_scene(
                path,
                &id,
                asset,
                image.as_ref(),
                MAX_VECTOR_SCENE_DOCUMENT_JSON.saturating_sub(vector_scene_expanded),
            )?;
            vector_scene_expanded = vector_scene_expanded.saturating_add(expanded_bytes);
            vector_scene_used = vector_scene_used.saturating_add(scene.retained_bytes());
            ensure!(
                vector_scene_used <= MAX_VECTOR_SCENE_DOCUMENT_BYTES,
                "Vector scenes exceed document memory limit"
            );
            vector_scene_count += 1;
            Some(scene)
        } else {
            None
        };
        ensure!(
            mask.is_none() || version >= if is_group { 6 } else { 4 },
            "Layer mask requires a newer project version"
        );
        for field in ["maskEnabled", "maskLinked"] {
            if record.get(field).is_some_and(|v| !v.is_null()) {
                flag(record, field, true)?;
                ensure!(mask.is_some(), "Mask settings without mask pixels");
            }
        }
        ensure!(
            !is_group || image.is_none(),
            "Group cannot have image pixels"
        );
        if let Some(mask) = &mask {
            ensure!(
                mask.pixels()
                    .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255),
                "Mask must contain opaque grayscale coverage"
            );
        }
        let (pixel_width, pixel_height) = image
            .as_ref()
            .map(|i| (f64::from(i.width()), f64::from(i.height())))
            .unwrap_or((size_x, size_y));
        let scale_x = (size_x / pixel_width) as f32
            * if flag(transform, "flipX", false)? {
                -1.
            } else {
                1.
            };
        let scale_y = (size_y / pixel_height) as f32
            * if flag(transform, "flipY", false)? {
                -1.
            } else {
                1.
            };
        let parent = record
            .get("parentID")
            .filter(|v| !v.is_null())
            .map(|v| canonical_id(v.as_str().context("Invalid parent ID")?))
            .transpose()?;
        ensure!(
            version > 1 || (!is_group && parent.is_none()),
            "Groups require project version 2+"
        );
        parents.insert(id.clone(), parent);
        order.push(id.clone());
        nodes.insert(
            id.clone(),
            Layer {
                id: id.clone(),
                name: name.into(),
                visible: flag(record, "isVisible", true)?,
                locked: flag(record, "locked", false)?,
                opacity,
                blend_mode: blend.into(),
                offset_x,
                offset_y,
                rotation,
                scale_x,
                scale_y,
                image: image.map(Into::into),
                mask: mask.map(Into::into),
                advanced: if let Some(asset) =
                    record.get("rustEditableAsset").filter(|v| !v.is_null())
                {
                    ensure!(
                        asset.as_str() == Some(format!("{id}.editable.json.z").as_str()),
                        "Invalid editable asset name"
                    );
                    Some(std::sync::Arc::new({
                        let state = crate::advanced::LayerState::load_assets_bounded(
                            &path.join("images"),
                            &id,
                            crate::advanced::MAX_DOCUMENT_BYTES.saturating_sub(advanced_used),
                        )?;
                        advanced_used = advanced_used.saturating_add(state.retained_bytes());
                        state
                    }))
                } else {
                    None
                },
                vector_scene,
                children: vec![],
                metadata: {
                    let mut normalized = record.clone();
                    if let Some(object) = normalized.as_object_mut() {
                        object.remove("rustVectorScene");
                    }
                    crate::project_text::normalize_record(&mut normalized, version)?;
                    ensure!(
                        version >= 10 || normalized.get("maskOutsideCoverage").is_none(),
                        "Explicit mask outside coverage requires project format 10"
                    );
                    normalized
                },
            },
        );
    }
    ensure!(
        version != PROJECT_SCENE_VERSION || vector_scene_count > 0,
        "Project format 11 requires at least one vector scene"
    );
    for id in &order {
        let mut visited = HashSet::from([id.as_str()]);
        let mut parent = parents[id].as_deref();
        while let Some(next) = parent {
            ensure!(
                visited.insert(next) && visited.len() <= MAX_DEPTH,
                "Cyclic or excessively deep layer hierarchy"
            );
            ensure!(
                nodes.get(next).is_some_and(Layer::is_group),
                "Missing or non-group parent"
            );
            parent = parents[next].as_deref();
        }
    }
    if let Some(active) = manifest.get("activeLayerID").filter(|v| !v.is_null()) {
        ensure!(
            nodes.contains_key(&canonical_id(
                active.as_str().context("Invalid active layer ID")?
            )?),
            "Missing active layer"
        );
    }
    let mut children: HashMap<Option<String>, Vec<String>> = HashMap::new();
    for id in order {
        children
            .entry(parents.remove(&id).unwrap())
            .or_default()
            .push(id);
    }
    fn assemble(
        parent: Option<String>,
        children: &mut HashMap<Option<String>, Vec<String>>,
        nodes: &mut HashMap<String, Layer>,
    ) -> Vec<Layer> {
        children
            .remove(&parent)
            .unwrap_or_default()
            .into_iter()
            .map(|id| {
                let mut node = nodes.remove(&id).unwrap();
                node.children = assemble(Some(id), children, nodes);
                node
            })
            .collect()
    }
    let mut background = [0; 4];
    if let Some(values) = manifest
        .get("compositorRustBackground")
        .and_then(Value::as_array)
    {
        ensure!(values.len() == 4, "Invalid background color");
        for (out, value) in background.iter_mut().zip(values) {
            *out = u8::try_from(value.as_u64().context("Invalid background color")?)?;
        }
    }
    let document = Document {
        width,
        height,
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .into(),
        background,
        layers: assemble(None, &mut children, &mut nodes),
        metadata: manifest,
    };
    validate_live_objects(&document.layers)?;
    let render_errors = crate::raster::validate(&document);
    ensure!(
        render_errors.is_empty(),
        "Project cannot be rendered safely: {}",
        render_errors.join(", ")
    );
    let unsupported = unsupported_features(&document);
    ensure!(
        unsupported.is_empty(),
        "This project needs features not yet supported by the Rust editor: {}. Open it in the preserved Compositor version; the file has not been changed.",
        unsupported.join(", ")
    );
    Ok(document)
}

fn validate_live_objects(layers: &[Layer]) -> Result<()> {
    fn collect<'a>(layers: &'a [Layer], all: &mut HashMap<&'a str, &'a Layer>) -> Result<()> {
        for layer in layers {
            if let Some(state) = &layer.advanced {
                state
                    .validate()
                    .with_context(|| format!("Invalid editable source on layer {}", layer.name))?;
                ensure!(
                    layer
                        .image
                        .as_ref()
                        .is_some_and(|image| image.dimensions() == state.result.dimensions()),
                    "Editable cache dimensions differ on {}",
                    layer.name
                );
                ensure!(
                    !layer.is_group(),
                    "Group cannot own an editable pixel source"
                );
            }
            if let Some(scene) = &layer.vector_scene {
                scene
                    .validate()
                    .with_context(|| format!("Invalid vector scene on layer {}", layer.name))?;
                ensure!(
                    !layer.is_group()
                        && layer.advanced.is_none()
                        && layer.image.as_ref().is_some_and(|image| {
                            image.dimensions() == (scene.width, scene.height)
                        })
                        && layer.metadata.get("text").is_none_or(Value::is_null)
                        && layer.metadata.get("shape").is_none_or(Value::is_null),
                    "Vector scene requires exclusive matching cached pixels on {}",
                    layer.name
                );
            }
            objects::validate_live_object(layer)
                .with_context(|| format!("Invalid live object on layer {}", layer.name))?;
            crate::effects::validate_layer_metadata(&layer.metadata)
                .with_context(|| format!("Invalid effect or adjustment on layer {}", layer.name))?;
            crate::effects::validate_mask_metadata(&layer.metadata)
                .with_context(|| format!("Invalid mask metadata on layer {}", layer.name))?;
            all.insert(layer.id.as_str(), layer);
            collect(&layer.children, all)?;
        }
        Ok(())
    }
    let mut all = HashMap::new();
    collect(layers, &mut all)?;
    for layer in all.values() {
        let mut path = HashSet::new();
        let mut current = Some(*layer);
        while let Some(node) = current {
            ensure!(
                path.len() < 256 && path.insert(node.id.as_str()),
                "Cyclic or excessively deep mask source graph"
            );
            let Some(raw) = node.metadata.get("maskSourceID").filter(|v| !v.is_null()) else {
                break;
            };
            ensure!(!node.is_group(), "Groups cannot use a mask source");
            let source_id = canonical_id(raw.as_str().context("Invalid mask source ID")?)?;
            let source = all
                .get(source_id.as_str())
                .copied()
                .context("Missing mask source layer")?;
            ensure!(
                !source.is_group() && source.metadata.get("adjustment").is_none_or(Value::is_null),
                "Mask source must be a pixel layer"
            );
            current = Some(source);
        }
    }
    Ok(())
}

/// Features retained on load but whose editing/saving needs additional native support.
/// Callers should display these before presenting a source project as fully editable.
pub fn unsupported_features(doc: &Document) -> Vec<String> {
    let mut result = vec![];
    let mut pending = vec![(&doc.layers, 0)];
    while let Some((layers, depth)) = pending.pop() {
        if depth >= MAX_DEPTH {
            result.push("Excessively deep layer hierarchy".into());
            continue;
        }
        for layer in layers {
            if !layer.children.is_empty() {
                pending.push((&layer.children, depth + 1));
            }
        }
    }
    result
}

fn flatten<'a>(
    layers: &'a [Layer],
    parent: Option<&'a str>,
    depth: usize,
    result: &mut Vec<(&'a Layer, Option<&'a str>)>,
) -> Result<()> {
    ensure!(depth < MAX_DEPTH, "Layer hierarchy too deep");
    for layer in layers {
        ensure!(result.len() < MAX_LAYERS, "Too many layers");
        result.push((layer, parent));
        if !layer.children.is_empty() {
            flatten(&layer.children, Some(&layer.id), depth + 1, result)?;
        }
    }
    Ok(())
}

pub fn save(doc: &Document, path: &Path) -> Result<()> {
    save_checked(doc, path, || Ok(()))
}

/// Save a complete package atomically after verifying a caller-owned condition.
///
/// `before_publish` runs only after every staged asset, manifest, and directory
/// has been synced, while the destination package remains untouched.  A caller
/// that has taken a cooperative save lock can therefore reject a stale snapshot
/// at the final safe point without replacing a newer package on disk.  On an
/// error, the staging package is removed and the existing destination remains
/// intact.
pub fn save_checked<F>(doc: &Document, path: &Path, before_publish: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    ensure!(
        valid_dimensions(doc.width, doc.height),
        "Canvas exceeds dimension or pixel limits"
    );
    validate_live_objects(&doc.layers)?;
    crate::advanced::validate_document_budget(doc)?;
    let render_errors = crate::raster::validate(doc);
    ensure!(
        render_errors.is_empty(),
        "Document cannot be rendered safely: {}",
        render_errors.join(", ")
    );
    let unsupported = unsupported_features(doc);
    ensure!(
        unsupported.is_empty(),
        "Saving would require unsupported feature handling: {}. Original project has not been changed.",
        unsupported.join(", ")
    );
    // The document schema stores backgrounds as layers. Require that explicit
    // representation rather than silently discarding the canvas background.
    ensure!(
        doc.background[3] == 0,
        "Convert the document background to a layer before saving an .omuse project"
    );
    let mut flat = vec![];
    flatten(&doc.layers, None, 0, &mut flat)?;
    let mut ids = HashSet::new();
    let mut records = vec![];
    let has_vector_scene = flat.iter().any(|(layer, _)| layer.vector_scene.is_some());
    let write_version = if has_vector_scene {
        PROJECT_SCENE_VERSION
    } else {
        PROJECT_WRITE_VERSION
    };
    let (mut used, mut mask_used) = (0u64, 0u64);
    let mut vector_scene_used = 0usize;
    let mut vector_scene_json_used = 0u64;
    for (layer, parent) in &flat {
        let layer_id = canonical_id(&layer.id)?;
        ensure!(ids.insert(layer_id.clone()), "Duplicate layer ID");
        ensure!(
            valid_blend(&layer.blend_mode),
            "Unsupported layer blend mode"
        );
        ensure!(
            !layer.is_group() || layer.blend_mode == "Normal",
            "Group blend mode must remain Normal"
        );
        ensure!(
            !layer.name.trim().is_empty() && layer.name.len() <= 16_384,
            "Invalid layer name"
        );
        ensure!(
            layer.opacity.is_finite() && (0. ..=1.).contains(&layer.opacity),
            "Invalid layer opacity"
        );
        ensure!(
            [
                layer.offset_x,
                layer.offset_y,
                layer.rotation,
                layer.scale_x,
                layer.scale_y
            ]
            .iter()
            .all(|n| n.is_finite()),
            "Non-finite layer transform"
        );
        ensure!(
            layer.offset_x.abs() <= 1_000_000. && layer.offset_y.abs() <= 1_000_000.,
            "Invalid layer origin"
        );
        ensure!(
            !layer.is_group() || layer.image.is_none(),
            "Groups cannot contain pixels"
        );
        for (image, total) in [(&layer.image, &mut used), (&layer.mask, &mut mask_used)] {
            if let Some(image) = image {
                let count = u64::from(image.width()) * u64::from(image.height());
                ensure!(
                    valid_dimensions(image.width(), image.height())
                        && count <= MAX_PIXELS.saturating_sub(*total),
                    "Project exceeds image pixel limits"
                );
                *total += count;
            }
        }
        if let Some(mask) = &layer.mask {
            ensure!(
                mask.pixels()
                    .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255),
                "Masks must be opaque grayscale images"
            );
        }
        let mut record = if layer.metadata.is_null() {
            Map::new()
        } else {
            object(&layer.metadata)?
        };
        record.insert("id".into(), json!(layer_id));
        record.insert("name".into(), json!(layer.name));
        record.insert("isVisible".into(), json!(layer.visible));
        record.insert("locked".into(), json!(layer.locked));
        record.insert("opacity".into(), json!(layer.opacity));
        record.insert("blendMode".into(), json!(layer.blend_mode));
        record.insert(
            "parentID".into(),
            json!(parent.map(canonical_id).transpose()?),
        );
        record.insert("isGroup".into(), json!(layer.is_group()));
        if layer.advanced.is_some() {
            record.insert(
                "rustEditableAsset".into(),
                json!(format!("{layer_id}.editable.json.z")),
            );
        } else {
            record.remove("rustEditableAsset");
        }
        if let Some(scene) = &layer.vector_scene {
            let scene_json = serialize_vector_scene(scene)?;
            vector_scene_json_used = vector_scene_json_used.saturating_add(scene_json.len() as u64);
            ensure!(
                vector_scene_json_used <= MAX_VECTOR_SCENE_DOCUMENT_JSON,
                "Vector scene sidecars exceed expanded document size limit"
            );
            vector_scene_used = vector_scene_used.saturating_add(scene.retained_bytes());
            ensure!(
                vector_scene_used <= MAX_VECTOR_SCENE_DOCUMENT_BYTES,
                "Vector scenes exceed document memory limit"
            );
            let image = layer
                .image
                .as_ref()
                .context("Vector scene requires cached layer pixels")?;
            record.insert(
                "rustVectorScene".into(),
                serde_json::to_value(vector_scene_asset(&layer_id, scene, image, &scene_json))?,
            );
        } else {
            record.remove("rustVectorScene");
        }
        record.insert(
            "imageFile".into(),
            json!(layer.image.as_ref().map(|_| format!("{layer_id}.png"))),
        );
        record.insert(
            "maskFile".into(),
            json!(layer.mask.as_ref().map(|_| format!("{layer_id}.mask.png"))),
        );
        if layer.mask.is_none() {
            for key in ["maskEnabled", "maskLinked", "maskPlacement"] {
                record.remove(key);
            }
        }
        let mut transform = record
            .get("transform")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let (width, height) = layer
            .image
            .as_ref()
            .map(|i| (f64::from(i.width()), f64::from(i.height())))
            .unwrap_or_else(|| {
                let original = layer
                    .metadata
                    .get("transform")
                    .and_then(|v| pair(v, "size").ok());
                original
                    .map(|(width, height)| (f64::from(width), f64::from(height)))
                    .unwrap_or((f64::from(doc.width), f64::from(doc.height)))
            });
        let (width, height) = (
            width * f64::from(layer.scale_x.abs()),
            height * f64::from(layer.scale_y.abs()),
        );
        ensure!(
            (1. ..=300_000.).contains(&width) && (1. ..=300_000.).contains(&height),
            "Invalid scaled layer size"
        );
        transform.insert("origin".into(), json!([layer.offset_x, layer.offset_y]));
        transform.insert("size".into(), json!([width, height]));
        transform.insert("rotation".into(), json!(layer.rotation));
        transform.insert("flipX".into(), json!(layer.scale_x < 0.));
        transform.insert("flipY".into(), json!(layer.scale_y < 0.));
        transform.entry("sampling").or_insert(json!("High quality"));
        record.insert("transform".into(), Value::Object(transform));
        records.push(Value::Object(record));
    }
    let mut manifest = if doc.metadata.is_null() {
        Map::new()
    } else {
        object(&doc.metadata)?
    };
    manifest.insert("format".into(), json!("com.compositor.project"));
    manifest.insert("version".into(), json!(write_version));
    manifest.insert("colorSpace".into(), json!("sRGB"));
    manifest.insert("width".into(), json!(doc.width));
    manifest.insert("height".into(), json!(doc.height));
    manifest.insert("layers".into(), Value::Array(records));
    manifest
        .entry("documentID")
        .or_insert_with(|| json!(uuid::Uuid::new_v4().to_string().to_uppercase()));
    uuid::Uuid::parse_str(
        manifest["documentID"]
            .as_str()
            .context("Invalid document ID")?,
    )?;
    let active = manifest
        .get("activeLayerID")
        .and_then(Value::as_str)
        .and_then(|id| canonical_id(id).ok())
        .filter(|id| ids.contains(id))
        .or_else(|| {
            flat.last()
                .and_then(|(layer, _)| canonical_id(&layer.id).ok())
        });
    manifest.insert("activeLayerID".into(), json!(active));
    validate_guides(&Value::Object(manifest.clone()), write_version)?;
    let data = serde_json::to_vec_pretty(&manifest)?;
    ensure!(data.len() as u64 <= MAX_MANIFEST, "Manifest too large");
    write_package(path, &data, &flat, before_publish)
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_package<F>(
    path: &Path,
    manifest: &[u8],
    layers: &[(&Layer, Option<&str>)],
    before_publish: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    ensure!(
        path.components()
            .all(|c| !matches!(c, Component::ParentDir)),
        "Save path must not contain parent traversal"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(parent.is_dir(), "Save directory does not exist");
    let destination = parent
        .canonicalize()?
        .join(path.file_name().context("Missing project filename")?);
    if let Ok(metadata) = fs::symlink_metadata(&destination) {
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Refusing to replace a non-directory or symbolic link"
        );
        // Prevent accidental replacement of an unrelated directory.
        regular_file(
            &destination.join("manifest.json"),
            Some(&destination),
            MAX_MANIFEST,
        )?;
        let existing: Value =
            serde_json::from_reader(File::open(destination.join("manifest.json"))?)?;
        ensure!(
            existing["format"] == "com.compositor.project",
            "Refusing to replace a directory that is not an Omuse-compatible project"
        );
    }
    let stage_path = parent.join(format!(".omuse-stage-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage_path)?;
    let stage = Staging(stage_path);
    fs::create_dir(stage.0.join("images"))?;
    let mut vector_scene_json_used = 0u64;
    for (layer, _) in layers {
        if let Some(advanced) = &layer.advanced {
            advanced.save_assets(&stage.0.join("images"), &canonical_id(&layer.id)?)?;
        }
        if let Some(scene) = &layer.vector_scene {
            let id = canonical_id(&layer.id)?;
            let json = serialize_vector_scene(scene)?;
            vector_scene_json_used = vector_scene_json_used.saturating_add(json.len() as u64);
            ensure!(
                vector_scene_json_used <= MAX_VECTOR_SCENE_DOCUMENT_JSON,
                "Vector scene sidecars exceed expanded document size limit"
            );
            let file = OpenOptions::new().write(true).create_new(true).open(
                stage
                    .0
                    .join("images")
                    .join(format!("{id}.vector-scene.json.z")),
            )?;
            let mut encoder = flate2::write::ZlibEncoder::new(file, flate2::Compression::fast());
            encoder.write_all(&json)?;
            let file = encoder.finish()?;
            ensure!(
                file.metadata()?.len() <= MAX_VECTOR_SCENE_COMPRESSED,
                "Compressed vector scene exceeds sidecar size limit"
            );
            file.sync_all()?;
        }
        if let Some(image) = &layer.image {
            let file = stage
                .0
                .join("images")
                .join(format!("{}.png", canonical_id(&layer.id)?));
            image.save_with_format(&file, ImageFormat::Png)?;
            crate::durable_fs::sync_path(file)?;
        }
        if let Some(mask) = &layer.mask {
            let file = stage
                .0
                .join("images")
                .join(format!("{}.mask.png", canonical_id(&layer.id)?));
            DynamicImage::ImageRgba8(mask.to_image())
                .into_luma8()
                .save_with_format(&file, ImageFormat::Png)?;
            crate::durable_fs::sync_path(file)?;
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.0.join("manifest.json"))?;
    file.write_all(manifest)?;
    file.sync_all()?;
    // Windows cannot move a directory while a file inside it is open.
    drop(file);
    crate::durable_fs::sync_path(stage.0.join("images"))?;
    crate::durable_fs::sync_path(&stage.0)?;
    before_publish().context("Save was rejected before publishing the staged project")?;
    if destination.exists() {
        exchange(&stage.0, &destination)?;
    } else {
        rename_new(&stage.0, &destination)?;
    }
    // A sync failure after successful exchange cannot honestly be reported as "not
    // saved"; the complete new package is already visible. Best-effort durability.
    let _ = crate::durable_fs::sync_path(parent);
    Ok(())
}

#[cfg(target_os = "linux")]
fn rename_flags(from: &Path, to: &Path, flags: u32) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn renameat2(
            olddirfd: i32,
            oldpath: *const std::ffi::c_char,
            newdirfd: i32,
            newpath: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let from = CString::new(from.as_os_str().as_bytes())?;
    let to = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: NUL-terminated owned paths remain alive for the syscall; AT_FDCWD
    // selects the current directory. No pointers are retained by libc.
    let result = unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), flags) };
    if result != 0 {
        return Err(std::io::Error::last_os_error())
            .context("Atomic project replacement failed; original retained");
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn exchange(from: &Path, to: &Path) -> Result<()> {
    rename_flags(from, to, 2)
}
#[cfg(target_os = "linux")]
fn rename_new(from: &Path, to: &Path) -> Result<()> {
    rename_flags(from, to, 1)
}
#[cfg(not(target_os = "linux"))]
fn exchange(from: &Path, to: &Path) -> Result<()> {
    crate::durable_fs::exchange_dirs(from, to)
        .context("Project replacement failed; original retained")
}
#[cfg(not(target_os = "linux"))]
fn rename_new(from: &Path, to: &Path) -> Result<()> {
    crate::durable_fs::rename_no_replace(from, to).context("Publishing the saved project")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use tempfile::tempdir;

    #[test]
    fn expanded_vector_geometry_is_charged_across_sidecars() {
        let directory = tempdir().unwrap();
        let images = directory.path().join("images");
        fs::create_dir(&images).unwrap();
        let scene = crate::vector_scene::VectorScene {
            version: crate::vector_scene::VECTOR_SCENE_VERSION,
            width: 2,
            height: 2,
            objects: vec![
                crate::vector_scene::VectorObject::rectangle(
                    "Square",
                    0.,
                    0.,
                    2.,
                    2.,
                    Some([10, 20, 30, 255]),
                    None,
                )
                .unwrap(),
            ],
        };
        let mut scene_json = serialize_vector_scene(&scene).unwrap();
        scene_json.extend(std::iter::repeat_n(b' ', 4_096));
        let image = RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 255]));

        let descriptor = |id: &str| {
            serde_json::to_value(VectorSceneAsset {
                version: VECTOR_SCENE_FORMAT_VERSION,
                asset: format!("{id}.vector-scene.json.z"),
                cache_width: 2,
                cache_height: 2,
                cache_sha256: crate::asset_library::sha256_hex(image.as_raw()),
                scene_sha256: crate::asset_library::sha256_hex(&scene_json),
            })
            .unwrap()
        };
        for id in ["FIRST", "SECOND"] {
            let file = File::create(images.join(format!("{id}.vector-scene.json.z"))).unwrap();
            let mut encoder = flate2::write::ZlibEncoder::new(file, flate2::Compression::fast());
            encoder.write_all(&scene_json).unwrap();
            encoder.finish().unwrap();
        }

        let project_budget = scene_json.len() as u64 * 2 - 1;
        let (_, first_used) = load_vector_scene(
            directory.path(),
            "FIRST",
            &descriptor("FIRST"),
            Some(&image),
            project_budget,
        )
        .unwrap();
        assert_eq!(first_used, scene_json.len() as u64);
        let error = load_vector_scene(
            directory.path(),
            "SECOND",
            &descriptor("SECOND"),
            Some(&image),
            project_budget - first_used,
        )
        .unwrap_err();
        assert!(error.to_string().contains("expanded document size limit"));

        let second = images.join("SECOND.vector-scene.json.z");
        OpenOptions::new()
            .append(true)
            .open(&second)
            .unwrap()
            .write_all(b"trailing")
            .unwrap();
        let error = load_vector_scene(
            directory.path(),
            "SECOND",
            &descriptor("SECOND"),
            Some(&image),
            project_budget,
        )
        .unwrap_err();
        assert!(error.to_string().contains("trailing compressed data"));
    }

    #[test]
    fn native_extension_detection_is_case_insensitive_and_exact() {
        for name in ["Artwork.omuse", "Artwork.OMUSE", "folder/Artwork.OmUsE"] {
            assert!(is_omuse_path(Path::new(name)), "{name}");
        }
        for name in ["Artwork.comp", "Artwork.omuse.png", "Artwork", ".omuse", ""] {
            assert!(!is_omuse_path(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn user_save_paths_preserve_chosen_names_and_always_use_omuse() {
        for (chosen, expected) in [
            ("Artwork", "Artwork.omuse"),
            ("Artwork.", "Artwork.omuse"),
            ("Artwork.omuse", "Artwork.omuse"),
            ("Artwork.OMUSE", "Artwork.OMUSE"),
            ("Artwork.comp", "Artwork.omuse"),
            ("Artwork.CoMp", "Artwork.omuse"),
            ("photo.png", "photo.png.omuse"),
            ("Campaign.v2", "Campaign.v2.omuse"),
            ("Campaign.v2.comp", "Campaign.v2.omuse"),
            (".private", ".private.omuse"),
            ("/art/Brand kit", "/art/Brand kit.omuse"),
            ("art/Musée.png", "art/Musée.png.omuse"),
        ] {
            let saved = project_save_path(Path::new(chosen)).unwrap();
            assert_eq!(saved, Path::new(expected), "{chosen}");
            assert!(is_omuse_path(&saved));
            assert_eq!(project_save_path(&saved).unwrap(), saved);
        }
    }

    #[test]
    fn user_save_paths_reject_blank_and_directory_targets() {
        for chosen in [
            "", " ", "\t\n", "/", ".", "..", "art/", "art/.", "art/..", "art/   ",
        ] {
            assert!(project_save_path(Path::new(chosen)).is_err(), "{chosen:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn user_save_paths_preserve_non_utf8_filename_bytes() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let chosen = Path::new(OsStr::from_bytes(b"Artwork-\xff.png"));
        let saved = project_save_path(chosen).unwrap();
        assert_eq!(saved.as_os_str().as_bytes(), b"Artwork-\xff.png.omuse");
        let legacy = Path::new(OsStr::from_bytes(b"Artwork-\xff.COMP"));
        assert_eq!(
            project_save_path(legacy).unwrap().as_os_str().as_bytes(),
            b"Artwork-\xff.omuse"
        );
    }

    fn read_manifest(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path.join("manifest.json")).unwrap()).unwrap()
    }
    fn edit_manifest(path: &Path, edit: impl FnOnce(&mut Value)) {
        let mut manifest = read_manifest(path);
        edit(&mut manifest);
        fs::write(
            path.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }
    fn patterned() -> Document {
        let mut doc = Document::new(4, 3);
        doc.layers[0].image = Some(
            RgbaImage::from_fn(4, 3, |x, y| {
                Rgba([
                    x as u8 * 50,
                    y as u8 * 60,
                    87,
                    if x == 2 { 123 } else { 255 },
                ])
            })
            .into(),
        );
        doc
    }

    #[test]
    fn uuid_case_is_canonical_and_group_mask_bounds_are_preserved() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Case.omuse");
        let mut doc = patterned();
        doc.layers[0].id = doc.layers[0].id.to_lowercase();
        let mut group = Layer::group("Masked folder");
        group.id = group.id.to_lowercase();
        group.metadata["transform"] = json!({"origin": [0, 0], "size": [12, 7]});
        group.mask = Some(RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255])).into());
        group.children.push(doc.layers.remove(0));
        doc.layers.push(group);
        save(&doc, &path).unwrap();
        let loaded = open(&path).unwrap();
        assert_eq!(loaded.layers[0].id, doc.layers[0].id.to_uppercase());
        assert_eq!(
            loaded.layers[0].metadata["transform"]["size"],
            json!([12.0, 7.0])
        );
        assert_eq!(
            loaded.layers[0].children[0].image,
            doc.layers[0].children[0].image
        );
        edit_manifest(&path, |v| {
            for layer in v["layers"].as_array_mut().unwrap() {
                layer["id"] = json!(layer["id"].as_str().unwrap().to_lowercase());
                if let Some(parent) = layer["parentID"].as_str() {
                    layer["parentID"] = json!(parent.to_lowercase());
                }
            }
        });
        assert_eq!(open(&path).unwrap().layers[0].children.len(), 1);
    }

    #[test]
    fn malformed_guides_and_blends_are_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Guides.omuse");
        let mut doc = patterned();
        doc.metadata["guides"] = json!([{"id": uuid::Uuid::new_v4().to_string(), "axis": "horizontal", "position": 1e20}]);
        assert!(save(&doc, &path).is_err());
        doc.metadata.as_object_mut().unwrap().remove("guides");
        save(&doc, &path).unwrap();
        edit_manifest(&path, |v| {
            v["layers"][0]["blendMode"] = json!("invented blend")
        });
        assert!(open(&path).unwrap_err().to_string().contains("blend mode"));
    }

    #[test]
    fn legacy_comp_roundtrip_retains_pixels_transform_nested_order_and_unknown_metadata() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Roundtrip.comp");
        let mut doc = patterned();
        doc.metadata["extension-note"] = json!({"author": "safe to preserve", "flags": [1, 2]});
        let mut group = Layer::group("Folder");
        group.opacity = 0.6;
        let mut second = Layer::paint("Transparent detail", 2, 2);
        second.offset_x = -2.5;
        second.offset_y = 1.25;
        second.rotation = 45.;
        second.scale_x = -2.;
        second.scale_y = 0.75;
        second.metadata["plugin-note"] = json!({"untouched": true});
        second.mask = Some(RgbaImage::from_pixel(1, 1, Rgba([127, 127, 127, 255])).into());
        second.metadata["maskEnabled"] = json!(false);
        group.children.push(second.clone());
        doc.layers.push(group);
        save(&doc, &path).unwrap();
        let loaded = open(&path).unwrap();
        assert_eq!(loaded.width, 4);
        assert_eq!(loaded.layers.len(), 2);
        assert_eq!(loaded.layers[0].image, doc.layers[0].image);
        assert_eq!(
            loaded.metadata["extension-note"],
            doc.metadata["extension-note"]
        );
        let detail = &loaded.layers[1].children[0];
        assert_eq!(detail.id, second.id);
        assert_eq!(detail.offset_x, -2.5);
        assert_eq!(detail.scale_x, -2.);
        assert_eq!(detail.scale_y, 0.75);
        assert_eq!(detail.rotation, 45.);
        assert_eq!(detail.mask, second.mask);
        assert_eq!(detail.metadata["maskEnabled"], false);
        assert_eq!(
            detail.metadata["plugin-note"],
            second.metadata["plugin-note"]
        );
        assert_eq!(
            image::open(path.join("images").join(format!("{}.mask.png", detail.id)))
                .unwrap()
                .color(),
            image::ColorType::L8
        );
    }

    #[test]
    fn fractional_high_quality_scale_roundtrips_bits_and_pixels() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Fractional.omuse");
        let mut doc = Document::new(240, 240);
        let layer = &mut doc.layers[0];
        layer.image = Some(
            RgbaImage::from_fn(389, 459, |x, y| {
                let stripe = ((x / 3) ^ (y / 5)) as u8;
                Rgba([
                    x.wrapping_mul(29) as u8,
                    y.wrapping_mul(17) as u8,
                    stripe.wrapping_mul(97),
                    if (x + y) % 7 == 0 { 127 } else { 255 },
                ])
            })
            .into(),
        );
        // 630 / 1350 is the wide-layout scale. For this 389px source, a
        // f32 multiply followed by a f32 divide recovers the next f32 value.
        let scale = 630_f32 / 1350_f32;
        layer.offset_x = 31.25;
        layer.offset_y = 11.5;
        layer.scale_x = -scale;
        layer.scale_y = scale;
        layer.metadata = json!({"transform": {"sampling": "High quality"}});
        let source_scale_bits = (layer.scale_x.to_bits(), layer.scale_y.to_bits());
        let expected = crate::raster::composite(&doc);

        save(&doc, &path).unwrap();
        let loaded = open(&path).unwrap();
        let restored = &loaded.layers[0];
        assert_eq!(restored.scale_x.to_bits(), source_scale_bits.0);
        assert_eq!(restored.scale_y.to_bits(), source_scale_bits.1);
        assert_eq!(crate::raster::composite(&loaded), expected);
    }

    #[test]
    fn atomic_replacement_and_validation_failure_preserve_previous_package() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Atomic.omuse");
        let mut doc = patterned();
        save(&doc, &path).unwrap();
        doc.layers[0].name = "Updated".into();
        save(&doc, &path).unwrap();
        assert_eq!(open(&path).unwrap().layers[0].name, "Updated");
        let before = fs::read(path.join("manifest.json")).unwrap();
        let image_before = fs::read(
            path.join("images")
                .join(format!("{}.png", doc.layers[0].id)),
        )
        .unwrap();
        doc.layers[0].scale_x = f32::NAN;
        assert!(save(&doc, &path).is_err());
        assert_eq!(fs::read(path.join("manifest.json")).unwrap(), before);
        assert_eq!(
            fs::read(
                path.join("images")
                    .join(format!("{}.png", doc.layers[0].id))
            )
            .unwrap(),
            image_before
        );
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no staging debris"
        );
    }

    #[test]
    fn rejected_prepublication_check_preserves_existing_package_and_cleans_stage() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Guarded.omuse");
        let mut original = patterned();
        original.layers[0].name = "Saved work".into();
        save(&original, &path).unwrap();
        let manifest_before = fs::read(path.join("manifest.json")).unwrap();
        let pixels_before = fs::read(
            path.join("images")
                .join(format!("{}.png", original.layers[0].id)),
        )
        .unwrap();

        let mut stale_snapshot = original.clone();
        stale_snapshot.layers[0].name = "Stale background save".into();
        let error = save_checked(&stale_snapshot, &path, || {
            assert_eq!(
                fs::read(path.join("manifest.json")).unwrap(),
                manifest_before
            );
            anyhow::bail!("destination changed while the snapshot was staged")
        })
        .unwrap_err()
        .to_string();

        assert!(error.contains("rejected before publishing"));
        assert_eq!(
            fs::read(path.join("manifest.json")).unwrap(),
            manifest_before
        );
        assert_eq!(
            fs::read(
                path.join("images")
                    .join(format!("{}.png", original.layers[0].id)),
            )
            .unwrap(),
            pixels_before
        );
        assert_eq!(open(&path).unwrap().layers[0].name, "Saved work");
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "rejected saves must not leave a staging package behind"
        );
    }

    #[test]
    fn unsupported_semantics_fail_without_modifying_source() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Effects.omuse");
        let mut doc = patterned();
        save(&doc, &path).unwrap();
        edit_manifest(&path, |v| {
            v["layers"][0]["adjustment"] = json!({"kind": "gaussianBlur", "radius": 10})
        });
        let before = fs::read(path.join("manifest.json")).unwrap();
        let error = open(&path).unwrap_err().to_string();
        assert!(error.contains("adjustment"));
        doc.layers[0].metadata["text"] = json!({"content": "Invalid", "fontSize": 0});
        assert!(
            save(&doc, &path)
                .unwrap_err()
                .to_string()
                .contains("live object")
        );
        assert_eq!(fs::read(path.join("manifest.json")).unwrap(), before);
    }

    #[test]
    fn rejects_duplicate_cycle_missing_parent_and_oversized_documents() {
        for mutation in 0..5 {
            let dir = tempdir().unwrap();
            let path = dir.path().join("Malformed.omuse");
            let mut doc = patterned();
            doc.layers.push(Layer::group("Folder"));
            save(&doc, &path).unwrap();
            edit_manifest(&path, |v| match mutation {
                0 => v["layers"][1]["id"] = v["layers"][0]["id"].clone(),
                1 => v["layers"][1]["parentID"] = v["layers"][1]["id"].clone(),
                2 => v["layers"][0]["parentID"] = json!(uuid::Uuid::new_v4().to_string()),
                3 => v["width"] = json!(u32::MAX),
                _ => v["layers"][0]["opacity"] = json!(2),
            });
            assert!(open(&path).is_err(), "accepted malformed case {mutation}");
        }
    }

    #[test]
    fn rejects_traversal_asset_names_and_symlink_escapes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Traversal.omuse");
        let doc = patterned();
        save(&doc, &path).unwrap();
        edit_manifest(&path, |v| {
            v["layers"][0]["imageFile"] = json!("../../outside.png")
        });
        assert!(open(&path).is_err());
        save(&doc, &path).unwrap();
        let asset = path
            .join("images")
            .join(format!("{}.png", doc.layers[0].id));
        let outside = dir.path().join("outside.png");
        fs::rename(&asset, &outside).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, &asset).unwrap();
            assert!(open(&path).is_err());
        }
    }

    #[test]
    fn refuses_unrelated_destination_and_retains_its_contents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("not-a-project");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("important.txt"), b"Keep me").unwrap();
        assert!(save(&patterned(), &path).is_err());
        assert_eq!(fs::read(path.join("important.txt")).unwrap(), b"Keep me");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn atomic_syscall_failure_does_not_remove_destination() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("original");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("original-data"), b"complete").unwrap();
        assert!(exchange(&dir.path().join("missing-stage"), &path).is_err());
        assert_eq!(fs::read(path.join("original-data")).unwrap(), b"complete");
        let staged = dir.path().join("stage");
        fs::create_dir(&staged).unwrap();
        assert!(
            rename_new(&staged, &path).is_err(),
            "must not overwrite racing destination"
        );
        assert!(staged.is_dir());
        assert_eq!(fs::read(path.join("original-data")).unwrap(), b"complete");
    }

    #[test]
    fn raw_image_import_preserves_rgba_and_bounds() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Source.png");
        let pixels = patterned().layers.remove(0).image.unwrap();
        pixels.save(&path).unwrap();
        let loaded = open(&path).unwrap();
        assert_eq!(loaded.layers[0].image.as_ref().unwrap(), &pixels);
        assert_eq!(loaded.name, "Source");
        assert_eq!((loaded.width, loaded.height), (4, 3));
    }

    #[test]
    fn rejects_non_grayscale_mask_without_mutation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Mask.omuse");
        let mut doc = patterned();
        save(&doc, &path).unwrap();
        let old = fs::read(path.join("manifest.json")).unwrap();
        doc.layers[0].mask = Some(RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255])).into());
        assert!(save(&doc, &path).is_err());
        assert_eq!(fs::read(path.join("manifest.json")).unwrap(), old);
    }

    #[test]
    fn unknown_future_version_and_corrupt_png_fail_clearly() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Corrupt.omuse");
        let doc = patterned();
        save(&doc, &path).unwrap();
        let future_version = PROJECT_MAX_READ_VERSION + 1;
        edit_manifest(&path, |v| v["version"] = json!(future_version));
        assert!(
            open(&path)
                .unwrap_err()
                .to_string()
                .contains(&format!("version {future_version}"))
        );
        save(&doc, &path).unwrap();
        fs::write(
            path.join("images")
                .join(format!("{}.png", doc.layers[0].id)),
            b"not a PNG",
        )
        .unwrap();
        assert!(open(&path).is_err());
    }
}
