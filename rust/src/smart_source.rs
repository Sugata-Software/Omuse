//! Explicit source import/update. External files are never polled or executed.
use crate::{
    advanced::LayerState,
    precision::{TiledImage16, WorkingSpace},
};
use anyhow::{Context, Result, ensure};
use image::{ImageDecoder, ImageReader};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub fn import(
    path: &Path,
    raw: crate::raw_import::DevelopSettings,
    cancel: &AtomicBool,
) -> Result<LayerState> {
    ensure!(!cancel.load(Ordering::Relaxed), "Import cancelled");
    let canonical = path.canonicalize()?;
    let name = path
        .file_name()
        .context("Source needs a filename")?
        .to_string_lossy()
        .into_owned();
    let (pixels, raw_bytes, raw_extension) = if crate::raw_import::matches(path) {
        let bytes = read_bounded(path, 512 * 1024 * 1024)?;
        let extension = path
            .extension()
            .and_then(|v| v.to_str())
            .context("RAW extension missing")?
            .to_ascii_lowercase();
        // Decode the very bytes that will be embedded. An external edit during
        // import cannot pair one file's preview with another file's original.
        let pixels = develop_embedded(&bytes, &extension, &raw)?;
        (pixels, Some(Arc::new(bytes)), Some(extension))
    } else if path.is_dir()
        || path
            .extension()
            .and_then(|p| p.to_str())
            .is_some_and(|p| p.eq_ignore_ascii_case("psd"))
    {
        let doc = crate::document::open(path)?;
        ensure!(
            u64::from(doc.width) * u64::from(doc.height) <= crate::advanced::MAX_ADVANCED_PIXELS,
            "Editable sources support at most 16 million pixels"
        );
        (
            image::DynamicImage::ImageRgba8(crate::raster::composite(&doc)).to_rgba16(),
            None,
            None,
        )
    } else {
        let bytes = read_bounded(path, 256 * 1024 * 1024)?;
        let mut reader = ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
        let is_tiff = reader.format() == Some(image::ImageFormat::Tiff);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(256 * 1024 * 1024);
        limits.max_image_width = Some(30_000);
        limits.max_image_height = Some(30_000);
        reader.limits(limits);
        let mut decoder = reader.into_decoder()?;
        let (w, h) = decoder.dimensions();
        ensure!(
            u64::from(w) * u64::from(h) <= crate::advanced::MAX_ADVANCED_PIXELS,
            "Editable sources support at most 16 million pixels"
        );
        let mut profile = decoder.icc_profile()?;
        if profile.is_none() && is_tiff {
            profile = crate::color_management::tiff_icc_profile(&bytes)?;
        }
        let orientation = decoder.orientation()?;
        let mut decoded = image::DynamicImage::from_decoder(decoder)?;
        decoded.apply_orientation(orientation);
        let pixels = decoded.to_rgba16();
        (
            crate::color_management::to_srgb16(&pixels, profile.as_deref())?,
            None,
            None,
        )
    };
    ensure!(!cancel.load(Ordering::Relaxed), "Import cancelled");
    let master = Arc::new(TiledImage16::from_rgba16_in(&pixels, WorkingSpace::Srgb)?);
    let mut state = LayerState::from_image(&master.to_rgba8_in(WorkingSpace::Srgb)?, &name)?;
    state.source = master.clone();
    state.result = master;
    state.raw_bytes = raw_bytes;
    state.recipe.linked_path = Some(canonical.to_string_lossy().into_owned());
    state.recipe.raw_extension = raw_extension;
    state.recipe.raw_settings = state.raw_bytes.as_ref().map(|_| raw);
    state.validate()?;
    Ok(state)
}

pub fn redevelop(
    state: &LayerState,
    settings: crate::raw_import::DevelopSettings,
    cancel: &AtomicBool,
) -> Result<LayerState> {
    state.validate()?;
    settings.validate()?;
    ensure!(!cancel.load(Ordering::Relaxed), "RAW development cancelled");
    let bytes = state
        .raw_bytes
        .as_ref()
        .context("This source has no embedded RAW original")?;
    let extension = state
        .recipe
        .raw_extension
        .as_ref()
        .context("Missing RAW extension")?;
    let pixels = develop_embedded(bytes, extension, &settings)?;
    ensure!(!cancel.load(Ordering::Relaxed), "RAW development cancelled");
    let mut next = state.clone();
    let mut source = TiledImage16::from_rgba16(&pixels)?;
    source.convert_working_space(state.recipe.working_space)?;
    next.source = Arc::new(source);
    next.result = next.source.clone();
    next.recipe.raw_settings = Some(settings);
    next.evaluate(cancel)
}

fn develop_embedded(
    bytes: &[u8],
    extension: &str,
    settings: &crate::raw_import::DevelopSettings,
) -> Result<crate::precision::Rgba16Image> {
    ensure!(
        !extension.is_empty()
            && extension.len() <= 10
            && extension.bytes().all(|b| b.is_ascii_alphanumeric()),
        "Invalid RAW extension"
    );
    let path =
        std::env::temp_dir().join(format!("omuse-raw-{}.{}", uuid::Uuid::new_v4(), extension));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let cleanup = Cleanup(path.clone());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    file.write_all(bytes)?;
    drop(file);
    let pixels = crate::raw_import::develop16(&path, settings)?;
    drop(cleanup);
    Ok(pixels)
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= max,
        "Source file exceeds the supported limit"
    );
    let mut bytes = vec![];
    file.take(max + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= max,
        "Source file exceeds the supported limit"
    );
    Ok(bytes)
}
