use image::Rgba;
use omuse::{
    editor::Editor,
    model::{Document, Layer},
};
use std::{hint::black_box, time::Instant};

#[test]
#[ignore = "manual snapshot benchmark; timings depend on hardware and system load"]
fn snapshot_timing() {
    let mut document = Document::new(2048, 2048);
    for n in 1..4 {
        document
            .layers
            .push(Layer::paint(format!("Layer {n}"), 2048, 2048));
    }
    for (n, layer) in document.layers.iter_mut().enumerate() {
        for pixel in layer.image.as_mut().unwrap().pixels_mut() {
            *pixel = Rgba([n as u8 * 40, 80, 150, 255]);
        }
    }
    let mut samples = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        let snapshot = black_box(document.clone());
        samples.push(start.elapsed().as_secs_f64() * 1000.);
        black_box(snapshot);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{{\"case\":\"snapshot_4x2048x2048\",\"median_ms\":{:.6}}}",
        samples[10]
    );
    let mut editor = Editor::new(document);
    let id = editor.active_layer.clone();
    let start = Instant::now();
    for n in 0..20 {
        assert!(editor.rename_layer(&id, &format!("Name {n}")));
    }
    println!(
        "{{\"case\":\"20_metadata_edits_4x2048x2048\",\"ms\":{:.6},\"retained_bytes\":{},\"undo_depth\":{}}}",
        start.elapsed().as_secs_f64() * 1000.,
        editor.history_bytes(),
        editor.undo_depth()
    );
}
