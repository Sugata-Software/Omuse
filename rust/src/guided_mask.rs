//! Optional, pinned EfficientSAM-Ti inference. Positive points and boxes only.
//! No network, project-specified paths, package installation, or negative labels.
use crate::{asset_library::sha256_hex, onnx_bindings as ort};
use anyhow::{Context, Result, ensure};
use image::{GrayImage, RgbaImage};
use libloading::Library;
use std::{
    ffi::{CStr, OsString, c_char},
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc, Mutex, TryLockError,
        atomic::{AtomicBool, Ordering},
    },
};

pub const MODEL_ID: &str = "efficientsam-ti-d8dbb1ee-v1";
pub const PREPROCESS_ID: &str = "encoded-srgb-grey-alpha-triangle-longedge1024-chw-v1";
pub const MAX_SOURCE_PIXELS: u64 = 16_777_216;
pub const MAX_PROMPTS: usize = 6;
pub const MAX_WORK_EDGE: u32 = 1024;
pub const MAX_ESTIMATED_WORKING_BYTES: u64 = 3 * 1024 * 1024 * 1024;
/// Policy estimate, not a hard RSS promise. Charge known retained owners plus
/// deliberately conservative model/render/work buffers before capturing a job.
pub fn admit_working_set(
    retained: usize,
    width: u32,
    height: u32,
    canvas_width: u32,
    canvas_height: u32,
    layers: usize,
    image_pixels: u64,
    captured_bytes: usize,
) -> Result<u64> {
    let source = u64::from(width)
        .checked_mul(u64::from(height))
        .context("Source size overflow")?;
    let canvas = u64::from(canvas_width)
        .checked_mul(u64::from(canvas_height))
        .context("Canvas size overflow")?;
    ensure!(
        source > 0 && source <= MAX_SOURCE_PIXELS && canvas > 0 && canvas <= MAX_SOURCE_PIXELS,
        "Guided mask review supports source and canvas up to 16 megapixels"
    );
    let (w, h) = work_dimensions(width, height);
    let working = u64::from(w) * u64::from(h);
    let render_factor = u64::try_from(layers)?
        .checked_mul(24)
        .and_then(|n| n.checked_add(12))
        .context("Render estimate overflow")?;
    let terms = [
        retained as u64,
        captured_bytes as u64,
        1024 * 1024 * 1024,
        working
            .checked_mul(32)
            .context("Working mask estimate overflow")?,
        8 * 1024 * 1024,
        source
            .checked_mul(10)
            .context("Prepared mask estimate overflow")?,
        canvas
            .checked_mul(render_factor)
            .context("Render estimate overflow")?,
        image_pixels
            .checked_mul(16)
            .context("Image conversion estimate overflow")?,
        canvas
            .checked_mul(12)
            .context("Display estimate overflow")?,
        64 * 1024 * 1024,
    ];
    let total = terms
        .into_iter()
        .try_fold(0_u64, |a, b| a.checked_add(b))
        .context("Working set estimate overflow")?;
    ensure!(
        total <= MAX_ESTIMATED_WORKING_BYTES,
        "This document and its history exceed the experimental local-mask working budget. Try a smaller photo document."
    );
    Ok(total)
}
const EMBEDDING_SHAPE: [i64; 4] = [1, 256, 64, 64];
pub const ENCODER_FILE: &str = "efficientsam_ti_encoder.onnx";
pub const DECODER_FILE: &str = "efficientsam_ti_decoder.onnx";
pub const ENCODER_BYTES: u64 = 24_799_761;
pub const DECODER_BYTES: u64 = 16_565_728;
pub const ENCODER_SHA256: &str = "84ed466ffcc5c1f8d08409bc34a23bb364ab2c15e402cb12d4335a42be0e0951";
pub const DECODER_SHA256: &str = "a62f8fa5ea080447c0689418d69e58f1e83e0b7adf9c142e2bd9bcc8045c0b11";

/// Captured once at review start. Do not rebuild or hash source pixels in render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceKey {
    pub editor: u64,
    pub page: u64,
    pub revision: u64,
    pub layer: String,
    pub width: u32,
    pub height: u32,
    pub pixels_sha256: String,
}
impl SourceKey {
    pub fn capture(
        editor: u64,
        page: u64,
        revision: u64,
        layer: String,
        image: &RgbaImage,
    ) -> Result<Self> {
        validate_source(image)?;
        Ok(Self {
            editor,
            page,
            revision,
            layer,
            width: image.width(),
            height: image.height(),
            pixels_sha256: sha256_hex(image.as_raw()),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prompt {
    Positive {
        x: f32,
        y: f32,
    },
    Box {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    },
}
pub fn validate_prompts(prompts: &[Prompt], width: u32, height: u32) -> Result<()> {
    ensure!(width > 0 && height > 0, "The source is empty");
    let mut count = 0;
    let mut boxes = 0;
    let point = |x: f32, y: f32| {
        x.is_finite()
            && y.is_finite()
            && x >= 0.
            && y >= 0.
            && x < width as f32
            && y < height as f32
    };
    for p in prompts {
        match *p {
            Prompt::Positive { x, y } => {
                ensure!(point(x, y), "A positive point is outside the source");
                count += 1;
            }
            Prompt::Box {
                left,
                top,
                right,
                bottom,
            } => {
                ensure!(
                    point(left, top) && point(right, bottom) && right > left && bottom > top,
                    "Draw a nonempty box inside the image"
                );
                boxes += 1;
                count += 2;
            }
        }
    }
    ensure!(boxes <= 1, "Use one box, then add positive points");
    ensure!(
        (1..=MAX_PROMPTS).contains(&count),
        "Use one to six points; a box counts as two"
    );
    Ok(())
}
pub fn prompt_count(prompts: &[Prompt]) -> usize {
    prompts
        .iter()
        .map(|p| {
            if matches!(p, Prompt::Box { .. }) {
                2
            } else {
                1
            }
        })
        .sum()
}

#[derive(Clone, Debug)]
pub struct Alternative {
    /// Working-resolution coverage, not a hard source crop or edited image.
    pub mask: Arc<GrayImage>,
    pub estimated_iou: f32,
}
#[derive(Clone, Debug)]
pub struct Prediction {
    pub source: SourceKey,
    pub alternatives: Vec<Alternative>,
    pub embedding_reused: bool,
    /// Prepared off the UI thread, with fallible bounded allocation.
    pub initial_mask: Arc<GrayImage>,
}

fn logit_coverage(logit: f32) -> u8 {
    if logit >= 0. { 255 } else { 0 }
}

pub fn prepare_source_mask(
    mask: &GrayImage,
    width: u32,
    height: u32,
    cancel: &AtomicBool,
) -> Result<Arc<GrayImage>> {
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_SOURCE_PIXELS,
        "Guided mask exceeds the source pixel budget"
    );
    ensure!(
        mask.width() > 0
            && mask.height() > 0
            && u64::from(mask.width()) * u64::from(mask.height())
                <= u64::from(MAX_WORK_EDGE).pow(2),
        "Invalid working mask dimensions"
    );
    ensure!(
        !cancel.load(Ordering::Acquire),
        "Mask preparation cancelled"
    );
    let count = usize::try_from(u64::from(width) * u64::from(height))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(count)
        .context("Not enough memory for the full-resolution editable mask")?;
    for y in 0..height {
        ensure!(
            !cancel.load(Ordering::Acquire),
            "Mask preparation cancelled"
        );
        let sy = ((f64::from(y) + 0.5) * f64::from(mask.height()) / f64::from(height) - 0.5)
            .clamp(0., f64::from(mask.height() - 1));
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(mask.height() - 1);
        let fy = sy - f64::from(y0);
        for x in 0..width {
            let sx = ((f64::from(x) + 0.5) * f64::from(mask.width()) / f64::from(width) - 0.5)
                .clamp(0., f64::from(mask.width() - 1));
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(mask.width() - 1);
            let fx = sx - f64::from(x0);
            let top = f64::from(mask.get_pixel(x0, y0)[0]) * (1. - fx)
                + f64::from(mask.get_pixel(x1, y0)[0]) * fx;
            let bottom = f64::from(mask.get_pixel(x0, y1)[0]) * (1. - fx)
                + f64::from(mask.get_pixel(x1, y1)[0]) * fx;
            bytes.push((top * (1. - fy) + bottom * fy).round().clamp(0., 255.) as u8);
        }
    }
    ensure!(
        !cancel.load(Ordering::Acquire),
        "Mask preparation cancelled"
    );
    Ok(Arc::new(
        GrayImage::from_raw(width, height, bytes).context("Prepared mask dimensions mismatch")?,
    ))
}

/// File locations are application-managed, never taken from an .omuse document.
/// The application does not download the optional pack automatically.
#[derive(Clone, Debug)]
pub struct ManagedPack {
    directory: PathBuf,
    runtime: PathBuf,
}
impl ManagedPack {
    pub fn installed() -> Result<Self> {
        let exe = std::env::current_exe().context("Locating Omuse")?;
        // Same trusted user/launcher override as the established subject
        // detector. identity also handles its existing legacy alias. A project
        // never supplies an environment variable or runtime path.
        let runtime = resolve_runtime(&exe, crate::identity::env_var_os("OMUSE_ONNX_RUNTIME"))?;
        Ok(Self {
            directory: crate::identity::data_dir().join("models").join(MODEL_ID),
            runtime,
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn available(&self) -> bool {
        self.runtime_available() && self.weights_present()
    }
    pub fn runtime_available(&self) -> bool {
        self.runtime.is_file()
    }
    pub fn weights_present(&self) -> bool {
        self.directory.join(ENCODER_FILE).is_file() && self.directory.join(DECODER_FILE).is_file()
    }
    /// Isolated development evidence only; do not expose path selection in UI or projects.
    #[cfg(test)]
    fn development(directory: PathBuf, runtime: PathBuf) -> Self {
        Self { directory, runtime }
    }
}
fn resolve_runtime(executable: &Path, trusted_override: Option<OsString>) -> Result<PathBuf> {
    if let Some(path) = trusted_override {
        ensure!(!path.is_empty(), "OMUSE_ONNX_RUNTIME is empty");
        return Ok(path.into());
    }
    // Installed bundle launchers use OMUSE_ONNX_RUNTIME for sibling bin/lib.
    // Preserve the existing executable-adjacent fallback for unpacked layouts.
    Ok(executable
        .parent()
        .context("Omuse has no application folder")?
        .join(if cfg!(windows) {
            "lib/onnxruntime.dll"
        } else {
            "lib/libonnxruntime.so"
        }))
}

struct ActiveRun {
    runtime: Arc<Runtime>,
    options: *mut ort::OrtRunOptions,
}
// Access to the run-options pointer and its release is serialized by active's mutex.
// ORT RunOptionsSetTerminate is designed to terminate an in-progress Run.
unsafe impl Send for ActiveRun {}
#[derive(Default)]
pub struct Cancellation {
    stopped: AtomicBool,
    active: Mutex<Option<ActiveRun>>,
}
impl Cancellation {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn is_cancelled(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.stopped.store(true, Ordering::Release);
        {
            let active = self
                .active
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(active) = active.as_ref() {
                // Cancellation must not panic or turn an error into an apply.
                unsafe {
                    let api = &*active.runtime.api;
                    let status = (api.RunOptionsSetTerminate.unwrap())(active.options);
                    if !status.is_null() {
                        (api.ReleaseStatus.unwrap())(status);
                    }
                }
            }
        }
    }
    fn check(&self) -> Result<()> {
        ensure!(!self.is_cancelled(), "Guided mask cancelled");
        Ok(())
    }
}
struct RunGuard<'a> {
    token: &'a Cancellation,
}
impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        {
            let mut slot = self
                .token
                .active
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(active) = slot.take() {
                unsafe {
                    ((*active.runtime.api).ReleaseRunOptions.unwrap())(active.options);
                }
            }
        }
    }
}

/// At most one active caller; try_lock refuses additional work rather than
/// queueing sources. One model pair and one 4MiB embedding are retained.
/// Admission limits bound allocations; they are not a hard process-RSS limit.
pub struct Worker {
    pack: ManagedPack,
    engine: Mutex<Option<Engine>>,
}
impl Worker {
    pub fn new(pack: ManagedPack) -> Self {
        Self {
            pack,
            engine: Mutex::new(None),
        }
    }
    pub fn predict(
        &self,
        source: &SourceKey,
        image: &RgbaImage,
        prompts: &[Prompt],
        cancel: &Cancellation,
    ) -> Result<Prediction> {
        validate_source(image)?;
        validate_prompts(prompts, image.width(), image.height())?;
        ensure!(
            source.width == image.width() && source.height == image.height(),
            "The source dimensions changed"
        );
        cancel.check()?;
        // Check hash outside the engine lock, but before admission to inference.
        ensure!(
            sha256_hex(image.as_raw()) == source.pixels_sha256,
            "The source pixels changed"
        );
        let mut lock = match self.engine.try_lock() {
            Ok(v) => v,
            Err(TryLockError::WouldBlock) => {
                anyhow::bail!("The local guided-mask worker is still stopping; try again shortly")
            }
            Err(TryLockError::Poisoned(_)) => {
                anyhow::bail!("The local guided-mask worker failed; reopen Omuse")
            }
        };
        cancel.check()?;
        if lock.is_none() {
            *lock = Some(Engine::load(&self.pack, cancel)?);
        }
        lock.as_mut()
            .unwrap()
            .predict(source, image, prompts, cancel)
    }
    /// Called on source close/change. Refuses concurrent cache mutation; the
    /// next different SourceKey also replaces the cache before use.
    pub fn clear_idle_cache(&self) {
        if let Ok(mut engine) = self.engine.try_lock() {
            if let Some(engine) = engine.as_mut() {
                engine.cache = None;
            }
        }
    }
}

fn validate_source(image: &RgbaImage) -> Result<()> {
    ensure!(
        image.width() > 0
            && image.height() > 0
            && u64::from(image.width()) * u64::from(image.height()) <= MAX_SOURCE_PIXELS,
        "Guided masks support image layers up to 16 megapixels"
    );
    ensure!(
        image.width() <= 65_535 && image.height() <= 65_535,
        "The source edge exceeds the guided-mask limit"
    );
    Ok(())
}
pub fn work_dimensions(width: u32, height: u32) -> (u32, u32) {
    let edge = width.max(height);
    if edge <= MAX_WORK_EDGE {
        return (width, height);
    }
    let scale = f64::from(MAX_WORK_EDGE) / f64::from(edge);
    (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    )
}
fn preprocess(image: &RgbaImage, cancel: &Cancellation) -> Result<(Vec<f32>, u32, u32)> {
    cancel.check()?;
    let (width, height) = work_dimensions(image.width(), image.height());
    let resized =
        image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle);
    cancel.check()?;
    let plane = width as usize * height as usize;
    let mut input = vec![0.; plane * 3];
    for (i, pixel) in resized.pixels().enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        let alpha = f32::from(pixel[3]) / 255.;
        for c in 0..3 {
            input[c * plane + i] = (f32::from(pixel[c]) / 255.) * alpha + 0.5 * (1. - alpha);
        }
    }
    Ok((input, width, height))
}

fn read_verified(
    path: &Path,
    expected_bytes: u64,
    digest: &str,
    cancel: &Cancellation,
) -> Result<Vec<u8>> {
    cancel.check()?;
    ensure!(
        !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
        "A model pack file must be an ordinary managed file"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x800);
    }
    let mut file: File = options
        .open(path)
        .with_context(|| format!("Optional guided-mask pack unavailable: {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() == expected_bytes,
        "The guided-mask model has the wrong type or byte length"
    );
    let mut data = Vec::with_capacity(expected_bytes as usize);
    let mut block = [0_u8; 65_536];
    loop {
        cancel.check()?;
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        ensure!(
            data.len()
                .checked_add(n)
                .is_some_and(|n| n <= expected_bytes as usize),
            "The guided-mask model grew while reading"
        );
        data.extend_from_slice(&block[..n]);
    }
    ensure!(
        data.len() as u64 == expected_bytes && sha256_hex(&data) == digest,
        "Guided-mask model verification failed; do not use this pack"
    );
    cancel.check()?;
    Ok(data)
}

struct Runtime {
    api: *const ort::OrtApi,
    env: *mut ort::OrtEnv,
    memory: *mut ort::OrtMemoryInfo,
    _library: Library,
}
// API table is immutable. ORT environment/CPU memory info and RunOptions terminate
// are thread-safe; sessions are only called inside Worker::engine's mutex.
unsafe impl Send for Runtime {}
unsafe impl Sync for Runtime {}
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let a = &*self.api;
            if !self.memory.is_null() {
                (a.ReleaseMemoryInfo.unwrap())(self.memory);
            }
            if !self.env.is_null() {
                (a.ReleaseEnv.unwrap())(self.env);
            }
        }
    }
}
impl Runtime {
    fn load(path: &Path) -> Result<Arc<Self>> {
        ensure!(path.is_file(), "The bundled ONNX Runtime is unavailable");
        let library = unsafe { Library::new(path) }.context("Loading bundled ONNX Runtime")?;
        let get: libloading::Symbol<'_, unsafe extern "C" fn() -> *const ort::OrtApiBase> =
            unsafe { library.get(b"OrtGetApiBase\0") }?;
        let base = unsafe { get() };
        ensure!(!base.is_null(), "ONNX Runtime returned no base");
        let api = unsafe { ((*base).GetApi.context("Missing ORT GetApi")?)(ort::ORT_API_VERSION) };
        ensure!(!api.is_null(), "ONNX Runtime API23 is unavailable");
        let mut result = Self {
            api,
            env: ptr::null_mut(),
            memory: ptr::null_mut(),
            _library: library,
        };
        unsafe {
            let a = &*api;
            check(
                api,
                (a.CreateEnv.context("Missing ORT CreateEnv")?)(
                    3,
                    c"omuse-guided-mask".as_ptr(),
                    &mut result.env,
                ),
            )?;
            check(
                api,
                (a.CreateCpuMemoryInfo.context("Missing ORT CPU memory")?)(
                    1,
                    0,
                    &mut result.memory,
                ),
            )?;
        }
        Ok(Arc::new(result))
    }
}
struct Session {
    runtime: Arc<Runtime>,
    value: *mut ort::OrtSession,
}
unsafe impl Send for Session {}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            ((*self.runtime.api).ReleaseSession.unwrap())(self.value);
        }
    }
}
struct Value {
    runtime: Arc<Runtime>,
    value: *mut ort::OrtValue,
}
unsafe impl Send for Value {}
impl Drop for Value {
    fn drop(&mut self) {
        unsafe {
            ((*self.runtime.api).ReleaseValue.unwrap())(self.value);
        }
    }
}
struct Options {
    runtime: Arc<Runtime>,
    value: *mut ort::OrtSessionOptions,
}
impl Drop for Options {
    fn drop(&mut self) {
        unsafe {
            ((*self.runtime.api).ReleaseSessionOptions.unwrap())(self.value);
        }
    }
}
struct TensorInfo {
    runtime: Arc<Runtime>,
    value: *mut ort::OrtTensorTypeAndShapeInfo,
}
impl Drop for TensorInfo {
    fn drop(&mut self) {
        unsafe {
            ((*self.runtime.api).ReleaseTensorTypeAndShapeInfo.unwrap())(self.value);
        }
    }
}

fn check(api: *const ort::OrtApi, status: ort::OrtStatusPtr) -> Result<()> {
    if status.is_null() {
        return Ok(());
    }
    unsafe {
        let message = CStr::from_ptr(((*api).GetErrorMessage.unwrap())(status))
            .to_string_lossy()
            .into_owned();
        ((*api).ReleaseStatus.unwrap())(status);
        anyhow::bail!("Local guided-mask inference: {message}")
    }
}
fn dimensions(
    runtime: &Arc<Runtime>,
    info: *const ort::OrtTensorTypeAndShapeInfo,
) -> Result<(u32, Vec<i64>)> {
    let a = unsafe { &*runtime.api };
    let mut ty = 0;
    let mut rank = 0;
    unsafe {
        check(
            runtime.api,
            (a.GetTensorElementType.unwrap())(info, &mut ty),
        )?;
        check(
            runtime.api,
            (a.GetDimensionsCount.unwrap())(info, &mut rank),
        )?;
    }
    ensure!(rank <= 5, "Unexpected guided-mask tensor rank");
    let mut shape = vec![0; rank];
    unsafe {
        check(
            runtime.api,
            (a.GetDimensions.unwrap())(info, shape.as_mut_ptr(), rank),
        )?;
    }
    Ok((ty, shape))
}
fn session(runtime: &Arc<Runtime>, bytes: &[u8]) -> Result<Session> {
    let a = unsafe { &*runtime.api };
    let mut raw = ptr::null_mut();
    unsafe {
        check(runtime.api, (a.CreateSessionOptions.unwrap())(&mut raw))?;
    }
    let options = Options {
        runtime: runtime.clone(),
        value: raw,
    };
    unsafe {
        check(
            runtime.api,
            (a.SetIntraOpNumThreads.unwrap())(options.value, 2),
        )?;
        check(
            runtime.api,
            (a.SetInterOpNumThreads.unwrap())(options.value, 1),
        )?;
    }
    let mut value = ptr::null_mut();
    unsafe {
        check(
            runtime.api,
            (a.CreateSessionFromArray.unwrap())(
                runtime.env,
                bytes.as_ptr().cast(),
                bytes.len(),
                options.value,
                &mut value,
            ),
        )?;
    }
    ensure!(!value.is_null(), "ONNX Runtime returned no session");
    Ok(Session {
        runtime: runtime.clone(),
        value,
    })
}
fn validate_schema(
    s: &Session,
    inputs: &[(&str, u32, &[i64])],
    outputs: &[(&str, u32, &[i64])],
) -> Result<()> {
    let runtime = &s.runtime;
    let a = unsafe { &*runtime.api };
    let mut allocator = ptr::null_mut();
    unsafe {
        check(
            runtime.api,
            (a.GetAllocatorWithDefaultOptions.unwrap())(&mut allocator),
        )?;
    }
    for (output, expected) in [(false, inputs), (true, outputs)] {
        let mut count = 0;
        unsafe {
            check(
                runtime.api,
                if output {
                    (a.SessionGetOutputCount.unwrap())(s.value, &mut count)
                } else {
                    (a.SessionGetInputCount.unwrap())(s.value, &mut count)
                },
            )?;
        }
        ensure!(
            count == expected.len(),
            "Unexpected guided-mask model input/output count"
        );
        for (i, (expected_name, ty, shape)) in expected.iter().enumerate() {
            let mut raw_name: *mut c_char = ptr::null_mut();
            unsafe {
                check(
                    runtime.api,
                    if output {
                        (a.SessionGetOutputName.unwrap())(s.value, i, allocator, &mut raw_name)
                    } else {
                        (a.SessionGetInputName.unwrap())(s.value, i, allocator, &mut raw_name)
                    },
                )?;
            }
            ensure!(!raw_name.is_null(), "The guided-mask tensor has no name");
            let name = unsafe { CStr::from_ptr(raw_name).to_string_lossy().into_owned() };
            unsafe {
                check(
                    runtime.api,
                    (a.AllocatorFree.unwrap())(allocator, raw_name.cast()),
                )?;
            }
            ensure!(
                name == *expected_name,
                "Unexpected guided-mask tensor name: {name}"
            );
            let mut raw_info = ptr::null_mut();
            unsafe {
                check(
                    runtime.api,
                    if output {
                        (a.SessionGetOutputTypeInfo.unwrap())(s.value, i, &mut raw_info)
                    } else {
                        (a.SessionGetInputTypeInfo.unwrap())(s.value, i, &mut raw_info)
                    },
                )?;
            }
            let mut tensor = ptr::null();
            let result = (|| {
                unsafe {
                    check(
                        runtime.api,
                        (a.CastTypeInfoToTensorInfo.unwrap())(raw_info, &mut tensor),
                    )?;
                }
                ensure!(!tensor.is_null(), "Guided-mask tensor type unavailable");
                dimensions(runtime, tensor)
            })();
            unsafe {
                (a.ReleaseTypeInfo.unwrap())(raw_info);
            }
            let (actual_ty, actual_shape) = result?;
            ensure!(
                actual_ty == *ty && actual_shape == *shape,
                "Unexpected guided-mask tensor type/dimensions for {name}"
            );
        }
    }
    Ok(())
}
/// A borrowed tensor cannot outlive its backing slice or move to another job.
struct InputValue<'a> {
    owned: Value,
    _borrow: std::marker::PhantomData<&'a mut [u8]>,
}
fn tensor<'a, T>(
    runtime: &Arc<Runtime>,
    data: &'a mut [T],
    shape: &[i64],
    ty: u32,
) -> Result<InputValue<'a>> {
    let count = shape
        .iter()
        .try_fold(1usize, |n, &d| n.checked_mul(usize::try_from(d).ok()?))
        .context("Invalid guided-mask input shape")?;
    ensure!(count == data.len(), "Guided-mask input length mismatch");
    let mut value = ptr::null_mut();
    unsafe {
        check(
            runtime.api,
            ((*runtime.api).CreateTensorWithDataAsOrtValue.unwrap())(
                runtime.memory,
                data.as_mut_ptr().cast(),
                std::mem::size_of_val(data),
                shape.as_ptr(),
                shape.len(),
                ty,
                &mut value,
            ),
        )?;
    }
    Ok(InputValue {
        owned: Value {
            runtime: runtime.clone(),
            value,
        },
        _borrow: std::marker::PhantomData,
    })
}
fn run(
    s: &Session,
    names: &[&CStr],
    inputs: &[*const ort::OrtValue],
    outputs: &[&CStr],
    cancel: &Cancellation,
) -> Result<Vec<Value>> {
    cancel.check()?;
    let runtime = &s.runtime;
    let a = unsafe { &*runtime.api };
    let mut options = ptr::null_mut();
    unsafe {
        check(runtime.api, (a.CreateRunOptions.unwrap())(&mut options))?;
    }
    {
        let mut active = cancel
            .active
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if active.is_some() {
            unsafe {
                (a.ReleaseRunOptions.unwrap())(options);
            }
            anyhow::bail!("Guided-mask token already running");
        }
        *active = Some(ActiveRun {
            runtime: runtime.clone(),
            options,
        });
        if cancel.is_cancelled() {
            unsafe {
                let status = (a.RunOptionsSetTerminate.unwrap())(options);
                if !status.is_null() {
                    (a.ReleaseStatus.unwrap())(status);
                }
            }
        }
    }
    let _guard = RunGuard { token: cancel };
    let names = names.iter().map(|n| n.as_ptr()).collect::<Vec<_>>();
    let output_names = outputs.iter().map(|n| n.as_ptr()).collect::<Vec<_>>();
    let mut raw = vec![ptr::null_mut(); outputs.len()];
    let status = unsafe {
        (a.Run.unwrap())(
            s.value,
            options,
            names.as_ptr(),
            inputs.as_ptr(),
            inputs.len(),
            output_names.as_ptr(),
            output_names.len(),
            raw.as_mut_ptr(),
        )
    };
    let result = raw
        .into_iter()
        .filter(|v| !v.is_null())
        .map(|value| Value {
            runtime: runtime.clone(),
            value,
        })
        .collect::<Vec<_>>();
    check(runtime.api, status)?;
    cancel.check()?;
    ensure!(result.len() == outputs.len(), "Missing guided-mask output");
    Ok(result)
}
fn floats(value: &Value, expected: &[i64]) -> Result<Vec<f32>> {
    let runtime = &value.runtime;
    let a = unsafe { &*runtime.api };
    let mut info = ptr::null_mut();
    unsafe {
        check(
            runtime.api,
            (a.GetTensorTypeAndShape.unwrap())(value.value, &mut info),
        )?;
    }
    let info = TensorInfo {
        runtime: runtime.clone(),
        value: info,
    };
    let (ty, shape) = dimensions(runtime, info.value)?;
    ensure!(
        ty == 1 && shape == expected,
        "Unexpected guided-mask output shape/type"
    );
    let count = expected
        .iter()
        .try_fold(1usize, |n, &d| n.checked_mul(usize::try_from(d).ok()?))
        .context("Guided-mask output overflow")?;
    ensure!(
        count <= 3 * 1024 * 1024,
        "Guided-mask output exceeds its admitted byte budget"
    );
    let mut data = ptr::null_mut();
    unsafe {
        check(
            runtime.api,
            (a.GetTensorMutableData.unwrap())(value.value, &mut data),
        )?;
    }
    ensure!(!data.is_null(), "Guided-mask output has no data");
    let values = unsafe { std::slice::from_raw_parts(data.cast::<f32>(), count) };
    ensure!(
        values.iter().all(|n| n.is_finite()),
        "The guided-mask model returned nonfinite values"
    );
    Ok(values.to_vec())
}
struct Cached {
    source: SourceKey,
    width: u32,
    height: u32,
    embedding: Value,
}
struct Engine {
    encoder: Session,
    decoder: Session,
    cache: Option<Cached>,
}
impl Engine {
    fn load(pack: &ManagedPack, cancel: &Cancellation) -> Result<Self> {
        // A directory symlink is rejected too. Trusted runtime location comes
        // only from the executing application; project metadata never selects it.
        ensure!(
            std::fs::symlink_metadata(&pack.directory)?.is_dir()
                && !std::fs::symlink_metadata(&pack.directory)?
                    .file_type()
                    .is_symlink(),
            "Install the optional verified guided-mask pack in its managed folder"
        );
        let encoder = read_verified(
            &pack.directory.join(ENCODER_FILE),
            ENCODER_BYTES,
            ENCODER_SHA256,
            cancel,
        )?;
        let decoder = read_verified(
            &pack.directory.join(DECODER_FILE),
            DECODER_BYTES,
            DECODER_SHA256,
            cancel,
        )?;
        let runtime = Runtime::load(&pack.runtime)?;
        let encoder = session(&runtime, &encoder)?;
        cancel.check()?;
        let decoder = session(&runtime, &decoder)?;
        cancel.check()?;
        validate_schema(
            &encoder,
            &[("batched_images", 1, &[-1, 3, -1, -1])],
            &[("image_embeddings", 1, &[-1, 256, -1, -1])],
        )?;
        validate_schema(
            &decoder,
            &[
                ("image_embeddings", 1, &[-1, 256, 64, 64]),
                ("batched_point_coords", 1, &[1, 1, -1, 2]),
                ("batched_point_labels", 1, &[1, 1, -1]),
                ("orig_im_size", 7, &[2]),
            ],
            &[
                ("output_masks", 1, &[-1, -1, -1, -1, -1]),
                ("iou_predictions", 1, &[1, 1, -1]),
                ("onnx::Shape_1830", 1, &[-1, -1, -1, -1]),
            ],
        )?;
        Ok(Self {
            encoder,
            decoder,
            cache: None,
        })
    }
    fn predict(
        &mut self,
        source: &SourceKey,
        image: &RgbaImage,
        prompts: &[Prompt],
        cancel: &Cancellation,
    ) -> Result<Prediction> {
        let reused = self.cache.as_ref().is_some_and(|c| c.source == *source);
        if !reused {
            self.cache = None;
            let (mut input, width, height) = preprocess(image, cancel)?;
            let tensor = tensor(
                &self.encoder.runtime,
                &mut input,
                &[1, 3, i64::from(height), i64::from(width)],
                1,
            )?;
            let mut outputs = run(
                &self.encoder,
                &[c"batched_images"],
                &[tensor.owned.value],
                &[c"image_embeddings"],
                cancel,
            )?;
            let embedding = outputs.remove(0);
            // Validate all floats before admitting this embedding to the cache.
            drop(floats(&embedding, &EMBEDDING_SHAPE)?);
            cancel.check()?;
            self.cache = Some(Cached {
                source: source.clone(),
                width,
                height,
                embedding,
            });
        }
        let cached = self
            .cache
            .as_ref()
            .context("Guided-mask embedding unavailable")?;
        let sx = cached.width as f32 / source.width as f32;
        let sy = cached.height as f32 / source.height as f32;
        let mut coords = Vec::with_capacity(12);
        let mut labels = Vec::with_capacity(6);
        for prompt in prompts {
            match *prompt {
                Prompt::Positive { x, y } => {
                    coords.extend([x * sx, y * sy]);
                    labels.push(1_f32);
                }
                Prompt::Box {
                    left,
                    top,
                    right,
                    bottom,
                } => {
                    coords.extend([left * sx, top * sy, right * sx, bottom * sy]);
                    labels.extend([2_f32, 3_f32]);
                }
            }
        }
        let n = labels.len() as i64;
        let mut size = [i64::from(cached.height), i64::from(cached.width)];
        let rt = &self.decoder.runtime;
        let points = tensor(rt, &mut coords, &[1, 1, n, 2], 1)?;
        let labels = tensor(rt, &mut labels, &[1, 1, n], 1)?;
        let size = tensor(rt, &mut size, &[2], 7)?;
        let output = run(
            &self.decoder,
            &[
                c"image_embeddings",
                c"batched_point_coords",
                c"batched_point_labels",
                c"orig_im_size",
            ],
            &[
                cached.embedding.value,
                points.owned.value,
                labels.owned.value,
                size.owned.value,
            ],
            &[c"output_masks", c"iou_predictions", c"onnx::Shape_1830"],
            cancel,
        )?;
        let logits = floats(
            &output[0],
            &[1, 1, 3, i64::from(cached.height), i64::from(cached.width)],
        )?;
        let iou = floats(&output[1], &[1, 1, 3])?;
        drop(floats(&output[2], &[1, 3, 256, 256])?);
        cancel.check()?;
        let plane = cached.width as usize * cached.height as usize;
        let mut alternatives = Vec::with_capacity(3);
        for i in 0..3 {
            let mut mask = Vec::with_capacity(plane);
            for (n, &logit) in logits[i * plane..(i + 1) * plane].iter().enumerate() {
                if n % 4096 == 0 {
                    cancel.check()?;
                }
                // Author pipeline: sigmoid(logits) >= 0.5, equivalently a
                // zero-logit foreground boundary. Model confidence is not matte
                // opacity. Source upsampling may soften only geometric edges.
                mask.push(logit_coverage(logit));
            }
            alternatives.push(Alternative {
                mask: Arc::new(
                    GrayImage::from_raw(cached.width, cached.height, mask)
                        .context("Guided-mask dimensions mismatch")?,
                ),
                estimated_iou: iou[i],
            });
        }
        alternatives.sort_by(|a, b| b.estimated_iou.total_cmp(&a.estimated_iou));
        let initial_mask = prepare_source_mask(
            &alternatives[0].mask,
            source.width,
            source.height,
            &cancel.stopped,
        )?;
        cancel.check()?;
        Ok(Prediction {
            source: source.clone(),
            alternatives,
            embedding_reused: reused,
            initial_mask,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_accounts_for_orphan_work_and_overflow_without_claiming_rss() {
        assert!(admit_working_set(1024, 8, 8, 8, 8, 1, 64, 0).is_ok());
        assert!(admit_working_set(1024, 8, 8, 8, 8, 1, 64, 4 * 1024 * 1024 * 1024_usize).is_err());
        assert!(admit_working_set(1024, 8, 8, 8, 8, usize::MAX, u64::MAX, 0).is_err());
    }
    #[test]
    fn runtime_resolution_uses_the_trusted_launcher_override_without_global_env_mutation() {
        let executable = Path::new("bundle/bin/omuse");
        let expected = PathBuf::from("bundle/lib/onnxruntime-library");
        assert_eq!(
            resolve_runtime(executable, Some(expected.clone().into_os_string())).unwrap(),
            expected
        );
        assert!(resolve_runtime(executable, Some(OsString::new())).is_err());
        assert_eq!(
            resolve_runtime(executable, None).unwrap(),
            PathBuf::from("bundle/bin").join(if cfg!(windows) {
                "lib/onnxruntime.dll"
            } else {
                "lib/libonnxruntime.so"
            })
        );
    }
    #[test]
    fn author_zero_logit_boundary_is_coverage_not_model_confidence_alpha() {
        assert_eq!(
            [-100., -1., -0.000_001, -0., 0., 0.000_001, 1., 100.].map(logit_coverage),
            [0, 0, 0, 255, 255, 255, 255, 255]
        );
        let working =
            GrayImage::from_raw(2, 1, vec![logit_coverage(-0.1), logit_coverage(0.1)]).unwrap();
        let source = prepare_source_mask(&working, 4, 1, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            source.as_raw(),
            &[0, 64, 191, 255],
            "Only bilinear geometric boundary reconstruction creates partial coverage; foreground/background remain opaque/empty"
        );
    }
    #[test]
    fn admission_is_explicit_and_no_negative_model_label_exists() {
        let p = Prompt::Positive { x: 20., y: 10. };
        assert!(validate_prompts(&[p; 6], 128, 96).is_ok());
        assert!(validate_prompts(&[p; 7], 128, 96).is_err());
        assert!(validate_prompts(&[Prompt::Positive { x: f32::NAN, y: 1. }], 128, 96).is_err());
        assert!(
            validate_prompts(
                &[Prompt::Box {
                    left: 1.,
                    top: 1.,
                    right: 1.,
                    bottom: 30.
                }],
                128,
                96
            )
            .is_err()
        );
        assert!(
            validate_prompts(
                &[
                    Prompt::Box {
                        left: 1.,
                        top: 1.,
                        right: 50.,
                        bottom: 30.
                    },
                    p,
                    p,
                    p,
                    p
                ],
                128,
                96
            )
            .is_ok()
        );
    }
    #[test]
    fn work_admission_and_preprocess_preserve_geometry_and_hide_transparent_rgb() {
        assert_eq!(work_dimensions(4000, 2000), (1024, 512));
        let image = RgbaImage::from_pixel(2, 1, image::Rgba([255, 0, 180, 0]));
        let (v, w, h) = preprocess(&image, &Cancellation::default()).unwrap();
        assert_eq!((w, h), (2, 1));
        assert!(v.iter().all(|&v| v == 0.5));
    }
    #[test]
    fn bad_pack_and_precancel_are_refused_before_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wrong.onnx");
        std::fs::write(&path, b"wrong").unwrap();
        assert!(
            read_verified(
                &path,
                ENCODER_BYTES,
                ENCODER_SHA256,
                &Cancellation::default()
            )
            .is_err()
        );
        let token = Cancellation::default();
        token.cancel();
        assert!(token.check().is_err());
    }
    #[test]
    #[ignore = "Requires the separately verified optional model pack; never downloads"]
    fn verified_pack_positive_box_reuse_and_cancellation() {
        let path = std::env::var_os("OMUSE_GUIDED_TEST_PACK").expect("isolated test pack path");
        let runtime =
            std::env::var_os("OMUSE_GUIDED_TEST_RUNTIME").expect("existing bundled runtime path");
        let worker = Worker::new(ManagedPack::development(path.into(), runtime.into()));
        let image = RgbaImage::from_fn(128, 96, |x, y| {
            image::Rgba(if (32..96).contains(&x) && (24..72).contains(&y) {
                [230, 56, 31, 255]
            } else {
                [26, 51, 107, 255]
            })
        });
        let key = SourceKey::capture(1, 1, 1, "source".into(), &image).unwrap();
        let a = worker
            .predict(
                &key,
                &image,
                &[Prompt::Positive { x: 64., y: 48. }],
                &Cancellation::default(),
            )
            .unwrap();
        assert!(!a.embedding_reused);
        assert_eq!(a.alternatives.len(), 3);
        let b = worker
            .predict(
                &key,
                &image,
                &[Prompt::Box {
                    left: 32.,
                    top: 24.,
                    right: 96.,
                    bottom: 72.,
                }],
                &Cancellation::default(),
            )
            .unwrap();
        assert!(b.embedding_reused);
        assert!(b.alternatives[0].mask.get_pixel(64, 48)[0] > 200);
        assert!(b.alternatives[0].mask.get_pixel(8, 8)[0] < 20);
        let token = Cancellation::new();
        token.cancel();
        assert!(
            worker
                .predict(&key, &image, &[Prompt::Positive { x: 64., y: 48. }], &token)
                .is_err()
        );
        worker.clear_idle_cache();
    }

    #[test]
    #[ignore = "Requires the separately verified optional model pack; never downloads"]
    fn cancellation_terminates_a_real_run_and_a_fresh_token_can_reuse_the_worker() {
        let path = std::env::var_os("OMUSE_GUIDED_TEST_PACK").expect("isolated pack");
        let runtime = std::env::var_os("OMUSE_GUIDED_TEST_RUNTIME").expect("bundled runtime");
        let worker = Arc::new(Worker::new(ManagedPack::development(
            path.into(),
            runtime.into(),
        )));
        let image = Arc::new(RgbaImage::from_pixel(
            128,
            96,
            image::Rgba([100, 120, 160, 255]),
        ));
        let key = SourceKey::capture(1, 1, 1, "source".into(), &image).unwrap();
        let token = Cancellation::new();
        let thread_token = token.clone();
        let thread_worker = worker.clone();
        let thread_key = key.clone();
        let thread_image = image.clone();
        let job = std::thread::spawn(move || {
            thread_worker.predict(
                &thread_key,
                &thread_image,
                &[Prompt::Positive { x: 64., y: 48. }],
                &thread_token,
            )
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while token.active.lock().unwrap().is_none() {
            assert!(
                !job.is_finished(),
                "run finished before cancellation was exercised"
            );
            assert!(std::time::Instant::now() < deadline, "run did not start");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        // Allow the cold encoder to enter Run, rather than only exercising a
        // token cancelled just before invocation. This is a host probe, not a
        // portable latency assertion for machines with different hardware.
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !job.is_finished(),
            "the cold encoder must still be running for this probe"
        );
        let start = std::time::Instant::now();
        token.cancel();
        let result = job.join().unwrap();
        assert!(
            result.is_err(),
            "cancelled inference cannot produce an applicable mask"
        );
        assert!(
            token.active.lock().unwrap().is_none(),
            "per-run options must be released"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(15),
            "termination did not complete within the probe safety timeout"
        );
        println!(
            "cancel_to_worker_return_seconds={:.6}",
            start.elapsed().as_secs_f64()
        );
        assert!(
            worker
                .predict(
                    &key,
                    &image,
                    &[Prompt::Positive { x: 64., y: 48. }],
                    &Cancellation::default()
                )
                .is_ok(),
            "fresh run options must not retain termination"
        );
    }
}
