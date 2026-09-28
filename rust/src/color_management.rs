//! ICC conversion for decoded straight-alpha RGBA pixels.
//! The application working space is sRGB. Embedded profiles are retained as
//! source metadata while pixels are converted through LittleCMS.
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use std::{
    ffi::{CStr, c_char, c_void},
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::Path,
    ptr::NonNull,
};

const TYPE_RGBA_8: u32 = (4 << 16) | (1 << 7) | (3 << 3) | 1;
const TYPE_RGBA_16: u32 = (4 << 16) | (1 << 7) | (3 << 3) | 2;
const INTENT_RELATIVE_COLORIMETRIC: u32 = 1;
const FLAGS_COPY_ALPHA: u32 = 0x0400_0000;
const MAX_PROFILE_BYTES: usize = 16 * 1024 * 1024;

/// Extract TIFF tag 34675 from the first image file directory. Image 0.25 can
/// fail to expose this valid field through `ImageDecoder::icc_profile`.
pub fn tiff_icc_profile(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    tiff_icc_profile_reader(Cursor::new(bytes), bytes.len() as u64)
}

/// Read only the TIFF header, first directory, and ICC payload. Large TIFFs
/// must not be duplicated in memory merely to recover their colour profile.
pub fn tiff_icc_profile_from_path(path: &Path) -> Result<Option<Vec<u8>>> {
    let file = File::open(path)?;
    let length = file.metadata()?.len();
    tiff_icc_profile_reader(file, length)
}

fn tiff_icc_profile_reader(mut reader: impl Read + Seek, length: u64) -> Result<Option<Vec<u8>>> {
    fn read_exact_at(
        reader: &mut (impl Read + Seek),
        at: u64,
        output: &mut [u8],
        length: u64,
    ) -> Result<()> {
        let end = at
            .checked_add(output.len() as u64)
            .context("TIFF offset overflow")?;
        ensure!(end <= length, "truncated TIFF field");
        reader.seek(SeekFrom::Start(at))?;
        reader.read_exact(output)?;
        Ok(())
    }
    fn read_u16(
        reader: &mut (impl Read + Seek),
        at: u64,
        little: bool,
        length: u64,
    ) -> Result<u16> {
        let mut raw = [0; 2];
        read_exact_at(reader, at, &mut raw, length)?;
        Ok(if little {
            u16::from_le_bytes(raw)
        } else {
            u16::from_be_bytes(raw)
        })
    }
    fn read_u32(
        reader: &mut (impl Read + Seek),
        at: u64,
        little: bool,
        length: u64,
    ) -> Result<u32> {
        let mut raw = [0; 4];
        read_exact_at(reader, at, &mut raw, length)?;
        Ok(if little {
            u32::from_le_bytes(raw)
        } else {
            u32::from_be_bytes(raw)
        })
    }

    let mut byte_order = [0; 2];
    read_exact_at(&mut reader, 0, &mut byte_order, length)?;
    let little = byte_order == *b"II";
    ensure!(little || byte_order == *b"MM", "invalid TIFF byte order");
    ensure!(
        read_u16(&mut reader, 2, little, length)? == 42,
        "invalid TIFF signature"
    );
    let ifd = u64::from(read_u32(&mut reader, 4, little, length)?);
    ensure!(ifd >= 8, "invalid TIFF image directory offset");
    let count = usize::from(read_u16(&mut reader, ifd, little, length)?);
    ensure!(count <= 4096, "TIFF image directory exceeds limit");
    for entry in 0..count {
        let at = ifd
            .checked_add(2)
            .and_then(|v| v.checked_add((entry as u64).checked_mul(12)?))
            .context("TIFF directory overflow")?;
        if read_u16(&mut reader, at, little, length)? != 34675 {
            continue;
        }
        ensure!(
            matches!(read_u16(&mut reader, at + 2, little, length)?, 1 | 7),
            "invalid TIFF ICC field type"
        );
        let len = usize::try_from(read_u32(&mut reader, at + 4, little, length)?)?;
        ensure!(
            (1..=MAX_PROFILE_BYTES).contains(&len),
            "invalid TIFF ICC profile size"
        );
        let start = if len <= 4 {
            at + 8
        } else {
            u64::from(read_u32(&mut reader, at + 8, little, length)?)
        };
        let mut profile = vec![0; len];
        read_exact_at(&mut reader, start, &mut profile, length)
            .context("truncated TIFF ICC profile")?;
        return Ok(Some(profile));
    }
    Ok(None)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceProfile {
    pub description: String,
    pub icc: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertedImage {
    pub image: RgbaImage,
    pub source_profile: Option<SourceProfile>,
}

#[link(name = "lcms2")]
unsafe extern "C" {
    fn cmsOpenProfileFromMem(mem: *const c_void, size: u32) -> *mut c_void;
    fn cmsCreate_sRGBProfile() -> *mut c_void;
    fn cmsCloseProfile(profile: *mut c_void) -> i32;
    fn cmsCreateTransform(
        input: *mut c_void,
        input_format: u32,
        output: *mut c_void,
        output_format: u32,
        intent: u32,
        flags: u32,
    ) -> *mut c_void;
    fn cmsDeleteTransform(transform: *mut c_void);
    fn cmsDoTransform(
        transform: *mut c_void,
        input: *const c_void,
        output: *mut c_void,
        count: u32,
    );
    fn cmsSaveProfileToMem(profile: *mut c_void, memory: *mut c_void, size: *mut u32) -> i32;
    fn cmsGetProfileInfoASCII(
        profile: *mut c_void,
        info: u32,
        language: *const c_char,
        country: *const c_char,
        buffer: *mut c_char,
        size: u32,
    ) -> u32;
}
struct Profile(NonNull<c_void>);
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

fn description(profile: &Profile) -> String {
    let needed = unsafe {
        cmsGetProfileInfoASCII(
            profile.0.as_ptr(),
            0,
            b"en\0".as_ptr().cast(),
            b"US\0".as_ptr().cast(),
            std::ptr::null_mut(),
            0,
        )
    };
    if needed <= 1 || needed > 16_384 {
        return "Embedded ICC profile".into();
    }
    let mut bytes = vec![0u8; needed as usize];
    let written = unsafe {
        cmsGetProfileInfoASCII(
            profile.0.as_ptr(),
            0,
            b"en\0".as_ptr().cast(),
            b"US\0".as_ptr().cast(),
            bytes.as_mut_ptr().cast(),
            needed,
        )
    };
    if written == 0 {
        return "Embedded ICC profile".into();
    }
    CStr::from_bytes_until_nul(&bytes)
        .ok()
        .and_then(|s| s.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or("Embedded ICC profile")
        .to_owned()
}

/// Validate and summarize an embedded source profile for import metadata.
/// Pixel conversion APIs intentionally retain the full bytes separately so a
/// document can report where its normalized sRGB pixels came from.
pub fn source_profile_metadata(bytes: &[u8]) -> Result<SourceProfile> {
    ensure!(
        (128..=MAX_PROFILE_BYTES).contains(&bytes.len()),
        "Invalid ICC profile size"
    );
    let size = u32::try_from(bytes.len()).context("ICC profile is too large")?;
    let profile = Profile(
        NonNull::new(unsafe { cmsOpenProfileFromMem(bytes.as_ptr().cast(), size) })
            .context("Invalid or unsupported ICC profile")?,
    );
    Ok(SourceProfile {
        description: description(&profile),
        icc: bytes.to_vec(),
    })
}

/// Return the canonical LittleCMS sRGB ICC bytes for encoder `set_icc_profile`.
pub fn srgb_profile() -> Result<Vec<u8>> {
    let profile = Profile(
        NonNull::new(unsafe { cmsCreate_sRGBProfile() })
            .context("Could not create sRGB profile")?,
    );
    let mut size = 0u32;
    ensure!(
        unsafe { cmsSaveProfileToMem(profile.0.as_ptr(), std::ptr::null_mut(), &mut size) } != 0
            && size > 0,
        "Could not size sRGB profile"
    );
    let mut bytes = vec![0u8; size as usize];
    ensure!(
        unsafe { cmsSaveProfileToMem(profile.0.as_ptr(), bytes.as_mut_ptr().cast(), &mut size) }
            != 0,
        "Could not serialize sRGB profile"
    );
    bytes.truncate(size as usize);
    Ok(bytes)
}

/// Convert a 16-bit master without an intermediate 8-bit surface.
pub fn to_srgb16(
    image: &image::ImageBuffer<image::Rgba<u16>, Vec<u16>>,
    icc: Option<&[u8]>,
) -> Result<image::ImageBuffer<image::Rgba<u16>, Vec<u16>>> {
    let Some(icc) = icc else {
        return Ok(image.clone());
    };
    ensure!(
        (128..=MAX_PROFILE_BYTES).contains(&icc.len()),
        "Invalid ICC profile size"
    );
    ensure!(
        crate::model::valid_dimensions(image.width(), image.height()),
        "Invalid ICC conversion dimensions"
    );
    let input = Profile(
        NonNull::new(unsafe { cmsOpenProfileFromMem(icc.as_ptr().cast(), icc.len() as u32) })
            .context("Invalid source ICC profile")?,
    );
    // Match the encoded profile written to exported files. ICC serialization
    // quantizes matrix tags; mixing a decoded input profile with an unencoded
    // factory output otherwise changes even an identity transform by a few
    // 16-bit levels on every import/export round trip.
    let destination_icc = srgb_profile()?;
    let output = Profile(
        NonNull::new(unsafe {
            cmsOpenProfileFromMem(
                destination_icc.as_ptr().cast(),
                destination_icc.len() as u32,
            )
        })
        .context("Cannot create sRGB profile")?,
    );
    let transform = Transform(
        NonNull::new(unsafe {
            cmsCreateTransform(
                input.0.as_ptr(),
                TYPE_RGBA_16,
                output.0.as_ptr(),
                TYPE_RGBA_16,
                INTENT_RELATIVE_COLORIMETRIC,
                FLAGS_COPY_ALPHA,
            )
        })
        .context("Unsupported source ICC colour model")?,
    );
    let mut result: image::ImageBuffer<image::Rgba<u16>, Vec<u16>> =
        image::ImageBuffer::new(image.width(), image.height());
    for (src, dst) in image
        .as_raw()
        .chunks(image.width() as usize * 4)
        .zip(result.as_mut().chunks_mut(image.width() as usize * 4))
    {
        unsafe {
            cmsDoTransform(
                transform.0.as_ptr(),
                src.as_ptr().cast(),
                dst.as_mut_ptr().cast(),
                image.width(),
            );
        }
    }
    Ok(result)
}

/// Convert decoded RGBA8 pixels from an embedded ICC profile into sRGB.
/// Missing profiles mean the decoder's bytes are already interpreted as sRGB.
/// Malformed or unsupported profiles return an error and never pass through.
pub fn to_srgb(image: &RgbaImage, icc: Option<&[u8]>) -> Result<ConvertedImage> {
    let Some(bytes) = icc else {
        return Ok(ConvertedImage {
            image: image.clone(),
            source_profile: None,
        });
    };
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_PROFILE_BYTES,
        "ICC profile is empty or exceeds 16 MiB"
    );
    let size = u32::try_from(bytes.len()).context("ICC profile is too large")?;
    let input = Profile(
        NonNull::new(unsafe { cmsOpenProfileFromMem(bytes.as_ptr().cast(), size) })
            .context("Invalid or unsupported ICC profile")?,
    );
    // Match the serialized sRGB profile attached to exported files. Using a
    // factory profile only on the output side can introduce a one-level drift
    // when a tagged Omuse export is opened again.
    let destination_icc = srgb_profile()?;
    let output = Profile(
        NonNull::new(unsafe {
            cmsOpenProfileFromMem(
                destination_icc.as_ptr().cast(),
                destination_icc.len() as u32,
            )
        })
        .context("Cannot create sRGB profile")?,
    );
    let transform = Transform(
        NonNull::new(unsafe {
            cmsCreateTransform(
                input.0.as_ptr(),
                TYPE_RGBA_8,
                output.0.as_ptr(),
                TYPE_RGBA_8,
                INTENT_RELATIVE_COLORIMETRIC,
                FLAGS_COPY_ALPHA,
            )
        })
        .context("ICC profile cannot convert RGBA pixels to sRGB")?,
    );
    let count = u32::try_from(u64::from(image.width()) * u64::from(image.height()))
        .context("Image has too many pixels for ICC conversion")?;
    let mut result = image.clone();
    if count > 0 {
        unsafe {
            cmsDoTransform(
                transform.0.as_ptr(),
                image.as_raw().as_ptr().cast(),
                result.as_flat_samples_mut().samples.as_mut_ptr().cast(),
                count,
            )
        }
    }
    Ok(ConvertedImage {
        image: result,
        source_profile: Some(SourceProfile {
            description: description(&input),
            icc: bytes.to_vec(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[repr(C)]
    struct XyY {
        x: f64,
        y: f64,
        big_y: f64,
    }
    #[repr(C)]
    struct Triple {
        red: XyY,
        green: XyY,
        blue: XyY,
    }
    unsafe extern "C" {
        fn cmsBuildGamma(context: *mut c_void, gamma: f64) -> *mut c_void;
        fn cmsFreeToneCurve(curve: *mut c_void);
        fn cmsCreateRGBProfile(
            white: *const XyY,
            primaries: *const Triple,
            curves: *const *mut c_void,
        ) -> *mut c_void;
    }
    fn linear_srgb_primaries() -> Vec<u8> {
        let white = XyY {
            x: 0.3127,
            y: 0.3290,
            big_y: 1.0,
        };
        let p = Triple {
            red: XyY {
                x: 0.64,
                y: 0.33,
                big_y: 1.,
            },
            green: XyY {
                x: 0.30,
                y: 0.60,
                big_y: 1.,
            },
            blue: XyY {
                x: 0.15,
                y: 0.06,
                big_y: 1.,
            },
        };
        unsafe {
            let curve = cmsBuildGamma(std::ptr::null_mut(), 1.0);
            assert!(!curve.is_null());
            let curves = [curve; 3];
            let profile = cmsCreateRGBProfile(&white, &p, curves.as_ptr());
            assert!(!profile.is_null());
            let mut size = 0;
            assert_ne!(
                cmsSaveProfileToMem(profile, std::ptr::null_mut(), &mut size),
                0
            );
            let mut bytes = vec![0; size as usize];
            assert_ne!(
                cmsSaveProfileToMem(profile, bytes.as_mut_ptr().cast(), &mut size),
                0
            );
            cmsCloseProfile(profile);
            cmsFreeToneCurve(curve);
            bytes
        }
    }
    #[test]
    fn independently_generated_linear_profile_converts_and_preserves_alpha() {
        let icc = linear_srgb_primaries();
        let image = RgbaImage::from_raw(1, 1, vec![128, 128, 128, 37]).unwrap();
        let made = to_srgb(&image, Some(&icc)).unwrap();
        let p = made.image.get_pixel(0, 0);
        assert!((186..=189).contains(&p[0]));
        assert_eq!(p[0], p[1]);
        assert_eq!(p[1], p[2]);
        assert_eq!(p[3], 37);
        assert_eq!(made.source_profile.unwrap().icc, icc);
        assert_eq!(image.as_raw(), &[128, 128, 128, 37]);
    }
    #[test]
    fn malformed_profile_fails_and_missing_profile_is_explicit() {
        let image = RgbaImage::from_raw(1, 1, vec![1, 2, 3, 4]).unwrap();
        assert!(to_srgb(&image, Some(b"not an icc profile")).is_err());
        let made = to_srgb(&image, None).unwrap();
        assert_eq!(made.image, image);
        assert!(made.source_profile.is_none());
    }
    #[test]
    fn tagged_png_import_converts_before_orientation_and_keeps_profile_note() {
        use image::{ImageEncoder, codecs::png::PngEncoder};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linear.png");
        let mut file = std::fs::File::create(&path).unwrap();
        let mut encoder = PngEncoder::new(&mut file);
        encoder.set_icc_profile(linear_srgb_primaries()).unwrap();
        encoder
            .write_image(&[128, 128, 128, 37], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        drop(file);
        let layer = crate::document::import_image(&path).unwrap();
        let p = layer.image.unwrap().get_pixel(0, 0).0;
        assert!((186..=189).contains(&p[0]));
        assert_eq!(p[3], 37);
        assert!(
            layer.metadata["sourceColorProfile"]["iccBytes"]
                .as_u64()
                .unwrap()
                > 100
        );
        assert!(
            layer.metadata["sourceColorProfile"]["fnv1a64"]
                .as_str()
                .is_some()
        );
    }
    #[test]
    fn tagged_tiff_import_converts_to_srgb_and_keeps_profile_note() {
        use image::{ImageEncoder, codecs::tiff::TiffEncoder};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linear.tiff");
        let mut file = std::fs::File::create(&path).unwrap();
        let mut encoder = TiffEncoder::new(&mut file);
        encoder.set_icc_profile(linear_srgb_primaries()).unwrap();
        encoder
            .write_image(&[128, 128, 128, 37], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        drop(file);
        let layer = crate::document::import_image(&path).unwrap();
        let p = layer.image.unwrap().get_pixel(0, 0).0;
        assert!((186..=189).contains(&p[0]), "{p:?}");
        assert_eq!(p[3], 37);
        assert!(
            layer.metadata["sourceColorProfile"]["iccBytes"]
                .as_u64()
                .unwrap()
                > 100
        );
    }
    #[test]
    fn tagged_oriented_tiff16_import_converts_without_byte_quantization() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linear16.tiff");
        let profile = linear_srgb_primaries();
        let source = image::ImageBuffer::from_raw(
            1,
            2,
            vec![1001u16, 32768, 50003, 65535, 2003, 24577, 40005, 60007],
        )
        .unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
        let mut encoded = encoder
            .new_image::<tiff::encoder::colortype::RGBA16>(1, 2)
            .unwrap();
        encoded
            .encoder()
            .write_tag(tiff::tags::Tag::IccProfile, profile.as_slice())
            .unwrap();
        encoded
            .encoder()
            .write_tag(tiff::tags::Tag::Orientation, 6u16)
            .unwrap();
        encoded.write_data(source.as_raw()).unwrap();
        drop(encoder);

        let mut expected =
            image::DynamicImage::ImageRgba16(to_srgb16(&source, Some(&profile)).unwrap());
        expected.apply_orientation(image::metadata::Orientation::Rotate90);
        let layer = crate::document::import_image(&path).unwrap();
        let exact = layer.advanced.unwrap().source.to_rgba16();
        assert_eq!(exact, expected.to_rgba16());
        assert_eq!(exact.dimensions(), (2, 1));
        assert!(
            exact
                .pixels()
                .any(|pixel| pixel.0.iter().any(|v| v % 257 != 0))
        );
        assert_eq!(
            layer.metadata["sourceColorProfile"]["iccBytes"],
            serde_json::json!(profile.len())
        );
    }
    #[test]
    fn canonical_export_profile_is_an_identity_at_the_8_bit_boundary() {
        let profile = srgb_profile().unwrap();
        let image = RgbaImage::from_fn(257, 3, |x, y| {
            image::Rgba([
                x.min(255) as u8,
                (x.wrapping_mul(37) + y * 11) as u8,
                (x.wrapping_mul(91) + y * 23) as u8,
                (x.wrapping_mul(17) + y * 7) as u8,
            ])
        });
        assert_eq!(to_srgb(&image, Some(&profile)).unwrap().image, image);
    }
    #[test]
    fn every_export_is_explicitly_tagged_srgb() {
        use image::{ImageDecoder, ImageReader};
        let dir = tempfile::tempdir().unwrap();
        let doc = crate::model::Document::new(2, 2);
        for ext in ["png", "jpg", "webp", "tiff"] {
            let path = dir.path().join(format!("tagged.{ext}"));
            crate::raster::export(&doc, &path).unwrap();
            let mut decoder = ImageReader::open(&path)
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .into_decoder()
                .unwrap();
            let icc = if ext == "tiff" {
                tiff_icc_profile(&std::fs::read(&path).unwrap()).unwrap()
            } else {
                decoder.icc_profile().unwrap()
            }
            .unwrap_or_else(|| panic!("{ext} export ICC missing"));
            assert!(icc.len() > 100);
            to_srgb(
                &RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4])),
                Some(&icc),
            )
            .unwrap();
        }
    }
}
