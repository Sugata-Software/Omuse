use super::*;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn recent_upgrade_persists_canvas_collection_order_and_keyboard_search(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    cx.update(|cx| install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx));
    let temp = tempfile::tempdir().unwrap();
    let canvas = temp.path().join("Product canvas.omuse");
    let collection = temp.path().join("Summer collection.omuse");
    document::save(&Document::new(8, 8), &canvas).unwrap();
    let mut project = omuse::create_project::Project::new("Summer", Document::new(7, 9));
    project.save(&collection).unwrap();
    let store = temp.path().join("recent.json");
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut v = EditorView::new(None, window, cx);
        v.dialog = Dialog::None;
        v.recovery = Recovery::at(temp.path().join("recovery"));
        v.editor = Editor::new(Document::new(8, 8));
        v.recent.store = Some(store.clone());
        v.note_recent(canvas.clone(), cx);
        v.note_recent(collection.clone(), cx);
        v.note_recent(canvas.clone(), cx);
        v.focus.focus(window, cx);
        v
    });
    cx.run_until_parked();
    let saved = RecentProjects::load(&store).unwrap();
    assert_eq!(saved.paths, vec![canvas.clone(), collection.clone()]);
    view.update(cx, |v, cx| {
        v.recent.history = RecentProjects::default();
        v.refresh_recent(cx);
    });
    cx.run_until_parked();
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.simulate_keystrokes("ctrl-alt-o");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_input("summer");
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.dialog, Dialog::Recent);
        assert_eq!(v.recent_results(cx), vec![collection.clone()]);
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.path.as_ref(), Some(&collection));
        assert!(v.create.session.is_some());
        assert_eq!(v.dialog, Dialog::None);
        assert_eq!(v.recent.history.paths.first(), Some(&collection));
    });
}

#[gpui_kit::test]
fn recent_upgrade_cancel_preserves_unsaved_work_and_failed_open_is_not_promoted(
    cx: &mut TestAppContext,
) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("Missing.omuse");
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut v = EditorView::new(None, window, cx);
        v.dialog = Dialog::None;
        v.recovery = Recovery::at(temp.path().join("recovery"));
        v.editor = Editor::new(Document::new(8, 8));
        v.editor.add_layer("Keep my edits");
        v.refresh(cx);
        v
    });
    view.update_in(cx, |v, window, cx| {
        v.choose_recent(target.clone(), window, cx);
        assert_eq!(v.dialog, Dialog::Unsaved);
        assert!(matches!(&v.pending,Some(Pending::OpenPath(p)) if *p==target));
    });
    cx.simulate_keystrokes("escape");
    view.update_in(cx, |v, window, cx| {
        assert_eq!(v.dialog, Dialog::None);
        assert!(v.pending.is_none());
        assert!(v.has_unsaved_work());
        assert!(
            v.editor
                .document
                .layers
                .iter()
                .any(|l| l.name == "Keep my edits")
        );
        v.editor.mark_saved();
        v.choose_recent(target.clone(), window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(v.path.is_none());
        assert!(v.recent.history.paths.is_empty());
        assert!(
            v.editor
                .document
                .layers
                .iter()
                .any(|l| l.name == "Keep my edits")
        );
        assert!(!v.busy);
    });
}

#[gpui_kit::test]
fn recent_upgrade_clear_repairs_corrupt_history_without_deleting_projects(cx: &mut TestAppContext) {
    cx.update(crate::init_test_theme);
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path().join("recent.json");
    let project = temp.path().join("Keep.omuse");
    document::save(&Document::new(4, 4), &project).unwrap();
    std::fs::write(&store, b"{invalid").unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut v = EditorView::new(None, window, cx);
        v.dialog = Dialog::None;
        v.recent.store = Some(store.clone());
        v.refresh_recent(cx);
        v
    });
    cx.run_until_parked();
    view.update(cx, |v, cx| {
        assert!(!v.recent.message.is_empty());
        v.clear_recent(cx);
    });
    cx.run_until_parked();
    assert!(RecentProjects::load(&store).unwrap().paths.is_empty());
    assert!(document::open(&project).is_ok());
    cx.update(|_, cx| assert!(view.read(cx).recent.message.is_empty()));
}
