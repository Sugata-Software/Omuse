//! Display-only LittleCMS conversion. Proofing never changes document pixels.
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{ffi::c_void, io::Read, path::Path, ptr::NonNull};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub monitor_profile: Option<String>,
    pub proof_profile: Option<String>,
    pub intent: u32,
    pub black_point_compensation: bool,
    pub enabled: bool,
}

#[link(name = "lcms2")]
unsafe extern "C" {
    fn cmsOpenProfileFromMem(data: *const c_void, size: u32) -> *mut c_void;
    fn cmsCloseProfile(profile: *mut c_void) -> i32;
    fn cmsCreate_sRGBProfile() -> *mut c_void;
    fn cmsGetColorSpace(profile: *mut c_void) -> u32;
    fn cmsCreateTransform(
        input: *mut c_void,
        input_format: u32,
        output: *mut c_void,
        output_format: u32,
        intent: u32,
        flags: u32,
    ) -> *mut c_void;
    fn cmsCreateProofingTransform(
        input: *mut c_void,
        input_format: u32,
        output: *mut c_void,
        output_format: u32,
        proof: *mut c_void,
        intent: u32,
        proof_intent: u32,
        flags: u32,
    ) -> *mut c_void;
    fn cmsDeleteTransform(transform: *mut c_void);
    fn cmsDoTransform(transform: *mut c_void, input: *const c_void, output: *mut c_void, size: u32);
}

struct Profile(NonNull<c_void>);
impl Profile {
    fn srgb() -> Result<Self> {
        Ok(Self(
            NonNull::new(unsafe { cmsCreate_sRGBProfile() })
                .context("Cannot create sRGB profile")?,
        ))
    }
    fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Cannot open ICC profile {}", path.display()))?;
        ensure!(
            file.metadata()?.is_file(),
            "ICC profile must be a regular file"
        );
        let mut bytes = Vec::new();
        file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            (128..=16 * 1024 * 1024).contains(&bytes.len()),
            "ICC profile exceeds size limits"
        );
        Ok(Self(
            NonNull::new(unsafe {
                cmsOpenProfileFromMem(bytes.as_ptr().cast(), bytes.len() as u32)
            })
            .context("Invalid ICC profile")?,
        ))
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        unsafe {
            cmsCloseProfile(self.0.as_ptr());
        }
    }
}
struct Transform(NonNull<c_void>);
impl Drop for Transform {
    fn drop(&mut self) {
        unsafe {
            cmsDeleteTransform(self.0.as_ptr());
        }
    }
}

/// Input is straight-alpha sRGB. Output is encoded for the chosen monitor.
/// An absent monitor profile uses sRGB; a proof profile simulates its gamut.
pub fn render(source: &RgbaImage, settings: &Settings) -> Result<RgbaImage> {
    ensure!(settings.intent <= 3, "Rendering intent must be 0 through 3");
    ensure!(
        crate::model::valid_dimensions(source.width(), source.height()),
        "Invalid proof image dimensions"
    );
    if !settings.enabled {
        return Ok(source.clone());
    }
    let input = Profile::srgb()?;
    let output = settings
        .monitor_profile
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(|p| Profile::load(Path::new(p)))
        .transpose()?
        .unwrap_or(Profile::srgb()?);
    ensure!(
        unsafe { cmsGetColorSpace(output.0.as_ptr()) } == 0x5247_4220,
        "Monitor profile must be RGB"
    );
    let proof = settings
        .proof_profile
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(|p| Profile::load(Path::new(p)))
        .transpose()?;
    const RGBA8: u32 = (4 << 16) | (1 << 7) | (3 << 3) | 1;
    let flags = 0x0400_0000
        | if settings.black_point_compensation {
            0x2000
        } else {
            0
        };
    // SAFETY: profiles live until after the transform is constructed; LittleCMS
    // owns its compiled tables. Buffers below contain the declared RGBA layout.
    let transform = unsafe {
        if let Some(proof) = &proof {
            cmsCreateProofingTransform(
                input.0.as_ptr(),
                RGBA8,
                output.0.as_ptr(),
                RGBA8,
                proof.0.as_ptr(),
                settings.intent,
                1,
                flags | 0x4000,
            )
        } else {
            cmsCreateTransform(
                input.0.as_ptr(),
                RGBA8,
                output.0.as_ptr(),
                RGBA8,
                settings.intent,
                flags,
            )
        }
    };
    let transform = Transform(
        NonNull::new(transform).context("ICC profiles cannot form a display/proof transform")?,
    );
    let mut result = RgbaImage::new(source.width(), source.height());
    let result_bytes: &mut [u8] = result.as_mut();
    for (input, output) in source
        .as_raw()
        .chunks(source.width() as usize * 4)
        .zip(result_bytes.chunks_mut(source.width() as usize * 4))
    {
        unsafe {
            cmsDoTransform(
                transform.0.as_ptr(),
                input.as_ptr().cast(),
                output.as_mut_ptr().cast(),
                source.width(),
            );
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_preserves_alpha_and_source() {
        let image = RgbaImage::from_fn(259, 1, |x, _| {
            image::Rgba([x as u8, (x * 7) as u8, (x * 13) as u8, x as u8])
        });
        let original = image.clone();
        let output = render(
            &image,
            &Settings {
                enabled: true,
                intent: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(image, original);
        for (a, b) in image.pixels().zip(output.pixels()) {
            assert_eq!(a[3], b[3]);
            for c in 0..3 {
                assert!(a[c].abs_diff(b[c]) <= 1);
            }
        }
    }
    #[test]
    fn proof_round_trip_and_invalid_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let profile = tmp.path().join("sRGB.icc");
        std::fs::write(&profile, crate::color_management::srgb_profile().unwrap()).unwrap();
        let image = RgbaImage::from_pixel(7, 5, image::Rgba([30, 130, 220, 19]));
        let mut settings = Settings {
            enabled: true,
            proof_profile: Some(profile.to_string_lossy().into_owned()),
            ..Default::default()
        };
        assert_eq!(render(&image, &settings).unwrap(), image);
        std::fs::write(&profile, [0u8; 128]).unwrap();
        assert!(render(&image, &settings).is_err());
        settings.enabled = false;
        assert_eq!(render(&image, &settings).unwrap(), image);
    }
}
