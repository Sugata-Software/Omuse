//! Coalesced filesystem checks never replace artwork while an edit is active.
use super::*;
use std::sync::atomic::Ordering;
#[cfg(all(test, feature = "ui-test"))]
#[path = "external_upgrade_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Notice {
    path: PathBuf,
    stamp: Option<u64>,
}
#[derive(Default)]
pub(super) struct ExternalState {
    sample: Option<Notice>,
    notice: Option<Notice>,
    ignored: Option<Notice>,
    checking: Option<(u64, PathBuf, u64)>,
}
impl EditorView {
    pub(super) fn start_external_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
                if view
                    .update_in(cx, |this, window, cx| {
                        if window.is_window_active() {
                            this.check_external_change(window, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn check_external_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.external.checking.is_some() || self.create.saving || self.photo_io.is_some() {
            return;
        }
        let (Some(path), Some(baseline)) = (self.path.clone(), self.live_stamp) else {
            return;
        };
        let identity = self.editor.instance_id();
        let key = (identity, path.clone(), baseline);
        self.external.checking = Some(key.clone());
        let worker_path = path.clone();
        let task = cx
            .background_executor()
            .spawn(async move { project_stamp(&worker_path) });
        cx.spawn_in(window, async move |view, cx| {
            let stamp = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if this.external.checking.as_ref() != Some(&key) {
                    return;
                }
                this.external.checking = None;
                if this.path.as_ref() != Some(&path)
                    || this.live_stamp != Some(baseline)
                    || this.editor.instance_id() != identity
                    || this.create.saving
                {
                    return;
                }
                if stamp == Some(baseline) {
                    this.external.sample = None;
                    this.external.notice = None;
                    this.external.ignored = None;
                    cx.notify();
                    return;
                }
                let candidate = Notice { path, stamp };
                // Two matching observations avoid reacting to an incomplete
                // external package write or an atomic directory exchange.
                if this.external.sample.as_ref() != Some(&candidate) {
                    this.external.sample = Some(candidate);
                    return;
                }
                if this.external.ignored.as_ref() == Some(&candidate) {
                    return;
                }
                this.external.notice = Some(candidate);
                if !this.has_unsaved_work()
                    && this.dialog == Dialog::None
                    && !this.busy
                    && this.inline_text.is_none()
                    && this.crop.is_none()
                    && this.drag_start.is_none()
                    && !this.numeric_scrubbing()
                    && this.transform_drag.is_none()
                    && this.editor.floating_selection_layer().is_none()
                    && stamp.is_some()
                {
                    this.dialog = Dialog::ExternalChange;
                    this.dialog_generation += 1;
                    this.reload_external(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn external_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(notice) = &self.external.notice else {
            return div().into_any_element();
        };
        if self.path.as_ref() != Some(&notice.path) || self.live_stamp == notice.stamp {
            return div().into_any_element();
        }
        let t = cx.omarchy().clone();
        div()
            .id("external-project-change")
            .debug_selector(|| "external-project-change".into())
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_1()
            .bg(t.selected_fill())
            .child(div().text_sm().child(if notice.stamp.is_some() {
                "This project changed on disk. Your current work is preserved."
            } else {
                "The saved project is temporarily unavailable. Your current work is preserved."
            }))
            .child(
                button(
                    "review-external-change",
                    "Review",
                    ButtonVariant::Outline,
                    cx,
                )
                .disabled(self.dialog != Dialog::None || self.busy || self.create.saving)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.finish_interaction(cx);
                    this.dialog = Dialog::ExternalChange;
                    this.dialog_generation += 1;
                    this.modal_focus.focus(window, cx);
                    cx.notify();
                })),
            )
            .into_any_element()
    }
    fn keep_external(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.external.ignored = self.external.notice.take();
        self.dialog = Dialog::None;
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn dismiss_external_notice(&mut self) {
        self.external.ignored = self.external.notice.take();
    }
    pub(super) fn external_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let available = self
            .external
            .notice
            .as_ref()
            .is_some_and(|n| n.stamp.is_some());
        let body=div().id("external-change-dialog").debug_selector(||"external-change-dialog".into())
            .w(px((f32::from(window.viewport_size().width)-40.).min(580.).max(240.)))
            .p_5().flex().flex_col().gap_3().bg(t.surface).border_1().border_color(t.control_border()).rounded(px(8.))
            .child(div().text_lg().child("Project changed on disk"))
            .child(div().text_sm().text_color(t.secondary).child(
                if self.has_unsaved_work(){"Keep editing, save your work as a separate copy, or discard your local edits and load the version on disk."}
                else{"The file was changed by another application. Reload the saved version, or keep your current view and save a separate copy."}))
            .child(div().text_sm().child(self.status.clone()))
            .child(div().flex().flex_wrap().gap_2()
                .child(button("external-keep","Keep editing",ButtonVariant::Outline,cx).disabled(self.busy)
                    .on_click(cx.listener(|this,_,window,cx|this.keep_external(window,cx))))
                .child(button("external-copy","Save a copy",ButtonVariant::Outline,cx).disabled(self.busy)
                    .on_click(cx.listener(|this,_,window,cx|{
                        let path=this.path.as_ref().map(|p|p.with_file_name(format!("{} copy.omuse",p.file_stem().unwrap_or_default().to_string_lossy())));
                        this.dialog=Dialog::None;this.save_dialog(false,window,cx);
                        if let Some(path)=path{this.path_input.update(cx,|input,cx|input.set_value(path.to_string_lossy(),window,cx));}
                    })))
                .child(button("external-reload",if self.has_unsaved_work(){"Discard local edits and reload"}else{"Reload from disk"},ButtonVariant::Primary,cx)
                    .disabled(self.busy||!available).on_click(cx.listener(|this,_,window,cx|this.reload_external(window,cx)))));
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .child(body.focus_trap("external-focus", &self.modal_focus))
            .into_any_element()
    }
    pub(super) fn reload_external(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(notice) = self.external.notice.clone() else {
            return;
        };
        if self.path.as_ref() != Some(&notice.path) || self.create.saving {
            return;
        }
        let Some(cancel) = self.begin_photo_io(cx) else {
            return;
        };
        let identity = (
            self.dialog,
            self.dialog_generation,
            self.create.epoch,
            self.editor.revision(),
        );
        let selected = self.selected_layer_ids();
        let active = self
            .create
            .session
            .as_ref()
            .map(|s| s.project.active_page_id().to_owned());
        self.status = "Reading the changed project…".into();
        cx.spawn_in(window, async move |view, cx| {
            let worker_cancel = cancel.clone();
            let path = notice.path.clone();
            let task = cx.background_executor().spawn(async move {
                let ((doc, project, pixels), stamp) =
                    omuse::save_guard::read_consistent(&path, || {
                        anyhow::ensure!(!worker_cancel.load(Ordering::Relaxed), "Reload cancelled");
                        let (mut doc, mut project) = Self::open_content(&path)?;
                        if let (Some(id), Some(project)) = (active, project.as_mut()) {
                            if project.set_active_page(&id).is_ok() {
                                doc = project.active_document()?.clone();
                            }
                        }
                        let pixels = raster::composite(&doc);
                        anyhow::ensure!(
                            pixels.dimensions() == (doc.width, doc.height),
                            "Reload could not render the project"
                        );
                        anyhow::ensure!(!worker_cancel.load(Ordering::Relaxed), "Reload cancelled");
                        Ok((doc, project, pixels))
                    })?;
                anyhow::ensure!(
                    stamp == notice.stamp,
                    "The project changed again. Review the newest version before reloading."
                );
                Ok::<_, anyhow::Error>((doc, project, pixels, stamp, path))
            });
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                if !this.finish_photo_io(&cancel, identity) {
                    return;
                }
                match result {
                    Ok((doc, project, pixels, stamp, path)) => {
                        this.install_opened_content(doc, project);
                        this.path = Some(path);
                        this.live_stamp = stamp;
                        let selected = selected
                            .into_iter()
                            .filter(|id| this.editor.document.find_layer(id).is_some())
                            .collect();
                        this.select_layer_ids(selected);
                        this.pixels = pixels;
                        this.present_pixels(cx);
                        this.external = ExternalState::default();
                        this.recovery.clear();
                        this.dialog = Dialog::None;
                        this.status = "Reloaded the saved project; view position preserved.".into();
                        this.focus.focus(window, cx);
                    }
                    Err(error) => {
                        this.status = format!("Reload left your work unchanged: {error:#}");
                        // Keep the review available, but do not repeatedly auto-reload
                        // the same corrupt or unsupported external snapshot.
                        this.external.ignored = this.external.notice.clone();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
