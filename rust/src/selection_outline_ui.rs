//! Coalesced display-only contour work. Selection snapshots are copied once per
//! selection generation and reused across zoom/pan requests; no mask scans occur
//! in canvas rendering or the marching-ants timer.
use super::*;
use omuse::{
    model::PixelRect,
    selection_outline::{self, Outline},
};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceKey {
    editor: u64,
    page: u64,
    revision: u64,
    width: u32,
    height: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Request {
    source: SourceKey,
    visible: PixelRect,
    cell: u32,
}

#[derive(Default)]
pub(super) struct SelectionContourCache {
    source_key: Option<SourceKey>,
    source: Option<Arc<Selection>>,
    requested: Option<Request>,
    completed: Option<Request>,
    pub(super) outline: Arc<Outline>,
    cancel: Arc<AtomicBool>,
    generation: u64,
}
impl Drop for SelectionContourCache {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl EditorView {
    fn contour_source_key(&self) -> Option<SourceKey> {
        self.editor.selection.as_ref().map(|s| SourceKey {
            editor: self.editor.instance_id(),
            page: self.create.epoch,
            revision: self.editor.selection_revision(),
            width: s.width,
            height: s.height,
        })
    }
    fn contour_request(&self) -> Option<Request> {
        let source = self.contour_source_key()?;
        let cell = selection_outline::display_cell(self.zoom);
        let viewport = self.viewport.get();
        let (vw, vh) = (
            f32::from(viewport.size.width),
            f32::from(viewport.size.height),
        );
        if !self.zoom.is_finite() || self.zoom <= 0. || vw <= 0. || vh <= 0. {
            return None;
        }
        // Transform viewport edges back into source coordinates. Include one
        // cell of padding so panning does not expose a clipped edge between jobs.
        let cx = self.editor.document.width as f32 / 2. - self.pan.0 / self.zoom;
        let cy = self.editor.document.height as f32 / 2. - self.pan.1 / self.zoom;
        let left = (cx - vw / (2. * self.zoom)).floor().max(0.) as u32;
        let top = (cy - vh / (2. * self.zoom)).floor().max(0.) as u32;
        let right = ((cx + vw / (2. * self.zoom)).ceil().max(0.) as u32).min(source.width);
        let bottom = ((cy + vh / (2. * self.zoom)).ceil().max(0.) as u32).min(source.height);
        let (left, top) = (
            left.saturating_sub(cell) / cell * cell,
            top.saturating_sub(cell) / cell * cell,
        );
        let right = right.saturating_add(cell).min(source.width);
        let bottom = bottom.saturating_add(cell).min(source.height);
        Some(Request {
            source,
            cell,
            visible: PixelRect {
                x: left,
                y: top,
                width: right.saturating_sub(left),
                height: bottom.saturating_sub(top),
            },
        })
    }

    pub(super) fn contour_for_canvas(&self, cx: &mut Context<Self>) -> Arc<Outline> {
        let Some(request) = self.contour_request() else {
            let mut cache = self.selection_contour.borrow_mut();
            if cache.source_key.is_some() {
                cache.cancel.store(true, Ordering::Relaxed);
                *cache = SelectionContourCache::default();
            }
            return cache.outline.clone();
        };
        let mut cache = self.selection_contour.borrow_mut();
        if cache.source_key != Some(request.source) {
            cache.cancel.store(true, Ordering::Relaxed);
            cache.source = None;
            cache.source_key = Some(request.source);
            cache.outline = Arc::default();
            cache.completed = None;
            cache.requested = None;
        }
        if cache.requested == Some(request) {
            return cache.outline.clone();
        }
        if request.visible.width == 0 || request.visible.height == 0 {
            cache.cancel.store(true, Ordering::Relaxed);
            cache.requested = Some(request);
            cache.completed = Some(request);
            cache.outline = Arc::default();
            return cache.outline.clone();
        }
        cache.cancel.store(true, Ordering::Relaxed);
        cache.cancel = Arc::new(AtomicBool::new(false));
        cache.generation = cache.generation.wrapping_add(1);
        cache.requested = Some(request);
        let generation = cache.generation;
        let cancel = cache.cancel.clone();
        let shown = cache.outline.clone();
        drop(cache);
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(90))
                .await;
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let source = view
                .update(cx, |this, _| {
                    if this.contour_request() != Some(request) {
                        let mut cache = this.selection_contour.borrow_mut();
                        if cache.generation == generation && cache.requested == Some(request) {
                            cache.requested = None;
                        }
                        return None;
                    }
                    let mut cache = this.selection_contour.borrow_mut();
                    if cache.generation != generation || cache.requested != Some(request) {
                        return None;
                    }
                    if cache.source.is_none() {
                        cache.source = this
                            .editor
                            .selection
                            .as_ref()
                            .map(|selection| Arc::new(selection.clone()));
                    }
                    cache.source.clone()
                })
                .ok()
                .flatten();
            let Some(source) = source else {
                return;
            };
            let worker_cancel = cancel.clone();
            let output = cx
                .background_executor()
                .spawn(async move {
                    selection_outline::generate(
                        &source,
                        request.cell,
                        request.visible,
                        selection_outline::MAX_POINTS,
                        &worker_cancel,
                    )
                })
                .await;
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let _ = view.update(cx, |this, cx| {
                if this.contour_request() != Some(request) {
                    this.contour_for_canvas(cx);
                    return;
                }
                let mut cache = this.selection_contour.borrow_mut();
                if cache.generation != generation || cache.requested != Some(request) {
                    return;
                }
                cache.completed = Some(request);
                cache.outline = Arc::new(output.unwrap_or_default());
                drop(cache);
                cx.notify();
            });
        })
        .detach();
        shown
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{TestAppContext, VisualTestContext};
    fn setup(cx: &mut TestAppContext) -> (Entity<EditorView>, &mut VisualTestContext) {
        cx.update(crate::init_test_theme);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.editor = Editor::new(Document::new(128, 96));
            view.dialog = Dialog::None;
            view.editor.select_rectangle(7., 9., 101., 72.);
            view.editor.record_selection_change(None);
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (view, cx)
    }
    fn complete(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
    }
    #[gpui_kit::test]
    fn contour_cache_reuses_a_selection_snapshot_for_zoom_and_paint_refresh(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = setup(cx);
        view.update(cx, |view, cx| {
            view.contour_for_canvas(cx);
        });
        complete(cx);
        let (source, before) = view.update(cx, |view, _| {
            let cache = view.selection_contour.borrow();
            assert!(cache.completed.is_some());
            assert!(!cache.outline.points.is_empty());
            (
                cache.source.as_ref().unwrap().clone(),
                view.editor.selection.clone(),
            )
        });
        view.update(cx, |view, cx| {
            view.zoom = 0.25;
            view.contour_for_canvas(cx);
        });
        complete(cx);
        view.update(cx, |view, cx| {
            view.refresh(cx);
            view.contour_for_canvas(cx);
            let cache = view.selection_contour.borrow();
            assert!(Arc::ptr_eq(&source, cache.source.as_ref().unwrap()));
            assert_eq!(view.editor.selection, before);
            assert!(cache.outline.cell_size > 1);
        });
    }
    #[gpui_kit::test]
    fn stale_contours_cannot_reappear_after_deselect_or_document_replacement(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = setup(cx);
        view.update(cx, |view, cx| {
            view.contour_for_canvas(cx);
            view.editor.clear_selection();
            view.contour_for_canvas(cx);
        });
        complete(cx);
        view.update(cx, |view, cx| {
            assert!(view.selection_contour.borrow().outline.points.is_empty());
            view.editor = Editor::new(Document::new(128, 96));
            view.editor.select_rectangle(80., 70., 10., 12.);
            view.editor.record_selection_change(None);
            view.contour_for_canvas(cx);
        });
        complete(cx);
        cx.update(|_, cx| {
            let view = view.read(cx);
            let cache = view.selection_contour.borrow();
            assert!(!cache.outline.points.is_empty());
            assert!(
                cache
                    .outline
                    .points
                    .iter()
                    .all(|&(x, y)| (80..90).contains(&x) && (70..82).contains(&y))
            );
        });
    }
}
