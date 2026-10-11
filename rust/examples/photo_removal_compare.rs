//! Explicit real-photo comparison; never downloads files or overwrites evidence.
//! Usage: INPUT OUTPUT_DIR x y w h rect|ellipse [sx sy sw sh] [feather=0..1]
//! The sampling rectangle defaults to the entire image. Keep input working-copy
//! provenance with the receipt. Timings are local measurements, not a benchmark
//! unless the host is otherwise idle and repeated runs are controlled.
use anyhow::{Result, ensure};
use omuse::{
    advanced_ops::{
        self, AdvancedOperation, ContentAwareAlgorithm, ContentAwareReplace, FilterNode, SoftMask,
    },
    asset_library::sha256_hex,
    retouch::texture,
};
use serde_json::json;
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().collect();
    let feather: f32 = if let Some(value) = args.last().and_then(|s| s.strip_prefix("feather=")) {
        let value = value.parse()?;
        args.pop();
        value
    } else {
        0.0
    };
    ensure!(
        feather.is_finite() && (0.0..=1.0).contains(&feather),
        "Invalid feather"
    );
    ensure!(
        args.len() == 8 || args.len() == 12,
        "INPUT OUTPUT_DIR x y w h rect|ellipse [sx sy sw sh] [feather=0..1]"
    );
    let input = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);
    let image = image::open(&input)?.to_rgba8();
    let (w, h) = image.dimensions();
    let rect: Vec<u32> = args[3..7]
        .iter()
        .map(|s| s.parse())
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        rect[2] > 0
            && rect[3] > 0
            && rect[0].checked_add(rect[2]).is_some_and(|v| v <= w)
            && rect[1].checked_add(rect[3]).is_some_and(|v| v <= h),
        "Selection must fit image"
    );
    ensure!(
        matches!(args[7].as_str(), "rect" | "ellipse"),
        "Unknown selection shape"
    );
    let sample: Vec<u32> = if args.len() == 12 {
        args[8..12]
            .iter()
            .map(|s| s.parse())
            .collect::<std::result::Result<_, _>>()?
    } else {
        vec![0, 0, w, h]
    };
    ensure!(
        sample[0].checked_add(sample[2]).is_some_and(|v| v <= w)
            && sample[1].checked_add(sample[3]).is_some_and(|v| v <= h),
        "Sampling area must fit image"
    );
    let mut target = vec![0u8; (w * h) as usize];
    let mut allowed = vec![0u8; target.len()];
    for y in rect[1]..rect[1] + rect[3] {
        for x in rect[0]..rect[0] + rect[2] {
            let nx =
                (x as f64 + 0.5 - (rect[0] as f64 + rect[2] as f64 / 2.0)) / (rect[2] as f64 / 2.0);
            let ny =
                (y as f64 + 0.5 - (rect[1] as f64 + rect[3] as f64 / 2.0)) / (rect[3] as f64 / 2.0);
            if args[7] == "rect" || nx * nx + ny * ny <= 1.0 {
                target[(y * w + x) as usize] = 255;
            }
        }
    }
    for y in sample[1]..sample[1] + sample[3] {
        for x in sample[0]..sample[0] + sample[2] {
            if target[(y * w + x) as usize] == 0 {
                allowed[(y * w + x) as usize] = 255;
            }
        }
    }
    std::fs::create_dir(&out)?;
    image.save(out.join("before.png"))?;
    image::GrayImage::from_raw(w, h, target.clone())
        .unwrap()
        .save(out.join("target.png"))?;
    image::GrayImage::from_raw(w, h, allowed.clone())
        .unwrap()
        .save(out.join("allowed.png"))?;
    let mut receipts = Vec::new();
    for name in ["contextual-v1", "texture-v2"] {
        let started = Instant::now();
        let run = || -> Result<image::RgbaImage> {
            if name == "contextual-v1" {
                advanced_ops::evaluate(
                    &image,
                    &[FilterNode {
                        id: "photo-removal-comparison".into(),
                        name: name.into(),
                        enabled: true,
                        opacity: 1.0,
                        soft_mask: None,
                        operation: AdvancedOperation::ContentAwareReplace(ContentAwareReplace {
                            algorithm: ContentAwareAlgorithm::ContextualV1,
                            target_mask: SoftMask::new(w, h, target.clone())?,
                            allowed_source_mask: SoftMask::new(w, h, allowed.clone())?,
                            search_radius: 64,
                            patch_radius: 2,
                            feather,
                        }),
                    }],
                )
            } else {
                let mut output = image.clone();
                texture::visit_samples(
                    &image,
                    &target,
                    &allowed,
                    64,
                    2,
                    feather,
                    &AtomicBool::new(false),
                    |x, y, sx, sy, amount| {
                        let donor = image.get_pixel(sx, sy);
                        let pixel = output.get_pixel_mut(x, y);
                        for channel in 0..4 {
                            pixel[channel] = (f32::from(pixel[channel])
                                + (f32::from(donor[channel]) - f32::from(pixel[channel])) * amount)
                                .round() as u8;
                        }
                        Ok(())
                    },
                )?;
                Ok(output)
            }
        };
        match run() {
            Err(error) => receipts.push(json!({"method":name,"error":format!("{error:#}")})),
            Ok(output) => {
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                let repeated = run()?;
                ensure!(output == repeated, "nondeterministic {name}");
                let mut protected = 0usize;
                let mut changed = 0usize;
                let mut mse = 0.0;
                for (i, (a, b)) in image.pixels().zip(output.pixels()).enumerate() {
                    if target[i] == 0 {
                        ensure!(a == b, "protected pixel {i} changed");
                        protected += 1;
                    } else {
                        for c in 0..3 {
                            mse += (f64::from(a[c]) - f64::from(b[c])).powi(2);
                        }
                    }
                    changed += usize::from(a != b);
                }
                let selected = target.iter().filter(|&&v| v > 0).count();
                output.save(out.join(format!("{name}.png")))?;
                receipts.push(json!({"method":name,"millisecondsUnderCurrentHostLoad":elapsed,"deterministicRepeat":true,
                    "protectedPixelsExact":protected,"selectedPixels":selected,"changedPixels":changed,
                    "selectedRgbMseAgainstInput":mse/selected as f64/3.0,
                    "mseMeaning":"Reconstruction error ONLY if the selected input region was deliberately intact ground truth; otherwise this is just change magnitude, not a quality metric",
                    "pixelsSha256":sha256_hex(output.as_raw())}));
            }
        }
    }
    std::fs::write(
        out.join("receipt.json"),
        serde_json::to_string_pretty(&json!({
            "input":input,"inputSha256":sha256_hex(&std::fs::read(&input)?),"inputPixelsSha256":sha256_hex(image.as_raw()),
            "dimensions":[w,h],"selectionRect":rect,"selectionShape":args[7],"allowedRect":sample,
        "settings":{"searchRadius":64,"patchRadius":2,"feather":feather},"methods":receipts,
            "scope":"Explicit local real-photo comparison; visual review required; no broad photo-quality claim"
        }))? + "\n",
    )?;
    println!("{}", out.join("receipt.json").display());
    Ok(())
}
