//! Explicit live qualification for the in-app subscription image workflow.
//!
//! This module is reachable only from the native UI smoke entry point. It
//! deliberately uses the same `AiState` request, review, Keep, package-save,
//! reopen, and collection-undo paths as the inspector. There is no provider
//! adapter, fixture image, or retry path here.

use super::*;
use anyhow::{Result, ensure};
use gpui_kit::{AsyncApp, WeakEntity};
use image::{Rgba, RgbaImage};
use omuse::{
    ai::{Capability, ConnectionState, ProviderId},
    ai_history::{ContextRole, Operation, SourceIdentity, StoredProposal},
    create,
    create_history::documents_match,
    create_project::Project,
    model::{Document, Layer},
    raster,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const QUALIFICATION_TIMEOUT: Duration = Duration::from_secs(300);
const QUALIFICATION_WIDTH: u32 = 64;
const QUALIFICATION_HEIGHT: u32 = 80;
const SUBJECT_LEFT: u32 = 20;
const SUBJECT_TOP: u32 = 28;
const SUBJECT_WIDTH: u32 = 24;
const SUBJECT_HEIGHT: u32 = 24;
const EDIT_LEFT: u32 = 5;
const EDIT_TOP: u32 = 58;
const EDIT_WIDTH: u32 = 14;
const EDIT_HEIGHT: u32 = 14;
const EXPAND_LEFT: u32 = 11;
const EXPAND_TOP: u32 = 7;
const EXPAND_RIGHT: u32 = 17;
const EXPAND_BOTTOM: u32 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeImageIntent {
    Generate,
    Replace,
    Remove,
    Background,
    Expand,
}

impl NativeImageIntent {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "generate" => Ok(Self::Generate),
            "replace" => Ok(Self::Replace),
            "remove" => Ok(Self::Remove),
            "background" => Ok(Self::Background),
            "expand" => Ok(Self::Expand),
            _ => anyhow::bail!("Unsupported native image qualification intent"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Replace => "replace",
            Self::Remove => "remove",
            Self::Background => "background",
            Self::Expand => "expand",
        }
    }

    fn image_intent(self) -> ImageIntent {
        match self {
            Self::Generate => ImageIntent::Generate,
            Self::Replace | Self::Remove => ImageIntent::Replace,
            Self::Background => ImageIntent::Background,
            Self::Expand => ImageIntent::Expand {
                left: EXPAND_LEFT,
                top: EXPAND_TOP,
                right: EXPAND_RIGHT,
                bottom: EXPAND_BOTTOM,
            },
        }
    }

    fn capability(self) -> Capability {
        if self == Self::Generate {
            Capability::ImageGeneration
        } else {
            Capability::ImageEditing
        }
    }
}

struct NativeSource {
    document: Document,
    protected_subject: Vec<u8>,
    brand_id: String,
}

struct NativeCandidate {
    result_id: String,
    provider: ProviderId,
    runtime: String,
    dimensions: [u32; 2],
    source: SourceIdentity,
    source_hash: String,
    source_mask_hash: Option<String>,
    result_asset_hash: String,
    task: String,
    variation_index: u8,
    variation_total: u8,
    selection: Option<Selection>,
    document: Document,
    project: Project,
    editable_mask: Option<NativeMask>,
}

/// A retained Generate result can be restored only as review evidence after a
/// restart. It intentionally omits a document/project because the normal
/// source-identity guard must continue to reject stale automatic application.
struct NativeRestoredGenerate {
    result_id: String,
    provider: ProviderId,
    runtime: String,
    dimensions: [u32; 2],
    source_identity_hash: String,
}

struct NativeMask {
    width: u32,
    height: u32,
    values: Vec<u8>,
}

impl EditorView {
    fn start_native_single_image_qualification(
        &mut self,
        dir: PathBuf,
        requested_intent: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |view, cx| {
            let outcome = async {
                let intent = NativeImageIntent::parse(&requested_intent)?;
                let image_intent = intent.image_intent();
                std::fs::create_dir_all(&dir)?;
                let source = view.update_in(cx, |this, _, cx| {
                    this.prepare_native_ai_source(&dir, cx)
                })??;

                cx.background_executor()
                    .timer(Duration::from_millis(700))
                    .await;
                cx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                })?;
                ensure!(
                    view.update(cx, |this, _| {
                        let viewport = this.viewport.get();
                        viewport.size.width > px(20.) && viewport.size.height > px(20.)
                    })?,
                    "Native AI qualification window has no usable canvas area"
                );

                if omuse::identity::env_var_os("OMUSE_NATIVE_WAIT").is_some() {
                    std::fs::write(dir.join("window-ready"), b"ready")?;
                    let handshake_deadline = Instant::now() + Duration::from_secs(10);
                    while !dir.join("start").is_file() {
                        ensure!(
                            Instant::now() < handshake_deadline,
                            "Native AI qualification did not receive its window-focus handshake"
                        );
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(400))
                        .await;
                }

                let native_bounds = cx.update(|window, _| {
                    format!(
                        "window={:?}, viewport={:?}",
                        window.bounds(),
                        window.viewport_size()
                    )
                })?;
                std::fs::write(dir.join("window-bounds.txt"), native_bounds)?;
                view.update_in(cx, |this, _, cx| {
                    capture_display(this, &source.document, &dir.join("before.png"), cx)
                })??;

                let readiness_deadline = Instant::now() + QUALIFICATION_TIMEOUT;
                let provider_runtime = loop {
                    let runtime = view.update(cx, |this, cx| -> Result<Option<String>> {
                        if this.ai.providers.is_empty() && !this.ai.checking {
                            this.discover_ai_connections(cx);
                        }
                        let Some(status) = this
                            .ai
                            .providers
                            .iter()
                            .find(|status| status.provider == ProviderId::CodexSubscription)
                        else {
                            return Ok(None);
                        };
                        if status.connection != ConnectionState::Ready || status.client.is_none() {
                            return Ok(None);
                        }
                        ensure!(
                            status.may_attempt(intent.capability()),
                            "The installed Codex subscription runtime does not advertise the requested image capability"
                        );
                        Ok(Some(
                            status
                                .version
                                .as_deref()
                                .filter(|version| !version.trim().is_empty())
                                .context("The verified connection omitted its runtime version")?
                                .to_owned(),
                        ))
                    })??;
                    if let Some(runtime) = runtime {
                        break runtime;
                    }
                    ensure!(
                        Instant::now() < readiness_deadline,
                        "Timed out waiting for the installed Codex subscription connection"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                };

                view.update_in(cx, |this, window, cx| -> Result<()> {
                    this.ai.routing.image_provider = ProviderId::CodexSubscription;
                    this.ai.variation_count = 1;
                    this.ai.variation_batch = None;
                    this.ai.references.clear();
                    this.ai.follow_up = None;
                    let prompt = match intent {
                        NativeImageIntent::Generate => {
                            "Generate one small abstract geometric study with warm paper, coral, plum, and gold. Do not include text, people, logos, or recognizable products."
                        }
                        NativeImageIntent::Replace => {
                            this.editor.select_rectangle(
                                EDIT_LEFT as f32,
                                EDIT_TOP as f32,
                                EDIT_WIDTH as f32,
                                EDIT_HEIGHT as f32,
                            );
                            "Replace only the selected warm-paper patch with a small plum-and-gold geometric detail. Preserve every pixel outside the selection."
                        }
                        NativeImageIntent::Remove => {
                            this.editor.select_rectangle(
                                EDIT_LEFT as f32,
                                EDIT_TOP as f32,
                                EDIT_WIDTH as f32,
                                EDIT_HEIGHT as f32,
                            );
                            "Remove the selected visual blemish and continue the surrounding warm-paper field naturally. Preserve every pixel outside the selection."
                        }
                        NativeImageIntent::Background => {
                            this.editor.select_rectangle(
                                SUBJECT_LEFT as f32,
                                SUBJECT_TOP as f32,
                                SUBJECT_WIDTH as f32,
                                SUBJECT_HEIGHT as f32,
                            );
                            "Replace only the editable background with a quiet abstract field of soft warm-paper geometry. Preserve the protected central coral square exactly."
                        }
                        NativeImageIntent::Expand => {
                            "Continue the warm-paper geometric composition naturally into every requested expanded edge. Preserve the complete original canvas exactly."
                        }
                    };
                    this.ai
                        .prompt
                        .update(cx, |state, cx| state.set_value(prompt, window, cx));
                    if intent == NativeImageIntent::Remove {
                        this.start_ai_image_job_with_instruction(
                            image_intent.clone(),
                            Some("Remove the selected object or unwanted element, then reconstruct the surrounding content naturally.".into()),
                            CapabilitySubmission::ExplicitNativeQualification,
                            cx,
                        );
                    } else {
                        this.start_native_ai_image_qualification(image_intent.clone(), cx);
                    }
                    Ok(())
                })??;

                let candidate =
                    wait_for_native_candidate(&view, cx, image_intent.clone(), intent.label())
                        .await?;
                ensure!(
                    candidate.provider == ProviderId::CodexSubscription,
                    "The image request did not use the selected Codex subscription provider"
                );
                ensure!(
                    candidate.runtime == provider_runtime,
                    "The completed result runtime differs from the verified connection runtime"
                );
                ensure!(
                    candidate.task == intent.label(),
                    "The completed result used a different Omuse image task"
                );
                ensure!(
                    candidate.variation_index == 1 && candidate.variation_total == 1,
                    "Native image qualification must issue exactly one variation"
                );
                ensure!(
                    !candidate.source_hash.is_empty()
                        && ((intent == NativeImageIntent::Generate
                            && candidate.source_mask_hash.is_none())
                            || (intent != NativeImageIntent::Generate
                                && candidate.source_mask_hash.is_some())),
                    "The completed result omitted its source or mask hash"
                );
                ensure!(
                    candidate.source_hash
                        == expected_native_source_hash(&source.document, &image_intent),
                    "The recorded source canvas hash does not match the exact staged pixels"
                );
                assert_retained_mask_matches_request(
                    &source.document,
                    candidate.selection.as_ref(),
                    &image_intent,
                    candidate.editable_mask.as_ref(),
                )?;
                if let Some(mask) = &candidate.editable_mask {
                    let retained_mask_hash = bytes_content_hash(&mask.values);
                    ensure!(
                        candidate.source_mask_hash.as_deref()
                            == Some(retained_mask_hash.as_str()),
                        "The recorded source mask hash does not match the exact retained mask"
                    );
                }
                ensure!(
                    view.update(cx, |this, _| this.ai_source_matches(&candidate.source))?,
                    "The completed result source identity no longer matches the live canvas"
                );
                ensure!(
                    view.update(cx, |this, _| documents_match(
                        &this.editor.document,
                        &source.document
                    ))?,
                    "The source canvas changed while the result was awaiting review"
                );
                assert_native_source_layers_survive(
                    &source.document,
                    &candidate.document,
                    &image_intent,
                )?;
                assert_intent_produced_visible_change(
                    &source.document,
                    &candidate.document,
                    &image_intent,
                    candidate.editable_mask.as_ref(),
                )?;
                match intent {
                    NativeImageIntent::Replace
                    | NativeImageIntent::Remove
                    | NativeImageIntent::Background => assert_protected_subject_is_exact(
                        &source.document,
                        &candidate.document,
                        &source.protected_subject,
                        candidate.editable_mask.as_ref().context(
                            "The native edit request did not retain its actual edit mask",
                        )?,
                    )?,
                    NativeImageIntent::Expand => assert_expanded_original_is_exact(
                        &source.document,
                        &candidate.document,
                        EXPAND_LEFT,
                        EXPAND_TOP,
                        EXPAND_RIGHT,
                        EXPAND_BOTTOM,
                    )?,
                    NativeImageIntent::Generate => {}
                }

                view.update_in(cx, |this, _, cx| {
                    capture_display(
                        this,
                        &candidate.document,
                        &dir.join("candidate.png"),
                        cx,
                    )
                })??;
                let package = dir.join(format!("native-ai-{}.omuse", intent.label()));
                view.update_in(cx, |this, window, cx| -> Result<()> {
                    this.apply_ai_plan(cx);
                    ensure!(this.ai.result.is_none(), "Keep did not commit the image candidate");
                    ensure!(
                        documents_match(&this.editor.document, &candidate.document),
                        "Keep did not preserve the reviewed image candidate"
                    );
                    let kept = this.editor.document.clone();
                    capture_display(this, &kept, &dir.join("kept.png"), cx)?;
                    this.save_to(package.clone(), window, cx);
                    ensure!(this.create.saving, "Keep did not start the native image save");
                    Ok(())
                })??;

                let save_deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    let saved = view.update(cx, |this, _| -> Result<bool> {
                        if this.create.saving {
                            return Ok(false);
                        }
                        ensure!(
                            this.path.as_ref() == Some(&package) && !this.has_unsaved_work(),
                            "The native image save did not complete: {}",
                            this.status
                        );
                        Ok(true)
                    })??;
                    if saved {
                        break;
                    }
                    ensure!(
                        Instant::now() < save_deadline,
                        "The native image save timed out"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }

                let reopen_path = package.clone();
                let (reopened_document, reopened_project) = cx
                    .background_executor()
                    .spawn(async move { EditorView::open_content(&reopen_path) })
                    .await?;
                view.update_in(cx, |this, window, cx| -> Result<()> {
                    let mut reopened = reopened_project
                        .context("The native opener did not recognize its saved collection")?;
                    let mut expected_project = candidate.project.clone();
                    assert_reopened_image_project(
                        &mut reopened,
                        &mut expected_project,
                        &source.document,
                        &source.brand_id,
                        &image_intent,
                    )?;
                    ensure!(
                        disk_semantic_document_matches(
                            &candidate.document,
                            &reopened_document
                        )?,
                        "The reopened canvas differs from the kept result"
                    );

                    this.command("undo", window, cx);
                    ensure!(
                        documents_match(&this.editor.document, &source.document),
                        "Undo did not restore the native source after Keep"
                    );
                    let undone = this.editor.document.clone();
                    capture_display(this, &undone, &dir.join("undo.png"), cx)?;
                    this.command("redo", window, cx);
                    ensure!(
                        documents_match(&this.editor.document, &candidate.document),
                        "Redo did not restore the kept image result"
                    );
                    let redone = this.editor.document.clone();
                    capture_display(this, &redone, &dir.join("redo.png"), cx)?;

                    this.install_opened_content(reopened_document.clone(), Some(reopened));
                    this.path = Some(package.clone());
                    capture_display(
                        this,
                        &reopened_document,
                        &dir.join("reopened.png"),
                        cx,
                    )?;
                    Ok(())
                })??;

                let capture_names = [
                    "before.png",
                    "candidate.png",
                    "kept.png",
                    "undo.png",
                    "redo.png",
                    "reopened.png",
                ];
                let render_captures = capture_names
                    .iter()
                    .map(|name| render_capture(&dir.join(name), name))
                    .collect::<Result<Vec<_>>>()?;
                ensure!(
                    render_captures[0]["contentHash"] == render_captures[3]["contentHash"],
                    "Undo render does not match the exact source render"
                );
                ensure!(
                    [2, 4, 5].into_iter().all(|index| {
                        render_captures[1]["contentHash"]
                            == render_captures[index]["contentHash"]
                    }),
                    "Keep, Redo or reopen render differs from the reviewed candidate"
                );
                Ok::<_, anyhow::Error>(serde_json::json!({
                    "schema": "omuse.native-ai-image-qualification.v1",
                    "status": "passed",
                    "intent": intent.label(),
                    "provider": candidate.provider,
                    "runtime": candidate.runtime,
                    "resultID": candidate.result_id,
                    "sourceIdentity": &candidate.source,
                    "hashes": {
                        "sourceCanvas": candidate.source_hash,
                        "sourceMask": candidate.source_mask_hash,
                        "resultAsset": candidate.result_asset_hash,
                    },
                    "dimensions": {
                        "source": [source.document.width, source.document.height],
                        "providerAsset": candidate.dimensions,
                        "keptCanvas": [candidate.document.width, candidate.document.height],
                        "reopenedCanvas": [reopened_document.width, reopened_document.height],
                    },
                    "variation": { "index": 1, "total": 1 },
                    "automaticRetries": 0,
                    "renderCaptures": render_captures,
                    "reviewSourceUnchanged": true,
                    "protectedPixelsByteExact": matches!(intent, NativeImageIntent::Replace | NativeImageIntent::Remove | NativeImageIntent::Background),
                    "expandedOriginalTranslatedByteExact": intent == NativeImageIntent::Expand,
                    "nativeOriginalSourcePreserved": true,
                    "brandPreserved": true,
                    "keepSaveReopenUndoRedo": "passed",
                }))
            }
            .await;

            match outcome {
                Ok(receipt) => {
                    let _ = write_json(&dir.join("native-results.json"), &receipt);
                }
                Err(error) => {
                    let error = sanitize_error(&error);
                    let failure = serde_json::json!({
                        "schema": "omuse.native-ai-image-qualification.v1",
                        "status": "failed",
                        "intent": sanitize_text(&requested_intent),
                        "error": error,
                    });
                    let _ = write_json(&dir.join("native-results.json"), &failure);
                    let _ = write_json(&dir.join("native-errors.json"), &failure);
                    let _ = std::fs::write(
                        dir.join("native-error.txt"),
                        failure["error"]
                            .as_str()
                            .unwrap_or("Native AI image qualification failed"),
                    );
                }
            }
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            if omuse::identity::env_var_os("OMUSE_NATIVE_WAIT").is_none() {
                let _ = cx.update(|_, cx| cx.quit());
            }
        })
        .detach();
    }

    /// Run Generate, protected-background editing and one editable carousel plan
    /// against the installed, validated Codex subscription runtime. The
    /// evidence directory is supplied by `--ui-smoke`; ordinary startup never
    /// calls this method.
    pub(in crate::ui) fn start_native_ai_qualification(
        &mut self,
        dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Ok(intent) = omuse::identity::env_var("OMUSE_NATIVE_AI_IMAGE_INTENT") {
            self.start_native_single_image_qualification(dir, intent, window, cx);
            return;
        }
        cx.spawn_in(window, async move |view, cx| {
        let outcome = async {
            std::fs::create_dir_all(&dir)?;
            let source = view.update_in(cx, |this, _, cx| {
                this.prepare_native_ai_source(&dir, cx)
            })??;

            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })?;
            ensure!(
                view.update(cx, |this, _| {
                    let viewport = this.viewport.get();
                    viewport.size.width > px(20.) && viewport.size.height > px(20.)
                })?,
                "Native AI qualification window has no usable canvas area"
            );

            if omuse::identity::env_var_os("OMUSE_NATIVE_WAIT").is_some() {
                std::fs::write(dir.join("window-ready"), b"ready")?;
                let handshake_deadline = Instant::now() + Duration::from_secs(10);
                while !dir.join("start").is_file() {
                    ensure!(
                        Instant::now() < handshake_deadline,
                        "Native AI qualification did not receive its window-focus handshake"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
            }

            let native_bounds = cx.update(|window, _| {
                format!(
                    "window={:?}, viewport={:?}",
                    window.bounds(),
                    window.viewport_size()
                )
            })?;
            std::fs::write(dir.join("window-bounds.txt"), native_bounds)?;

            view.update_in(cx, |this, _, cx| {
                capture_display(this, &source.document, &dir.join("before.png"), cx)
            })??;

            let readiness_deadline = Instant::now() + QUALIFICATION_TIMEOUT;
            let provider_runtime = loop {
                let runtime = view.update(cx, |this, cx| -> Result<Option<String>> {
                    if this.ai.providers.is_empty() && !this.ai.checking {
                        this.discover_ai_connections(cx);
                    }
                    let Some(status) = this
                        .ai
                        .providers
                        .iter()
                        .find(|status| status.provider == ProviderId::CodexSubscription)
                    else {
                        return Ok(None);
                    };
                    if status.connection != ConnectionState::Ready || status.client.is_none() {
                        return Ok(None);
                    }
                    ensure!(
                        status.may_attempt(Capability::ImageGeneration)
                            && status.may_attempt(Capability::ImageEditing),
                        "The installed Codex subscription runtime does not advertise image generation and editing"
                    );
                    Ok(Some(sanitize_runtime(status.version.as_deref())))
                })??;
                if let Some(runtime) = runtime {
                    break runtime;
                }
                ensure!(
                    Instant::now() < readiness_deadline,
                    "Timed out waiting for the installed Codex subscription connection"
                );
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
            };

            // Keep the resumed and fresh paths on the same explicit provider
            // state. A retained review must never inherit an arbitrary UI
            // selection or queue from an earlier window.
            view.update(cx, |this, _| {
                this.ai.routing.image_provider = ProviderId::CodexSubscription;
                this.ai.variation_count = 1;
                this.ai.references.clear();
                this.ai.follow_up = None;
            })?;
            let resume_generate = omuse::identity::env_var("OMUSE_NATIVE_AI_RESUME_GENERATE")
                .as_deref()
                == Ok("1");
            let generate_operation = if resume_generate {
                let result_id = view.update(cx, |this, _| this.native_resumable_generate_id())??;
                view.update_in(cx, |this, _, cx| -> Result<()> {
                    this.select_ai_history(&result_id, cx);
                    Ok(())
                })??;
                let generate =
                    wait_for_native_restored_generate(&view, cx, &result_id).await?;
                ensure!(
                    generate.provider == ProviderId::CodexSubscription,
                    "The restored generate result did not use the selected Codex subscription provider"
                );
                serde_json::json!({
                    "intent": "generate",
                    "resumedFromHistory": true,
                    "originalProvenance": {
                        "resultID": generate.result_id,
                        "provider": generate.provider,
                        "runtime": generate.runtime,
                        "importedAssetDimensions": generate.dimensions,
                        "sourceIdentityHash": generate.source_identity_hash,
                        "restore": "reviewed through local history; not applied to the new source",
                    },
                })
            } else {
                view.update_in(cx, |this, window, cx| -> Result<()> {
                    this.ai.prompt.update(cx, |prompt, cx| {
                        prompt.set_value(
                            "Generate one small, harmless abstract geometric study with warm paper, coral, plum, and gold. Do not include text, people, logos, or recognizable products.",
                            window,
                            cx,
                        )
                    });
                    this.start_native_ai_image_qualification(ImageIntent::Generate, cx);
                    Ok(())
                })??;

                let generate = wait_for_native_candidate(
                    &view,
                    cx,
                    ImageIntent::Generate,
                    "Generate image",
                )
                .await?;
                ensure!(
                    generate.provider == ProviderId::CodexSubscription,
                    "The generate request did not use the selected Codex subscription provider"
                );
                view.update_in(cx, |this, _, cx| {
                    capture_display(
                        this,
                        &generate.document,
                        &dir.join("generate-candidate.png"),
                        cx,
                    )
                })??;
                serde_json::json!({
                    "intent": "generate",
                    "resumedFromHistory": false,
                    "provider": generate.provider,
                    "runtime": generate.runtime,
                    "importedAssetDimensions": generate.dimensions,
                })
            };
            view.update_in(cx, |this, window, cx| -> Result<()> {
                // The generated asset is deliberately review-only here. The
                // second request must prove protected-background editing on
                // the unmodified native source rather than on a flattened
                // generation.
                this.clear_ai_result(cx);
                this.refresh(cx);
                this.editor.select_rectangle(
                    SUBJECT_LEFT as f32,
                    SUBJECT_TOP as f32,
                    SUBJECT_WIDTH as f32,
                    SUBJECT_HEIGHT as f32,
                );
                this.ai.prompt.update(cx, |prompt, cx| {
                    prompt.set_value(
                        "Replace only the editable background with a quiet abstract field of soft warm-paper geometry. Preserve the protected central coral square exactly. Do not add text, people, logos, or recognizable products.",
                        window,
                        cx,
                    )
                });
                this.start_native_ai_image_qualification(ImageIntent::Background, cx);
                Ok(())
            })??;

            let background = wait_for_native_candidate(
                &view,
                cx,
                ImageIntent::Background,
                "Background edit",
            )
            .await?;
            ensure!(
                background.provider == ProviderId::CodexSubscription,
                "The background edit did not use the selected Codex subscription provider"
            );

            let package = dir.join("native-ai-qualification.omuse");
            view.update_in(cx, |this, window, cx| -> Result<()> {
                capture_display(
                    this,
                    &background.document,
                    &dir.join("candidate.png"),
                    cx,
                )?;
                this.apply_ai_plan(cx);
                ensure!(
                    this.ai.result.is_none(),
                    "Keep did not commit the background candidate"
                );
                ensure!(
                    documents_match(&this.editor.document, &background.document),
                    "Keep did not preserve the reviewed native background candidate"
                );
                assert_protected_subject_is_exact(
                    &source.document,
                    &this.editor.document,
                    &source.protected_subject,
                    background
                        .editable_mask
                        .as_ref()
                        .context("The native background request did not retain its actual edit mask")?,
                )?;
                let after = this.editor.document.clone();
                capture_display(
                    this,
                    &after,
                    &dir.join("after.png"),
                    cx,
                )?;

                this.save_to(package.clone(), window, cx);
                ensure!(this.create.saving, "Keep did not start the native background save");
                Ok(())
            })??;
            let save_deadline = Instant::now() + Duration::from_secs(30);
            loop {
                let saved = view.update(cx, |this, _| -> Result<bool> {
                    if this.create.saving {
                        return Ok(false);
                    }
                    ensure!(
                        this.path.as_ref() == Some(&package) && !this.has_unsaved_work(),
                        "The native background save did not complete: {}", this.status
                    );
                    Ok(true)
                })??;
                if saved { break; }
                ensure!(Instant::now() < save_deadline, "The native background save timed out");
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
            let reopen_path = package.clone();
            let (reopened_document, reopened_project) = cx.background_executor().spawn(async move {
                EditorView::open_content(&reopen_path)
            }).await?;
            let mut receipt = view.update_in(cx, |this, window, cx| -> Result<serde_json::Value> {
                let mut reopened = reopened_project.context("The native opener did not recognize its saved collection")?;
                let mut expected_project = background.project.clone();
                assert_reopened_project_preserves_native_source(
                    &mut reopened,
                    &mut expected_project,
                    &source.document,
                    &source.brand_id,
                )?;

                this.command("undo", window, cx);
                ensure!(
                    documents_match(&this.editor.document, &source.document),
                    "Collection undo did not restore the native source after Keep"
                );
                let undone = this.editor.document.clone();
                capture_display(
                    this,
                    &undone,
                    &dir.join("undo.png"),
                    cx,
                )?;
                ensure!(
                    disk_semantic_document_matches(&background.document, &reopened_document)?,
                    "The reopened canvas differs from the kept result"
                );
                this.install_opened_content(reopened_document.clone(), Some(reopened));
                this.path = Some(package.clone());
                capture_display(this, &reopened_document, &dir.join("reopened.png"), cx)?;

                Ok(serde_json::json!({
                    "schema": "omuse.native-ai-qualification.v1",
                    "status": "passed",
                    "provider": ProviderId::CodexSubscription,
                    "runtime": provider_runtime,
                    "operations": [
                        generate_operation,
                        {
                            "intent": "background",
                            "provider": background.provider,
                            "runtime": background.runtime,
                            "importedAssetDimensions": background.dimensions,
                        },
                    ],
                    "captures": if resume_generate {
                        vec!["before.png", "candidate.png", "after.png", "undo.png", "reopened.png"]
                    } else {
                        vec!["before.png", "generate-candidate.png", "candidate.png", "after.png", "undo.png", "reopened.png"]
                    },
                    "keep": "applied through the native Keep path and restored with the standard Undo command",
                    "reopened": {
                        "brandPreserved": true,
                        "nativeSourcePreserved": true,
                    },
                    "protectedSubjectByteExact": true,
                    "undoRestoredNativeSource": true,
                }))
            })??;
            receipt["assistantCarousel"] = qualify_native_assistant(&view, cx, &dir).await?;
            Ok::<_, anyhow::Error>(receipt)
        }
        .await;

        match outcome {
            Ok(receipt) => {
                let _ = write_json(&dir.join("native-results.json"), &receipt);
            }
            Err(error) => {
                let error = sanitize_error(&error);
                let failure = serde_json::json!({
                    "schema": "omuse.native-ai-qualification.v1",
                    "status": "failed",
                    "error": error,
                });
                let _ = write_json(&dir.join("native-results.json"), &failure);
                let _ = write_json(&dir.join("native-errors.json"), &failure);
                let _ = std::fs::write(dir.join("native-error.txt"), failure["error"].as_str().unwrap_or("Native AI qualification failed"));
            }
        }
        cx.background_executor()
            .timer(Duration::from_millis(250))
            .await;
        if omuse::identity::env_var_os("OMUSE_NATIVE_WAIT").is_none() {
            let _ = cx.update(|_, cx| cx.quit());
        }
    })
    .detach();
    }
}

async fn qualify_native_assistant(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncApp,
    dir: &Path,
) -> Result<serde_json::Value> {
    view.update_in(cx, |this, window, cx| -> Result<()> {
        let brand = create::sugata_brand_kit();
        let first = create::instantiate_template("lesson-cover", Some(&brand))?;
        let first = create::resize_layout(&first, 1080, 1350, create::ResizeStrategy::Adapt)?;
        this.create.session = None;
        this.editor = Editor::new(first);
        this.path = None;
        this.ensure_create()?;
        let mut project = this.content_snapshot()?;
        project.title = "Assistant qualification · Sugata focus".into();
        let brand_id = project.add_brand(brand)?;
        create::apply_brand_to_project(&mut project, &brand_id)?;
        this.apply_creative_project(project, cx)?;
        this.clear_ai_result(cx);
        this.ai.references.clear();
        this.ai.follow_up = None;
        this.ai.routing.assistant = ProviderId::CodexSubscription;
        this.ai.prompt.update(cx, |prompt, cx| prompt.set_value(
            "Create a useful six-slide Sugata carousel about five ways to work with focus. This collection already has its first native cover page. Keep that cover editable and set its caption and alt text. Add exactly FIVE more editable pages using add_template_page, with concise headline/body copy, captions and alt text on each. Use the active brand and only valid catalog templates and named fields. Immediately after each added page, use resize_page to adapt it to 1080 by 1350 and animate_page with fade for 2000 ms. The final collection must contain exactly six pages, all with live text, meaningful captions and meaningful alt text. Do not generate or import raster images. Return one complete bounded native editing plan.",
            window, cx,
        ));
        this.start_native_ai_assistant_qualification(JobOperation::Assistant, cx);
        Ok(())
    })??;
    let deadline = Instant::now() + QUALIFICATION_TIMEOUT;
    let mut saw_active = false;
    let (mut draft, plan_json) = loop {
        let (active, candidate, activity) = view.update(cx, |this, _| -> Result<_> {
            let candidate = this
                .ai
                .result
                .as_ref()
                .map(|proposal| -> Result<_> {
                    ensure!(
                        proposal.intent.is_none(),
                        "Unexpected image result in the assistant qualification"
                    );
                    if let Some(error) = &proposal.error {
                        anyhow::bail!("Assistant draft failed: {error}");
                    }
                    Ok((
                        proposal
                            .project
                            .clone()
                            .context("Assistant did not propose an editable collection")?,
                        proposal
                            .plan_json
                            .clone()
                            .context("Assistant did not retain its native plan")?,
                    ))
                })
                .transpose()?;
            Ok((
                this.ai.running.is_some() || this.ai.preparing_image,
                candidate,
                this.ai.activity.clone(),
            ))
        })??;
        saw_active |= active;
        if let Some(candidate) = candidate {
            break candidate;
        }
        ensure!(
            !saw_active || active,
            "Assistant finished without a draft: {}",
            sanitize_text(&activity)
        );
        ensure!(
            Instant::now() < deadline,
            "Assistant qualification exceeded its time bound"
        );
        cx.background_executor()
            .timer(Duration::from_millis(250))
            .await;
    };
    ensure!(
        draft.page_ids().len() == 6,
        "Assistant draft did not contain exactly six pages"
    );
    fs_write_plan(dir, &plan_json)?;
    let mut pages = Vec::new();
    for (index, id) in draft.page_ids().iter().enumerate() {
        let document = draft.page_document(id)?;
        ensure!(
            (document.width, document.height) == (1080, 1350),
            "Assistant page has incorrect dimensions"
        );
        ensure!(
            document
                .layers
                .iter()
                .any(|layer| omuse::objects::live_text(layer).ok().flatten().is_some()),
            "Assistant page lost native text"
        );
        for field in ["caption", "altText"] {
            ensure!(
                document.metadata["omuseContent"][field]
                    .as_str()
                    .is_some_and(|value| !value.trim().is_empty()),
                "Assistant omitted {field}"
            );
        }
        raster::composite(document).save(dir.join(format!("assistant-page-{}.png", index + 1)))?;
        pages.push(serde_json::json!({"id": id, "name": document.name, "caption": document.metadata["omuseContent"]["caption"], "altText": document.metadata["omuseContent"]["altText"]}));
    }
    let path = dir.join("native-assistant-carousel.omuse");
    view.update_in(cx, |this, window, cx| -> Result<()> {
        this.apply_ai_plan(cx);
        ensure!(this.ai.result.is_none(), "Assistant Keep did not commit");
        ensure!(
            this.content_snapshot()?.page_ids().len() == 6,
            "Assistant Keep lost pages"
        );
        this.save_to(path.clone(), window, cx);
        ensure!(
            this.create.saving,
            "Assistant Keep did not start a collection save"
        );
        Ok(())
    })??;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let saved = view.update(cx, |this, _| -> Result<bool> {
            if this.create.saving {
                return Ok(false);
            }
            ensure!(
                this.path.as_ref() == Some(&path) && !this.has_unsaved_work(),
                "Assistant collection save failed: {}",
                this.status
            );
            Ok(true)
        })??;
        if saved {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "Assistant collection save timed out"
        );
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
    }
    let reopen_path = path.clone();
    let mut reopened = cx
        .background_executor()
        .spawn(async move { Project::open(&reopen_path) })
        .await?;
    ensure!(
        reopened.page_ids() == draft.page_ids(),
        "Assistant pages changed after reopen"
    );
    ensure!(
        reopened
            .active_brand()
            .is_some_and(|brand| brand.fonts.heading == "Outfit"),
        "Assistant brand was lost"
    );
    ensure!(
        disk_semantic_project_matches(&mut draft, &mut reopened)?,
        "Assistant collection semantics changed after reopen"
    );
    view.update_in(cx, |this, window, cx| -> Result<()> {
        this.command("undo", window, cx);
        ensure!(
            this.content_snapshot()?.page_ids().len() == 1,
            "Standard Undo did not restore the pre-assistant collection"
        );
        this.command("redo", window, cx);
        ensure!(
            this.content_snapshot()?.page_ids().len() == 6,
            "Standard Redo did not restore the assistant collection"
        );
        this.command("fit", window, cx);
        Ok(())
    })??;
    Ok(
        serde_json::json!({"pages":pages,"nativeTextPreserved":true,"brandPreserved":true,"keepSaveReopenUndoRedo":"passed"}),
    )
}

fn fs_write_plan(dir: &Path, plan: &str) -> Result<()> {
    std::fs::write(dir.join("assistant-plan.json"), plan)?;
    Ok(())
}

impl EditorView {
    fn prepare_native_ai_source(
        &mut self,
        dir: &Path,
        cx: &mut Context<Self>,
    ) -> Result<NativeSource> {
        ensure!(
            self.ai.running.is_none() && !self.ai.preparing_image,
            "An existing AI request is still active"
        );
        self.recovery = Recovery::at(dir.join("recovery"));
        self.dialog = Dialog::None;
        self.path = None;
        self.create.session = None;
        self.editor = Editor::new(native_source_document()?);
        self.ensure_create()?;
        let mut project = self.content_snapshot()?;
        let brand = create::sugata_brand_kit();
        let brand_id = project.add_brand(brand)?;
        create::apply_brand_to_project(&mut project, &brand_id)?;
        self.apply_creative_project(project, cx)?;
        self.editor.mark_saved();
        self.ai.routing.image_provider = ProviderId::CodexSubscription;
        self.ai.variation_count = 1;
        self.ai.references.clear();
        self.ai.follow_up = None;
        self.inspector_tab = studio_ui::InspectorTab::Assistant;
        self.inspector_visible = true;
        self.ai.connections_visible = false;
        self.zoom = 6.;
        self.show_grid = false;
        self.clear_ai_result(cx);
        self.refresh(cx);
        Ok(NativeSource {
            document: self.editor.document.clone(),
            protected_subject: native_subject_mask(),
            brand_id,
        })
    }

    /// Resolve exactly one retained Generate result in the isolated native
    /// evidence store. A resume never selects an arbitrary user history entry
    /// and never falls back to sending another provider request.
    fn native_resumable_generate_id(&self) -> Result<String> {
        native_resumable_generate_id(&self.ai.history)
    }
}

fn native_resumable_generate_id(entries: &[StoredProposal]) -> Result<String> {
    let entries = entries
        .iter()
        .filter(|entry| {
            entry.operation == Operation::GenerateImage
                && entry.provider == ProviderId::CodexSubscription
                && !entry.assets.is_empty()
        })
        .collect::<Vec<_>>();
    ensure!(
        entries.len() == 1,
        "Native resume requires exactly one retained Codex Generate result in this isolated evidence store"
    );
    Ok(entries[0].id.clone())
}

async fn wait_for_native_candidate(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncApp,
    intent: ImageIntent,
    operation: &str,
) -> Result<NativeCandidate> {
    let deadline = Instant::now() + QUALIFICATION_TIMEOUT;
    let mut saw_active_request = false;
    loop {
        let (active, candidate, activity) = view.update(cx, |this, _| -> Result<_> {
            let active = this.ai.running.is_some() || this.ai.preparing_image;
            let candidate = this
                .ai
                .result
                .as_ref()
                .map(|proposal| native_candidate(proposal, &intent))
                .transpose()?;
            Ok((active, candidate, this.ai.activity.clone()))
        })??;
        saw_active_request |= active;
        if let Some(candidate) = candidate {
            return Ok(candidate);
        }
        ensure!(
            !saw_active_request || active,
            "{operation} finished without a reviewable candidate: {}",
            sanitize_text(&activity)
        );
        ensure!(
            Instant::now() < deadline,
            "{operation} exceeded the 300 second qualification bound"
        );
        cx.background_executor()
            .timer(Duration::from_millis(250))
            .await;
    }
}

async fn wait_for_native_restored_generate(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncApp,
    result_id: &str,
) -> Result<NativeRestoredGenerate> {
    let deadline = Instant::now() + QUALIFICATION_TIMEOUT;
    loop {
        let (active, candidate, activity) = view.update(cx, |this, _| -> Result<_> {
            let candidate = this
                .ai
                .result
                .as_ref()
                .filter(|proposal| proposal.id == result_id)
                .map(native_restored_generate)
                .transpose()?;
            Ok((
                this.ai.running.is_some() || this.ai.preparing_image,
                candidate,
                this.ai.activity.clone(),
            ))
        })??;
        if let Some(candidate) = candidate {
            return Ok(candidate);
        }
        ensure!(
            active,
            "Retained Generate result did not restore through local history: {}",
            sanitize_text(&activity)
        );
        ensure!(
            Instant::now() < deadline,
            "Retained Generate result exceeded the 300 second local restore bound"
        );
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
    }
}

fn native_candidate(proposal: &AiProposal, intent: &ImageIntent) -> Result<NativeCandidate> {
    ensure!(
        proposal.intent.as_ref() == Some(intent),
        "A completed request returned an unexpected image intent"
    );
    if let Some(error) = &proposal.error {
        anyhow::bail!("The provider result could not become a native candidate: {error}");
    }
    let asset = proposal
        .assets
        .first()
        .context("The provider returned no image asset")?;
    ensure!(
        asset.width > 0 && asset.height > 0,
        "The provider returned an image with invalid dimensions"
    );
    let source_hash = proposal.provenance["sourceIdentityHash"]
        .as_str()
        .map(sanitize_text)
        .filter(|value| !value.is_empty())
        .context("The completed result omitted its source canvas hash")?;
    let source_mask_hash = proposal.provenance["sourceMaskHash"]
        .as_str()
        .map(sanitize_text)
        .filter(|value| !value.is_empty());
    let task = proposal.provenance["omuseTask"]
        .as_str()
        .map(sanitize_text)
        .filter(|value| !value.is_empty())
        .context("The completed result omitted its Omuse task")?;
    let runtime = proposal.provenance["providerRuntimeVersion"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .context("The completed result omitted its provider runtime version")?
        .to_owned();
    Ok(NativeCandidate {
        result_id: sanitize_text(&proposal.id),
        provider: proposal.provider,
        runtime,
        dimensions: [asset.width, asset.height],
        source: proposal.source.clone(),
        source_hash,
        source_mask_hash,
        result_asset_hash: file_content_hash(&asset.path)?,
        task,
        variation_index: proposal.variation_index,
        variation_total: proposal.variation_total,
        selection: proposal.selection.clone(),
        document: proposal
            .document
            .clone()
            .context("The provider result has no editable native candidate")?,
        project: proposal
            .project
            .clone()
            .context("The provider result has no retained editable project")?,
        editable_mask: proposal
            .context_assets
            .iter()
            .find(|asset| asset.role == ContextRole::SelectionMask)
            .map(|asset| -> Result<NativeMask> {
                let image = image::ImageReader::open(&asset.path)?
                    .with_guessed_format()?
                    .decode()
                    .context("The native selection mask could not be read")?
                    .into_luma8();
                Ok(NativeMask {
                    width: image.width(),
                    height: image.height(),
                    values: image.into_raw(),
                })
            })
            .transpose()?,
    })
}

fn native_restored_generate(proposal: &AiProposal) -> Result<NativeRestoredGenerate> {
    ensure!(
        proposal.operation == Operation::GenerateImage
            && proposal.intent == Some(ImageIntent::Generate),
        "The retained result is not a generated image"
    );
    if let Some(error) = &proposal.error {
        anyhow::bail!("The retained result could not become a review: {error}");
    }
    let asset = proposal
        .assets
        .first()
        .context("The retained result has no image asset")?;
    ensure!(
        asset.width > 0 && asset.height > 0 && proposal.preview.is_some(),
        "The retained result did not restore a displayable image review"
    );
    ensure!(
        proposal.document.is_none() && proposal.project.is_none(),
        "A retained result from a previous source must remain review-only"
    );
    let source_identity_hash = proposal.provenance["sourceIdentityHash"]
        .as_str()
        .map(sanitize_text)
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| "not supplied".into());
    Ok(NativeRestoredGenerate {
        result_id: sanitize_text(&proposal.id),
        provider: proposal.provider,
        runtime: sanitize_runtime(proposal.provenance["providerRuntimeVersion"].as_str()),
        dimensions: [asset.width, asset.height],
        source_identity_hash,
    })
}

fn native_source_document() -> Result<Document> {
    let mut document = Document::new(QUALIFICATION_WIDTH, QUALIFICATION_HEIGHT);
    document.name = "Native AI qualification".into();
    document.layers.clear();
    document.metadata["nativeAiQualification"] = serde_json::json!({
        "synthetic": true,
        "canvas": [QUALIFICATION_WIDTH, QUALIFICATION_HEIGHT],
    });

    let mut background = Layer::paint(
        "Native AI warm background",
        QUALIFICATION_WIDTH,
        QUALIFICATION_HEIGHT,
    );
    background.image = Some(
        RgbaImage::from_pixel(
            QUALIFICATION_WIDTH,
            QUALIFICATION_HEIGHT,
            Rgba([246, 235, 214, 255]),
        )
        .into(),
    );

    let mut subject = Layer::paint(
        "Protected coral subject",
        QUALIFICATION_WIDTH,
        QUALIFICATION_HEIGHT,
    );
    subject.image = Some(
        RgbaImage::from_fn(QUALIFICATION_WIDTH, QUALIFICATION_HEIGHT, |x, y| {
            if (SUBJECT_LEFT..SUBJECT_LEFT + SUBJECT_WIDTH).contains(&x)
                && (SUBJECT_TOP..SUBJECT_TOP + SUBJECT_HEIGHT).contains(&y)
            {
                Rgba([226, 98, 74, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        })
        .into(),
    );
    subject.locked = true;
    let mut title = Layer::paint("Protected native headline", 1, 1);
    omuse::objects::set_live_text(
        &mut title,
        omuse::objects::LiveTextStyle {
            content: "OMUSE".into(),
            font_name: "Outfit".into(),
            font_size: 9.,
            red: 0.2,
            green: 0.1,
            blue: 0.2,
            ..Default::default()
        },
    )?;
    title.offset_x = 5.;
    title.offset_y = 5.;
    title.locked = true;
    let mut logo = Layer::paint("Protected editable mark", 8, 8);
    omuse::objects::set_live_shape(
        &mut logo,
        omuse::objects::LiveShapeStyle {
            kind: omuse::objects::LiveShapeKind::Ellipse,
            red: 0.6,
            green: 0.3,
            blue: 0.1,
            corner_radius: 0.,
            line_width: None,
            start: None,
            end: None,
        },
        8,
        8,
    )?;
    logo.offset_x = 51.;
    logo.offset_y = 5.;
    logo.locked = true;
    let mut removable = Layer::paint("Editable plum blemish", 7, 7);
    omuse::objects::set_live_shape(
        &mut removable,
        omuse::objects::LiveShapeStyle {
            kind: omuse::objects::LiveShapeKind::Ellipse,
            red: 0.32,
            green: 0.08,
            blue: 0.28,
            corner_radius: 0.,
            line_width: None,
            start: None,
            end: None,
        },
        7,
        7,
    )?;
    removable.offset_x = 8.;
    removable.offset_y = 61.;
    document.layers = vec![background, subject, title, logo, removable];
    Ok(document)
}

fn native_subject_mask() -> Vec<u8> {
    (0..QUALIFICATION_WIDTH * QUALIFICATION_HEIGHT)
        .map(|index| {
            let x = index % QUALIFICATION_WIDTH;
            let y = index / QUALIFICATION_WIDTH;
            if (SUBJECT_LEFT..SUBJECT_LEFT + SUBJECT_WIDTH).contains(&x)
                && (SUBJECT_TOP..SUBJECT_TOP + SUBJECT_HEIGHT).contains(&y)
            {
                255
            } else {
                0
            }
        })
        .collect()
}

fn capture_display(
    view: &mut EditorView,
    document: &Document,
    path: &Path,
    cx: &mut Context<EditorView>,
) -> Result<()> {
    view.pixels = raster::composite(document);
    view.present_pixels(cx);
    view.pixels.save(path)?;
    Ok(())
}

fn assert_protected_subject_is_exact(
    source: &Document,
    after: &Document,
    protected_subject: &[u8],
    editable_mask: &NativeMask,
) -> Result<()> {
    let before = raster::composite(source);
    let after = raster::composite(after);
    ensure!(
        before.dimensions() == after.dimensions()
            && protected_subject.len() == before.as_raw().len() / 4
            && (editable_mask.width, editable_mask.height) == before.dimensions()
            && editable_mask.values.len() == protected_subject.len(),
        "Native edit result has invalid protected-region dimensions"
    );
    for (index, (before, after)) in before.pixels().zip(after.pixels()).enumerate() {
        if protected_subject[index] == 255 {
            ensure!(
                editable_mask.values[index] == 0,
                "The retained native edit mask did not protect the central source subject"
            );
        }
        if editable_mask.values[index] == 0 {
            ensure!(
                before.0 == after.0,
                "Source pixel outside the retained editable mask changed at index {index}"
            );
        }
    }
    Ok(())
}

fn assert_expanded_original_is_exact(
    source: &Document,
    expanded: &Document,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
) -> Result<()> {
    ensure!(
        expanded.width == source.width + left + right
            && expanded.height == source.height + top + bottom,
        "Expanded result has incorrect asymmetric dimensions"
    );
    let source_pixels = raster::composite(source);
    let expanded_pixels = raster::composite(expanded);
    for (x, y, pixel) in source_pixels.enumerate_pixels() {
        ensure!(
            expanded_pixels.get_pixel(x + left, y + top) == pixel,
            "Original source pixel changed after translation at {x},{y}"
        );
    }
    Ok(())
}

fn assert_intent_produced_visible_change(
    source: &Document,
    candidate: &Document,
    intent: &ImageIntent,
    editable_mask: Option<&NativeMask>,
) -> Result<()> {
    let before = raster::composite(source);
    let after = raster::composite(candidate);
    match intent {
        ImageIntent::Generate => ensure!(
            before != after,
            "Generated result did not visibly differ from the synthetic source"
        ),
        ImageIntent::Replace | ImageIntent::Background => {
            let mask = editable_mask.context("The image edit omitted its retained mask")?;
            ensure!(
                before.dimensions() == after.dimensions()
                    && mask.values.len() == before.as_raw().len() / 4,
                "The image edit cannot be compared with its retained mask"
            );
            ensure!(
                before
                    .pixels()
                    .zip(after.pixels())
                    .zip(&mask.values)
                    .any(|((before, after), mask)| *mask > 0 && before != after),
                "The image edit made no visible change inside its editable region"
            );
        }
        ImageIntent::Expand {
            left,
            top,
            right: _,
            bottom: _,
        } => ensure!(
            after.enumerate_pixels().any(|(x, y, pixel)| {
                (x < *left || x >= *left + source.width || y < *top || y >= *top + source.height)
                    && pixel[3] > 0
            }),
            "Expanded result did not generate visible pixels outside the original canvas"
        ),
    }
    Ok(())
}

fn expected_native_source_hash(source: &Document, intent: &ImageIntent) -> String {
    let source_pixels = raster::composite(source);
    if let ImageIntent::Expand {
        left,
        top,
        right,
        bottom,
    } = *intent
    {
        let mut expanded =
            RgbaImage::new(source.width + left + right, source.height + top + bottom);
        image::imageops::replace(
            &mut expanded,
            &source_pixels,
            i64::from(left),
            i64::from(top),
        );
        bytes_content_hash(expanded.as_raw())
    } else {
        bytes_content_hash(source_pixels.as_raw())
    }
}

fn assert_retained_mask_matches_request(
    source: &Document,
    selection: Option<&Selection>,
    intent: &ImageIntent,
    retained: Option<&NativeMask>,
) -> Result<()> {
    let expected = ai_edits::prepare_input(source, selection, intent)?.mask;
    match (expected, retained) {
        (None, None) => Ok(()),
        (Some(expected), Some(retained)) => {
            ensure!(
                (retained.width, retained.height) == expected.dimensions()
                    && retained.values.as_slice() == expected.as_raw().as_slice(),
                "The retained editable mask differs from the exact submitted request mask"
            );
            Ok(())
        }
        _ => {
            anyhow::bail!("The retained editable mask presence differs from the submitted request")
        }
    }
}

fn assert_native_source_layers_survive(
    source: &Document,
    candidate: &Document,
    intent: &ImageIntent,
) -> Result<()> {
    ensure!(
        candidate.layers.len() > source.layers.len(),
        "The image result did not retain the native source layers separately from its generated layer"
    );
    let mut expected = source.clone();
    if let ImageIntent::Expand {
        left,
        top,
        right,
        bottom,
    } = *intent
    {
        let mut editor = Editor::new(expected);
        editor.set_history_limit(0);
        ensure!(
            editor.crop_canvas(
                -(left as i32),
                -(top as i32),
                source.width + left + right,
                source.height + top + bottom,
            ),
            "Could not construct the expected translated native source"
        );
        expected = editor.document;
    }
    let mut retained = candidate.clone();
    retained.layers.truncate(source.layers.len());
    ensure!(
        disk_semantic_document_matches(&expected, &retained)?,
        "The generated candidate changed or flattened the native source layers"
    );
    Ok(())
}

fn assert_reopened_image_project(
    project: &mut Project,
    expected: &mut Project,
    source: &Document,
    brand_id: &str,
    intent: &ImageIntent,
) -> Result<()> {
    ensure!(
        project.active_brand_id.as_deref() == Some(brand_id),
        "The active brand was not retained after reopening the native package"
    );
    let brand = project
        .active_brand()
        .context("The saved native brand is missing")?;
    ensure!(
        brand.name == "Sugata" && brand.fonts.heading == "Outfit" && brand.fonts.body == "Outfit",
        "The saved native brand tokens changed after reopening"
    );
    ensure!(
        disk_semantic_project_matches(expected, project)?,
        "The saved native project changed its editable pages, metadata, components, or protected resources"
    );
    let reopened = project.active_document()?.clone();
    assert_native_source_layers_survive(source, &reopened, intent)
}

fn render_capture(path: &Path, name: &str) -> Result<serde_json::Value> {
    let image = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()?;
    Ok(serde_json::json!({
        "file": name,
        "contentHash": file_content_hash(path)?,
        "dimensions": [image.0, image.1],
    }))
}

/// Compare a just-saved native project by its retained editing semantics.
///
/// `document::open` deliberately retains the complete manifest and each
/// complete layer record in metadata. Those records add format, asset, parent,
/// and transform envelope keys that did not exist in an in-memory template, so
/// `create_history::documents_match` is correctly too strict after disk I/O.
/// Keep that exact guard for live undo/history and use this bounded comparator
/// only in the native qualification's save/reopen assertion.
fn disk_semantic_project_matches(expected: &mut Project, reopened: &mut Project) -> Result<bool> {
    if expected.id != reopened.id
        || expected.title != reopened.title
        || expected.active_page_id() != reopened.active_page_id()
        || expected.active_brand_id != reopened.active_brand_id
        || expected.brand_kits != reopened.brand_kits
        || expected.metadata != reopened.metadata
    {
        return Ok(false);
    }

    let expected_pages = expected.page_summaries();
    let reopened_pages = reopened.page_summaries();
    if expected_pages.len() != reopened_pages.len() {
        return Ok(false);
    }
    for (expected_page, reopened_page) in expected_pages.iter().zip(&reopened_pages) {
        if expected_page.id != reopened_page.id
            || expected_page.name != reopened_page.name
            || expected_page.width != reopened_page.width
            || expected_page.height != reopened_page.height
            || expected_page.template_id != reopened_page.template_id
        {
            return Ok(false);
        }
        let expected_document = expected.page_document(&expected_page.id)?.clone();
        let reopened_document = reopened.page_document(&reopened_page.id)?.clone();
        if !disk_semantic_document_matches(&expected_document, &reopened_document)? {
            return Ok(false);
        }
    }

    let expected_components = expected.component_summaries();
    let reopened_components = reopened.component_summaries();
    if expected_components.len() != reopened_components.len() {
        return Ok(false);
    }
    for (expected_component, reopened_component) in
        expected_components.iter().zip(&reopened_components)
    {
        if expected_component.id != reopened_component.id
            || expected_component.name != reopened_component.name
            || expected_component.revision != reopened_component.revision
        {
            return Ok(false);
        }
        let (_, expected_layers) = expected.component_snapshot(&expected_component.id)?;
        let (_, reopened_layers) = reopened.component_snapshot(&reopened_component.id)?;
        if !disk_semantic_layers_match(&expected_layers, &reopened_layers)? {
            return Ok(false);
        }
    }

    let expected_resources = expected.resource_summaries();
    let reopened_resources = reopened.resource_summaries();
    if expected_resources.len() != reopened_resources.len() {
        return Ok(false);
    }
    for (expected_resource, reopened_resource) in expected_resources.iter().zip(&reopened_resources)
    {
        if expected_resource.id != reopened_resource.id
            || expected_resource.name != reopened_resource.name
            || expected_resource.media_type != reopened_resource.media_type
            || expected_resource.byte_len != reopened_resource.byte_len
            || expected.resource_bytes(&expected_resource.id)?
                != reopened.resource_bytes(&reopened_resource.id)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn disk_semantic_document_matches(expected: &Document, reopened: &Document) -> Result<bool> {
    if expected.width != reopened.width
        || expected.height != reopened.height
        || expected.name != reopened.name
        || expected.background != reopened.background
        || !disk_semantic_json_matches(
            &normalized_document_metadata(&expected.metadata),
            &normalized_document_metadata(&reopened.metadata),
        )
        || raster::composite(expected) != raster::composite(reopened)
    {
        return Ok(false);
    }
    disk_semantic_layers_match(&expected.layers, &reopened.layers)
}

fn disk_semantic_layers_match(expected: &[Layer], reopened: &[Layer]) -> Result<bool> {
    if expected.len() != reopened.len() {
        return Ok(false);
    }
    for (expected_layer, reopened_layer) in expected.iter().zip(reopened) {
        if expected_layer.id != reopened_layer.id
            || expected_layer.name != reopened_layer.name
            || expected_layer.visible != reopened_layer.visible
            || expected_layer.locked != reopened_layer.locked
            || expected_layer.opacity.to_bits() != reopened_layer.opacity.to_bits()
            || expected_layer.blend_mode != reopened_layer.blend_mode
            || expected_layer.offset_x.to_bits() != reopened_layer.offset_x.to_bits()
            || expected_layer.offset_y.to_bits() != reopened_layer.offset_y.to_bits()
            || expected_layer.rotation.to_bits() != reopened_layer.rotation.to_bits()
            || expected_layer.scale_x.to_bits() != reopened_layer.scale_x.to_bits()
            || expected_layer.scale_y.to_bits() != reopened_layer.scale_y.to_bits()
            || expected_layer.is_group() != reopened_layer.is_group()
            || !disk_semantic_images_match(&expected_layer.image, &reopened_layer.image)
            || !disk_semantic_images_match(&expected_layer.mask, &reopened_layer.mask)
            || !disk_semantic_advanced_matches(expected_layer, reopened_layer)?
            || !disk_semantic_vector_scene_matches(expected_layer, reopened_layer)?
            || omuse::objects::live_text(expected_layer)?
                != omuse::objects::live_text(reopened_layer)?
            || omuse::objects::live_shape(expected_layer)?
                != omuse::objects::live_shape(reopened_layer)?
            || !disk_semantic_json_matches(
                &normalized_layer_metadata(&expected_layer.metadata),
                &normalized_layer_metadata(&reopened_layer.metadata),
            )
            || !disk_semantic_layers_match(&expected_layer.children, &reopened_layer.children)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn disk_semantic_images_match(
    expected: &Option<omuse::shared_image::SharedImage>,
    reopened: &Option<omuse::shared_image::SharedImage>,
) -> bool {
    match (expected, reopened) {
        (None, None) => true,
        (Some(expected), Some(reopened)) => {
            expected.dimensions() == reopened.dimensions() && expected.as_raw() == reopened.as_raw()
        }
        _ => false,
    }
}

fn disk_semantic_advanced_matches(expected: &Layer, reopened: &Layer) -> Result<bool> {
    match (&expected.advanced, &reopened.advanced) {
        (None, None) => Ok(true),
        (Some(expected), Some(reopened)) => Ok(serde_json::to_value(&expected.recipe)?
            == serde_json::to_value(&reopened.recipe)?
            && expected.raw_bytes.as_deref() == reopened.raw_bytes.as_deref()),
        _ => Ok(false),
    }
}

fn disk_semantic_vector_scene_matches(expected: &Layer, reopened: &Layer) -> Result<bool> {
    match (&expected.vector_scene, &reopened.vector_scene) {
        (None, None) => Ok(true),
        (Some(expected), Some(reopened)) => Ok(
            serde_json::to_value(expected.as_ref())? == serde_json::to_value(reopened.as_ref())?
        ),
        _ => Ok(false),
    }
}

fn normalized_document_metadata(metadata: &serde_json::Value) -> serde_json::Value {
    let mut metadata = metadata.clone();
    let Some(record) = metadata.as_object_mut() else {
        return metadata;
    };
    // These are the serialized document envelope. Their semantics are covered
    // by the typed document fields and the layer comparison below.
    for key in [
        "format",
        "version",
        "colorSpace",
        "width",
        "height",
        "layers",
        "activeLayerID",
        "documentID",
        "compositorRustBackground",
    ] {
        record.remove(key);
    }
    metadata
}

fn normalized_layer_metadata(metadata: &serde_json::Value) -> serde_json::Value {
    let mut metadata = metadata.clone();
    let Some(record) = metadata.as_object_mut() else {
        return metadata;
    };
    // These are complete-layer record fields emitted by document::save. Layer
    // ids, hierarchy, typed transforms, pixels, masks and advanced state are
    // checked separately; preserve every unknown extension and meaningful
    // metadata field such as text, shape, frame, animation and mask placement.
    for key in [
        "id",
        "name",
        "isVisible",
        "locked",
        "opacity",
        "blendMode",
        "parentID",
        "isGroup",
        "rustEditableAsset",
        "rustVectorScene",
        "imageFile",
        "maskFile",
    ] {
        record.remove(key);
    }
    let remove_transform = if let Some(transform) = record
        .get_mut("transform")
        .and_then(serde_json::Value::as_object_mut)
    {
        for key in ["origin", "size", "rotation", "flipX", "flipY"] {
            transform.remove(key);
        }
        // The renderer treats absent sampling as High quality, and save adds
        // that default. Smooth and Nearest remain meaningful and are retained.
        if transform
            .get("sampling")
            .and_then(serde_json::Value::as_str)
            == Some("High quality")
        {
            transform.remove("sampling");
        }
        transform.is_empty()
    } else {
        false
    };
    if remove_transform {
        record.remove("transform");
    }
    metadata
}

/// Accept only the known one-ULP parser discrepancy for floating values that
/// still encode the same `f32`. Live-object styles are checked separately as
/// typed `f32` values; integers and all other metadata remain exact.
fn disk_semantic_json_matches(expected: &serde_json::Value, reopened: &serde_json::Value) -> bool {
    if expected == reopened {
        return true;
    }
    match (expected, reopened) {
        (serde_json::Value::Number(expected), serde_json::Value::Number(reopened)) => {
            if expected.is_i64() || expected.is_u64() || reopened.is_i64() || reopened.is_u64() {
                return false;
            }
            let Some(expected) = expected.as_f64() else {
                return false;
            };
            let Some(reopened) = reopened.as_f64() else {
                return false;
            };
            let expected_f32 = expected as f32;
            let reopened_f32 = reopened as f32;
            expected_f32.is_finite()
                && reopened_f32.is_finite()
                && expected_f32.to_bits() == reopened_f32.to_bits()
                && expected.to_bits().abs_diff(reopened.to_bits()) <= 1
        }
        (serde_json::Value::Array(expected), serde_json::Value::Array(reopened)) => {
            expected.len() == reopened.len()
                && expected
                    .iter()
                    .zip(reopened)
                    .all(|(expected, reopened)| disk_semantic_json_matches(expected, reopened))
        }
        (serde_json::Value::Object(expected), serde_json::Value::Object(reopened)) => {
            expected.len() == reopened.len()
                && expected.iter().all(|(key, expected)| {
                    reopened
                        .get(key)
                        .is_some_and(|reopened| disk_semantic_json_matches(expected, reopened))
                })
        }
        _ => false,
    }
}

fn assert_reopened_project_preserves_native_source(
    project: &mut Project,
    expected: &mut Project,
    source: &Document,
    brand_id: &str,
) -> Result<()> {
    ensure!(
        project.active_brand_id.as_deref() == Some(brand_id),
        "The active brand was not retained after reopening the native package"
    );
    let brand = project
        .active_brand()
        .context("The saved native brand is missing")?;
    ensure!(
        brand.name == "Sugata" && brand.fonts.heading == "Outfit" && brand.fonts.body == "Outfit",
        "The saved native brand tokens changed after reopening"
    );
    ensure!(
        disk_semantic_project_matches(expected, project)?,
        "The saved native project changed its editable pages, metadata, components, or protected resources"
    );
    let reopened = project.active_document()?.clone();
    ensure!(
        reopened.layers.len() >= source.layers.len(),
        "The reopened project lost its native source layers"
    );
    let mut reopened_source = reopened;
    reopened_source.layers.truncate(source.layers.len());
    ensure!(
        disk_semantic_document_matches(source, &reopened_source)?,
        "The reopened project changed the native source layers"
    );
    Ok(())
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn sanitize_runtime(value: Option<&str>) -> String {
    value
        .map(sanitize_text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "not supplied".into())
}

fn sanitize_error(error: &anyhow::Error) -> String {
    sanitize_text(&error.to_string())
}

fn sanitize_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len().min(256));
    for character in value.chars() {
        if output.len() >= 256 {
            break;
        }
        if character.is_ascii_alphanumeric()
            || matches!(
                character,
                ' ' | '.' | ',' | ':' | ';' | '-' | '_' | '(' | ')'
            )
        {
            output.push(character);
        } else {
            output.push(' ');
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_image_intent_parser_is_explicit_and_asymmetric_expand_is_fixed() {
        for (name, expected) in [
            ("generate", NativeImageIntent::Generate),
            ("replace", NativeImageIntent::Replace),
            ("remove", NativeImageIntent::Remove),
            ("background", NativeImageIntent::Background),
            ("expand", NativeImageIntent::Expand),
        ] {
            assert_eq!(NativeImageIntent::parse(name).unwrap(), expected);
        }
        assert!(NativeImageIntent::parse("all").is_err());
        assert_eq!(
            NativeImageIntent::Expand.image_intent(),
            ImageIntent::Expand {
                left: 11,
                top: 7,
                right: 17,
                bottom: 5,
            }
        );
    }

    #[test]
    fn expanded_original_guard_checks_every_translated_source_pixel() -> Result<()> {
        let source = native_source_document()?;
        let intent = NativeImageIntent::Expand.image_intent();
        let mut editor = Editor::new(source.clone());
        editor.set_history_limit(0);
        assert!(editor.crop_canvas(
            -(EXPAND_LEFT as i32),
            -(EXPAND_TOP as i32),
            source.width + EXPAND_LEFT + EXPAND_RIGHT,
            source.height + EXPAND_TOP + EXPAND_BOTTOM,
        ));
        let mut expanded = editor.document;
        expanded.layers.push(Layer::paint(
            "Transparent generated surround",
            expanded.width,
            expanded.height,
        ));
        assert_expanded_original_is_exact(
            &source,
            &expanded,
            EXPAND_LEFT,
            EXPAND_TOP,
            EXPAND_RIGHT,
            EXPAND_BOTTOM,
        )?;
        assert_native_source_layers_survive(&source, &expanded, &intent)?;

        expanded
            .layers
            .last_mut()
            .unwrap()
            .image
            .as_mut()
            .unwrap()
            .put_pixel(EXPAND_LEFT, EXPAND_TOP, Rgba([1, 2, 3, 255]));
        assert!(
            assert_expanded_original_is_exact(
                &source,
                &expanded,
                EXPAND_LEFT,
                EXPAND_TOP,
                EXPAND_RIGHT,
                EXPAND_BOTTOM,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn disk_semantic_comparator_accepts_template_roundtrip_and_rejects_content_or_geometry_loss()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let brand = create::sugata_brand_kit();
        let mut document = create::instantiate_template("customer-voice", Some(&brand))?;
        let scene = omuse::vector_scene::VectorScene {
            version: omuse::vector_scene::VECTOR_SCENE_VERSION,
            width: 16,
            height: 12,
            objects: vec![omuse::vector_scene::VectorObject::rectangle(
                "Comparator object",
                2.,
                2.,
                10.,
                8.,
                Some([20, 80, 140, 255]),
                None,
            )?],
        };
        let mut scene_layer = Layer::paint("Scene comparator", scene.width, scene.height);
        scene_layer.image = Some(
            scene
                .render(&std::sync::atomic::AtomicBool::new(false))?
                .into(),
        );
        scene_layer.vector_scene = Some(std::sync::Arc::new(scene));
        document.layers.push(scene_layer);
        let mut expected = Project::new("Native template round trip", document);
        let page_id = expected.active_page_id().to_owned();
        expected.set_page_template(&page_id, Some("customer-voice"))?;
        expected.add_brand(brand)?;
        {
            let document = expected.active_document_mut()?;
            document.metadata["omuseContent"] = serde_json::json!({
                "caption": "A retained native template caption",
                "altText": "An editable native customer voice template",
            });
            document.metadata["motion"] = serde_json::json!({
                "preset": "fade",
                "durationMs": 2000,
            });
        }
        expected.add_resource(
            "Protected native AI provenance",
            "application/json",
            br#"{"schema":"omuse.test.provenance.v1"}"#.to_vec(),
        )?;

        let package = directory.path().join("native-template.omuse");
        expected.save(&package)?;
        let mut reopened = Project::open(&package)?;
        assert!(disk_semantic_project_matches(&mut expected, &mut reopened)?);

        let mut content_loss = reopened.clone();
        content_loss.active_document_mut()?.metadata["omuseContent"]["caption"] =
            serde_json::json!("Caption lost after reopen");
        assert!(!disk_semantic_project_matches(
            &mut expected,
            &mut content_loss
        )?);

        let mut geometry_loss = reopened.clone();
        geometry_loss.active_document_mut()?.layers[0].offset_x += 1.0;
        assert!(!disk_semantic_project_matches(
            &mut expected,
            &mut geometry_loss
        )?);

        let mut scene_loss = reopened.clone();
        let scene = scene_loss
            .active_document_mut()?
            .layers
            .iter_mut()
            .find(|layer| layer.name == "Scene comparator")
            .and_then(|layer| layer.vector_scene.as_mut())
            .context("Missing vector scene after reopen")?;
        std::sync::Arc::make_mut(scene).objects[0].name = "Lost object identity".into();
        assert!(!disk_semantic_project_matches(
            &mut expected,
            &mut scene_loss
        )?);
        Ok(())
    }

    #[test]
    fn disk_semantic_float_exception_is_limited_to_one_parser_ulp() {
        let source = f64::from(235_f32 / 255_f32);
        let parsed_one_ulp_lower = f64::from_bits(source.to_bits() - 1);
        let custom_change_in_same_f32_bucket = source + 1.0e-8;
        assert_eq!(source as f32, parsed_one_ulp_lower as f32);
        assert_eq!(source as f32, custom_change_in_same_f32_bucket as f32);
        assert!(disk_semantic_json_matches(
            &serde_json::json!(source),
            &serde_json::json!(parsed_one_ulp_lower),
        ));
        assert!(!disk_semantic_json_matches(
            &serde_json::json!(source),
            &serde_json::json!(custom_change_in_same_f32_bucket),
        ));
        assert!(!disk_semantic_json_matches(
            &serde_json::json!(1_u64),
            &serde_json::json!(2_u64),
        ));
    }

    fn retained_generate(id: &str) -> StoredProposal {
        StoredProposal {
            id: id.into(),
            group_id: "native-generate".into(),
            variation_index: 1,
            variation_total: 1,
            source: omuse::ai_history::SourceIdentity {
                document_id: "native-source".into(),
                project_id: Some("native-project".into()),
                page_id: Some("native-page".into()),
                session_id: "prior-native-window".into(),
                epoch: 2,
                revision: 4,
                selection_revision: 0,
                project_generation: Some(3),
            },
            operation: Operation::GenerateImage,
            provider: ProviderId::CodexSubscription,
            prompt: "private prompt stays in the local store".into(),
            summary: "Generated image".into(),
            plan_json: None,
            assets: vec![omuse::ai_history::StoredAsset {
                file: "assets/result.asset".into(),
                media_type: "image/png".into(),
                width: 64,
                height: 80,
                byte_len: 12,
                provider_item_id: None,
            }],
            context_assets: vec![],
            image_edit: false,
            provenance: serde_json::json!({"intent": "generate"}),
            completed_unix_ms: 1,
        }
    }

    #[test]
    fn native_resume_requires_one_retained_codex_generate_and_never_falls_back() {
        let entry = retained_generate("retained-generate");
        assert_eq!(
            native_resumable_generate_id(&[entry.clone()]).unwrap(),
            "retained-generate"
        );

        let mut non_generate = entry.clone();
        non_generate.operation = Operation::EditImage;
        assert!(native_resumable_generate_id(&[non_generate]).is_err());
        assert!(native_resumable_generate_id(&[entry.clone(), entry]).is_err());
    }
}
