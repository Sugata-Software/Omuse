use image::{Rgba, RgbaImage};
use omuse::{create_project::Project, model::Document};

fn document(width: u32, height: u32, color: [u8; 4]) -> Document {
    let mut document = Document::new(width, height);
    document.layers[0].image = Some(RgbaImage::from_pixel(width, height, Rgba(color)).into());
    document
}

fn first_pixel(document: &Document) -> [u8; 4] {
    document.layers[0].image.as_ref().unwrap().get_pixel(0, 0).0
}

#[test]
fn pre_checkout_snapshot_is_complete_and_frozen() {
    let active = document(12, 9, [10, 20, 30, 255]);
    let mut project = Project::new("Campaign", active.clone());
    let first = project.active_page_id().to_owned();
    let second = project
        .add_page("Second", document(7, 11, [90, 80, 70, 255]))
        .unwrap();
    let mut snapshot = project.clone();

    project
        .checkout_page_documents(&[(&first, &active)])
        .unwrap();
    assert!(project.page_document(&first).is_err());
    assert!(project.validate().is_err());

    assert_eq!(
        first_pixel(snapshot.page_document(&first).unwrap()),
        [10, 20, 30, 255]
    );
    assert_eq!(
        first_pixel(snapshot.page_document(&second).unwrap()),
        [90, 80, 70, 255]
    );
    snapshot.validate().unwrap();

    let mut edited = active;
    edited.layers[0]
        .image
        .as_mut()
        .unwrap()
        .put_pixel(0, 0, Rgba([1, 2, 3, 255]));
    assert_eq!(
        first_pixel(snapshot.page_document(&first).unwrap()),
        [10, 20, 30, 255]
    );
}

#[test]
fn checked_out_clone_can_be_restored_saved_and_reopened_exactly() {
    let first_document = document(10, 8, [12, 34, 56, 255]);
    let second_document = document(6, 13, [210, 110, 40, 255]);
    let mut project = Project::new("Restorable", first_document.clone());
    let first = project.active_page_id().to_owned();
    let second = project.add_page("Tall", second_document.clone()).unwrap();
    project
        .checkout_page_documents(&[(&first, &first_document), (&second, &second_document)])
        .unwrap();

    let mut restored = project.clone();
    restored
        .replace_page_document(&first, first_document.clone())
        .unwrap();
    restored
        .replace_page_document(&second, second_document.clone())
        .unwrap();
    restored.validate().unwrap();

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Restored.omuse");
    restored.save(&path).unwrap();
    let mut reopened = Project::open(&path).unwrap();
    assert_eq!(
        reopened.page_document(&first).unwrap().layers[0].image,
        first_document.layers[0].image
    );
    assert_eq!(
        reopened.page_document(&second).unwrap().layers[0].image,
        second_document.layers[0].image
    );
}

#[test]
fn rejected_checkout_batches_are_atomic() {
    let first_document = document(9, 7, [20, 40, 60, 255]);
    let second_document = document(5, 6, [80, 100, 120, 255]);
    let mut project = Project::new("Atomic", first_document.clone());
    let first = project.active_page_id().to_owned();
    let second = project.add_page("Second", second_document.clone()).unwrap();
    let summaries = project.page_summaries();

    assert!(
        project
            .checkout_page_documents(&[(&first, &first_document), ("missing", &second_document)])
            .is_err()
    );
    assert_eq!(project.page_summaries(), summaries);
    project.validate().unwrap();
    assert_eq!(
        first_pixel(project.page_document(&first).unwrap()),
        [20, 40, 60, 255]
    );

    assert!(
        project
            .checkout_page_documents(&[(&first, &first_document), (&first, &first_document)])
            .is_err()
    );
    assert_eq!(project.page_summaries(), summaries);
    project.validate().unwrap();

    let mut invalid = second_document.clone();
    invalid.width = 0;
    assert!(
        project
            .checkout_page_documents(&[(&first, &first_document), (&second, &invalid)])
            .is_err()
    );
    assert_eq!(project.page_summaries(), summaries);
    project.validate().unwrap();
    assert_eq!(
        first_pixel(project.page_document(&second).unwrap()),
        [80, 100, 120, 255]
    );
}

#[test]
fn checkout_updates_page_dimensions_without_losing_page_metadata() {
    let original = document(8, 6, [4, 5, 6, 255]);
    let mut project = Project::new("Resize", original);
    let page = project.active_page_id().to_owned();
    project.set_page_name(&page, "Portrait").unwrap();
    project
        .set_page_template(&page, Some("portrait-template"))
        .unwrap();
    let resized = document(17, 23, [7, 8, 9, 255]);

    project
        .checkout_page_documents(&[(&page, &resized)])
        .unwrap();
    let summary = &project.page_summaries()[0];
    assert_eq!(summary.id, page);
    assert_eq!(summary.name, "Portrait");
    assert_eq!((summary.width, summary.height), (17, 23));
    assert_eq!(summary.template_id.as_deref(), Some("portrait-template"));

    project.replace_page_document(&page, resized).unwrap();
    project.validate().unwrap();
    assert_eq!(project.active_document().unwrap().name, "Portrait");
}

#[test]
fn incomplete_save_does_not_replace_an_existing_destination() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Existing.omuse");
    let mut existing = Project::new("Existing", document(4, 4, [1, 2, 3, 255]));
    existing.save(&path).unwrap();
    let existing_id = existing.active_page_id().to_owned();

    let current_document = document(12, 10, [200, 100, 50, 255]);
    let mut incomplete = Project::new("Incomplete", current_document.clone());
    let current_id = incomplete.active_page_id().to_owned();
    incomplete
        .checkout_page_documents(&[(&current_id, &current_document)])
        .unwrap();
    assert!(incomplete.save(&path).is_err());

    let mut reopened = Project::open(&path).unwrap();
    assert_eq!(reopened.title, "Existing");
    assert_eq!(reopened.active_page_id(), existing_id);
    assert_eq!(
        first_pixel(reopened.active_document().unwrap()),
        [1, 2, 3, 255]
    );
}

#[test]
fn lazy_snapshot_remains_readable_after_live_project_checkout_and_restore() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Lazy.omuse");
    let first_document = document(8, 8, [31, 41, 59, 255]);
    let second_document = document(9, 7, [26, 53, 58, 255]);
    let mut original = Project::new("Lazy", first_document.clone());
    let first = original.active_page_id().to_owned();
    let second = original
        .add_page("Second", second_document.clone())
        .unwrap();
    original.save(&path).unwrap();

    let mut live = Project::open(&path).unwrap();
    assert!(live.page_summaries().iter().all(|page| !page.is_loaded));
    let mut lazy_snapshot = live.clone();
    live.checkout_page_documents(&[(&first, &first_document)])
        .unwrap();
    assert!(live.page_document(&first).is_err());

    assert_eq!(
        first_pixel(lazy_snapshot.page_document(&first).unwrap()),
        [31, 41, 59, 255]
    );
    assert_eq!(
        first_pixel(lazy_snapshot.page_document(&second).unwrap()),
        [26, 53, 58, 255]
    );

    live.replace_page_document(&first, first_document).unwrap();
    assert_eq!(
        first_pixel(live.page_document(&second).unwrap()),
        [26, 53, 58, 255]
    );
    live.validate().unwrap();
}

#[test]
fn lazy_snapshot_keeps_old_pixels_after_live_project_saves_over_same_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Same-path.omuse");
    let old_document = document(8, 8, [11, 22, 33, 255]);
    let mut original = Project::new("Original", old_document.clone());
    let page = original.active_page_id().to_owned();
    original.save(&path).unwrap();

    let mut live = Project::open(&path).unwrap();
    let mut frozen_lazy_snapshot = live.clone();
    live.checkout_page_documents(&[(&page, &old_document)])
        .unwrap();

    let new_document = document(8, 8, [210, 120, 30, 255]);
    live.replace_page_document(&page, new_document).unwrap();
    live.title = "Replacement".into();
    live.save(&path).unwrap();

    assert_eq!(
        first_pixel(frozen_lazy_snapshot.page_document(&page).unwrap()),
        [11, 22, 33, 255],
        "checkout must freeze a shared lazy source before the live package is replaced"
    );
    let mut reopened = Project::open(&path).unwrap();
    assert_eq!(reopened.title, "Replacement");
    assert_eq!(
        first_pixel(reopened.page_document(&page).unwrap()),
        [210, 120, 30, 255]
    );
}

#[test]
fn fully_frozen_lazy_clone_traverses_and_saves_after_same_path_exchange() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("Frozen-source.omuse");
    let copy_path = directory.path().join("Frozen-copy.omuse");
    let first_document = document(8, 8, [8, 13, 21, 255]);
    let second_document = document(7, 9, [34, 55, 89, 255]);
    let mut original = Project::new("Frozen", first_document.clone());
    let first = original.active_page_id().to_owned();
    let second = original
        .add_page("Second", second_document.clone())
        .unwrap();
    let component = original
        .define_component("Badge", first_document.layers.clone())
        .unwrap();
    let resource = original
        .add_resource("Bytes", "application/octet-stream", vec![3, 1, 4, 1, 5, 9])
        .unwrap();
    original.save(&source_path).unwrap();

    let mut live = Project::open(&source_path).unwrap();
    let mut frozen = live.clone();
    // Saving the live clone over its source freezes every lazy value shared by
    // `frozen` before atomically exchanging the package directory.
    live.title = "New live title".into();
    live.save(&source_path).unwrap();

    let mut pages = Vec::new();
    frozen
        .for_each_page_document(|summary, document| {
            pages.push((summary.id.clone(), first_pixel(document)));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        pages,
        vec![
            (first.clone(), [8, 13, 21, 255]),
            (second.clone(), [34, 55, 89, 255])
        ]
    );
    assert_eq!(
        frozen.resource_bytes(&resource).unwrap(),
        &[3, 1, 4, 1, 5, 9]
    );
    let (_, component_layers) = frozen.component_snapshot(&component).unwrap();
    assert_eq!(
        component_layers[0]
            .image
            .as_ref()
            .unwrap()
            .get_pixel(0, 0)
            .0,
        [8, 13, 21, 255]
    );

    frozen.save(&copy_path).unwrap();
    let mut reopened = Project::open(&copy_path).unwrap();
    assert_eq!(reopened.title, "Frozen");
    assert_eq!(
        first_pixel(reopened.page_document(&first).unwrap()),
        [8, 13, 21, 255]
    );
    assert_eq!(
        first_pixel(reopened.page_document(&second).unwrap()),
        [34, 55, 89, 255]
    );
    assert_eq!(
        reopened.resource_bytes(&resource).unwrap(),
        &[3, 1, 4, 1, 5, 9]
    );
    let (_, reopened_component) = reopened.component_snapshot(&component).unwrap();
    assert_eq!(
        reopened_component[0]
            .image
            .as_ref()
            .unwrap()
            .get_pixel(0, 0)
            .0,
        [8, 13, 21, 255]
    );
}

#[test]
fn uncached_lazy_value_still_rejects_external_source_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("Guarded-source.omuse");
    let destination_path = directory.path().join("Preserved-destination.omuse");
    let mut source = Project::new("Source", document(6, 6, [10, 20, 30, 255]));
    let first = source.active_page_id().to_owned();
    source
        .add_page("Still lazy", document(5, 7, [40, 50, 60, 255]))
        .unwrap();
    source.save(&source_path).unwrap();

    let mut stale = Project::open(&source_path).unwrap();
    // Cache one page but deliberately leave the other page lazy.
    assert_eq!(
        first_pixel(stale.page_document(&first).unwrap()),
        [10, 20, 30, 255]
    );

    let mut external = Project::new("External", document(4, 4, [200, 10, 20, 255]));
    external.save(&source_path).unwrap();
    assert!(stale.for_each_page_document(|_, _| Ok(())).is_err());

    let mut destination = Project::new("Keep me", document(3, 3, [7, 8, 9, 255]));
    let destination_page = destination.active_page_id().to_owned();
    destination.save(&destination_path).unwrap();
    assert!(stale.save(&destination_path).is_err());

    let mut reopened = Project::open(&destination_path).unwrap();
    assert_eq!(reopened.title, "Keep me");
    assert_eq!(reopened.active_page_id(), destination_page);
    assert_eq!(
        first_pixel(reopened.active_document().unwrap()),
        [7, 8, 9, 255]
    );
}
