//! Portable, bounded editing recipes and headless folder processing.
use crate::{document, filters, model::Document, raster};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Adjustment {
        adjustment: crate::editor::Adjustment,
    },
    Filter {
        filter: filters::Filter,
    },
    Resize {
        width: u32,
        height: u32,
    },
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    Rotate {
        clockwise_quarters: u8,
    },
    Flip {
        horizontal: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub name: String,
    pub steps: Vec<Step>,
}
impl Default for Recipe {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Editing recipe".into(),
            steps: vec![],
        }
    }
}
impl Recipe {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported recipe version");
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 256,
            "Invalid recipe name"
        );
        ensure!(
            self.steps.len() <= 256,
            "Recipes support at most 256 operations"
        );
        for step in &self.steps {
            match step {
                Step::Adjustment { adjustment } => {
                    use crate::editor::Adjustment::*;
                    match adjustment {
                        Invert | Grayscale => {}
                        Blur(v) | Sharpen(v) => ensure!(
                            v.is_finite() && *v > 0. && *v <= 64.,
                            "Invalid recipe blur radius"
                        ),
                        Brightness(v) | Contrast(v) | Saturation(v) => ensure!(
                            v.is_finite() && (-1.0..=1.0).contains(v),
                            "Invalid recipe adjustment"
                        ),
                    }
                }
                Step::Filter { filter } => filters::validate(filter)?,
                Step::Resize { width, height } | Step::Crop { width, height, .. } => ensure!(
                    crate::model::valid_dimensions(*width, *height)
                        && u64::from(*width) * u64::from(*height) <= 16_777_216,
                    "Recipe dimensions exceed 16 million pixels"
                ),
                Step::Rotate { clockwise_quarters } => ensure!(
                    *clockwise_quarters <= 3,
                    "Quarter turns must be 0 through 3"
                ),
                Step::Flip { .. } => {}
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        use std::io::Read;
        let file = std::fs::File::open(path)?;
        ensure!(
            file.metadata()?.is_file() && file.metadata()?.len() <= 1024 * 1024,
            "Recipe exceeds 1 MiB"
        );
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 1024 * 1024, "Recipe exceeds 1 MiB");
        let value: Self = serde_json::from_slice(&bytes)?;
        value.validate()?;
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        ensure!(bytes.len() <= 1024 * 1024, "Recipe exceeds 1 MiB");
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let staging = parent.join(format!(".omuse-recipe-{}.tmp", uuid::Uuid::new_v4()));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(staging.clone());
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::hard_link(&staging, path)
            .context("Choose a new recipe filename; existing recipes are preserved")?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
    /// Returns a new surface. Source documents are never changed by a recipe.
    pub fn apply(
        &self,
        source: &image::RgbaImage,
        cancel: &AtomicBool,
    ) -> Result<image::RgbaImage> {
        self.validate()?;
        ensure!(
            crate::model::valid_dimensions(source.width(), source.height())
                && u64::from(source.width()) * u64::from(source.height()) <= 16_777_216,
            "Recipe input exceeds 16 million pixels"
        );
        ensure!(!cancel.load(Ordering::Relaxed), "Recipe cancelled");
        let mut out = source.clone();
        for step in &self.steps {
            ensure!(!cancel.load(Ordering::Relaxed), "Recipe cancelled");
            match step {
                Step::Adjustment { adjustment } => {
                    let mut document = Document::new(out.width(), out.height());
                    document.layers[0].image = Some(out.into());
                    let mut editor = crate::editor::Editor::new(document);
                    editor.adjust(*adjustment);
                    out = (**editor.document.layers[0].image.as_ref().unwrap()).clone();
                }
                Step::Filter { filter } => filters::apply_cancellable(&mut out, filter, cancel)?,
                Step::Resize { width, height } => {
                    if out.dimensions() != (*width, *height) {
                        let mut document = Document::new(out.width(), out.height());
                        document.layers[0].image = Some(out.into());
                        let mut editor = crate::editor::Editor::new(document);
                        ensure!(
                            editor.resize_image(*width, *height),
                            "Recipe resize exceeds processing limits"
                        );
                        out = (**editor.document.layers[0].image.as_ref().unwrap()).clone();
                    }
                }
                Step::Crop {
                    x,
                    y,
                    width,
                    height,
                } => {
                    ensure!(
                        x.checked_add(*width).is_some_and(|v| v <= out.width())
                            && y.checked_add(*height).is_some_and(|v| v <= out.height()),
                        "Recipe crop exceeds this image"
                    );
                    out = image::imageops::crop_imm(&out, *x, *y, *width, *height).to_image();
                }
                Step::Rotate { clockwise_quarters } => {
                    out = match clockwise_quarters {
                        1 => image::imageops::rotate90(&out),
                        2 => image::imageops::rotate180(&out),
                        3 => image::imageops::rotate270(&out),
                        _ => out,
                    }
                }
                Step::Flip { horizontal } => {
                    out = if *horizontal {
                        image::imageops::flip_horizontal(&out)
                    } else {
                        image::imageops::flip_vertical(&out)
                    }
                }
            }
        }
        ensure!(!cancel.load(Ordering::Relaxed), "Recipe cancelled");
        Ok(out)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchItem {
    pub input: PathBuf,
    pub output: PathBuf,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BatchReport {
    pub items: Vec<BatchItem>,
    pub cancelled: bool,
}

/// Nonrecursive, sorted enumeration. Each output is exclusively published;
/// collisions and per-file failures are reported and never overwrite a file.
pub fn batch(
    recipe: &Recipe,
    input: &Path,
    output: &Path,
    extension: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<BatchReport> {
    recipe.validate()?;
    ensure!(
        input.is_dir() && output.is_dir(),
        "Input and output must be existing directories"
    );
    ensure!(
        input.canonicalize()? != output.canonicalize()?,
        "Choose a separate output directory"
    );
    ensure!(
        matches!(extension, "png" | "jpg" | "webp" | "tiff"),
        "Unsupported batch export format"
    );
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(input)? {
        if cancel.load(Ordering::Relaxed) {
            return Ok(BatchReport {
                cancelled: true,
                ..Default::default()
            });
        }
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if (kind.is_dir() && ext == "comp")
            || (kind.is_file()
                && (matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "tif" | "tiff" | "webp" | "bmp" | "gif" | "psd"
                ) || crate::raw_import::matches(&path)))
        {
            paths.push(path);
        }
        ensure!(paths.len() <= 10_000, "Batch exceeds 10,000 files");
    }
    paths.sort();
    let total = paths.len();
    let mut report = BatchReport::default();
    for (index, path) in paths.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let name = path
            .file_name()
            .context("Missing source filename")?
            .to_string_lossy();
        // Keep the input suffix to disambiguate photo.jpg and photo.png.
        let destination = output.join(format!("{name}.{extension}"));
        let result = (|| -> Result<()> {
            ensure!(!destination.exists(), "Output already exists");
            let source = document::open(&path)?;
            ensure!(
                u64::from(source.width) * u64::from(source.height) <= 16_777_216,
                "Recipe source exceeds 16 million pixels"
            );
            let image = recipe.apply(&raster::composite(&source), cancel)?;
            let mut doc = Document::new(image.width(), image.height());
            doc.layers[0].image = Some(image.into());
            let staging = output.join(format!(
                ".omuse-batch-{}.{}",
                uuid::Uuid::new_v4(),
                extension
            ));
            struct Cleanup(PathBuf);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    let _ = std::fs::remove_file(&self.0);
                }
            }
            let _cleanup = Cleanup(staging.clone());
            raster::export(&doc, &staging)?;
            ensure!(!cancel.load(Ordering::Relaxed), "Batch cancelled");
            // An exclusive hard-link publication is atomic on this same filesystem.
            std::fs::hard_link(&staging, &destination)
                .context("Cannot publish output without replacing an existing file")?;
            Ok(())
        })();
        report.items.push(BatchItem {
            input: path,
            output: destination,
            error: result.err().map(|e| format!("{e:#}")),
        });
        progress(index + 1, total);
    }
    report.cancelled |= cancel.load(Ordering::Relaxed);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_recipe_exact_pixels_cancel_and_bounds() {
        let recipe = Recipe {
            steps: vec![
                Step::Filter {
                    filter: filters::Filter::Invert,
                },
                Step::Rotate {
                    clockwise_quarters: 1,
                },
            ],
            ..Default::default()
        };
        let recipe: Recipe = serde_json::from_slice(&serde_json::to_vec(&recipe).unwrap()).unwrap();
        let image = image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([(x * 30) as u8, (y * 70) as u8, 200, 255])
        });
        let result = recipe.apply(&image, &AtomicBool::new(false)).unwrap();
        assert_eq!(result.dimensions(), (2, 3));
        assert_eq!(result.get_pixel(1, 2).0, [195, 255, 55, 255]);
        assert!(recipe.apply(&image, &AtomicBool::new(true)).is_err());
        let bad = Recipe {
            steps: vec![Step::Crop {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 2,
            }],
            ..Default::default()
        };
        assert!(bad.apply(&image, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn batch_is_sorted_collision_safe_and_preserves_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("in");
        let output = tmp.path().join("out");
        std::fs::create_dir(&input).unwrap();
        std::fs::create_dir(&output).unwrap();
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([30, 80, 120, 255]));
        image.save(input.join("b.png")).unwrap();
        image.save(input.join("a.png")).unwrap();
        std::fs::write(output.join("a.png.png"), b"preserved").unwrap();
        let report = batch(
            &Recipe::default(),
            &input,
            &output,
            "png",
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        assert!(report.items[0].error.is_some());
        assert!(report.items[1].error.is_none());
        assert_eq!(
            std::fs::read(output.join("a.png.png")).unwrap(),
            b"preserved"
        );
        assert_eq!(image::open(input.join("b.png")).unwrap().to_rgba8(), image);
        assert_eq!(
            image::open(output.join("b.png.png")).unwrap().to_rgba8(),
            image
        );
    }
}
