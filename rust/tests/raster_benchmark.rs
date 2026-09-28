use image::{Rgba, RgbaImage};
use omuse::{
    filters::{self, Filter},
    model::{Document, Layer, PixelRect},
    raster,
};
use std::{hint::black_box, time::Instant};
#[test]
#[ignore = "manual CPU benchmark; timings depend on hardware and system load"]
fn reference_raster_timing() {
    let mut doc = Document::new(1024, 768);
    doc.layers.clear();
    for (index, alpha) in [255, 128, 96].into_iter().enumerate() {
        let mut layer = Layer::paint(format!("Layer {index}"), 1024, 768);
        layer.image = Some(
            RgbaImage::from_fn(1024, 768, |x, y| {
                Rgba([
                    (x % 256) as u8,
                    (y % 256) as u8,
                    80 + index as u8 * 50,
                    alpha,
                ])
            })
            .into(),
        );
        doc.layers.push(layer);
    }
    for _ in 0..3 {
        black_box(raster::composite(&doc));
    }
    let mut times = vec![];
    for _ in 0..20 {
        let start = Instant::now();
        black_box(raster::composite(black_box(&doc)));
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"case\":\"3_identity_layers_1024x768\",\"samples\":20,\"median_ms\":{:.3},\"min_ms\":{:.3},\"max_ms\":{:.3}}}",
        times[10], times[0], times[19]
    );
    doc.layers[1].opacity = 0.63;
    let mut times = vec![];
    for _ in 0..20 {
        let start = Instant::now();
        black_box(raster::composite(black_box(&doc)));
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"case\":\"3_identity_layers_partial_opacity_1024x768\",\"samples\":20,\"median_ms\":{:.3},\"min_ms\":{:.3},\"max_ms\":{:.3}}}",
        times[10], times[0], times[19]
    );
    doc.layers[1].opacity = 1.0;
    doc.layers[2].rotation = 17.0;
    doc.layers[2].scale_x = 0.85;
    doc.layers[2].scale_y = 0.85;
    let mut times = vec![];
    for _ in 0..20 {
        let start = Instant::now();
        black_box(raster::composite(black_box(&doc)));
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"case\":\"3_layers_one_transformed_1024x768\",\"samples\":20,\"median_ms\":{:.3},\"min_ms\":{:.3},\"max_ms\":{:.3}}}",
        times[10], times[0], times[19]
    );
    let mut img = doc.layers[0].image.as_ref().unwrap().to_image();
    let start = Instant::now();
    filters::apply(&mut img, &Filter::GaussianBlur { sigma: 8.0 }).unwrap();
    black_box(img);
    println!(
        "{{\"case\":\"gaussian_sigma8_1024x768\",\"samples\":1,\"ms\":{:.3}}}",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let mut region_doc = Document::new(2048, 2048);
    region_doc.layers.clear();
    for index in 0..3 {
        let mut layer = Layer::paint(format!("Region layer {index}"), 2048, 2048);
        layer.image = Some(RgbaImage::from_pixel(2048, 2048, Rgba([40, 90, 160, 96])).into());
        region_doc.layers.push(layer);
    }
    let mut full_doc = region_doc.clone();
    let mut region_target = raster::composite(&region_doc);
    let patch = PixelRect {
        x: 1016,
        y: 1016,
        width: 16,
        height: 16,
    };
    let mut region_times = Vec::new();
    let mut full_times = Vec::new();
    for sample in 0..20u8 {
        for y in patch.y..patch.y + patch.height {
            for x in patch.x..patch.x + patch.width {
                region_doc.layers[2].image.as_mut().unwrap().put_pixel(
                    x,
                    y,
                    Rgba([sample, 180, 70, 211]),
                );
            }
        }
        let start = Instant::now();
        assert!(raster::composite_region(
            &region_doc,
            &mut region_target,
            patch
        ));
        black_box(&region_target);
        region_times.push(start.elapsed().as_secs_f64() * 1000.0);

        for y in patch.y..patch.y + patch.height {
            for x in patch.x..patch.x + patch.width {
                full_doc.layers[2].image.as_mut().unwrap().put_pixel(
                    x,
                    y,
                    Rgba([sample, 180, 70, 211]),
                );
            }
        }
        let start = Instant::now();
        black_box(raster::composite(&full_doc));
        full_times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    region_times.sort_by(f64::total_cmp);
    full_times.sort_by(f64::total_cmp);
    println!(
        "{{\"case\":\"3_layers_small_patch_2048x2048\",\"samples\":20,\"region_median_ms\":{:.3},\"full_median_ms\":{:.3}}}",
        region_times[10], full_times[10]
    );
}
