//! Canonical active Editor owners and inactive Create owners are disjoint
//! admission contracts. Job/render reserves retain their legacy values.
use image::Rgba;
use omuse::{
    advanced::LayerState,
    advanced_ops::{AdvancedOperation, FilterNode},
    camera_raw::native,
    create_project::Project,
    editor::Editor,
    filters::Filter,
    model::Document,
    precision::Rgba16Image,
};
use std::sync::{Arc, atomic::AtomicBool};
fn editor() -> Editor {
    let pixels = Rgba16Image::from_fn(24, 18, |x, y| {
        Rgba([
            1001 + x as u16 * 181,
            21003 + y as u16 * 137,
            45007,
            [1, 12345, 65535][((x + y) % 3) as usize],
        ])
    });
    let mut state = LayerState::from_rgba16(&pixels, "owned admission").unwrap();
    let mut raw = Vec::with_capacity(8192);
    raw.extend_from_slice(b"authored retained raw ownership");
    state.raw_bytes = Some(Arc::new(raw));
    state.recipe.raw_extension = Some("dng".into());
    state.recipe.raw_settings = Some(Default::default());
    let mut doc = Document::new(24, 18);
    doc.layers[0].image = Some(state.proxy().unwrap().into());
    doc.layers[0].advanced = Some(Arc::new(state));
    Editor::new(doc)
}
fn check_owners(editor: &Editor, outside: u64) -> native::UiAdmission {
    let selection = editor.selection.as_ref().map_or(0, |s| s.mask.capacity()) as u64;
    let legacy = native::ui_admission_with_context(
        &editor.document,
        editor.history_bytes() as u64,
        (24, 18),
        true,
        true,
        selection,
        outside,
    )
    .unwrap();
    let owned =
        native::ui_admission_for_editor(editor, (24, 18), true, true, selection, outside).unwrap();
    let canonical = editor.resident_memory_estimate().unwrap();
    assert_eq!(owned.active_editor_bytes, canonical);
    assert_eq!((owned.existing_document_bytes, owned.history_bytes), (0, 0));
    assert_eq!(legacy.active_editor_bytes, 0);
    assert_eq!(owned.contextual_bytes, outside);
    assert_eq!(owned.renderer_reserve_bytes, legacy.renderer_reserve_bytes);
    assert_eq!(owned.kernel_scratch_bytes, legacy.kernel_scratch_bytes);
    assert_eq!(
        owned.estimated_bytes,
        legacy.estimated_bytes - legacy.existing_document_bytes - legacy.history_bytes + canonical,
        "Only overlapping retained owner accounting changes; job/capture/render reserves stay identical"
    );
    owned
}
#[test]
fn retained_grade_undo_and_redo_are_counted_once_without_trimming_history() {
    let mut editor = editor();
    let before = check_owners(&editor, 0);
    let state = editor.editable_state(&editor.active_layer).unwrap();
    let mut request = state.clone();
    request.recipe.nodes.push(FilterNode {
        id: "admission-grade".into(),
        name: "Exposure".into(),
        enabled: true,
        opacity: 1.,
        soft_mask: None,
        operation: AdvancedOperation::Filter(Filter::Exposure { stops: 0.35 }),
    });
    let prepared = request.evaluate(&AtomicBool::new(false)).unwrap();
    editor
        .replace_editable_states(vec![(editor.active_layer.clone(), prepared)])
        .unwrap();
    assert_eq!(editor.undo_depth(), 1);
    let graded = check_owners(&editor, 0);
    assert!(graded.active_editor_bytes > before.active_editor_bytes);
    assert!(editor.undo());
    assert_eq!(editor.redo_depth(), 1);
    check_owners(&editor, 0);
    assert!(editor.redo());
    assert_eq!(editor.undo_depth(), 1);
    check_owners(&editor, 0);
    assert_eq!(
        editor.document.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .raw_bytes
            .as_ref()
            .unwrap()
            .as_slice(),
        b"authored retained raw ownership"
    );
}
#[test]
fn create_context_and_job_reserves_are_never_subtracted_or_allowed_to_overflow() {
    let editor = editor();
    let plain = check_owners(&editor, 0);
    let mut project = Project::new("Outside current Editor", editor.document.clone());
    project.add_page("Inactive", Document::new(17, 13)).unwrap();
    let prototype = project.active_document().unwrap().layers[0].clone();
    project
        .define_component("Prototype", vec![prototype])
        .unwrap();
    project
        .add_resource("Owned bytes", "application/octet-stream", vec![7; 4096])
        .unwrap();
    let outside = project.resident_memory_estimate().unwrap().bytes;
    let contextual = check_owners(&editor, outside);
    assert_eq!(contextual.estimated_bytes, plain.estimated_bytes + outside);
    assert!(native::ui_admission_for_editor(&editor, (24, 18), true, true, 0, u64::MAX).is_err());
    assert!(native::ui_admission_for_editor(&editor, (0, 18), true, false, 0, 0).is_err());
    assert!(native::ui_admission_for_editor(&editor, (4097, 4097), true, false, 0, 0).is_err());
    assert_eq!(native::MAX_NATIVE_UI_ESTIMATE_BYTES, 2 * 1024 * 1024 * 1024);
}
