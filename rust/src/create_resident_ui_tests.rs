use super::*;
#[test]
fn create_context_counts_inactive_editors_and_both_checkpoint_owners() {
    let active = Editor::new(Document::new(8, 6));
    let project = Project::new("Budget", active.document.clone());
    let mut session = CreateSession::new(project);
    let plain = session.contextual_resident_bytes().unwrap();
    let id = session.project.add_blank_page("Inactive", 8, 6).unwrap();
    let mut inactive = Editor::new(Document::new(8, 6));
    inactive.select_rect(1, 1, 3, 2);
    assert!(inactive.record_selection_change(None));
    let editor_bytes = inactive.resident_memory_estimate().unwrap();
    session.editors.insert(id, inactive);
    let with_inactive = session.contextual_resident_bytes().unwrap();
    assert!(with_inactive >= plain.saturating_add(editor_bytes));
    let checkpoint = session.project.clone();
    let per = checkpoint.resident_memory_estimate().unwrap().bytes;
    session.undo.push_back(CollectionUndo {
        project: checkpoint.clone(),
        expected: Some(checkpoint.clone()),
        structural: false,
    });
    assert!(
        session.contextual_resident_bytes().unwrap()
            >= with_inactive.saturating_add(per.saturating_mul(2))
    );
    let before = session.contextual_resident_bytes().unwrap();
    let entry = session.undo.pop_back().unwrap();
    session.redo.push_back(entry);
    assert!(session.contextual_resident_bytes().unwrap() >= before);
    assert_eq!(session.editors.values().next().unwrap().undo_depth(), 1);
}
#[test]
fn checked_out_current_document_is_owned_by_the_active_editor_not_loaded_by_estimation() {
    let active = Editor::new(Document::new(8, 6));
    let mut session = CreateSession::new(Project::new("Checked out", active.document.clone()));
    let before = session.contextual_resident_bytes().unwrap();
    session.checkout_editors(&active).unwrap();
    let after = session.contextual_resident_bytes().unwrap();
    assert!(after < before);
    let estimate = session.project.resident_memory_estimate().unwrap();
    assert_eq!(estimate.checked_out_pages, 1);
    assert_eq!(estimate.loaded_documents, 0);
    assert!(session.project.resident_memory_estimate().is_ok());
    assert_eq!(active.undo_depth(), 0);
}
