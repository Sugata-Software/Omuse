//! Native Create workspace. The canvas remains the editor; pages retain their
//! own editor history, while project operations have a separate reversible stack.
use super::inspector_ui::panel_input as input;
use super::inspector_ui::{
    panel_button as button, panel_header, panel_note, panel_section, panel_width,
};
use super::*;
use gpui_kit::{Div, FontWeight};
use omuse::create_project::Project;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct CreateSession {
    pub project: Project,
    pub(super) generation: u64,
    editors: BTreeMap<String, Editor>,
    dirty: bool,
    undo: VecDeque<CollectionUndo>,
    redo: VecDeque<CollectionUndo>,
    #[cfg(test)]
    pub(super) retain_synced_documents_for_test: bool,
}
#[derive(Clone)]
struct CollectionUndo {
    project: Project,
    expected: Option<Project>,
    structural: bool,
}
impl CreateSession {
    fn new(project: Project) -> Self {
        Self {
            project,
            generation: 0,
            editors: BTreeMap::new(),
            dirty: false,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            #[cfg(test)]
            retain_synced_documents_for_test: false,
        }
    }
    fn materialize(&self, editor: &Editor) -> anyhow::Result<Project> {
        let mut project = self.project.clone();
        project.replace_active_document(editor.document.clone())?;
        for (id, editor) in &self.editors {
            project.replace_page_document(id, editor.document.clone())?;
        }
        project.validate()?;
        Ok(project)
    }
    fn seal_checkpoint(&mut self, snapshot: &Project) {
        if let Some(entry) = self.undo.back_mut() {
            if !entry.structural && entry.expected.is_none() {
                entry.expected = Some(snapshot.clone());
            }
        }
    }
    fn sync(&mut self, editor: &Editor) -> anyhow::Result<()> {
        // Mutating collection operations require a complete Project. Prepare
        // it before replacing the structural cache, so a failed overlay cannot
        // leave a partially synchronized project or expected undo snapshot.
        let project = self.materialize(editor)?;
        self.seal_checkpoint(&project);
        self.project = project;
        Ok(())
    }
    fn checkout_editors(&mut self, editor: &Editor) -> anyhow::Result<()> {
        let active = self.project.active_page_id().to_owned();
        let mut documents = vec![(active.as_str(), &editor.document)];
        documents.extend(
            self.editors
                .iter()
                .map(|(id, editor)| (id.as_str(), &editor.document)),
        );
        self.project.checkout_page_documents(&documents)
    }
    fn snapshot(&mut self, editor: &Editor) -> anyhow::Result<Project> {
        let project = self.materialize(editor)?;
        self.checkout_editors(editor)?;
        self.seal_checkpoint(&project);
        Ok(project)
    }
    pub(super) fn checkpoint(&mut self) {
        debug_assert!(
            self.project.validate().is_ok(),
            "Collection checkpoint needs complete page documents"
        );
        self.generation = self.generation.wrapping_add(1);
        self.undo.push_back(CollectionUndo {
            project: self.project.clone(),
            expected: None,
            structural: false,
        });
        self.redo.clear();
        while self.undo.len() > 8 {
            self.undo.pop_front();
        }
        self.dirty = true;
    }
    pub(super) fn checkpoint_structure(&mut self) {
        self.checkpoint();
        self.undo.back_mut().unwrap().structural = true;
    }
    fn dirty(&self) -> bool {
        self.dirty || self.editors.values().any(Editor::is_dirty)
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum CreateTab {
    #[default]
    Pages,
    Templates,
    Brand,
    Assets,
    Motion,
    Export,
}

pub(super) struct CreateState {
    pub session: Option<CreateSession>,
    pub(super) design: create_design_ui::CreateDesignState,
    pub(super) motion: motion_ui::MotionUiState,
    pub(super) restore: restore_ui::RestoreState,
    pub saving: bool,
    tab: CreateTab,
    pub(super) fields: Vec<Entity<InputState>>,
    fields_identity: Option<(u64, u64)>,
    pub safe_areas: bool,
    pub safe_preset: omuse::social_preview::SafeAreaPreset,
    preflight_busy: bool,
    preflight: Option<(u64, u64, u64, Vec<omuse::social_preview::PreflightIssue>)>,
    pub phone_preview: bool,
    preview_cache: RefCell<Option<(u64, u64, Arc<RenderImage>)>>,
    pub(super) template_previews: create_previews::TemplatePreviews,
    pub epoch: u64,
    pub(super) bulk_job: Option<create_design_ui::BulkDesignJob>,
    pub(super) bulk_report: Option<String>,
    pub(super) job: Option<ContentJob>,
    pub(super) library: Option<omuse::asset_library::AssetLibrary>,
    pub(super) library_error: Option<String>,
}
pub(super) struct ContentJob {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) receiver: std::sync::mpsc::Receiver<ContentEvent>,
    pub(super) detail: String,
}
pub(super) enum ContentEvent {
    Progress(String),
    Finished(Result<PathBuf, String>),
}
impl Drop for ContentJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl CreateState {
    pub fn new(
        project: Option<Project>,
        window: &mut Window,
        cx: &mut Context<EditorView>,
    ) -> Self {
        let defaults = [
            "Untitled collection",
            "Page",
            "1080",
            "1350",
            "My brand",
            "#E9DCC6",
            "#202B34",
            "#D98566",
            "sans-serif",
            "",
            "",
            "",
            "3",
            "30",
            "",
            "",
            "",
            "0",
            "0",
            "700",
            "#D98566",
            "",
            "",
            "false",
            "16",
        ];
        let fields: Vec<_> = defaults
            .iter()
            .map(|v| cx.new(|cx| InputState::new(window, cx).default_value(*v)))
            .collect();
        // Prefix swatches belong to the inspector, not the text editor entity.
        // Repaint on typing and programmatic brand changes without applying a kit.
        for field in &fields[5..8] {
            cx.observe(field, |_, _, cx| cx.notify()).detach();
        }
        Self {
            session: project.map(CreateSession::new),
            design: create_design_ui::CreateDesignState::new(window, cx),
            motion: motion_ui::MotionUiState::new(window, cx),
            restore: restore_ui::RestoreState::new(window, cx),
            saving: false,
            tab: CreateTab::Pages,
            fields,
            fields_identity: None,
            safe_areas: false,
            safe_preset: Default::default(),
            preflight_busy: false,
            preflight: None,
            phone_preview: false,
            preview_cache: RefCell::new(None),
            template_previews: Default::default(),
            epoch: 0,
            bulk_job: None,
            bulk_report: None,
            job: None,
            library: None,
            library_error: None,
        }
    }
    pub(super) fn value(&self, index: usize, cx: &App) -> String {
        self.fields[index].read(cx).value().trim().to_string()
    }
}

pub(super) fn section(title: &str, cx: &App) -> Div {
    panel_section(title.to_owned(), cx)
}
pub(super) fn note(value: impl Into<SharedString>, cx: &App) -> Div {
    panel_note(value, cx)
}

impl EditorView {
    pub(super) fn sync_create_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let identity = (
            self.create.epoch,
            self.create
                .session
                .as_ref()
                .map_or(0, |session| session.generation),
        );
        if self.create.fields_identity == Some(identity) {
            return;
        }
        self.create.fields_identity = Some(identity);
        let doc = &self.editor.document;
        let title = self
            .create
            .session
            .as_ref()
            .map_or_else(|| doc.name.clone(), |session| session.project.title.clone());
        let mut values = vec![
            (0, title),
            (1, doc.name.clone()),
            (2, doc.width.to_string()),
            (3, doc.height.to_string()),
            (
                10,
                doc.metadata["omuseContent"]["caption"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            ),
            (
                11,
                doc.metadata["omuseContent"]["altText"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            ),
        ];
        if let Some(brand) = self
            .create
            .session
            .as_ref()
            .and_then(|session| session.project.active_brand())
        {
            values.push((4, brand.name.clone()));
            values.push((8, brand.fonts.heading.clone()));
            for (index, role) in [(5, "paper"), (6, "ink"), (7, "accent")] {
                if let Some(color) = brand.colors.get(role) {
                    values.push((
                        index,
                        format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]),
                    ));
                }
            }
            let body_font = brand.fonts.body.clone();
            self.create
                .design
                .brand_body_font
                .update(cx, |state, cx| state.set_value(body_font, window, cx));
        }
        for (index, value) in values {
            self.create.fields[index].update(cx, |state, cx| state.set_value(value, window, cx));
        }
    }
    /// Only called by the explicit native smoke-test journey with synthetic art.
    pub(super) fn prepare_create_inspection(
        &mut self,
        panel: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        use omuse::create::{instantiate_template, sugata_brand_kit};
        let brand = sugata_brand_kit();
        let mut project = Project::new(
            "Sugata field notes",
            instantiate_template("lesson-cover", Some(&brand))?,
        );
        project.add_brand(brand.clone())?;
        for index in 2..=6 {
            project.add_page(
                format!("{index:02} · A clear idea"),
                instantiate_template("lesson-step", Some(&brand))?,
            )?;
        }
        let document = project.active_document()?.clone();
        self.install_opened_content(document, Some(project));
        self.path = None;
        self.live_stamp = None;
        self.inspector_visible = true;
        self.inspector_tab = if panel == "assistant" {
            studio_ui::InspectorTab::Assistant
        } else {
            studio_ui::InspectorTab::Create
        };
        self.create.tab = match panel {
            "templates" => CreateTab::Templates,
            "content-export" => CreateTab::Export,
            "motion" => CreateTab::Motion,
            _ => CreateTab::Pages,
        };
        if panel == "templates" {
            self.load_template_previews(cx);
        }
        self.tool = Tool::Move;
        self.refresh(cx);
        Ok(())
    }
    pub(super) fn has_unsaved_work(&self) -> bool {
        self.editor.is_dirty()
            || self
                .create
                .session
                .as_ref()
                .is_some_and(CreateSession::dirty)
    }
    pub(super) fn sync_create(&mut self) -> anyhow::Result<()> {
        if let Some(session) = self.create.session.as_mut() {
            session.sync(&self.editor)?;
        }
        Ok(())
    }
    pub(super) fn apply_creative_project(
        &mut self,
        mut project: Project,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        project.validate()?;
        let document = project.active_document()?.clone();
        self.finish_interaction(cx);
        self.ensure_create()?;
        let session = self.create.session.as_mut().unwrap();
        session.checkpoint();
        session.project = project;
        session.editors.clear();
        self.editor = Editor::new(document);
        self.create.epoch = self.create.epoch.wrapping_add(1);
        self.layer_selection.ids.clear();
        self.selection_box = None;
        self.changed(cx);
        Ok(())
    }
    /// The ordinary editor history remains the first undo target. Collection
    /// history is used only after the active canvas has no undoable work.
    pub(super) fn can_undo_or_collection(&self) -> bool {
        self.editor.can_undo()
            || self
                .create
                .session
                .as_ref()
                .is_some_and(|session| !session.undo.is_empty())
    }
    pub(super) fn can_redo_or_collection(&self) -> bool {
        self.editor.can_redo()
            || self
                .create
                .session
                .as_ref()
                .is_some_and(|session| !session.redo.is_empty())
    }
    pub(super) fn undo_or_collection(&mut self, cx: &mut Context<Self>) -> anyhow::Result<bool> {
        if self.editor.undo() {
            self.selection_box = None;
            self.changed(cx);
            return Ok(true);
        }
        if self
            .create
            .session
            .as_ref()
            .is_some_and(|session| !session.undo.is_empty())
        {
            self.undo_create_change(false, cx)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn redo_or_collection(&mut self, cx: &mut Context<Self>) -> anyhow::Result<bool> {
        if self.editor.redo() {
            self.changed(cx);
            return Ok(true);
        }
        if self
            .create
            .session
            .as_ref()
            .is_some_and(|session| !session.redo.is_empty())
        {
            self.undo_create_change(true, cx)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn ensure_create(&mut self) -> anyhow::Result<()> {
        if self.create.session.is_none() {
            let title = self.editor.document.name.clone();
            self.create.session = Some(CreateSession::new(Project::new(
                title,
                self.editor.document.clone(),
            )));
            self.path = None;
            self.live_stamp = None;
            self.create.session.as_mut().unwrap().dirty = true;
        }
        self.sync_create()
    }
    pub(super) fn schedule_content_recovery(&mut self) {
        if let Some(session) = self.create.session.as_mut() {
            #[cfg(test)]
            if session.retain_synced_documents_for_test {
                match session.sync(&self.editor) {
                    Ok(()) => self.recovery.schedule_project(
                        &session.project,
                        self.editor.is_dirty() || session.dirty(),
                    ),
                    Err(error) => self.status = format!("Project recovery: {error:#}"),
                }
                return;
            }
            match session.snapshot(&self.editor) {
                Ok(project) => self
                    .recovery
                    .schedule_project_owned(project, self.editor.is_dirty() || session.dirty()),
                Err(error) => self.status = format!("Project recovery: {error:#}"),
            }
        } else {
            self.recovery
                .schedule(&self.editor.document, self.editor.is_dirty());
        }
    }
    pub(super) fn save_content_background(
        &mut self,
        path: PathBuf,
        expected_stamp: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(!self.create.saving, "A save is already running");
        let identity = (
            self.create
                .session
                .as_ref()
                .map(|session| session.project.id.clone()),
            self.create
                .session
                .as_ref()
                .map(|session| session.generation),
            self.create.epoch,
            self.editor.revision(),
        );
        let mut project = self
            .create
            .session
            .as_mut()
            .map(|session| session.snapshot(&self.editor))
            .transpose()?;
        let document = project.is_none().then(|| self.editor.document.clone());
        let dialog_identity = (self.dialog, self.dialog_generation);
        self.create.saving = true;
        self.status = "Saving project…".into();
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result = (|| -> anyhow::Result<_> {
                        let guard = omuse::save_guard::SaveGuard::acquire(&path)?;
                        guard.verify(&path, expected_stamp)?;
                        if let Some(project) = &mut project {
                            project.save_checked(&path, || guard.verify(&path, expected_stamp))?;
                        } else {
                            document::save_checked(document.as_ref().unwrap(), &path, || {
                                guard.verify(&path, expected_stamp)
                            })?;
                        }
                        let stamp = project_stamp(&path);
                        Ok((project, path, stamp))
                    })();
                    result.map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.create.saving = false;
                let save_again = std::mem::take(&mut this.save_again);
                match result {
                    Ok((project, path, stamp)) => {
                        if this
                            .create
                            .session
                            .as_ref()
                            .map(|session| session.project.id.clone())
                            != identity.0
                            || (identity.0.is_none() && this.create.epoch != identity.2)
                        {
                            cx.notify();
                            return;
                        }
                        let unchanged = this.create.epoch == identity.2
                            && this.editor.revision() == identity.3
                            && this
                                .create
                                .session
                                .as_ref()
                                .map(|session| session.generation)
                                == identity.1;
                        this.path = Some(path.clone());
                        this.live_stamp = stamp;
                        this.note_recent(path.clone(), cx);
                        if unchanged {
                            if let (Some(session), Some(project)) =
                                (this.create.session.as_mut(), project)
                            {
                                session.project = project;
                                session.dirty = false;
                                for editor in session.editors.values_mut() {
                                    editor.mark_saved();
                                }
                                // The worker's returned Project rebases lazy
                                // package paths. Keep that metadata without
                                // retaining a second live raster owner.
                                // A failed cache release leaves the complete
                                // saved Project intact and only costs a later
                                // copy; it must not turn a successful save into
                                // an error or discard the Editor's document.
                                let checkout = session.checkout_editors(&this.editor);
                                debug_assert!(checkout.is_ok());
                            }
                            this.editor.mark_saved();
                            this.recovery.clear();
                            if matches!(this.dialog, Dialog::Save | Dialog::Unsaved)
                                && (this.dialog, this.dialog_generation) == dialog_identity
                            {
                                this.dialog = Dialog::None;
                                this.focus.focus(window, cx);
                            }
                            this.status = format!("Saved {}", path.display());
                            if this.dialog == Dialog::None
                                && let Some(pending) = this.pending.take()
                            {
                                this.perform(pending, window, cx);
                            }
                        } else {
                            if save_again {
                                if let Err(error) =
                                    this.save_content_background(path, stamp, window, cx)
                                {
                                    this.status = format!("Queued save failed: {error:#}");
                                    if this.pending.is_some() {
                                        this.dialog = Dialog::Unsaved;
                                    }
                                }
                                cx.notify();
                                return;
                            }
                            this.status =
                                "Saved a snapshot. Your newer edits still need saving.".into();
                            this.schedule_content_recovery();
                            if this.pending.is_some() {
                                this.dialog = Dialog::Unsaved;
                                this.modal_focus.focus(window, cx);
                            }
                        }
                    }
                    Err(error) => {
                        this.status = format!("Save failed: {error}");
                        if this.pending.is_some() {
                            this.dialog = Dialog::Unsaved;
                            this.modal_focus.focus(window, cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        Ok(())
    }
    pub(super) fn install_opened_content(&mut self, doc: Document, project: Option<Project>) {
        self.cancel_camera_raw();
        self.external = external_change_ui::ExternalState::default();
        self.crop = None;
        self.drag_start = None;
        self.selection_box = None;
        self.transform_drag = None;
        self.transform_original_box = None;
        self.transform_draft = None;
        self.distort_draft = None;
        self.guide_drag = None;
        self.create.session = project.map(CreateSession::new);
        self.create.epoch = self.create.epoch.wrapping_add(1);
        self.editor = Editor::new(doc);
    }
    pub(crate) fn open_content(
        path: &std::path::Path,
    ) -> anyhow::Result<(Document, Option<Project>)> {
        if path.join("project.json").is_file() {
            let mut project = Project::open(path)?;
            let doc = project.active_document()?.clone();
            Ok((doc, Some(project)))
        } else {
            Ok((document::open(path)?, None))
        }
    }
    pub(super) fn activate_page(&mut self, id: &str, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.sync_create()?;
        let session = self
            .create
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Create project is not open"))?;
        let old_id = session.project.active_page_id().to_string();
        if old_id == id {
            return Ok(());
        }
        let doc = session.project.page_document(id)?.clone();
        session.project.set_active_page(id)?;
        let mut next = session
            .editors
            .remove(id)
            .unwrap_or_else(|| Editor::new(doc));
        next.set_history_limit(64 * 1024 * 1024);
        let mut old = std::mem::replace(&mut self.editor, next);
        old.set_history_limit(64 * 1024 * 1024);
        session.editors.insert(old_id, old);
        // Pixel storage remains shared with the project. Keep at most three
        // inactive histories so long carousels cannot grow an unbounded undo pool.
        while session.editors.len() > 3 {
            let key = session.editors.keys().next().unwrap().clone();
            if session.editors[&key].is_dirty() {
                session.dirty = true;
            }
            session.editors.remove(&key);
        }
        self.create.epoch = self.create.epoch.wrapping_add(1);
        self.selection_box = None;
        self.layer_selection.ids.clear();
        self.pan = (0., 0.);
        self.status = format!("Page: {}", self.editor.document.name);
        self.changed(cx);
        Ok(())
    }
    pub(super) fn create_error(&mut self, result: anyhow::Result<()>, cx: &mut Context<Self>) {
        if let Err(error) = result {
            self.status = format!("Create: {error:#}");
        }
        cx.notify();
    }
    fn add_create_page(&mut self, duplicate: bool, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.ensure_create()?;
        let name = self.create.value(1, cx);
        let w = self.create.value(2, cx).parse::<u32>()?;
        let h = self.create.value(3, cx).parse::<u32>()?;
        let session = self.create.session.as_mut().unwrap();
        // Prepare mutations before replacing the live project. A rejected size
        // or page limit therefore does not create a phantom undo entry.
        let mut draft = session.project.clone();
        let active = draft.active_page_id().to_string();
        let id = if duplicate {
            draft.duplicate_page(&active)?
        } else {
            draft.add_blank_page(name, w, h)?
        };
        if draft.metadata.shared_background_component_id.is_some() {
            omuse::create::apply_shared_background_to_page(&mut draft, &id)?;
        }
        session.checkpoint_structure();
        session.project = draft;
        self.activate_page(&id, cx)?;
        self.schedule_content_recovery();
        Ok(())
    }
    fn rename_collection(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.ensure_create()?;
        let title = self.create.value(0, cx);
        let session = self.create.session.as_mut().unwrap();
        let mut draft = session.project.clone();
        draft.title = title;
        draft.validate()?;
        session.checkpoint_structure();
        session.project = draft;
        self.schedule_content_recovery();
        cx.notify();
        Ok(())
    }
    fn move_create_page(
        &mut self,
        id: &str,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.sync_create()?;
        let session = self.create.session.as_mut().unwrap();
        let ids = session.project.page_ids();
        let index = ids
            .iter()
            .position(|v| v == id)
            .ok_or_else(|| anyhow::anyhow!("Page not found"))?;
        let target = (index as isize + delta).clamp(0, ids.len() as isize - 1) as usize;
        if index != target {
            session.checkpoint_structure();
            session.project.reorder_page(id, target)?;
            self.schedule_content_recovery();
        }
        cx.notify();
        Ok(())
    }
    fn remove_create_page(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.sync_create()?;
        let session = self.create.session.as_mut().unwrap();
        let mut draft = session.project.clone();
        let id = draft.active_page_id().to_string();
        draft.remove_page(&id)?;
        let doc = draft.active_document()?.clone();
        session.checkpoint_structure();
        session.project = draft;
        session.editors.remove(&id);
        let active = session.project.active_page_id().to_string();
        self.editor = session
            .editors
            .remove(&active)
            .unwrap_or_else(|| Editor::new(doc));
        self.create.epoch = self.create.epoch.wrapping_add(1);
        self.changed(cx);
        Ok(())
    }
    pub(super) fn undo_create_change(
        &mut self,
        redo: bool,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.finish_interaction(cx);
        self.sync_create()?;
        let session = self.create.session.as_mut().unwrap();
        let entry = if redo {
            session.redo.back()
        } else {
            session.undo.back()
        }
        .cloned();
        if let Some(mut entry) = entry {
            if entry.structural {
                let ids = entry.project.page_ids();
                session.project.for_each_page_document(|page, doc| {
                    if ids.contains(&page.id) {
                        entry.project.replace_page_document(&page.id, doc.clone())?;
                    }
                    Ok(())
                })?;
            } else if let Some(expected) = entry.expected.as_mut() {
                anyhow::ensure!(
                    omuse::create_history::project_documents_match(&mut session.project, expected)?,
                    "Undo the later canvas edits before undoing this collection change. Your current artwork has been preserved."
                );
            }
            let doc = entry.project.active_document()?.clone();
            let next_active = entry.project.active_page_id().to_owned();
            let old_active = session.project.active_page_id().to_owned();
            let expected = if entry.structural {
                None
            } else {
                Some(entry.project.clone())
            };
            let current = std::mem::replace(&mut session.project, entry.project);
            let inverse = CollectionUndo {
                project: current,
                expected,
                structural: entry.structural,
            };
            if redo {
                session.redo.pop_back();
                session.undo.push_back(inverse);
            } else {
                session.undo.pop_back();
                session.redo.push_back(inverse);
            }
            if entry.structural {
                let old = std::mem::replace(&mut self.editor, Editor::new(doc));
                session.editors.insert(old_active, old);
                let surviving = session.project.page_ids();
                session.editors.retain(|id, _| surviving.contains(id));
                if let Some(editor) = session.editors.remove(&next_active) {
                    self.editor = editor;
                }
            } else {
                session.editors.clear();
                self.editor = Editor::new(doc);
            }
            session.dirty = true;
            session.generation = session.generation.wrapping_add(1);
            self.create.epoch = self.create.epoch.wrapping_add(1);
            self.changed(cx);
        }
        Ok(())
    }
    pub(super) fn create_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = &self.create.session else {
            return div().into_any_element();
        };
        let t = cx.omarchy().clone();
        let mut strip = div()
            .id("create-pages-strip")
            .debug_selector(|| "create-pages-strip".into())
            .h(px(42.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .bg(t.surface)
            .border_t_1()
            .border_color(t.divider())
            .overflow_x_scroll();
        for (index, page) in session.project.page_summaries().iter().enumerate() {
            let id = page.id.clone();
            let selected = id == session.project.active_page_id();
            strip = strip.child(
                button(
                    SharedString::from(format!("page-strip-{id}")),
                    SharedString::from(format!("{:02}  {}", index + 1, page.name)),
                    ButtonVariant::Secondary,
                    cx,
                )
                .selected(selected)
                .h(px(28.))
                .text_size(px(11.))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let result = this.activate_page(&id, cx);
                    this.create_error(result, cx);
                })),
            );
        }
        strip
            .child(
                button("page-strip-add", "+", ButtonVariant::Secondary, cx)
                    .accessibility_label("Add page")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = this.add_create_page(false, cx);
                        this.create_error(result, cx);
                    })),
            )
            .into_any_element()
    }
    pub(super) fn create_inspector(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut tabs = div().flex().flex_col().flex_shrink_0().gap_2().p_3();
        for row_tabs in [
            [
                (CreateTab::Pages, "create-pages", "Pages"),
                (CreateTab::Templates, "create-templates", "Design"),
                (CreateTab::Brand, "create-brand", "Brand"),
            ],
            [
                (CreateTab::Assets, "create-assets", "Assets"),
                (CreateTab::Motion, "create-motion", "Motion"),
                (CreateTab::Export, "create-export", "Export"),
            ],
        ] {
            let mut row = div().flex().gap_2();
            for (tab, id, label) in row_tabs {
                row = row.child(
                    button(id, label, ButtonVariant::Secondary, cx)
                        .selected(self.create.tab == tab)
                        .flex_1()
                        .min_w_0()
                        .debug_selector(move || id.into())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.create.tab = tab;
                            if tab == CreateTab::Templates {
                                this.load_template_previews(cx);
                            }
                            if tab == CreateTab::Assets {
                                this.load_asset_library();
                            }
                            this.refresh(cx);
                        })),
                );
            }
            tabs = tabs.child(row);
        }
        let mut body = div()
            .id("create-inspector-content")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .px_3()
            .pb_3();
        match self.create.tab {
            CreateTab::Pages => {
                let mut pages = section("YOUR COLLECTION", cx);
                if let Some(session) = &self.create.session {
                    pages = pages.child(note(
                        format!(
                            "{} · {} pages",
                            session.project.title,
                            session.project.page_ids().len()
                        ),
                        cx,
                    ));
                    for (index, page) in session.project.page_summaries().into_iter().enumerate() {
                        let id = page.id.clone();
                        let prev = id.clone();
                        let next = id.clone();
                        pages = pages
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        button(
                                            SharedString::from(format!("page-{id}")),
                                            SharedString::from(format!(
                                                "{:02}  {}",
                                                index + 1,
                                                page.name
                                            )),
                                            ButtonVariant::Secondary,
                                            cx,
                                        )
                                        .flex_1()
                                        .min_w_0()
                                        .selected(id == session.project.active_page_id())
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                let result = this.activate_page(&id, cx);
                                                this.create_error(result, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        button(
                                            SharedString::from(format!("page-up-{prev}")),
                                            "↑",
                                            ButtonVariant::Secondary,
                                            cx,
                                        )
                                        .accessibility_label("Move page earlier")
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                let result = this.move_create_page(&prev, -1, cx);
                                                this.create_error(result, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        button(
                                            SharedString::from(format!("page-down-{next}")),
                                            "↓",
                                            ButtonVariant::Secondary,
                                            cx,
                                        )
                                        .accessibility_label("Move page later")
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                let result = this.move_create_page(&next, 1, cx);
                                                this.create_error(result, cx);
                                            }),
                                        ),
                                    ),
                            )
                            .child(note(format!("{} × {}", page.width, page.height), cx));
                    }
                    pages = pages.child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                button(
                                    "project-undo",
                                    "Undo page change",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .disabled(session.undo.is_empty())
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        let result = this.undo_create_change(false, cx);
                                        this.create_error(result, cx);
                                    },
                                )),
                            )
                            .child(
                                button("project-redo", "Redo", ButtonVariant::Secondary, cx)
                                    .disabled(session.redo.is_empty())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let result = this.undo_create_change(true, cx);
                                        this.create_error(result, cx);
                                    })),
                            ),
                    );
                } else {
                    pages=pages.child(note("Build a carousel, campaign or collection. Your current canvas becomes its first page.",cx));
                }
                pages = pages
                    .child(note("Collection name", cx))
                    .child(input(
                        "create-collection-title",
                        &self.create.fields[0],
                        window,
                        cx,
                    ))
                    .child(
                        button(
                            "create-rename-collection",
                            "Name collection",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .w_full()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.rename_collection(cx);
                            this.create_error(result, cx);
                        })),
                    );
                body = body
                    .child(pages)
                    .child(
                        section("ADD A PAGE", cx)
                            .child(note("Page name", cx))
                            .child(input("create-field-1", &self.create.fields[1], window, cx))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(note("Width (px)", cx))
                                            .child(input(
                                                "create-field-2",
                                                &self.create.fields[2],
                                                window,
                                                cx,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(note("Height (px)", cx))
                                            .child(input(
                                                "create-field-3",
                                                &self.create.fields[3],
                                                window,
                                                cx,
                                            )),
                                    ),
                            )
                            .child(
                                button("create-add-page", "Add page", ButtonVariant::Primary, cx)
                                    .w_full()
                                    .debug_selector(|| "create-add-page".into())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let result = this.add_create_page(false, cx);
                                        this.create_error(result, cx);
                                    })),
                            )
                            .child(
                                button(
                                    "create-duplicate-page",
                                    "Duplicate current page",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .w_full()
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        let result = this.add_create_page(true, cx);
                                        this.create_error(result, cx);
                                    },
                                )),
                            )
                            .child(
                                button(
                                    "create-remove-page",
                                    "Remove current page",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .w_full()
                                .disabled(
                                    self.create
                                        .session
                                        .as_ref()
                                        .is_none_or(|s| s.project.page_ids().len() < 2),
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        let result = this.remove_create_page(cx);
                                        this.create_error(result, cx);
                                    },
                                )),
                            ),
                    )
                    .child(self.create_layout_section(window, cx));
            }
            CreateTab::Templates => {
                body = body.child(self.create_templates_section(cx));
            }
            CreateTab::Brand => {
                body = body.child(self.create_brand_section(window, cx));
            }
            CreateTab::Assets => {
                body = body
                    .child(self.create_restoration_section(window, cx))
                    .child(self.create_assets_section(window, cx));
            }
            CreateTab::Motion => {
                body = body.child(self.create_motion_section(window, cx));
            }
            CreateTab::Export => {
                body = body.child(self.create_export_section(window, cx));
            }
        }
        div()
            .id("create-inspector")
            .debug_selector(|| "create-inspector".into())
            .w(panel_width(window))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_l_1()
            .border_color(t.divider())
            .child(
                panel_header("Create", "Pages, design and export", "sparkles", cx).child(
                    button("create-close", "Editor", ButtonVariant::Secondary, cx).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.inspector_tab = studio_ui::InspectorTab::Layers;
                            this.refresh(cx);
                        }),
                    ),
                ),
            )
            .child(tabs)
            .child(body)
            .into_any_element()
    }
    fn create_export_section(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut body=section("READY TO SHARE",cx).child(note("Export ordered pages, captions and alt text, plus a multi-page PDF. Files are published together when the export finishes.",cx))
            .child(button("create-export-pack","Export content pack…",ButtonVariant::Primary,cx).w_full().disabled(self.create.job.is_some()).on_click(cx.listener(|this,_,window,cx|this.choose_content_export(window,cx))))
            .child(button("create-safe-areas","Show content guide",ButtonVariant::Secondary,cx).w_full().selected(self.create.safe_areas).on_click(cx.listener(|this,_,_,cx|{this.create.safe_areas=!this.create.safe_areas;cx.notify();})))
            .child(button("create-phone-preview","Phone preview",ButtonVariant::Secondary,cx).w_full().selected(self.create.phone_preview).on_click(cx.listener(|this,_,_,cx|{this.create.phone_preview=!this.create.phone_preview;cx.notify();})))
            .child(note("Caption",cx)).child(input("create-field-10",&self.create.fields[10],window,cx)).child(note("Alt text",cx)).child(input("create-field-11",&self.create.fields[11],window,cx))
            .child(button("create-save-caption","Apply caption & alt text",ButtonVariant::Secondary,cx).w_full().on_click(cx.listener(|this,_,_,cx|{let plan=omuse::creative_commands::CreativePlan{summary:"Content details".into(),operations:vec![omuse::creative_commands::CreativeOperation::SetContent{caption:this.create.value(10,cx),alt_text:this.create.value(11,cx)}]};let result=plan.prepare(&this.editor.document).and_then(|doc|this.editor.replace_document_transaction(doc));if result.is_ok(){this.changed(cx);}this.create_error(result,cx);})))
            ;
        let mut checks=section("CONTENT CHECK",cx)
            .child(note("Choose a guide for text and logos. App controls vary by device and caption; these are conservative composition guides.",cx));
        for (index, preset) in omuse::social_preview::SafeAreaPreset::ALL
            .into_iter()
            .enumerate()
        {
            checks = checks.child(
                button(
                    SharedString::from(format!("content-guide-{index}")),
                    preset.label(),
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .selected(self.create.safe_preset == preset)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.create.safe_preset = preset;
                    this.create.safe_areas = true;
                    this.create.preflight = None;
                    cx.notify();
                })),
            );
        }
        checks = checks.child(
            button(
                "create-preflight",
                if self.create.preflight_busy {
                    "Checking all pages…"
                } else {
                    "Check all pages"
                },
                ButtonVariant::Secondary,
                cx,
            )
            .w_full()
            .disabled(self.create.preflight_busy)
            .on_click(cx.listener(|this, _, _, cx| {
                let result = this.start_content_preflight(cx);
                this.create_error(result, cx);
            })),
        );
        if let Some((epoch, revision, generation, issues)) = &self.create.preflight {
            let stale = *epoch != self.create.epoch
                || *revision != self.editor.revision()
                || *generation
                    != self
                        .create
                        .session
                        .as_ref()
                        .map_or(0, |session| session.generation);
            checks = checks.child(note(
                if stale {
                    "Artwork changed — run the check again.".into()
                } else if issues.is_empty() {
                    "No text overflow, missing fonts or missing descriptions found.".into()
                } else {
                    format!("{} items to review", issues.len())
                },
                cx,
            ));
            if !stale {
                for issue in issues.iter().take(16) {
                    checks = checks.child(note(
                        format!(
                            "{}{}: {}",
                            issue.page,
                            issue
                                .layer
                                .as_ref()
                                .map_or(String::new(), |name| format!(" / {name}")),
                            issue.detail
                        ),
                        cx,
                    ));
                }
            }
        }
        body = body.child(checks);
        if let Some(job) = &self.create.job {
            body = body.child(note(job.detail.clone(), cx)).child(
                button(
                    "create-cancel-export",
                    "Cancel export",
                    ButtonVariant::Secondary,
                    cx,
                )
                .w_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(job) = &this.create.job {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    cx.notify();
                })),
            );
        }
        body.into_any_element()
    }
    fn start_content_preflight(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.create.preflight_busy,
            "A content check is already running"
        );
        let mut project = self.content_snapshot()?;
        let preset = self.create.safe_preset;
        let identity = (
            self.create.epoch,
            self.editor.revision(),
            self.create
                .session
                .as_ref()
                .map_or(0, |session| session.generation),
        );
        self.create.preflight_busy = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    omuse::social_preview::inspect_project(&mut project, preset)
                        .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = view.update(cx, |this, cx| {
                this.create.preflight_busy = false;
                match result {
                    Ok(issues) => {
                        this.create.preflight = Some((identity.0, identity.1, identity.2, issues))
                    }
                    Err(error) => this.status = format!("Content check: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }
    fn choose_content_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.create.job.is_some() {
            return;
        }
        self.finish_interaction(cx);
        let dir = omuse::identity::media_dir(
            omuse::identity::home_dir().unwrap_or_default(),
            omuse::identity::MediaFolder::Pictures,
        );
        let task = cx.prompt_for_new_path(&dir, Some("Omuse content pack"));
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, _, cx| {
                if let Ok(Ok(Some(path))) = result {
                    let result = this.start_content_export(path, cx);
                    this.create_error(result, cx);
                }
            });
        })
        .detach();
    }
    pub(super) fn content_snapshot(&mut self) -> anyhow::Result<Project> {
        match self.create.session.as_mut() {
            Some(session) => session.snapshot(&self.editor),
            None => Ok(Project::new(
                self.editor.document.name.clone(),
                self.editor.document.clone(),
            )),
        }
    }
    fn start_content_export(
        &mut self,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(self.create.job.is_none(), "An export is already running");
        let mut project = self.content_snapshot()?;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(32);
        std::thread::Builder::new()
            .name("omuse-content-export".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<PathBuf> {
                    use omuse::content_export::*;
                    let mut writer = ExportPackageWriter::begin(
                        &path,
                        project.page_ids().len(),
                        PackageOptions {
                            formats: vec![
                                RasterExportFormat::Png,
                                RasterExportFormat::Jpeg,
                                RasterExportFormat::WebP,
                            ],
                            collision: CollisionPolicy::KeepBoth,
                            ..Default::default()
                        },
                        &worker_cancel,
                    )?;
                    project.for_each_page_document(|page, doc| {
                        let content = &doc.metadata["omuseContent"];
                        writer.write_page(
                            doc,
                            PageExportMetadata {
                                id: page.id.clone(),
                                name: page.name.clone(),
                                caption: content["caption"].as_str().unwrap_or_default().into(),
                                alt_text: content["altText"].as_str().unwrap_or_default().into(),
                            },
                            &worker_cancel,
                            |p| {
                                let _ = tx.try_send(ContentEvent::Progress(p.detail));
                            },
                        )
                    })?;
                    Ok(writer.finish(&worker_cancel)?.path)
                })();
                let _ = tx.send(ContentEvent::Finished(result.map_err(|e| format!("{e:#}"))));
            })?;
        self.create.job = Some(ContentJob {
            cancel,
            receiver: rx,
            detail: "Preparing content pack…".into(),
        });
        self.poll_content_job(cx);
        Ok(())
    }
    pub(super) fn poll_content_job(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move|view,cx|{loop{cx.background_executor().timer(std::time::Duration::from_millis(100)).await;let keep=view.update(cx,|this,cx|{
            let mut finished=None;if let Some(job)=&mut this.create.job{loop { match job.receiver.try_recv() { Ok(ContentEvent::Progress(detail)) => job.detail=detail, Ok(ContentEvent::Finished(result)) => {finished=Some(result);break;}, Err(std::sync::mpsc::TryRecvError::Empty)=>break, Err(std::sync::mpsc::TryRecvError::Disconnected)=>{finished=Some(Err("The export worker stopped before it could finish. No incomplete pack was published.".into()));break;} } }}
            if let Some(result)=finished{this.create.job=None;this.status=match result{Ok(path)=>format!("Exported {}",path.display()),Err(error)=>format!("Export: {error}")};}cx.notify();this.create.job.is_some()
        }).unwrap_or(false);if !keep{break;}}}).detach();
    }
}

impl EditorView {
    pub(super) fn phone_preview_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let key = (self.create.epoch, self.editor.revision());
        let mut cache = self.create.preview_cache.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|(epoch, revision, _)| (*epoch, *revision) != key)
        {
            if let Some((_, _, previous)) = cache.take() {
                cx.drop_image(previous, None);
            }
            *cache = Some((
                key.0,
                key.1,
                render_image(&image::imageops::thumbnail(&self.pixels, 768, 1024)),
            ));
        }
        let image = cache.as_ref().unwrap().2.clone();
        drop(cache);
        let t = cx.omarchy().clone();
        let caption = self.editor.document.metadata["omuseContent"]["caption"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        div()
            .id("phone-preview-overlay")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x000000cc))
            .child(
                div()
                    .w(px(354.))
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_3()
                    .rounded(px(28.))
                    .bg(t.surface)
                    .border_2()
                    .border_color(t.divider())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Mobile preview"),
                            )
                            .child(
                                button(
                                    "phone-preview-close",
                                    "Close",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.create.phone_preview = false;
                                        this.focus.focus(window, cx);
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .max_h(px(500.))
                            .overflow_hidden()
                            .rounded(px(8.))
                            .bg(t.background)
                            .child(
                                gpui_kit::img(image)
                                    .w_full()
                                    .max_h(px(500.))
                                    .object_fit(gpui_kit::ObjectFit::Contain),
                            ),
                    )
                    .child(note(caption, cx))
                    .child(note(
                        "Check text size, contrast and framing at phone scale.",
                        cx,
                    )),
            )
            .into_any_element()
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext};

    #[test]
    fn rejected_editor_overlay_keeps_project_and_collection_checkpoint_complete() {
        let mut editor = Editor::new(Document::new(8, 8));
        let mut session = CreateSession::new(Project::new("Atomic", editor.document.clone()));
        session.checkpoint();
        editor.document.width = 0;
        assert!(session.snapshot(&editor).is_err());
        session.project.validate().unwrap();
        assert_eq!(session.project.active_document().unwrap().width, 8);
        assert!(session.undo.back().unwrap().expected.is_none());
        editor.document.width = 8;
        session
            .editors
            .insert("missing-page".into(), Editor::new(Document::new(2, 2)));
        assert!(session.snapshot(&editor).is_err());
        session.project.validate().unwrap();
        assert!(session.undo.back().unwrap().expected.is_none());
    }

    #[gpui_kit::test]
    fn checked_out_pages_survive_cache_eviction_resize_and_structural_undo(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let mut project = Project::new("Six pages", Document::new(16, 16));
        for index in 1..6 {
            project
                .add_blank_page(format!("Page {}", index + 1), 16, 16)
                .unwrap();
        }
        let ids = project.page_ids();
        let document = project.active_document().unwrap().clone();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.install_opened_content(document, Some(project));
            view.dialog = Dialog::None;
            view
        });
        view.update(cx, |view, cx| {
            for (index, id) in ids.iter().enumerate() {
                view.activate_page(id, cx).unwrap();
                view.editor.add_layer(&format!("Edited page {index}"));
                view.editor.brush.size = 5.;
                view.editor.brush.color = [20 + index as u8, 80, 160, 255];
                assert!(view.editor.begin_stroke(8., 8., 1., PaintTool::Brush));
                assert!(view.editor.finish_stroke());
                view.changed(cx);
                assert!(view.create.session.as_ref().unwrap().editors.len() <= 3);
            }
            // More than three inactive editors forces eviction. Their latest
            // artwork must already be owned by the structural Project.
            let mut snapshot = view.content_snapshot().unwrap();
            for (index, id) in ids.iter().enumerate() {
                let doc = snapshot.page_document(id).unwrap();
                assert!(
                    doc.layers
                        .iter()
                        .any(|layer| layer.name == format!("Edited page {index}"))
                );
                let pixels = raster::composite(doc);
                view.activate_page(id, cx).unwrap();
                assert_eq!(raster::composite(&view.editor.document), pixels);
            }
            assert!(view.editor.resize_canvas(20, 18));
            view.changed(cx);
            let session = view.create.session.as_mut().unwrap();
            let summary = session
                .project
                .page_summaries()
                .into_iter()
                .find(|page| page.id == ids[5])
                .unwrap();
            assert_eq!((summary.width, summary.height), (20, 18));
            assert!(session.project.active_document().is_err());
            let removed_pixels = raster::composite(&view.editor.document);
            view.remove_create_page(cx).unwrap();
            assert_eq!(
                view.create
                    .session
                    .as_ref()
                    .unwrap()
                    .project
                    .page_ids()
                    .len(),
                5
            );
            view.undo_create_change(false, cx).unwrap();
            assert_eq!(
                view.create
                    .session
                    .as_ref()
                    .unwrap()
                    .project
                    .active_page_id(),
                ids[5]
            );
            assert_eq!(raster::composite(&view.editor.document), removed_pixels);
            view.redo_or_collection(cx).unwrap();
            assert_eq!(
                view.create
                    .session
                    .as_ref()
                    .unwrap()
                    .project
                    .page_ids()
                    .len(),
                5
            );
        });
    }

    #[gpui_kit::test]
    fn structural_undo_and_redo_keep_later_page_edits(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(8, 8));
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.create.fields[2].update(cx, |state, cx| state.set_value("8", window, cx));
            view.create.fields[3].update(cx, |state, cx| state.set_value("8", window, cx));
            view.add_create_page(false, cx).unwrap();
            let ids = view.create.session.as_ref().unwrap().project.page_ids();
            view.move_create_page(&ids[1], -1, cx).unwrap();
            view.editor.add_layer("Keep this later edit");
            view.changed(cx);
            view.undo_create_change(false, cx).unwrap();
            assert_eq!(
                view.create.session.as_ref().unwrap().project.page_ids(),
                ids
            );
            assert!(
                view.editor
                    .document
                    .layers
                    .iter()
                    .any(|layer| layer.name == "Keep this later edit")
            );
            assert!(view.editor.can_undo());
            view.undo_create_change(true, cx).unwrap();
            assert!(
                view.editor
                    .document
                    .layers
                    .iter()
                    .any(|layer| layer.name == "Keep this later edit")
            );
        });
    }
    #[gpui_kit::test]
    fn collection_content_undo_refuses_to_drop_later_edits(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(8, 8));
            view.dialog = Dialog::None;
            view
        });
        view.update(cx, |view, cx| {
            let mut draft = view.content_snapshot().unwrap();
            let mut doc = draft.active_document().unwrap().clone();
            doc.background = [220, 80, 40, 255];
            draft.replace_active_document(doc).unwrap();
            view.apply_creative_project(draft, cx).unwrap();
            view.editor.add_layer("Later canvas edit");
            view.changed(cx);
            assert!(view.undo_create_change(false, cx).is_err());
            assert!(
                view.editor
                    .document
                    .layers
                    .iter()
                    .any(|layer| layer.name == "Later canvas edit")
            );
            view.editor.undo();
            view.changed(cx);
            view.undo_create_change(false, cx).unwrap();
            assert_ne!(view.editor.document.background, [220, 80, 40, 255]);
        });
    }
    #[gpui_kit::test]
    fn ordinary_undo_and_redo_fall_back_to_a_kept_project_draft_after_canvas_history(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(40, 30));
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            assert!(view.create.session.is_none());
            assert!(!view.can_undo_or_collection());
            assert!(!view.can_redo_or_collection());
            view.command("undo", window, cx);
            view.command("redo", window, cx);

            let mut kept_draft = view.content_snapshot().unwrap();
            // A collection gives its first page the native page name. The
            // collection checkpoint therefore restores this canonical page
            // document, rather than the pre-collection canvas label.
            let baseline = kept_draft.active_document().unwrap().clone();
            let mut draft_document = kept_draft.active_document().unwrap().clone();
            draft_document.background = [220, 80, 40, 255];
            kept_draft.replace_active_document(draft_document).unwrap();
            view.apply_creative_project(kept_draft, cx).unwrap();
            assert!(!view.editor.can_undo());
            assert!(view.can_undo_or_collection());

            view.command("undo", window, cx);
            assert!(omuse::create_history::documents_match(
                &view.editor.document,
                &baseline
            ));
            assert!(view.can_redo_or_collection());

            view.command("redo", window, cx);
            assert_eq!(view.editor.document.background, [220, 80, 40, 255]);

            view.editor.add_layer("Later canvas edit");
            view.changed(cx);
            assert!(view.editor.can_undo());
            view.command("undo", window, cx);
            assert_eq!(view.editor.document.background, [220, 80, 40, 255]);
            assert!(
                !view
                    .editor
                    .document
                    .layers
                    .iter()
                    .any(|layer| layer.name == "Later canvas edit")
            );

            view.command("undo", window, cx);
            assert!(omuse::create_history::documents_match(
                &view.editor.document,
                &baseline
            ));
            view.command("redo", window, cx);
            assert_eq!(view.editor.document.background, [220, 80, 40, 255]);
        });
    }
    #[gpui_kit::test]
    fn collection_switch_save_and_reopen_preserves_inactive_edits(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let recovery = temp.path().join("recovery");
        let path = temp.path().join("campaign.omuse");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(recovery);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(40, 30));
            view.refresh(cx);
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.create.fields[2].update(cx, |input, cx| input.set_value("48", window, cx));
            view.create.fields[3].update(cx, |input, cx| input.set_value("32", window, cx));
            view.editor.add_layer("First page edit");
            view.add_create_page(false, cx).unwrap();
            let pages = view.create.session.as_ref().unwrap().project.page_ids();
            assert_eq!(pages.len(), 2);
            view.editor.add_layer("Second page edit");
            view.changed(cx);
            view.activate_page(&pages[0], cx).unwrap();
            assert!(
                view.editor
                    .document
                    .layers
                    .iter()
                    .any(|l| l.name == "First page edit")
            );
            assert!(view.has_unsaved_work());
            view.save_to(path.clone(), window, cx);
            assert!(view.create.saving);
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            let pages = view.create.session.as_ref().unwrap().project.page_ids();
            assert!(!view.has_unsaved_work());
            let (doc, project) = EditorView::open_content(&path).unwrap();
            assert_eq!(doc.width, 40);
            let mut project = project.unwrap();
            assert_eq!(project.page_ids(), pages);
            assert!(
                project
                    .page_document(&pages[1])
                    .unwrap()
                    .layers
                    .iter()
                    .any(|l| l.name == "Second page edit")
            );
            view.activate_page(&pages[1], cx).unwrap();
            assert!(view.editor.can_undo());
            view.editor.undo();
            assert!(view.has_unsaved_work());
        });
    }
    #[gpui_kit::test]
    fn opened_collection_save_with_newer_edits_can_recover_export_and_save_again(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("opened.omuse");
        let mut project = Project::new("Opened", Document::new(12, 10));
        let inactive = project.add_blank_page("Keep inactive", 9, 7).unwrap();
        project.page_document_mut(&inactive).unwrap().layers[0].image =
            Some(image::RgbaImage::from_pixel(9, 7, image::Rgba([90, 80, 70, 255])).into());
        project.save(&path).unwrap();
        let (document, project) = EditorView::open_content(&path).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.install_opened_content(document, project);
            view.path = Some(path.clone());
            view.live_stamp = project_stamp(&path);
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.editor.add_layer("Included in first save");
            view.save_to(path.clone(), window, cx);
            view.editor.add_layer("Newer unsaved work");
            view.changed(cx);
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert!(!view.create.saving);
            assert!(view.has_unsaved_work());
            let recovery_path = view
                .recovery
                .wait_idle_for_test(std::time::Duration::from_secs(10))
                .unwrap();
            let mut recovered = Project::open(&recovery_path).unwrap();
            assert!(
                recovered
                    .active_document()
                    .unwrap()
                    .layers
                    .iter()
                    .any(|layer| layer.name == "Newer unsaved work")
            );
            assert_eq!(
                raster::composite(recovered.page_document(&inactive).unwrap())
                    .get_pixel(0, 0)
                    .0,
                [90, 80, 70, 255]
            );
            let mut snapshot = view.content_snapshot().unwrap();
            let mut pages = 0;
            snapshot
                .for_each_page_document(|_, _| {
                    pages += 1;
                    Ok(())
                })
                .unwrap();
            assert_eq!(pages, 2);
            snapshot
                .save(&temp.path().join("export-copy.omuse"))
                .unwrap();
            view.save_to(path.clone(), window, cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.create.saving);
            assert!(!view.has_unsaved_work(), "{}", view.status);
        });
        let mut saved = Project::open(&path).unwrap();
        assert!(
            saved
                .active_document()
                .unwrap()
                .layers
                .iter()
                .any(|layer| layer.name == "Newer unsaved work")
        );
        assert_eq!(
            raster::composite(saved.page_document(&inactive).unwrap())
                .get_pixel(0, 0)
                .0,
            [90, 80, 70, 255]
        );
    }

    #[gpui_kit::test]
    fn edits_during_collection_save_remain_dirty(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("draft.omuse");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(8, 8));
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.ensure_create().unwrap();
            view.editor.add_layer("Saved before edit");
            view.save_to(path.clone(), window, cx);
            view.editor.add_layer("Newer unsaved edit");
            view.changed(cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.create.saving);
            assert!(view.has_unsaved_work());
            assert!(
                view.editor
                    .document
                    .layers
                    .iter()
                    .any(|layer| layer.name == "Newer unsaved edit")
            );
        });
        let (saved, _) = EditorView::open_content(&path).unwrap();
        assert!(
            saved
                .layers
                .iter()
                .any(|layer| layer.name == "Saved before edit")
        );
        assert!(
            !saved
                .layers
                .iter()
                .any(|layer| layer.name == "Newer unsaved edit")
        );
    }
    #[gpui_kit::test]
    fn photo_save_keeps_newer_edits_dirty_and_saves_the_frozen_snapshot(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("photo.omuse");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(8, 8));
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.editor.fill_selection([25, 100, 170, 255]);
            view.save_to(path.clone(), window, cx);
            assert!(view.create.saving);
            view.editor.fill_selection([210, 70, 35, 255]);
            view.changed(cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.create.saving, "{}", view.status);
            assert!(view.editor.is_dirty());
            assert_eq!(view.pixels.get_pixel(0, 0).0, [210, 70, 35, 255]);
        });
        assert_eq!(
            raster::composite(&document::open(&path).unwrap())
                .get_pixel(0, 0)
                .0,
            [25, 100, 170, 255]
        );
        view.update_in(cx, |view, window, cx| view.save(window, cx));
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.editor.is_dirty(), "{}", view.status)
        });
        assert_eq!(
            raster::composite(&document::open(&path).unwrap())
                .get_pixel(0, 0)
                .0,
            [210, 70, 35, 255]
        );
    }
    #[gpui_kit::test]
    fn photo_save_refuses_a_destination_changed_after_submission(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("photo.omuse");
        document::save(&Document::new(8, 8), &path).unwrap();
        let manifest = std::fs::read(path.join("manifest.json")).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(8, 8));
            view.path = Some(path.clone());
            view.live_stamp = project_stamp(&path);
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.editor.fill_selection([210, 70, 35, 255]);
            view.save(window, cx);
            assert!(view.create.saving);
            std::fs::write(path.join("external-edit"), b"Preserve this version").unwrap();
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.create.saving);
            assert!(view.editor.is_dirty());
            assert!(view.status.starts_with("Save failed:"), "{}", view.status);
        });
        assert_eq!(std::fs::read(path.join("manifest.json")).unwrap(), manifest);
        assert_eq!(
            std::fs::read(path.join("external-edit")).unwrap(),
            b"Preserve this version"
        );
    }
    #[gpui_kit::test]
    fn photo_save_completion_preserves_a_newer_dialog(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("photo.omuse");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(8, 8));
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.save_to(path.clone(), window, cx);
            view.command("tool-settings", window, cx);
            assert_eq!(view.dialog, Dialog::ToolSettings);
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.create.saving);
            assert!(!view.editor.is_dirty());
            assert_eq!(view.dialog, Dialog::ToolSettings);
        });
        assert!(path.join("manifest.json").is_file());
    }
    #[gpui_kit::test]
    fn photo_save_waits_before_replacing_a_clean_document(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("photo.omuse");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(8, 8));
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.save_to(path.clone(), window, cx);
            view.request(Pending::New, window, cx);
            assert!(view.create.saving);
            assert_eq!(view.dialog, Dialog::None);
            assert!(view.pending.is_some());
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.create.saving);
            assert!(view.pending.is_none());
            assert_eq!(view.dialog, Dialog::New);
        });
        assert!(path.join("manifest.json").is_file());
    }
    #[gpui_kit::test]
    fn failed_photo_save_makes_a_waiting_new_request_explicit(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("missing-folder").join("photo.omuse");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor = Editor::new(Document::new(8, 8));
            view.editor.fill_selection([42, 78, 131, 255]);
            view.dialog = Dialog::None;
            view
        });
        view.update_in(cx, |view, window, cx| {
            view.save_to(path, window, cx);
            view.request(Pending::New, window, cx);
            assert!(view.create.saving);
            assert!(view.pending.is_some());
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(!view.create.saving);
            assert!(view.status.starts_with("Save failed:"), "{}", view.status);
            assert_eq!(view.dialog, Dialog::Unsaved);
            assert!(view.pending.is_some());
            assert!(view.has_unsaved_work());
            assert_eq!(
                raster::composite(&view.editor.document).get_pixel(0, 0).0,
                [42, 78, 131, 255]
            );
        });
    }

    #[gpui_kit::test]
    fn create_workspace_is_reachable_and_contained_at_minimum_window(cx: &mut TestAppContext) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let recovery = temp.path().to_path_buf();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(None, window, cx);
            view.recovery = Recovery::at(recovery);
            view.dialog = Dialog::None;
            view.editor = Editor::new(Document::new(40, 30));
            view.refresh(cx);
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let button = cx
            .debug_bounds("workspace-create")
            .expect("Create workspace button");
        cx.simulate_click(button.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let inspector = cx
            .debug_bounds("create-inspector")
            .expect("Create inspector");
        assert!(inspector.bottom_right().x <= px(800.) && inspector.bottom_right().y <= px(600.));
        let first_row = ["create-pages", "create-templates", "create-brand"]
            .map(|id| cx.debug_bounds(id).expect("first Create navigation row"));
        let second_row = ["create-assets", "create-motion", "create-export"]
            .map(|id| cx.debug_bounds(id).expect("second Create navigation row"));
        assert!(
            first_row
                .iter()
                .all(|bounds| bounds.bottom_right().x <= inspector.bottom_right().x)
        );
        assert!(
            second_row
                .iter()
                .all(|bounds| bounds.bottom_right().x <= inspector.bottom_right().x)
        );
        assert!(first_row[0].bottom_right().y < second_row[0].origin.y);
        assert!(cx.debug_bounds("create-add-page").is_some());
        cx.update(|_, cx| assert_eq!(view.read(cx).inspector_tab, studio_ui::InspectorTab::Create));
    }
    #[test]
    fn external_change_to_inactive_page_changes_project_stamp() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("campaign.omuse");
        let mut project = Project::new("Campaign", Document::new(8, 8));
        project.add_blank_page("Second", 8, 8).unwrap();
        project.save(&path).unwrap();
        let before = project_stamp(&path).unwrap();
        std::fs::write(path.join("external-change"), "changed").unwrap();
        assert_ne!(project_stamp(&path), Some(before));
    }

    #[gpui_kit::test]
    fn externally_changed_lazy_page_cannot_replace_current_unsaved_artwork(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init_test_theme);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("campaign.omuse");
        let mut project = Project::new("Campaign", Document::new(8, 8));
        let first = project.active_page_id().to_owned();
        let second = project.add_blank_page("Second", 8, 8).unwrap();
        project.set_active_page(&first).unwrap();
        project.save(&path).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = EditorView::new(Some(path.clone()), window, cx);
            view.recovery = Recovery::at(temp.path().join("recovery"));
            view.editor.fill_selection([42, 78, 131, 255]);
            view
        });
        let mut external = Document::new(8, 8);
        external.layers[0].image =
            Some(image::RgbaImage::from_pixel(8, 8, image::Rgba([201, 54, 24, 255])).into());
        document::save(
            &external,
            &path.join("pages").join(format!("{second}.omuse")),
        )
        .unwrap();
        view.update(cx, |view, cx| {
            let revision = view.editor.revision();
            let error = view.activate_page(&second, cx).unwrap_err();
            assert!(
                format!("{error:#}").contains("changed on disk"),
                "{error:#}"
            );
            assert_eq!(
                view.create
                    .session
                    .as_ref()
                    .unwrap()
                    .project
                    .active_page_id(),
                first
            );
            assert_eq!(view.editor.revision(), revision);
            assert!(view.has_unsaved_work());
            assert_eq!(
                raster::composite(&view.editor.document).get_pixel(0, 0).0,
                [42, 78, 131, 255]
            );
            assert!(view.editor.undo());
        });
    }
}

impl CreateState {
    pub(super) fn release(&mut self, cx: &mut App) {
        self.restore.release(cx);
        self.job = None;
        self.bulk_job = None;
        if let Some((_, _, image)) = self.preview_cache.get_mut().take() {
            cx.drop_image(image, None);
        }
        for (_, image) in std::mem::take(&mut self.template_previews.images) {
            cx.drop_image(image, None);
        }
    }
}
