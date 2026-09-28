use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn setup(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::init_test_theme(cx);
        install_shortcuts(&Shortcuts::default(), &Shortcuts::default(), cx);
    });
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = EditorView::new(None, window, cx);
        let mut doc = Document::new(40, 30);
        doc.layers[0].image =
            Some(image::RgbaImage::from_pixel(40, 30, image::Rgba([80, 90, 100, 255])).into());
        view.editor = Editor::new(doc);
        view.refresh(cx);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    (view, cx)
}
fn click_id(cx: &mut VisualTestContext, id: &'static str) {
    let b = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("missing {id}"));
    cx.simulate_click(b.center(), Modifiers::default());
    draw(cx);
}

#[gpui_kit::test]
fn vector_mask_add_smooth_apply_undo_and_cancel(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    let initial = view.update(cx, |v, _| v.editor.revision());
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(true, window, cx)));
    draw(cx);
    let source_bounds = view.update(cx, |v, _| {
        v.vector_draft.as_ref().unwrap().preview_bounds.get()
    });
    for (x, y) in [(0.2, 0.2), (0.8, 0.2), (0.5, 0.8)] {
        let p = point(
            source_bounds.origin.x + px(f32::from(source_bounds.size.width) * x),
            source_bounds.origin.y + px(f32::from(source_bounds.size.height) * y),
        );
        cx.simulate_click(p, Modifiers::default());
        draw(cx);
    }
    click_id(cx, "vector-smooth");
    click_id(cx, "vector-close");
    click_id(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let cancel_revision = view.update(cx, |v, _| {
        let layer = v
            .editor
            .document
            .find_layer(&v.editor.active_layer)
            .unwrap();
        let state = layer.advanced.as_ref().unwrap();
        assert!(state.recipe.vector_is_mask);
        assert_eq!(
            state.recipe.vector.as_ref().unwrap().subpaths[0]
                .anchors
                .len(),
            3
        );
        assert!(layer.mask.is_some());
        assert_eq!(v.editor.undo_depth(), 1);
        assert!(v.editor.undo());
        // Undo restores the document revision stored with the snapshot.
        assert_eq!(v.editor.revision(), initial);
        assert!(v.editor.document.layers[0].advanced.is_none());
        assert!(v.editor.document.layers[0].mask.is_none());
        v.editor.revision()
    });
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(false, window, cx)));
    draw(cx);
    let preview = cx.debug_bounds("vector-path-preview").unwrap();
    cx.simulate_click(preview.center(), Modifiers::default());
    draw(cx);
    click_id(cx, "cancel-dialog");
    view.update(cx, |v, _| assert_eq!(v.editor.revision(), cancel_revision));
}

#[gpui_kit::test]
fn path_styling_recomputes_retained_filters_and_survives_reopen(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx);
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_vector(false, window, cx)));
    view.update(cx, |v, _| {
        let draft = v.vector_draft.as_mut().unwrap();
        draft.path = VectorPath {
            subpaths: vec![Subpath {
                closed: true,
                anchors: [(5., 5.), (35., 5.), (35., 25.), (5., 25.)]
                    .into_iter()
                    .map(|(x, y)| Anchor {
                        position: VectorPoint { x, y },
                        incoming: None,
                        outgoing: None,
                    })
                    .collect(),
            }],
            fill_rule: Default::default(),
        };
        draft
            .state
            .recipe
            .nodes
            .push(omuse::advanced_ops::FilterNode {
                id: "inversion".into(),
                name: "Invert".into(),
                enabled: true,
                opacity: 1.,
                soft_mask: None,
                operation: omuse::advanced_ops::AdvancedOperation::Filter(
                    omuse::filters::Filter::Invert,
                ),
            });
    });
    for (index, value) in ["#102040FF", "#80A020FF", "2"].into_iter().enumerate() {
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.detail_inputs[index].update(cx, |input, cx| input.set_value(value, window, cx))
            })
        });
    }
    draw(cx);
    click_id(cx, "vector-style-preview");
    click_id(cx, "confirm-dialog");
    cx.run_until_parked();
    draw(cx);
    let document = view.update(cx, |v, _| {
        assert_eq!(v.dialog, Dialog::None, "{}", v.status);
        let state = v.editor.document.layers[0].advanced.as_ref().unwrap();
        assert_eq!(state.recipe.vector_fill, [16, 32, 64, 255]);
        assert_eq!(state.recipe.vector_stroke.unwrap().width, 2.);
        assert_eq!(
            state.proxy().unwrap().get_pixel(20, 15).0,
            [239, 223, 191, 255]
        );
        assert_eq!(v.editor.undo_depth(), 1);
        v.editor.document.clone()
    });
    let tmp = tempfile::tempdir().unwrap();
    let package = tmp.path().join("styled.comp");
    omuse::document::save(&document, &package).unwrap();
    let reopened = omuse::document::open(&package).unwrap();
    let state = reopened.layers[0].advanced.as_ref().unwrap();
    assert_eq!(
        state.recipe.vector_stroke.unwrap().color,
        [128, 160, 32, 255]
    );
    assert_eq!(
        state.proxy().unwrap(),
        document.layers[0]
            .advanced
            .as_ref()
            .unwrap()
            .proxy()
            .unwrap()
    );
    view.update(cx, |v, _| {
        assert!(v.editor.undo());
        assert!(v.editor.document.layers[0].advanced.is_none());
    });
}
