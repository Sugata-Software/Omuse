//! Authored native16 qualification probe. No external photograph/provider/files.
//! Build centrally, then run the executable itself under /usr/bin/time -v so
//! compiler memory is excluded. Its whole-process peak includes authored input
//! construction, one history result, grading, sampling and display compositing.
use image::Rgba;
use omuse::{
    advanced::LayerState,
    camera_raw::{self, SampleStage, Settings},
    editor::Editor,
    model::Document,
    precision::{Rgba16Image, TiledImage16, WorkingSpace},
    raster,
};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};
fn main() -> anyhow::Result<()> {
    let width: u32 = std::env::args()
        .nth(1)
        .as_deref()
        .unwrap_or("3072")
        .parse()?;
    let height: u32 = std::env::args()
        .nth(2)
        .as_deref()
        .unwrap_or("2560")
        .parse()?;
    let cancel = AtomicBool::new(false);
    camera_raw::native::work_bytes((width, height))?;
    let start = Instant::now();
    let authored = Rgba16Image::from_fn(width, height, |x, y| {
        let nx = f64::from(x) / f64::from(width.max(2) - 1);
        let ny = f64::from(y) / f64::from(height.max(2) - 1);
        Rgba([
            (800. + nx * 60000.) as u16,
            (1000. + ny * 60000.) as u16,
            (2000. + (nx + ny) * 27000.) as u16,
            if x % 113 == 0 { 1 } else { 65535 },
        ])
    });
    let mut state = LayerState::from_rgba16(&authored, "Authored gradient/edge fixture")?;
    let source = Arc::new(TiledImage16::from_rgba16_in(
        &authored,
        WorkingSpace::DisplayP3,
    )?);
    drop(authored);
    state.source = source.clone();
    state.result = source.clone();
    state.recipe.working_space = WorkingSpace::DisplayP3;
    let mut document = Document::new(width, height);
    document.background = [0; 4];
    let id = document.layers[0].id.clone();
    document.layers[0].image = Some(state.proxy()?.into());
    document.layers[0].advanced = Some(Arc::new(state.clone()));
    let mut editor = Editor::new(document);
    editor.active_layer = id.clone();
    let (first, _) = state.with_native_camera_raw(
        Settings {
            exposure: 0.1,
            ..Settings::for_native_edit()
        },
        None,
        &cancel,
        None,
    )?;
    editor.replace_editable_states(vec![(id.clone(), first)])?;
    let input = editor.editable_state(&id)?;
    let mut settings = Settings {
        exposure: 0.3,
        texture: 17.,
        clarity: 12.,
        glow: 8.,
        grain_amount: 3.,
        ..Settings::for_native_edit()
    };
    settings.detail.noise_luminance = 30.;
    settings.detail.noise_color = 20.;
    settings.detail.sharpen_amount = 40.;
    settings.optics.distortion = 12.;
    settings.optics.remove_chromatic_aberration = true;
    settings.geometry.rotate = 2.;
    settings.geometry.scale = 2.;
    settings.geometry.constrain_crop = true;
    let admission = camera_raw::native::ui_admission(
        &editor.document,
        editor.history_bytes() as u64,
        (width, height),
        true,
        false,
        0,
    )?;
    let grade = Instant::now();
    let (prepared, sample) =
        input.with_native_camera_raw(settings, None, &cancel, Some(SampleStage::PointColor))?;
    let grade_ms = grade.elapsed().as_millis();
    let sample = sample.expect("native stage sample");
    anyhow::ensure!(
        sample.dimensions() == (width, height),
        "Sample extent differs"
    );
    let mut preview = Editor::new(editor.document.clone());
    preview.replace_editable_states(vec![(id.clone(), prepared.clone())])?;
    let composite = raster::composite(&preview.document);
    anyhow::ensure!(
        composite.dimensions() == (width, height),
        "Composite extent differs"
    );
    editor.replace_editable_states(vec![(id.clone(), prepared.clone())])?;
    anyhow::ensure!(
        editor.undo_depth() == 2,
        "Expected one history item per kept grade"
    );
    anyhow::ensure!(editor.undo(), "Undo failed");
    anyhow::ensure!(editor.redo(), "Redo failed");
    anyhow::ensure!(
        Arc::ptr_eq(&source, &prepared.source),
        "Retained original was replaced"
    );
    println!(
        "{}",
        serde_json::json!({"fixture":"original authored 16-bit P3 ramps and one-word-alpha strips","width":width,"height":height,"grade_ms":grade_ms,"total_ms":start.elapsed().as_millis(),"kernel_scratch_bound_bytes":admission.kernel_scratch_bytes,"conservative_ui_admission_bytes":admission.estimated_bytes,"history_bytes":editor.history_bytes(),"sample_word":sample.get_pixel(width/2,height/2).0,"result_word":prepared.result.get_pixel(width/2,height/2).0,"undo_redo":true,"source_arc_preserved":true,"limits":"Whole authored process; not a real-photo/decoder/GUI latency or colour-accuracy benchmark. Peak RSS must be read from external time output."})
    );
    Ok(())
}
