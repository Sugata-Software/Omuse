//! Native retouch runs behind the shared image-operation admission slot.
//! Cancellation keeps that slot until the worker exits; no pixels are published
//! until the editor verifies the captured document, selection and target.
use super::*;
use omuse::retouch_brush::RetouchMode;
use std::sync::atomic::AtomicBool;

impl EditorView {
    pub(super) fn start_background_retouch(
        &mut self,
        points: Vec<(f32, f32)>,
        mode: RetouchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.dialog != Dialog::None {
            return;
        }
        let Some(cancel) = self.begin_photo_io(cx) else {
            return;
        };
        let mask = self.paint_mask;
        let request = match self.editor.prepare_retouch(points, mode, mask) {
            Ok(request) => request,
            Err(error) => {
                // No worker was launched, so there is nothing left to drain.
                self.photo_io = None;
                self.busy = false;
                self.status = format!("Retouch: {error}");
                cx.notify();
                return;
            }
        };
        let identity = (
            self.dialog,
            self.dialog_generation,
            self.create.epoch,
            self.editor.revision(),
        );
        let action = match mode {
            RetouchMode::Blur => "Blurring",
            RetouchMode::Smudge => "Smudging",
            RetouchMode::Liquify => "Liquifying",
        };
        self.status = format!(
            "{action} {}… Esc to cancel",
            if mask { "mask" } else { "pixels" }
        );
        self.focus.focus(window, cx);
        cx.notify();
        let worker_cancel = cancel.clone();
        let task = cx.background_executor().spawn(async move {
            request
                .compute(&worker_cancel)
                .map_err(|error| format!("{error:#}"))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.finish_background_retouch(cancel, identity, mask, result, cx);
            });
        })
        .detach();
    }

    fn finish_background_retouch(
        &mut self,
        cancel: Arc<AtomicBool>,
        identity: (Dialog, u64, u64, u64),
        mask: bool,
        result: Result<omuse::editor::PreparedRetouch, String>,
        cx: &mut Context<Self>,
    ) {
        if !self.finish_photo_io(&cancel, identity) {
            cx.notify();
            return;
        }
        // The mask/raster toggle is UI state rather than an editor revision.
        if self.paint_mask != mask {
            self.status = "Retouch discarded because the editing target changed.".into();
            cx.notify();
            return;
        }
        let applied = result.and_then(|prepared| {
            self.editor
                .apply_prepared_retouch(prepared, &cancel)
                .map_err(|error| format!("{error:#}"))
        });
        match applied {
            Ok(true) => {
                self.status = if mask {
                    "Mask retouch stroke applied"
                } else {
                    "Retouch stroke applied"
                }
                .into();
                self.changed(cx);
            }
            Ok(false) => {
                self.status = "Retouch stroke made no change".into();
                cx.notify();
            }
            Err(error) => {
                self.status = format!("Retouch: {error}");
                cx.notify();
            }
        }
    }
}

#[cfg(all(test, feature = "ui-test"))]
#[path = "retouch_ui_tests.rs"]
mod tests;
