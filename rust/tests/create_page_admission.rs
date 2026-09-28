use omuse::{
    create_history::project_documents_match,
    create_project::{MAX_TOTAL_PAGE_CANVAS_PIXELS, Project},
    model::Document,
};

fn sparse_page(width: u32, height: u32) -> Document {
    // Large authored canvases may contain small placed objects. Keep the
    // fixture's pixel allocation tiny while exercising the real canvas limit.
    let mut document = Document::new(1, 1);
    document.width = width;
    document.height = height;
    document
}

#[test]
fn rejected_duplicate_preserves_a_collection_at_its_canvas_budget() {
    let mut project = Project::new("Full campaign", sparse_page(8_000, 8_000));
    for number in 2..=8 {
        project
            .add_page(format!("Page {number}"), sparse_page(8_000, 8_000))
            .unwrap();
    }
    assert_eq!(8 * 8_000 * 8_000, MAX_TOTAL_PAGE_CANVAS_PIXELS);
    project.validate().unwrap();

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Full-campaign.omuse");
    project.save(&path).unwrap();
    let mut project = Project::open(&path).unwrap();
    let mut before = project.clone();
    let active = project.active_page_id().to_owned();
    let summaries = project.page_summaries();
    assert!(summaries.iter().all(|page| !page.is_loaded));

    let error = project.duplicate_page(&active).unwrap_err();
    assert!(error.to_string().contains("canvas pixel budget"));
    assert_eq!(project.active_page_id(), active);
    assert_eq!(project.page_summaries(), summaries);
    project.validate().unwrap();
    assert!(project_documents_match(&mut project, &mut before).unwrap());
    // A rejected duplicate must not leave the collection unsavable.
    project.save(&path).unwrap();
}

#[test]
fn rejected_duplicate_preserves_long_ascii_and_unicode_names() {
    for name in ["A".repeat(2_044), "名".repeat(682), "A".repeat(2_048)] {
        let mut project = Project::new("Long page name", Document::new(2, 2));
        let active = project.active_page_id().to_owned();
        project.set_page_name(&active, name).unwrap();
        let summaries = project.page_summaries();
        let mut before = project.clone();

        let error = project.duplicate_page(&active).unwrap_err();
        assert!(error.to_string().contains("page name"));
        assert_eq!(project.active_page_id(), active);
        assert_eq!(project.page_summaries(), summaries);
        project.validate().unwrap();
        assert!(project_documents_match(&mut project, &mut before).unwrap());
    }
}

#[test]
fn duplicate_accepts_a_generated_name_exactly_at_the_limit() {
    let mut project = Project::new("Boundary name", Document::new(2, 2));
    let active = project.active_page_id().to_owned();
    project.set_page_name(&active, "A".repeat(2_043)).unwrap();
    let duplicate = project.duplicate_page(&active).unwrap();
    assert_eq!(project.page_document(&duplicate).unwrap().name.len(), 2_048);
    project.validate().unwrap();
}

#[test]
fn adding_a_page_with_invalid_dimensions_preserves_the_collection() {
    let mut project = Project::new("Valid campaign", Document::new(2, 2));
    let summaries = project.page_summaries();
    let error = project
        .add_page("Invalid imported page", sparse_page(0, 8))
        .unwrap_err();
    assert!(error.to_string().contains("page dimensions"));
    assert_eq!(project.page_summaries(), summaries);
    project.validate().unwrap();
}
