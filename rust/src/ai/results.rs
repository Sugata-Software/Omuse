use crate::ai::types::{AiError, JobLimits, ResultAsset, is_within};
use image::ImageFormat;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub(crate) fn validate_reference(
    path: &Path,
    work_dir: &Path,
    limits: &JobLimits,
) -> Result<ResultAsset, AiError> {
    validate_file(path, work_dir, limits, None)
}

pub(crate) fn capture_codex_image(
    work_dir: &Path,
    item_id: Option<&str>,
    saved_path: Option<&str>,
    result: &str,
    limits: &JobLimits,
) -> Result<ResultAsset, AiError> {
    if let Some(saved_path) = saved_path {
        return capture_saved_image(
            Path::new(saved_path),
            work_dir,
            limits,
            item_id.map(str::to_owned),
        );
    }

    if let Some(path) = result.strip_prefix("file://") {
        return capture_saved_image(
            Path::new(path),
            work_dir,
            limits,
            item_id.map(str::to_owned),
        );
    }
    let direct_path = Path::new(result);
    if direct_path.is_absolute() {
        return capture_saved_image(direct_path, work_dir, limits, item_id.map(str::to_owned));
    }

    let (media_type, encoded) = parse_inline_image(result)?;
    let bytes = decode_base64_bounded(encoded, limits.max_asset_bytes as usize)?;
    let extension = match media_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => {
            return Err(AiError::Protocol(
                "Provider returned an unsupported image type".into(),
            ));
        }
    };
    let safe_id = item_id
        .map(sanitize_component)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let path = work_dir.join(format!("omuse-ai-result-{safe_id}.{extension}"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    let validated = validate_file(&path, work_dir, limits, item_id.map(str::to_owned));
    if validated.is_err() {
        let _ = fs::remove_file(&path);
    }
    validated
}

/// Codex's built-in image tool writes into CODEX_HOME/generated_images. Copy
/// only validated image outputs into the job's asset area before the isolated
/// runtime home is removed. Auth, sessions and all other runtime paths remain
/// forbidden, including symlinks escaping the generated-images subtree.
fn capture_saved_image(
    path: &Path,
    work_dir: &Path,
    limits: &JobLimits,
    provider_item_id: Option<String>,
) -> Result<ResultAsset, AiError> {
    let root = work_dir.canonicalize()?;
    let source = path.canonicalize()?;
    let relative = source
        .strip_prefix(&root)
        .map_err(|_| AiError::Protocol("Provider image escaped the job workspace".into()))?;
    let parts: Vec<_> = relative.components().collect();
    let runtime_output = parts
        .first()
        .and_then(|part| part.as_os_str().to_str())
        .is_some_and(|name| name.starts_with(".codex-runtime-"));
    if parts.len() < 3 || parts[1].as_os_str() != "generated_images" {
        return Err(AiError::Protocol(
            "Provider result did not originate in the isolated generated-images directory".into(),
        ));
    }
    if !runtime_output {
        return Err(AiError::Protocol(
            "Provider result did not originate in the isolated generated-images directory".into(),
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x80000);
    }
    let input = options.open(&source)?;
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() > limits.max_asset_bytes {
        return Err(AiError::Protocol(
            "Provider image exceeds the configured byte limit".into(),
        ));
    }
    let destination = root.join(format!("omuse-ai-result-{}.asset", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        let written = std::io::copy(&mut input.take(limits.max_asset_bytes + 1), &mut output)?;
        if written > limits.max_asset_bytes || written != metadata.len() {
            return Err(AiError::Protocol(
                "Provider image changed while being retained".into(),
            ));
        }
        output.sync_all()?;
        validate_file(&destination, &root, limits, provider_item_id)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&destination);
    }
    result
}

fn parse_inline_image(value: &str) -> Result<(&str, &str), AiError> {
    if let Some(rest) = value.strip_prefix("data:") {
        let (header, payload) = rest.split_once(',').ok_or_else(|| {
            AiError::Protocol("Provider returned a malformed inline image".into())
        })?;
        let media_type = header.strip_suffix(";base64").ok_or_else(|| {
            AiError::Protocol("Provider inline image is not base64 encoded".into())
        })?;
        return Ok((media_type, payload));
    }
    // Installed app-server schemas type `result` as an opaque string. Current
    // builds may return raw base64 when no savedPath is present; the decoded
    // image header below is still validated before import.
    Ok(("image/png", value))
}

fn validate_file(
    path: &Path,
    work_dir: &Path,
    limits: &JobLimits,
    provider_item_id: Option<String>,
) -> Result<ResultAsset, AiError> {
    let root = work_dir.canonicalize()?;
    let path = path.canonicalize()?;
    if !is_within(&path, &root) {
        return Err(AiError::Protocol(
            "Provider image escaped the job workspace".into(),
        ));
    }
    if path
        .strip_prefix(&root)
        .ok()
        .and_then(|relative| relative.components().next())
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|name| name.starts_with(".codex-runtime-"))
    {
        return Err(AiError::Protocol(
            "Provider result overlaps private runtime state".into(),
        ));
    }
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() {
        return Err(AiError::Protocol(
            "Provider result is not a regular file".into(),
        ));
    }
    if metadata.len() > limits.max_asset_bytes {
        return Err(AiError::Protocol(
            "Provider image exceeds the configured byte limit".into(),
        ));
    }
    let reader = image::ImageReader::open(&path)?.with_guessed_format()?;
    let format = reader
        .format()
        .ok_or_else(|| AiError::Protocol("Provider result has no image format".into()))?;
    let media_type = supported_media_type(format)
        .ok_or_else(|| AiError::Protocol("Provider returned an unsupported image format".into()))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| AiError::Protocol(format!("Provider image header is invalid: {error}")))?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| AiError::Protocol("Provider image dimensions overflow".into()))?;
    if width == 0 || height == 0 || pixels > limits.max_image_pixels {
        return Err(AiError::Protocol(
            "Provider image dimensions exceed the configured limit".into(),
        ));
    }
    Ok(ResultAsset {
        path,
        media_type: media_type.into(),
        width,
        height,
        byte_len: metadata.len(),
        provider_item_id,
    })
}

fn supported_media_type(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::WebP => Some("image/webp"),
        _ => None,
    }
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .filter(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
        .take(80)
        .collect()
}

fn decode_base64_bounded(value: &str, max_bytes: usize) -> Result<Vec<u8>, AiError> {
    let estimated = value
        .len()
        .checked_mul(3)
        .map(|value| value / 4 + 3)
        .ok_or_else(|| AiError::Protocol("Provider image size overflow".into()))?;
    if estimated > max_bytes.saturating_add(3) {
        return Err(AiError::Protocol(
            "Provider image exceeds the configured byte limit".into(),
        ));
    }
    let mut output = Vec::with_capacity(estimated.min(max_bytes));
    let mut block = [0_u8; 4];
    let mut used = 0;
    for byte in value.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        block[used] = byte;
        used += 1;
        if used == 4 {
            decode_block(block, &mut output)?;
            if output.len() > max_bytes {
                return Err(AiError::Protocol(
                    "Provider image exceeds the configured byte limit".into(),
                ));
            }
            used = 0;
        }
    }
    if used != 0 {
        return Err(AiError::Protocol(
            "Provider returned malformed base64 image data".into(),
        ));
    }
    Ok(output)
}

fn decode_block(block: [u8; 4], output: &mut Vec<u8>) -> Result<(), AiError> {
    let a = base64_value(block[0])?;
    let b = base64_value(block[1])?;
    let c_padding = block[2] == b'=';
    let d_padding = block[3] == b'=';
    if c_padding && !d_padding {
        return Err(AiError::Protocol(
            "Provider returned malformed base64 padding".into(),
        ));
    }
    let c = if c_padding {
        0
    } else {
        base64_value(block[2])?
    };
    let d = if d_padding {
        0
    } else {
        base64_value(block[3])?
    };
    output.push((a << 2) | (b >> 4));
    if !c_padding {
        output.push((b << 4) | (c >> 2));
    }
    if !d_padding {
        output.push((c << 6) | d);
    }
    Ok(())
}

fn base64_value(value: u8) -> Result<u8, AiError> {
    match value {
        b'A'..=b'Z' => Ok(value - b'A'),
        b'a'..=b'z' => Ok(value - b'a' + 26),
        b'0'..=b'9' => Ok(value - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(AiError::Protocol(
            "Provider returned malformed base64 image data".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn validates_images_only_below_the_job_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("outside.png");
        RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]))
            .save(&path)
            .unwrap();
        let error = validate_reference(&path, root.path(), &JobLimits::default()).unwrap_err();
        assert!(error.to_string().contains("escaped"));
    }

    #[test]
    fn inline_results_are_written_and_header_validated() {
        let root = tempfile::tempdir().unwrap();
        // A 1x1 transparent PNG.
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAF/gL+XwVvWQAAAABJRU5ErkJggg==";
        let result = capture_codex_image(
            root.path(),
            Some("item/1"),
            None,
            png,
            &JobLimits::default(),
        )
        .unwrap();
        assert_eq!((result.width, result.height), (1, 1));
        assert!(result.path.starts_with(root.path()));
    }

    #[test]
    fn dimensions_are_bounded_before_decode() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("large.png");
        RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0]))
            .save(&path)
            .unwrap();
        let limits = JobLimits {
            max_image_pixels: 50,
            ..JobLimits::default()
        };
        assert!(validate_reference(&path, root.path(), &limits).is_err());
    }

    #[test]
    fn official_generated_image_is_retained_after_runtime_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join(".codex-runtime-fixture");
        let generated = runtime.join("generated_images");
        fs::create_dir_all(&generated).unwrap();
        let path = generated.join("image.png");
        RgbaImage::from_pixel(3, 2, Rgba([10, 20, 30, 255]))
            .save(&path)
            .unwrap();
        let result = capture_codex_image(
            root.path(),
            Some("generated-1"),
            path.to_str(),
            "",
            &JobLimits::default(),
        )
        .unwrap();
        fs::remove_dir_all(runtime).unwrap();
        assert_eq!((result.width, result.height), (3, 2));
        assert_eq!(
            image::ImageReader::open(result.path)
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .decode()
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0)
                .0,
            [10, 20, 30, 255]
        );
    }

    #[test]
    fn runtime_files_and_escaping_image_links_remain_forbidden() {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join(".codex-runtime-fixture");
        let generated = runtime.join("generated_images");
        fs::create_dir_all(&generated).unwrap();
        let private = runtime.join("private.png");
        RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]))
            .save(&private)
            .unwrap();
        assert!(
            capture_codex_image(
                root.path(),
                None,
                private.to_str(),
                "",
                &JobLimits::default()
            )
            .is_err()
        );
        #[cfg(unix)]
        {
            let link = generated.join("pretend.png");
            std::os::unix::fs::symlink(&private, &link).unwrap();
            assert!(
                capture_codex_image(root.path(), None, link.to_str(), "", &JobLimits::default())
                    .is_err()
            );
        }
    }

    #[test]
    fn saved_provider_results_cannot_reuse_a_staged_reference() {
        let root = tempfile::tempdir().unwrap();
        let reference = root.path().join("reference.png");
        RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 255]))
            .save(&reference)
            .unwrap();

        let error = capture_codex_image(
            root.path(),
            Some("provider-item"),
            reference.to_str(),
            "",
            &JobLimits::default(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("generated-images"));
        assert!(!fs::read(&reference).unwrap().is_empty());
        assert!(fs::read_dir(root.path()).unwrap().flatten().all(|entry| {
            !entry
                .file_name()
                .to_string_lossy()
                .starts_with("omuse-ai-result-")
        }));
    }

    #[test]
    fn invalid_inline_image_is_removed_after_header_rejection() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            capture_codex_image(
                root.path(),
                Some("invalid"),
                None,
                "data:image/png;base64,AAAA",
                &JobLimits::default(),
            )
            .is_err()
        );
        assert!(fs::read_dir(root.path()).unwrap().flatten().all(|entry| {
            !entry
                .file_name()
                .to_string_lossy()
                .starts_with("omuse-ai-result-")
        }));
    }
}
