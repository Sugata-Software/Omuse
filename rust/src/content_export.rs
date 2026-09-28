//! Ordered, bounded exports for multi-page Create projects.
//!
//! Package construction is deliberately incremental so a lazy project does not
//! need to retain every page in memory.  A complete package is published with a
//! single directory rename; errors and cancellation remove the unpublished
//! staging directory.

use crate::{model::Document, raster};
use anyhow::{Context, Result, bail, ensure};
use flate2::{Compression, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_EXPORT_PAGES: usize = 256;
pub const MAX_EXPORT_PIXELS: u64 = 2_000_000_000;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_KEEP_BOTH_ATTEMPTS: u32 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RasterExportFormat {
    Png,
    Jpeg,
    WebP,
}

impl RasterExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollisionPolicy {
    #[default]
    Reject,
    KeepBoth,
}

#[derive(Clone, Debug)]
pub struct PackageOptions {
    pub formats: Vec<RasterExportFormat>,
    pub include_pdf: bool,
    pub jpeg_quality: u8,
    pub matte: [u8; 3],
    pub collision: CollisionPolicy,
    pub max_total_pixels: u64,
}

impl Default for PackageOptions {
    fn default() -> Self {
        Self {
            formats: vec![RasterExportFormat::Png],
            include_pdf: true,
            jpeg_quality: 90,
            matte: [255; 3],
            collision: CollisionPolicy::Reject,
            max_total_pixels: MAX_EXPORT_PIXELS,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PageExportMetadata {
    pub id: String,
    pub name: String,
    pub caption: String,
    pub alt_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportProgress {
    pub completed_units: usize,
    pub total_units: usize,
    pub page_index: usize,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct ExportPackageResult {
    pub path: PathBuf,
    pub pages: usize,
    pub files: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportManifest {
    pub format_version: u32,
    pub pages: Vec<ExportManifestPage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportManifestPage {
    pub order: usize,
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub resolution_dpi: f64,
    pub files: Vec<String>,
    pub caption: String,
    pub alt_text: String,
}

#[derive(Clone, Debug, Serialize)]
struct ContentManifest<'a> {
    format_version: u32,
    pages: Vec<ContentManifestPage<'a>>,
}

#[derive(Clone, Debug, Serialize)]
struct ContentManifestPage<'a> {
    order: usize,
    id: &'a str,
    caption: &'a str,
    alt_text: &'a str,
}

#[derive(Clone, Debug)]
struct PdfSpool {
    path: PathBuf,
    width: u32,
    height: u32,
    resolution: f64,
}

/// Incremental export transaction. Call `write_page` exactly `expected_pages`
/// times, in the intended presentation order, then `finish`.
pub struct ExportPackageWriter {
    destination: PathBuf,
    stage: Option<PathBuf>,
    options: PackageOptions,
    expected_pages: usize,
    pages: Vec<ExportManifestPage>,
    pdf_spools: Vec<PdfSpool>,
    total_pixels: u64,
    files: usize,
    completed_units: usize,
}

impl ExportPackageWriter {
    pub fn begin(
        destination: impl AsRef<Path>,
        expected_pages: usize,
        options: PackageOptions,
        cancel: &AtomicBool,
    ) -> Result<Self> {
        check_cancel(cancel)?;
        ensure!(
            (1..=MAX_EXPORT_PAGES).contains(&expected_pages),
            "export packages require 1–{MAX_EXPORT_PAGES} pages"
        );
        validate_options(&options)?;
        let requested = destination.as_ref();
        ensure!(
            !requested.as_os_str().is_empty(),
            "missing export destination"
        );
        let parent = requested
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create {}", parent.display()))?;
        ensure_directory(parent)?;
        let destination = choose_destination(requested, options.collision)?;
        let stage = parent.join(format!(".omuse-package-{}.tmp", uuid::Uuid::new_v4()));
        fs::create_dir(&stage).context("Creating export staging directory")?;
        if let Err(error) = fs::create_dir(stage.join("images")) {
            let _ = fs::remove_dir_all(&stage);
            return Err(error).context("Creating package image directory");
        }
        if options.include_pdf
            && let Err(error) = fs::create_dir(stage.join(".pdf-spool"))
        {
            let _ = fs::remove_dir_all(&stage);
            return Err(error).context("Creating PDF staging directory");
        }
        Ok(Self {
            destination,
            stage: Some(stage),
            options,
            expected_pages,
            pages: Vec::with_capacity(expected_pages),
            pdf_spools: Vec::with_capacity(expected_pages),
            total_pixels: 0,
            files: 0,
            completed_units: 0,
        })
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }

    pub fn write_page(
        &mut self,
        document: &Document,
        metadata: PageExportMetadata,
        cancel: &AtomicBool,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<()> {
        check_cancel(cancel)?;
        ensure!(
            self.pages.len() < self.expected_pages,
            "more pages supplied than declared"
        );
        validate_metadata(&metadata)?;
        let render_errors = raster::validate(document);
        ensure!(
            render_errors.is_empty(),
            "cannot export page {}: {}",
            self.pages.len() + 1,
            render_errors.join("; ")
        );
        let pixels = u64::from(document.width) * u64::from(document.height);
        let total_pixels = self
            .total_pixels
            .checked_add(pixels)
            .context("export pixel count overflow")?;
        ensure!(
            total_pixels <= self.options.max_total_pixels,
            "export exceeds the configured total pixel limit"
        );

        let index = self.pages.len();
        let stem = format!("{:03}-{}", index + 1, slug(&metadata.name));
        let stage = self.stage.as_ref().context("export already finished")?;
        let mut files = Vec::with_capacity(self.options.formats.len());
        for format in &self.options.formats {
            check_cancel(cancel)?;
            let relative = format!("images/{stem}.{}", format.extension());
            raster::export_with_options(
                document,
                &stage.join(&relative),
                raster::ExportOptions {
                    jpeg_quality: self.options.jpeg_quality,
                    matte: self.options.matte,
                },
            )
            .with_context(|| format!("Exporting page {} as {}", index + 1, format.extension()))?;
            files.push(relative);
            self.files += 1;
            self.completed_units += 1;
            progress(self.progress(index, format!("Rendered {}", format.extension())));
        }

        let resolution = resolution(document);
        if self.options.include_pdf {
            check_cancel(cancel)?;
            let spool = stage.join(".pdf-spool").join(format!("{index:03}.rgbz"));
            write_pdf_spool(document, &spool, self.options.matte, cancel)?;
            self.pdf_spools.push(PdfSpool {
                path: spool,
                width: document.width,
                height: document.height,
                resolution,
            });
            self.completed_units += 1;
            progress(self.progress(index, "Prepared PDF page".into()));
        }
        self.pages.push(ExportManifestPage {
            order: index + 1,
            id: if metadata.id.is_empty() {
                format!("page-{}", index + 1)
            } else {
                metadata.id
            },
            name: if metadata.name.is_empty() {
                format!("Page {}", index + 1)
            } else {
                metadata.name
            },
            width: document.width,
            height: document.height,
            resolution_dpi: resolution,
            files,
            caption: metadata.caption,
            alt_text: metadata.alt_text,
        });
        self.total_pixels = total_pixels;
        Ok(())
    }

    fn progress(&self, page_index: usize, detail: String) -> ExportProgress {
        ExportProgress {
            completed_units: self.completed_units,
            total_units: self.expected_pages
                * (self.options.formats.len() + usize::from(self.options.include_pdf)),
            page_index,
            detail,
        }
    }

    pub fn finish(mut self, cancel: &AtomicBool) -> Result<ExportPackageResult> {
        check_cancel(cancel)?;
        ensure!(
            self.pages.len() == self.expected_pages,
            "expected {} pages but received {}",
            self.expected_pages,
            self.pages.len()
        );
        let stage = self.stage.as_ref().context("export already finished")?;
        if self.options.include_pdf {
            write_pdf_from_spools(&stage.join("pages.pdf"), &self.pdf_spools, cancel)?;
            self.files += 1;
        }
        write_json_atomic(
            &stage.join("manifest.json"),
            &ExportManifest {
                format_version: 1,
                pages: self.pages.clone(),
            },
        )?;
        let content = ContentManifest {
            format_version: 1,
            pages: self
                .pages
                .iter()
                .map(|page| ContentManifestPage {
                    order: page.order,
                    id: &page.id,
                    caption: &page.caption,
                    alt_text: &page.alt_text,
                })
                .collect(),
        };
        write_json_atomic(&stage.join("content.json"), &content)?;
        self.files += 2;
        if self.options.include_pdf {
            fs::remove_dir_all(stage.join(".pdf-spool"))?;
        }
        check_cancel(cancel)?;
        sync_package(stage)?;
        rename_new(stage, &self.destination).with_context(|| {
            format!(
                "Publishing export package to {}",
                self.destination.display()
            )
        })?;
        self.stage = None;
        if let Some(parent) = self.destination.parent() {
            let _ = File::open(parent).and_then(|directory| directory.sync_all());
        }
        Ok(ExportPackageResult {
            path: self.destination.clone(),
            pages: self.pages.len(),
            files: self.files,
        })
    }
}

impl Drop for ExportPackageWriter {
    fn drop(&mut self) {
        if let Some(stage) = self.stage.take() {
            let _ = fs::remove_dir_all(stage);
        }
    }
}

pub struct ExportPage<'a> {
    pub document: &'a Document,
    pub metadata: PageExportMetadata,
}

#[derive(Clone, Debug)]
pub struct PdfExportOptions {
    pub matte: [u8; 3],
    pub collision: CollisionPolicy,
    pub max_total_pixels: u64,
}

impl Default for PdfExportOptions {
    fn default() -> Self {
        Self {
            matte: [255; 3],
            collision: CollisionPolicy::Reject,
            max_total_pixels: MAX_EXPORT_PIXELS,
        }
    }
}

/// Convenience PDF export for callers that already hold the page documents.
/// The page order in the slice is retained exactly.
pub fn export_pdf(
    pages: &[ExportPage<'_>],
    destination: impl AsRef<Path>,
    options: PdfExportOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(ExportProgress),
) -> Result<PathBuf> {
    ensure!(
        (1..=MAX_EXPORT_PAGES).contains(&pages.len()),
        "PDF exports require 1–{MAX_EXPORT_PAGES} pages"
    );
    ensure!(
        options.max_total_pixels > 0 && options.max_total_pixels <= MAX_EXPORT_PIXELS,
        "invalid PDF pixel limit"
    );
    let destination = choose_destination(destination.as_ref(), options.collision)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let stage = parent.join(format!(".omuse-pdf-{}.tmp", uuid::Uuid::new_v4()));
    fs::create_dir(&stage)?;
    let outcome = (|| -> Result<()> {
        let mut spools = Vec::with_capacity(pages.len());
        let mut total = 0u64;
        for (index, page) in pages.iter().enumerate() {
            check_cancel(cancel)?;
            let errors = raster::validate(page.document);
            ensure!(
                errors.is_empty(),
                "cannot export PDF page: {}",
                errors.join("; ")
            );
            total = total
                .checked_add(u64::from(page.document.width) * u64::from(page.document.height))
                .context("PDF pixel count overflow")?;
            ensure!(
                total <= options.max_total_pixels,
                "PDF exceeds total pixel limit"
            );
            let path = stage.join(format!("{index:03}.rgbz"));
            write_pdf_spool(page.document, &path, options.matte, cancel)?;
            spools.push(PdfSpool {
                path,
                width: page.document.width,
                height: page.document.height,
                resolution: resolution(page.document),
            });
            progress(ExportProgress {
                completed_units: index + 1,
                total_units: pages.len(),
                page_index: index,
                detail: "Prepared PDF page".into(),
            });
        }
        let temporary = stage.join("output.pdf");
        write_pdf_from_spools(&temporary, &spools, cancel)?;
        check_cancel(cancel)?;
        publish_file_new(&temporary, &destination)?;
        let _ = File::open(parent).and_then(|directory| directory.sync_all());
        Ok(())
    })();
    let _ = fs::remove_dir_all(&stage);
    outcome?;
    Ok(destination)
}

fn validate_options(options: &PackageOptions) -> Result<()> {
    ensure!(
        !options.formats.is_empty() || options.include_pdf,
        "select at least one export format"
    );
    ensure!(
        (1..=100).contains(&options.jpeg_quality),
        "JPEG quality must be 1–100"
    );
    ensure!(
        options.max_total_pixels > 0 && options.max_total_pixels <= MAX_EXPORT_PIXELS,
        "invalid export pixel limit"
    );
    let mut unique = options.formats.clone();
    unique.sort_by_key(|format| *format as u8);
    unique.dedup();
    ensure!(
        unique.len() == options.formats.len(),
        "duplicate raster export format"
    );
    Ok(())
}

fn validate_metadata(metadata: &PageExportMetadata) -> Result<()> {
    for (name, value) in [
        ("page ID", &metadata.id),
        ("page name", &metadata.name),
        ("caption", &metadata.caption),
        ("alt text", &metadata.alt_text),
    ] {
        ensure!(value.len() <= MAX_TEXT_BYTES, "{name} is too long");
        ensure!(!value.chars().any(|ch| ch == '\0'), "{name} contains NUL");
    }
    Ok(())
}

fn resolution(document: &Document) -> f64 {
    document
        .metadata
        .get("resolution")
        .and_then(serde_json::Value::as_f64)
        .filter(|dpi| dpi.is_finite() && (1.0..=9600.0).contains(dpi))
        .unwrap_or(72.0)
}

fn slug(name: &str) -> String {
    let mut result = String::new();
    let mut pending_dash = false;
    for ch in name.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !result.is_empty() {
                result.push('-');
            }
            pending_dash = false;
            result.push(ch);
        } else {
            pending_dash = true;
        }
        if result.len() >= 64 {
            break;
        }
    }
    if result.is_empty() {
        "page".into()
    } else {
        result
    }
}

fn choose_destination(requested: &Path, collision: CollisionPolicy) -> Result<PathBuf> {
    if !requested.exists() {
        return Ok(requested.to_owned());
    }
    if collision == CollisionPolicy::Reject {
        bail!("export destination already exists: {}", requested.display());
    }
    let parent = requested.parent().unwrap_or_else(|| Path::new("."));
    let stem = requested
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("export");
    let extension = requested.extension().and_then(|value| value.to_str());
    for suffix in 2..=MAX_KEEP_BOTH_ATTEMPTS {
        let name = match extension {
            Some(extension) => format!("{stem}-{suffix}.{extension}"),
            None => format!("{stem}-{suffix}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("could not choose an unused export name")
}

fn ensure_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_dir(),
        "export parent is not a directory"
    );
    Ok(())
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "export cancelled");
    Ok(())
}

fn write_pdf_spool(
    document: &Document,
    path: &Path,
    matte: [u8; 3],
    cancel: &AtomicBool,
) -> Result<()> {
    check_cancel(cancel)?;
    let image = raster::composite(document);
    ensure!(
        image.dimensions() == (document.width, document.height),
        "page compositor returned invalid dimensions"
    );
    let output = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut encoder = ZlibEncoder::new(BufWriter::new(output), Compression::new(6));
    let mut row = Vec::with_capacity(document.width as usize * 3);
    for pixels in image.rows() {
        check_cancel(cancel)?;
        row.clear();
        for pixel in pixels {
            let alpha = u32::from(pixel[3]);
            for channel in 0..3 {
                row.push(
                    ((u32::from(pixel[channel]) * alpha
                        + u32::from(matte[channel]) * (255 - alpha)
                        + 127)
                        / 255) as u8,
                );
            }
        }
        encoder.write_all(&row)?;
    }
    let mut output = encoder.finish()?;
    output.flush()?;
    output.get_ref().sync_all()?;
    Ok(())
}

fn write_pdf_from_spools(path: &Path, pages: &[PdfSpool], cancel: &AtomicBool) -> Result<()> {
    ensure!(!pages.is_empty(), "PDF requires at least one page");
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut output = BufWriter::new(file);
    output.write_all(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n")?;
    let object_count = 2 + pages.len() * 3;
    let mut offsets = vec![0u64; object_count + 1];

    write_object(&mut output, &mut offsets, 1, |out| {
        out.write_all(b"<< /Type /Catalog /Pages 2 0 R >>\n")
    })?;
    write_object(&mut output, &mut offsets, 2, |out| {
        write!(out, "<< /Type /Pages /Count {} /Kids [", pages.len())?;
        for index in 0..pages.len() {
            write!(out, "{} 0 R ", 3 + index * 3)?;
        }
        out.write_all(b"] >>\n")
    })?;

    for (index, page) in pages.iter().enumerate() {
        check_cancel(cancel)?;
        let page_object = 3 + index * 3;
        let image_object = page_object + 1;
        let content_object = page_object + 2;
        let width_points = f64::from(page.width) * 72.0 / page.resolution;
        let height_points = f64::from(page.height) * 72.0 / page.resolution;
        write_object(&mut output, &mut offsets, page_object, |out| {
            write!(
                out,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.4} {:.4}] /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>\n",
                width_points, height_points, image_object, content_object
            )
        })?;
        let length = fs::metadata(&page.path)?.len();
        write_object(&mut output, &mut offsets, image_object, |out| {
            write!(
                out,
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                page.width, page.height, length
            )?;
            let mut input = File::open(&page.path)?;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                check_cancel(cancel).map_err(std::io::Error::other)?;
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                out.write_all(&buffer[..read])?;
            }
            out.write_all(b"\nendstream\n")
        })?;
        let command = format!(
            "q {:.4} 0 0 {:.4} 0 0 cm /Im0 Do Q\n",
            width_points, height_points
        );
        write_object(&mut output, &mut offsets, content_object, |out| {
            write!(out, "<< /Length {} >>\nstream\n", command.len())?;
            out.write_all(command.as_bytes())?;
            out.write_all(b"endstream\n")
        })?;
    }
    check_cancel(cancel)?;
    let xref = output.stream_position()?;
    ensure!(
        xref <= 9_999_999_999,
        "PDF exceeds classic cross-reference limit"
    );
    writeln!(output, "xref\n0 {}", object_count + 1)?;
    output.write_all(b"0000000000 65535 f \n")?;
    for offset in offsets.iter().skip(1) {
        ensure!(*offset <= 9_999_999_999, "PDF object offset is too large");
        writeln!(output, "{offset:010} 00000 n ")?;
    }
    write!(
        output,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        object_count + 1,
        xref
    )?;
    output.flush()?;
    output.get_ref().sync_all()?;
    Ok(())
}

fn write_object(
    output: &mut BufWriter<File>,
    offsets: &mut [u64],
    id: usize,
    body: impl FnOnce(&mut BufWriter<File>) -> std::io::Result<()>,
) -> Result<()> {
    offsets[id] = output.stream_position()?;
    writeln!(output, "{id} 0 obj")?;
    body(output)?;
    output.write_all(b"endobj\n")?;
    Ok(())
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let outcome = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(temporary);
    }
    outcome
}

fn sync_package(stage: &Path) -> Result<()> {
    for entry in fs::read_dir(stage.join("images"))? {
        File::open(entry?.path())?.sync_all()?;
    }
    File::open(stage.join("images"))?.sync_all()?;
    for name in ["manifest.json", "content.json"] {
        File::open(stage.join(name))?.sync_all()?;
    }
    if stage.join("pages.pdf").exists() {
        File::open(stage.join("pages.pdf"))?.sync_all()?;
    }
    File::open(stage)?.sync_all()?;
    Ok(())
}

fn publish_file_new(from: &Path, to: &Path) -> Result<()> {
    fs::hard_link(from, to).context("Publishing export without replacing an existing file")?;
    let _ = fs::remove_file(from);
    Ok(())
}

#[cfg(target_os = "linux")]
fn rename_new(from: &Path, to: &Path) -> Result<()> {
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
    // SAFETY: both owned NUL-terminated paths outlive this syscall and libc
    // retains neither pointer. RENAME_NOREPLACE makes publication collision-safe.
    let result = unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 1) };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("Publishing export package");
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn rename_new(from: &Path, to: &Path) -> Result<()> {
    ensure!(!to.exists(), "export destination already exists");
    fs::rename(from, to)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn document(width: u32, height: u32, color: [u8; 4]) -> Document {
        let mut document = Document::new(width, height);
        document.layers[0].image = Some(RgbaImage::from_pixel(width, height, Rgba(color)).into());
        document
    }

    #[test]
    fn pdf_retains_page_order_and_dimensions() {
        let directory = tempfile::tempdir().unwrap();
        let first = document(20, 30, [255, 0, 0, 255]);
        let second = document(40, 50, [0, 0, 255, 255]);
        let pages = [
            ExportPage {
                document: &first,
                metadata: PageExportMetadata::default(),
            },
            ExportPage {
                document: &second,
                metadata: PageExportMetadata::default(),
            },
        ];
        let path = export_pdf(
            &pages,
            directory.path().join("ordered.pdf"),
            PdfExportOptions::default(),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let bytes = fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let first_box = text.find("/MediaBox [0 0 20.0000 30.0000]").unwrap();
        let second_box = text.find("/MediaBox [0 0 40.0000 50.0000]").unwrap();
        assert!(first_box < second_box);
        assert!(text.contains("/Width 20 /Height 30"));
        assert!(text.contains("/Width 40 /Height 50"));
        assert!(text.ends_with("%%EOF\n"));
    }

    #[test]
    fn package_is_atomic_and_collision_safe() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("campaign");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep"), b"important").unwrap();
        let error = ExportPackageWriter::begin(
            &destination,
            1,
            PackageOptions::default(),
            &AtomicBool::new(false),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("already exists"));
        assert_eq!(fs::read(destination.join("keep")).unwrap(), b"important");
    }

    #[test]
    fn cancellation_never_publishes_partial_package() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("campaign");
        let mut writer = ExportPackageWriter::begin(
            &destination,
            1,
            PackageOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let cancel = AtomicBool::new(true);
        assert!(
            writer
                .write_page(
                    &document(8, 8, [1, 2, 3, 255]),
                    PageExportMetadata::default(),
                    &cancel,
                    |_| {},
                )
                .is_err()
        );
        drop(writer);
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
