use anyhow::{Context, Result, ensure};
use image::{Rgba, RgbaImage};
use libloading::Library;
use std::{
    ffi::{CStr, c_char, c_void},
    path::{Path, PathBuf},
    ptr::NonNull,
};
const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_PIXELS: u64 = 100_000_000;
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DevelopSettings {
    pub exposure: f32,
    pub temperature: f32,
    pub tint: f32,
    pub boost: f32,
    pub as_shot_temperature: f32,
    pub as_shot_tint: f32,
}
impl Default for DevelopSettings {
    fn default() -> Self {
        Self {
            exposure: 0.,
            temperature: 5000.,
            tint: 0.,
            boost: 1.,
            as_shot_temperature: 5000.,
            as_shot_tint: 0.,
        }
    }
}
impl DevelopSettings {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.exposure.is_finite() && (-3.0..=3.).contains(&self.exposure),
            "exposure outside -3...3"
        );
        ensure!(
            self.temperature.is_finite() && (2000.0..=12000.).contains(&self.temperature),
            "temperature outside 2000...12000"
        );
        ensure!(
            self.tint.is_finite() && (-150.0..=150.).contains(&self.tint),
            "tint outside -150...150"
        );
        ensure!(
            self.boost.is_finite() && (0.0..=1.).contains(&self.boost),
            "boost outside 0...1"
        );
        ensure!(
            self.as_shot_temperature.is_finite()
                && (2000.0..=12000.).contains(&self.as_shot_temperature)
                && self.as_shot_tint.is_finite()
                && (-150.0..=150.).contains(&self.as_shot_tint),
            "invalid as-shot white balance"
        );
        Ok(())
    }
}
#[repr(C)]
struct Processed {
    kind: i32,
    height: u16,
    width: u16,
    colors: u16,
    bits: u16,
    data_size: u32,
    data: [u8; 1],
}
type Init = unsafe extern "C" fn(u32) -> *mut c_void;
#[cfg(not(windows))]
type Open = unsafe extern "C" fn(*mut c_void, *const c_char) -> i32;
/// `libraw_open_wfile` takes a UTF-16 `wchar_t` path on Windows.
#[cfg(windows)]
type Open = unsafe extern "C" fn(*mut c_void, *const u16) -> i32;
type One = unsafe extern "C" fn(*mut c_void) -> i32;
type Close = unsafe extern "C" fn(*mut c_void);
type SetI = unsafe extern "C" fn(*mut c_void, i32);
type SetF = unsafe extern "C" fn(*mut c_void, f32);
type Gamma = unsafe extern "C" fn(*mut c_void, i32, f32);
type Mul = unsafe extern "C" fn(*mut c_void, i32, f32);
type GetI = unsafe extern "C" fn(*mut c_void) -> i32;
type GetF = unsafe extern "C" fn(*mut c_void, i32) -> f32;
type Make = unsafe extern "C" fn(*mut c_void, *mut i32) -> *mut Processed;
type Clear = unsafe extern "C" fn(*mut Processed);
type ErrFn = unsafe extern "C" fn(i32) -> *const c_char;
struct Api {
    _lib: Library,
    init: Init,
    open: Open,
    unpack: One,
    process: One,
    close: Close,
    set_color: SetI,
    set_bps: SetI,
    set_auto: SetI,
    set_bright: SetF,
    set_gamma: Gamma,
    set_mul: Mul,
    width: GetI,
    height: GetI,
    cam_mul: GetF,
    pre_mul: GetF,
    make: Make,
    clear: Clear,
    error: ErrFn,
}
impl Api {
    fn load() -> Result<Self> {
        let path = library_path()?;
        let lib = unsafe { Library::new(&path) }
            .with_context(|| format!("Load LibRaw from {}", path.display()))?;
        unsafe {
            macro_rules! s {
                ($n:literal,$t:ty) => {
                    *lib.get::<$t>(concat!($n, "\0").as_bytes())
                        .with_context(|| concat!("Missing LibRaw symbol ", $n))?
                };
            }
            Ok(Self {
                init: s!("libraw_init", Init),
                #[cfg(not(windows))]
                open: s!("libraw_open_file", Open),
                #[cfg(windows)]
                open: s!("libraw_open_wfile", Open),
                unpack: s!("libraw_unpack", One),
                process: s!("libraw_dcraw_process", One),
                close: s!("libraw_close", Close),
                set_color: s!("libraw_set_output_color", SetI),
                set_bps: s!("libraw_set_output_bps", SetI),
                set_auto: s!("libraw_set_no_auto_bright", SetI),
                set_bright: s!("libraw_set_bright", SetF),
                set_gamma: s!("libraw_set_gamma", Gamma),
                set_mul: s!("libraw_set_user_mul", Mul),
                width: s!("libraw_get_iwidth", GetI),
                height: s!("libraw_get_iheight", GetI),
                cam_mul: s!("libraw_get_cam_mul", GetF),
                pre_mul: s!("libraw_get_pre_mul", GetF),
                make: s!("libraw_dcraw_make_mem_image", Make),
                clear: s!("libraw_dcraw_clear_mem", Clear),
                error: s!("libraw_strerror", ErrFn),
                _lib: lib,
            })
        }
    }
    fn check(&self, n: i32, step: &str) -> Result<()> {
        if n == 0 {
            return Ok(());
        }
        let p = unsafe { (self.error)(n) };
        let text = if p.is_null() {
            "unknown error".into()
        } else {
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        };
        anyhow::bail!("{step}: {text} ({n})")
    }
}
const LIBRARY_NAME: &str = if cfg!(windows) {
    "libraw.dll"
} else {
    "libraw.so"
};
fn library_path() -> Result<PathBuf> {
    if let Some(v) = crate::identity::env_var_os("OMUSE_LIBRAW") {
        let p = PathBuf::from(v);
        return Ok(if p.is_dir() { p.join(LIBRARY_NAME) } else { p });
    }
    let e = std::env::current_exe()?;
    Ok(e.parent()
        .context("executable has no parent")?
        .join("lib")
        .join(LIBRARY_NAME))
}
pub fn matches(path: &Path) -> bool {
    path.extension().and_then(|x| x.to_str()).is_some_and(|x| {
        matches!(
            x.to_ascii_lowercase().as_str(),
            "3fr"
                | "ari"
                | "arw"
                | "bay"
                | "cap"
                | "cr2"
                | "cr3"
                | "crw"
                | "dcr"
                | "dcs"
                | "dng"
                | "drf"
                | "eip"
                | "erf"
                | "fff"
                | "gpr"
                | "iiq"
                | "k25"
                | "kdc"
                | "mdc"
                | "mef"
                | "mos"
                | "mrw"
                | "nef"
                | "nrw"
                | "obm"
                | "orf"
                | "pef"
                | "ptx"
                | "pxn"
                | "r3d"
                | "raf"
                | "raw"
                | "rw2"
                | "rwl"
                | "rwz"
                | "sr2"
                | "srf"
                | "srw"
                | "x3f"
        )
    })
}
struct Raw<'a> {
    a: &'a Api,
    p: NonNull<c_void>,
}
impl Drop for Raw<'_> {
    fn drop(&mut self) {
        unsafe { (self.a.close)(self.p.as_ptr()) }
    }
}
struct Mem<'a> {
    a: &'a Api,
    p: NonNull<Processed>,
}
impl Drop for Mem<'_> {
    fn drop(&mut self) {
        unsafe { (self.a.clear)(self.p.as_ptr()) }
    }
}
fn temp_rgb(k: f32) -> [f32; 3] {
    let t = k / 100.;
    let r = if t <= 66. {
        255.
    } else {
        329.69873 * (t - 60.).powf(-0.13320476)
    };
    let g = if t <= 66. {
        99.4708 * t.ln() - 161.11957
    } else {
        288.12216 * (t - 60.).powf(-0.07551485)
    };
    let b = if t >= 66. {
        255.
    } else if t <= 19. {
        0.
    } else {
        138.51773 * (t - 10.).ln() - 305.0448
    };
    [r.clamp(1., 255.), g.clamp(1., 255.), b.clamp(1., 255.)]
}

fn white_balance_multipliers(
    camera: [f32; 4],
    daylight: [f32; 4],
    s: &DevelopSettings,
) -> Result<[f32; 4]> {
    let valid = |v: f32| v.is_finite() && v > 0.;
    // Camera gains need not be normalized: some cameras use green=256 or 1024.
    // Keep a complete set on the same scale; use LibRaw's daylight calibration
    // if the camera has no usable as-shot RGB metadata.
    let mut gains = if camera[..3].iter().copied().all(valid) {
        camera
    } else if daylight[..3].iter().copied().all(valid) {
        daylight
    } else {
        [1.; 4]
    };
    if !valid(gains[3]) {
        gains[3] = gains[1];
    }
    let wanted = temp_rgb(s.temperature);
    let base = temp_rgb(s.as_shot_temperature);
    let tint = 2f32.powf((s.tint - s.as_shot_tint) / 150.);
    for (i, gain) in gains.iter_mut().enumerate() {
        let channel = if i == 3 { 1 } else { i };
        *gain *= base[channel] / wanted[channel];
        // Both green sites in a Bayer mosaic must receive the same adjustment.
        if channel == 1 {
            *gain *= tint;
        }
        ensure!(valid(*gain), "invalid RAW white-balance gain");
    }
    Ok(gains)
}
/// Develop through LibRaw. Boost interpolates LibRaw neutral/display gamma and is not Apple's proprietary curve.
pub fn develop(path: &Path, s: &DevelopSettings) -> Result<RgbaImage> {
    Ok(develop_bitmap(path, s, 8)?.to_rgba8())
}

/// Preserve sensor-development precision for editable embedded RAW sources.
pub fn develop16(
    path: &Path,
    s: &DevelopSettings,
) -> Result<image::ImageBuffer<Rgba<u16>, Vec<u16>>> {
    Ok(develop_bitmap(path, s, 16)?.to_rgba16())
}

fn develop_bitmap(path: &Path, s: &DevelopSettings, bits: i32) -> Result<image::DynamicImage> {
    let pixel_limit = if bits == 16 {
        crate::advanced::MAX_ADVANCED_PIXELS
    } else {
        MAX_PIXELS
    };
    s.validate()?;
    ensure!(matches(path), "unrecognized camera RAW extension");
    let meta = std::fs::metadata(path)?;
    ensure!(
        meta.is_file() && meta.len() <= MAX_FILE,
        "RAW exceeds 512 MiB"
    );
    let a = Api::load()?;
    let raw = Raw {
        a: &a,
        p: NonNull::new(unsafe { (a.init)(0) }).context("LibRaw init failed")?,
    };
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;
    #[cfg(unix)]
    let name = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    #[cfg(windows)]
    let name = {
        use std::os::windows::ffi::OsStrExt;
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        ensure!(!wide.contains(&0), "RAW path contains NUL");
        wide.push(0);
        wide
    };
    a.check(
        unsafe { (a.open)(raw.p.as_ptr(), name.as_ptr()) },
        "open RAW",
    )?;
    let (w, h) = (unsafe { (a.width)(raw.p.as_ptr()) }, unsafe {
        (a.height)(raw.p.as_ptr())
    });
    ensure!(
        w > 0 && h > 0 && w <= 30_000 && h <= 30_000 && (w as u64) * (h as u64) <= pixel_limit,
        "RAW dimensions exceed limits"
    );
    unsafe {
        (a.set_color)(raw.p.as_ptr(), 1);
        (a.set_bps)(raw.p.as_ptr(), bits);
        (a.set_auto)(raw.p.as_ptr(), 1);
        (a.set_bright)(raw.p.as_ptr(), 2f32.powf(s.exposure));
        (a.set_gamma)(raw.p.as_ptr(), 0, 1. - s.boost * 0.55);
        (a.set_gamma)(raw.p.as_ptr(), 1, 1. + s.boost * 3.5)
    }
    let camera = std::array::from_fn(|i| unsafe { (a.cam_mul)(raw.p.as_ptr(), i as i32) });
    let daylight = std::array::from_fn(|i| unsafe { (a.pre_mul)(raw.p.as_ptr(), i as i32) });
    for (i, gain) in white_balance_multipliers(camera, daylight, s)?
        .into_iter()
        .enumerate()
    {
        unsafe { (a.set_mul)(raw.p.as_ptr(), i as i32, gain) }
    }
    a.check(unsafe { (a.unpack)(raw.p.as_ptr()) }, "unpack RAW")?;
    a.check(unsafe { (a.process)(raw.p.as_ptr()) }, "develop RAW")?;
    let mut err = 0;
    let mem = Mem {
        a: &a,
        p: NonNull::new(unsafe { (a.make)(raw.p.as_ptr(), &mut err) })
            .context("no developed bitmap")?,
    };
    a.check(err, "make bitmap")?;
    let m = unsafe { mem.p.as_ref() };
    ensure!(
        m.kind == 2 && m.colors == 3 && i32::from(m.bits) == bits,
        "unsupported LibRaw bitmap"
    );
    let n = u64::from(m.width) * u64::from(m.height);
    ensure!(
        n <= pixel_limit && u64::from(m.data_size) == n * 3 * (bits as u64 / 8),
        "invalid LibRaw bitmap size"
    );
    let data = unsafe { std::slice::from_raw_parts(m.data.as_ptr(), m.data_size as usize) };
    if bits == 16 {
        return Ok(image::DynamicImage::ImageRgba16(
            image::ImageBuffer::from_fn(u32::from(m.width), u32::from(m.height), |x, y| {
                let i = (y as usize * m.width as usize + x as usize) * 6;
                Rgba([
                    u16::from_ne_bytes([data[i], data[i + 1]]),
                    u16::from_ne_bytes([data[i + 2], data[i + 3]]),
                    u16::from_ne_bytes([data[i + 4], data[i + 5]]),
                    65535,
                ])
            }),
        ));
    }
    Ok(image::DynamicImage::ImageRgba8(RgbaImage::from_fn(
        u32::from(m.width),
        u32::from(m.height),
        |x, y| {
            let i = (y as usize * m.width as usize + x as usize) * 3;
            Rgba([data[i], data[i + 1], data[i + 2], 255])
        },
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn as_shot_gains_preserve_both_greens_on_the_camera_scale() {
        for camera in [[506., 256., 293., 256.], [2114., 1024., 1515., 1024.]] {
            assert_eq!(
                white_balance_multipliers(camera, [1.; 4], &DevelopSettings::default()).unwrap(),
                camera
            );
        }
    }

    #[test]
    fn temperature_and_tint_adjust_both_green_sites_equally() {
        let camera = [2., 1., 1.5, 1.25];
        let settings = DevelopSettings {
            temperature: 7000.,
            tint: 75.,
            ..Default::default()
        };
        let gains = white_balance_multipliers(camera, [1.; 4], &settings).unwrap();
        assert!((gains[1] / camera[1] - gains[3] / camera[3]).abs() < 1e-6);
        let untinted = white_balance_multipliers(
            camera,
            [1.; 4],
            &DevelopSettings {
                tint: 0.,
                ..settings
            },
        )
        .unwrap();
        assert_eq!(gains[0], untinted[0]);
        assert_eq!(gains[2], untinted[2]);
        for i in [1, 3] {
            assert!((gains[i] / untinted[i] - 2f32.sqrt()).abs() < 1e-6);
        }
    }

    #[test]
    fn missing_camera_gains_use_a_consistent_fallback_scale() {
        let settings = DevelopSettings::default();
        assert_eq!(
            white_balance_multipliers([506., 256., 293., 0.], [1.; 4], &settings).unwrap(),
            [506., 256., 293., 256.]
        );
        assert_eq!(
            white_balance_multipliers([f32::NAN, 256., 0., 1.], [2., 1., 1.5, 0.], &settings)
                .unwrap(),
            [2., 1., 1.5, 1.]
        );
        assert_eq!(
            white_balance_multipliers([0.; 4], [f32::INFINITY; 4], &settings).unwrap(),
            [1.; 4]
        );
    }

    #[test]
    fn extensions_and_ranges() {
        assert!(matches(Path::new("a.CR3")));
        assert!(!matches(Path::new("a.jpg")));
        let mut s = DevelopSettings::default();
        s.exposure = 4.;
        assert!(s.validate().is_err());
        assert!(develop(Path::new("missing.jpg"), &DevelopSettings::default()).is_err());
    }

    #[test]
    fn oversized_raw_is_rejected_before_loading_libraw() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("omuse-oversize-{nonce}.dng"));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE + 1).unwrap();
        drop(file);
        let error = develop(&path, &DevelopSettings::default())
            .unwrap_err()
            .to_string();
        let _ = std::fs::remove_file(path);
        assert!(error.contains("512 MiB"), "{error}");
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    #[test]
    fn configured_real_camera_file_develops_with_exposure_control() {
        let Some(path) = crate::identity::env_var_os("OMUSE_RAW_FIXTURE") else {
            return;
        };
        let base = develop(Path::new(&path), &DevelopSettings::default()).unwrap();
        assert!(base.width() > 100 && base.height() > 100);
        assert!(base.pixels().all(|p| p[3] == 255));
        let darker = develop(
            Path::new(&path),
            &DevelopSettings {
                exposure: -1.,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(darker.dimensions(), base.dimensions());
        let energy = |image: &RgbaImage| {
            image
                .pixels()
                .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
                .sum::<u64>()
        };
        assert!(energy(&darker) < energy(&base));
    }
}
