//! Local U2NETP foreground segmentation through the ONNX Runtime C API.
use crate::onnx_bindings as ort;
use anyhow::{Context, Result, ensure};
use image::{GrayImage, Luma, RgbaImage};
use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Mutex, OnceLock};

const SIDE: u32 = 320;
const RUNTIME_LIBRARY: &str = if cfg!(windows) {
    "lib/onnxruntime.dll"
} else {
    "lib/libonnxruntime.so"
};
static ENGINE: OnceLock<std::result::Result<Mutex<Engine>, String>> = OnceLock::new();

/// Return an 8-bit mask at the source dimensions, white over the detected subject.
pub fn segment(image: &RgbaImage) -> Result<GrayImage> {
    ensure!(
        crate::model::valid_dimensions(image.width(), image.height()),
        "invalid image dimensions"
    );
    ensure!(
        u64::from(image.width()) * u64::from(image.height()) <= 16_777_216,
        "Subject detection supports images up to 16 million pixels"
    );
    let engine =
        ENGINE.get_or_init(|| Engine::load().map(Mutex::new).map_err(|e| format!("{e:#}")));
    let engine = engine
        .as_ref()
        .map_err(|message| anyhow::anyhow!(message.clone()))?;
    let mut engine = engine
        .lock()
        .map_err(|_| anyhow::anyhow!("Subject detector lock was poisoned"))?;
    let output = engine.run(&preprocess(image)?)?;
    postprocess(&output, image.width(), image.height())
}

struct Engine {
    api: *const ort::OrtApi,
    env: *mut ort::OrtEnv,
    session: *mut ort::OrtSession,
    input_name: CString,
    output_name: CString,
    _library: Library,
}
// Access is serialized by the process-wide Mutex, and ONNX Runtime owns all pointees.
unsafe impl Send for Engine {}

impl Engine {
    fn load() -> Result<Self> {
        let runtime = asset_path("OMUSE_ONNX_RUNTIME", RUNTIME_LIBRARY)?;
        let model = asset_path("OMUSE_SUBJECT_MODEL", "models/u2netp.onnx")?;
        ensure!(
            runtime.is_file(),
            "ONNX Runtime not found at {}",
            runtime.display()
        );
        ensure!(
            model.is_file(),
            "Subject model not found at {}",
            model.display()
        );
        let library = unsafe { Library::new(&runtime) }
            .with_context(|| format!("Loading {}", runtime.display()))?;
        let get_base: libloading::Symbol<'_, unsafe extern "C" fn() -> *const ort::OrtApiBase> =
            unsafe { library.get(b"OrtGetApiBase\0") }
                .context("ONNX Runtime has no OrtGetApiBase")?;
        let base = unsafe { get_base() };
        ensure!(!base.is_null(), "ONNX Runtime returned no API base");
        let get_api = unsafe { (*base).GetApi }.context("ONNX Runtime has no GetApi")?;
        let api = unsafe { get_api(ort::ORT_API_VERSION) };
        ensure!(
            !api.is_null(),
            "ONNX Runtime API {} is unavailable",
            ort::ORT_API_VERSION
        );
        let mut env = ptr::null_mut();
        let log_id = CString::new("omuse-subject")?;
        check(api, unsafe {
            ((*api).CreateEnv.context("Missing CreateEnv")?)(
                ort::OrtLoggingLevel_ORT_LOGGING_LEVEL_WARNING,
                log_id.as_ptr(),
                &mut env,
            )
        })?;
        let mut options = ptr::null_mut();
        if let Err(error) = (|| -> Result<()> {
            check(api, unsafe {
                ((*api)
                    .CreateSessionOptions
                    .context("Missing CreateSessionOptions")?)(&mut options)
            })?;
            check(api, unsafe {
                ((*api)
                    .SetIntraOpNumThreads
                    .context("Missing SetIntraOpNumThreads")?)(options, 2)
            })?;
            check(api, unsafe {
                ((*api)
                    .SetInterOpNumThreads
                    .context("Missing SetInterOpNumThreads")?)(options, 1)
            })?;
            Ok(())
        })() {
            unsafe {
                release_options(api, options);
                release_env(api, env);
            }
            return Err(error);
        }
        let model_path = model_path_argument(&model)?;
        let mut session = ptr::null_mut();
        let created = check(api, unsafe {
            ((*api).CreateSession.context("Missing CreateSession")?)(
                env,
                model_path.as_ptr(),
                options,
                &mut session,
            )
        });
        unsafe {
            release_options(api, options);
        }
        if let Err(error) = created {
            unsafe {
                release_env(api, env);
            }
            return Err(error);
        }
        let names = session_names(api, session);
        let (input_name, output_name) = match names {
            Ok(names) => names,
            Err(error) => {
                unsafe {
                    release_session(api, session);
                    release_env(api, env);
                }
                return Err(error);
            }
        };
        Ok(Self {
            api,
            env,
            session,
            input_name,
            output_name,
            _library: library,
        })
    }

    fn run(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        ensure!(
            input.len() == 3 * SIDE as usize * SIDE as usize,
            "invalid model input length"
        );
        let api = self.api;
        let mut memory = ptr::null_mut();
        check(api, unsafe {
            ((*api)
                .CreateCpuMemoryInfo
                .context("Missing CreateCpuMemoryInfo")?)(
                ort::OrtAllocatorType_OrtArenaAllocator,
                ort::OrtMemType_OrtMemTypeDefault,
                &mut memory,
            )
        })?;
        let memory = MemoryGuard { api, value: memory };
        let shape = [1_i64, 3, i64::from(SIDE), i64::from(SIDE)];
        let mut input_value = ptr::null_mut();
        check(api, unsafe {
            ((*api)
                .CreateTensorWithDataAsOrtValue
                .context("Missing CreateTensorWithDataAsOrtValue")?)(
                memory.value,
                input.as_ptr().cast_mut().cast::<c_void>(),
                std::mem::size_of_val(input),
                shape.as_ptr(),
                shape.len(),
                ort::ONNXTensorElementDataType_ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT,
                &mut input_value,
            )
        })?;
        let input_value = ValueGuard {
            api,
            value: input_value,
        };
        let input_names = [self.input_name.as_ptr()];
        let inputs = [input_value.value as *const ort::OrtValue];
        let output_names = [self.output_name.as_ptr()];
        let mut output = ptr::null_mut();
        check(api, unsafe {
            ((*api).Run.context("Missing Run")?)(
                self.session,
                ptr::null(),
                input_names.as_ptr(),
                inputs.as_ptr(),
                1,
                output_names.as_ptr(),
                1,
                &mut output,
            )
        })?;
        let output = ValueGuard { api, value: output };
        let mut info = ptr::null_mut();
        check(api, unsafe {
            ((*api)
                .GetTensorTypeAndShape
                .context("Missing GetTensorTypeAndShape")?)(output.value, &mut info)
        })?;
        let info = ShapeGuard { api, value: info };
        let mut element_type = 0;
        check(api, unsafe {
            ((*api)
                .GetTensorElementType
                .context("Missing GetTensorElementType")?)(info.value, &mut element_type)
        })?;
        ensure!(
            element_type == ort::ONNXTensorElementDataType_ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT,
            "Subject model output is not float32"
        );
        let mut count = 0usize;
        check(api, unsafe {
            ((*api)
                .GetTensorShapeElementCount
                .context("Missing GetTensorShapeElementCount")?)(info.value, &mut count)
        })?;
        ensure!(
            count == SIDE as usize * SIDE as usize,
            "Unexpected subject model output size: {count}"
        );
        let mut data = ptr::null_mut();
        check(api, unsafe {
            ((*api)
                .GetTensorMutableData
                .context("Missing GetTensorMutableData")?)(output.value, &mut data)
        })?;
        ensure!(!data.is_null(), "Subject model returned no tensor data");
        Ok(unsafe { std::slice::from_raw_parts(data.cast::<f32>(), count) }.to_vec())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            release_session(self.api, self.session);
            release_env(self.api, self.env);
        }
    }
}

struct ValueGuard {
    api: *const ort::OrtApi,
    value: *mut ort::OrtValue,
}
impl Drop for ValueGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.value.is_null() {
                if let Some(f) = (*self.api).ReleaseValue {
                    f(self.value)
                }
            }
        }
    }
}
struct MemoryGuard {
    api: *const ort::OrtApi,
    value: *mut ort::OrtMemoryInfo,
}
impl Drop for MemoryGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.value.is_null() {
                if let Some(f) = (*self.api).ReleaseMemoryInfo {
                    f(self.value)
                }
            }
        }
    }
}
struct ShapeGuard {
    api: *const ort::OrtApi,
    value: *mut ort::OrtTensorTypeAndShapeInfo,
}
impl Drop for ShapeGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.value.is_null() {
                if let Some(f) = (*self.api).ReleaseTensorTypeAndShapeInfo {
                    f(self.value)
                }
            }
        }
    }
}

fn session_names(
    api: *const ort::OrtApi,
    session: *mut ort::OrtSession,
) -> Result<(CString, CString)> {
    let mut inputs = 0;
    let mut outputs = 0;
    check(api, unsafe {
        ((*api)
            .SessionGetInputCount
            .context("Missing SessionGetInputCount")?)(session, &mut inputs)
    })?;
    check(api, unsafe {
        ((*api)
            .SessionGetOutputCount
            .context("Missing SessionGetOutputCount")?)(session, &mut outputs)
    })?;
    ensure!(
        inputs == 1 && outputs >= 1,
        "Expected one model input and at least one output"
    );
    let mut allocator = ptr::null_mut();
    check(api, unsafe {
        ((*api)
            .GetAllocatorWithDefaultOptions
            .context("Missing default allocator")?)(&mut allocator)
    })?;
    ensure!(!allocator.is_null(), "ONNX Runtime returned no allocator");
    Ok((
        session_name(api, session, allocator, true)?,
        session_name(api, session, allocator, false)?,
    ))
}
fn session_name(
    api: *const ort::OrtApi,
    session: *mut ort::OrtSession,
    allocator: *mut ort::OrtAllocator,
    input: bool,
) -> Result<CString> {
    let mut raw: *mut c_char = ptr::null_mut();
    let status = unsafe {
        if input {
            ((*api)
                .SessionGetInputName
                .context("Missing SessionGetInputName")?)(
                session, 0, allocator, &mut raw
            )
        } else {
            ((*api)
                .SessionGetOutputName
                .context("Missing SessionGetOutputName")?)(
                session, 0, allocator, &mut raw
            )
        }
    };
    check(api, status)?;
    ensure!(!raw.is_null(), "Model has an empty tensor name");
    let copied = unsafe { CStr::from_ptr(raw) }.to_owned();
    let freed =
        unsafe { ((*api).AllocatorFree.context("Missing AllocatorFree")?)(allocator, raw.cast()) };
    check(api, freed)?;
    Ok(copied)
}

fn check(api: *const ort::OrtApi, status: ort::OrtStatusPtr) -> Result<()> {
    if status.is_null() {
        return Ok(());
    }
    let message = unsafe {
        (*api).GetErrorMessage.and_then(|f| {
            let value = f(status);
            (!value.is_null()).then(|| CStr::from_ptr(value).to_string_lossy().into_owned())
        })
    }
    .unwrap_or_else(|| "ONNX Runtime error".into());
    unsafe {
        if let Some(release) = (*api).ReleaseStatus {
            release(status)
        }
    }
    anyhow::bail!(message)
}
unsafe fn release_options(api: *const ort::OrtApi, value: *mut ort::OrtSessionOptions) {
    if !value.is_null() {
        if let Some(f) = unsafe { (*api).ReleaseSessionOptions } {
            unsafe { f(value) }
        }
    }
}
unsafe fn release_session(api: *const ort::OrtApi, value: *mut ort::OrtSession) {
    if !value.is_null() {
        if let Some(f) = unsafe { (*api).ReleaseSession } {
            unsafe { f(value) }
        }
    }
}
unsafe fn release_env(api: *const ort::OrtApi, value: *mut ort::OrtEnv) {
    if !value.is_null() {
        if let Some(f) = unsafe { (*api).ReleaseEnv } {
            unsafe { f(value) }
        }
    }
}

/// ONNX Runtime takes the model path as bytes on Unix and UTF-16 on Windows.
#[cfg(unix)]
fn model_path_argument(model: &Path) -> Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(model.as_os_str().as_bytes()).context("Model path contains NUL")
}

#[cfg(windows)]
fn model_path_argument(model: &Path) -> Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = model.as_os_str().encode_wide().collect();
    ensure!(!wide.contains(&0), "Model path contains NUL");
    wide.push(0);
    Ok(wide)
}

fn asset_path(variable: &str, relative: &str) -> Result<PathBuf> {
    if let Some(path) = crate::identity::env_var_os(variable) {
        ensure!(!path.is_empty(), "{variable} is empty");
        return Ok(path.into());
    }
    let executable = std::env::current_exe().context("Locating application executable")?;
    Ok(executable
        .parent()
        .context("Application executable has no directory")?
        .join(relative))
}

fn preprocess(image: &RgbaImage) -> Result<Vec<f32>> {
    let resized = image::imageops::resize(image, SIDE, SIDE, image::imageops::FilterType::Triangle);
    let maximum = resized
        .pixels()
        .flat_map(|p| {
            let alpha = f32::from(p[3]) / 255.;
            [
                f32::from(p[0]) / 255. * alpha,
                f32::from(p[1]) / 255. * alpha,
                f32::from(p[2]) / 255. * alpha,
            ]
        })
        .fold(0f32, f32::max);
    ensure!(maximum > f32::EPSILON, "No visible pixels to segment");
    let plane = SIDE as usize * SIDE as usize;
    let mut tensor = vec![0f32; plane * 3];
    let means = [0.485, 0.456, 0.406];
    let deviations = [0.229, 0.224, 0.225];
    for (index, pixel) in resized.pixels().enumerate() {
        let alpha = f32::from(pixel[3]) / 255.;
        for channel in 0..3 {
            let normalized = (f32::from(pixel[channel]) / 255. * alpha) / maximum;
            tensor[channel * plane + index] = (normalized - means[channel]) / deviations[channel];
        }
    }
    ensure!(
        tensor.iter().all(|v| v.is_finite()),
        "Invalid model input pixels"
    );
    Ok(tensor)
}

fn postprocess(output: &[f32], width: u32, height: u32) -> Result<GrayImage> {
    ensure!(
        output.len() == SIDE as usize * SIDE as usize && output.iter().all(|v| v.is_finite()),
        "Invalid subject model output"
    );
    let minimum = output.iter().copied().fold(f32::INFINITY, f32::min);
    let maximum = output.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    ensure!(
        maximum - minimum > 1e-8,
        "No foreground subject was detected"
    );
    let small = GrayImage::from_fn(SIDE, SIDE, |x, y| {
        let value =
            (output[y as usize * SIDE as usize + x as usize] - minimum) / (maximum - minimum);
        Luma([(value.clamp(0., 1.) * 255.).round() as u8])
    });
    Ok(image::imageops::resize(
        &small,
        width,
        height,
        image::imageops::FilterType::Triangle,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preprocessing_is_nchw_finite_and_alpha_aware() {
        let image = RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 128]));
        let tensor = preprocess(&image).unwrap();
        assert_eq!(tensor.len(), 3 * 320 * 320);
        assert!(tensor.iter().all(|v| v.is_finite()));
    }
    #[test]
    fn flat_output_is_rejected_as_no_subject() {
        assert!(postprocess(&vec![0.; 320 * 320], 10, 10).is_err());
    }
    #[test]
    fn local_u2netp_inference_is_plausible_when_assets_are_provided() {
        let Some(sample) = crate::identity::env_var_os("OMUSE_SEGMENTATION_SAMPLE") else {
            return;
        };
        let image = image::open(sample).unwrap().to_rgba8();
        let mask = segment(&image).unwrap();
        assert_eq!(mask.dimensions(), image.dimensions());
        let foreground = mask.pixels().filter(|pixel| pixel[0] >= 128).count();
        let fraction = foreground as f64 / f64::from(mask.width() * mask.height());
        assert!(
            (0.01..0.90).contains(&fraction),
            "implausible foreground fraction {fraction}"
        );
        let minimum = mask.pixels().map(|p| p[0]).min().unwrap();
        let maximum = mask.pixels().map(|p| p[0]).max().unwrap();
        assert!(
            minimum < 32 && maximum > 223,
            "mask lacks foreground/background contrast"
        );
    }
}
