//! Bounded, local replay of an already completed optional AI workflow.
//!
//! This module does not submit provider work or read provider output. It
//! validates the retained steps again, prepares every edit on cloned state,
//! and returns one candidate project for the UI to review and apply once.

use crate::{
    ai::{ProviderId, TaskKind},
    ai_edits::{self, ImageIntent, ProductPresentation},
    create_project::Project,
    creative_commands::{CreativeOperation, CreativePlan},
    editor::Selection,
};
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeSet,
    io::{self, Write},
};

pub const WORKFLOW_PLAN_VERSION: u8 = 1;
pub const MAX_WORKFLOW_STEPS: usize = 3;
pub const MAX_WORKFLOW_BYTES: usize = 256 * 1024;
pub const MAX_WORKFLOW_LAYER_IDS: usize = 4;
const MAX_CREATIVE_PLAN_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPlan {
    pub version: u8,
    /// The temporary page ID assigned when a standalone document was wrapped
    /// as a one-page project. A later wrapper gets a fresh ID, so replay maps
    /// only this recorded ID onto the new wrapper's sole page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_page_id: Option<String>,
    pub steps: Vec<WorkflowStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowStep {
    Assistant {
        task: TaskKind,
        provider: ProviderId,
        plan: CreativePlan,
        active_layer: String,
    },
    Image {
        provider: ProviderId,
        intent: ImageIntent,
        product_presentation: ProductPresentation,
        layer_ids: Vec<String>,
    },
}

impl WorkflowPlan {
    pub fn parse(text: &str) -> Result<Self> {
        ensure!(
            text.len() <= MAX_WORKFLOW_BYTES,
            "Workflow plan exceeds the {} byte limit",
            MAX_WORKFLOW_BYTES
        );
        let workflow: Self =
            serde_json::from_str(text).context("Workflow plan is not valid JSON")?;
        workflow.validate()?;
        Ok(workflow)
    }

    pub fn serialize(&self) -> Result<String> {
        self.validate()?;
        bounded_json(self, MAX_WORKFLOW_BYTES, "Workflow plan")
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == WORKFLOW_PLAN_VERSION,
            "Unsupported workflow plan version {}",
            self.version
        );
        ensure!(
            !self.steps.is_empty() && self.steps.len() <= MAX_WORKFLOW_STEPS,
            "A workflow needs 1-{MAX_WORKFLOW_STEPS} steps"
        );
        if let Some(source_page_id) = &self.source_page_id {
            validate_canonical_uuid(source_page_id, "Workflow source page ID")?;
        }

        let mut tasks = BTreeSet::new();
        let mut follow_ups = Vec::new();
        for (index, step) in self.steps.iter().enumerate() {
            match step {
                WorkflowStep::Assistant {
                    task,
                    plan,
                    active_layer,
                    ..
                } => {
                    ensure!(
                        matches!(task, TaskKind::Design | TaskKind::Photo | TaskKind::Caption),
                        "Assistant workflow steps support Design, Photo or Caption tasks"
                    );
                    ensure!(tasks.insert(*task), "Workflow tasks cannot repeat");
                    validate_assistant_plan(*task, plan, active_layer)?;
                    if index > 0 {
                        ensure!(
                            matches!(task, TaskKind::Design | TaskKind::Caption),
                            "Follow-up workflow steps support Design or Caption only"
                        );
                        if *task == TaskKind::Design {
                            validate_follow_up_design_plan(plan)?;
                        }
                        follow_ups.push(*task);
                    }
                }
                WorkflowStep::Image {
                    intent,
                    product_presentation,
                    layer_ids,
                    ..
                } => {
                    ensure!(index == 0, "An image workflow step must be first");
                    validate_recorded_layer_ids(intent, product_presentation, layer_ids)?;
                    tasks.insert(image_task(intent));
                }
            }
        }

        ensure!(
            follow_ups.as_slice() != [TaskKind::Caption, TaskKind::Design],
            "Two follow-up steps must run Design before Caption"
        );
        Ok(())
    }

    pub fn replay(
        &self,
        source: &Project,
        selection: Option<&Selection>,
        generated: Option<&RgbaImage>,
    ) -> Result<Project> {
        self.validate()?;
        let replay_source_page_id = if self.source_page_id.is_some() {
            ensure!(
                source.page_summaries().len() == 1,
                "A standalone-document workflow can replay only on a one-page project"
            );
            Some(source.active_page_id().to_owned())
        } else {
            None
        };
        let mut candidate = source.clone();
        for (index, step) in self.steps.iter().enumerate() {
            match step {
                WorkflowStep::Image {
                    provider,
                    intent,
                    product_presentation,
                    layer_ids,
                } => {
                    ensure!(index == 0, "An image workflow step must be first");
                    let generated = generated.context(
                        "The completed provider image is required to replay this workflow",
                    )?;
                    let source_document = candidate.active_document()?.clone();
                    let provenance = json!({
                        "workflow": true,
                        "provider": provider,
                        "workflowVersion": self.version,
                    });
                    let mut document = if *intent == ImageIntent::Background {
                        ai_edits::prepare_product_background_result(
                            &source_document,
                            selection.context(
                                "Select the protected subject to replay this background workflow",
                            )?,
                            generated,
                            provenance,
                            product_presentation,
                        )?
                    } else {
                        ai_edits::prepare_result(
                            &source_document,
                            selection,
                            intent,
                            generated,
                            provenance,
                        )?
                    };
                    restore_added_layer_ids(&source_document, &mut document, layer_ids)?;
                    candidate.replace_active_document(document)?;
                }
                WorkflowStep::Assistant { plan, .. } => {
                    let plan = remap_source_page(
                        plan,
                        self.source_page_id.as_deref(),
                        replay_source_page_id.as_deref(),
                    );
                    if plan.requires_project() {
                        candidate = plan.prepare_project(&candidate)?;
                    } else {
                        let document = plan.prepare(candidate.active_document()?)?;
                        candidate.replace_active_document(document)?;
                    }
                }
            }
        }
        candidate.validate()?;
        Ok(candidate)
    }
}

fn remap_source_page(
    plan: &CreativePlan,
    recorded_source_page_id: Option<&str>,
    replay_source_page_id: Option<&str>,
) -> CreativePlan {
    let mut plan = plan.clone();
    let Some((recorded, replay)) = recorded_source_page_id.zip(replay_source_page_id) else {
        return plan;
    };
    for operation in &mut plan.operations {
        if let CreativeOperation::SelectPage { page_id } = operation
            && page_id == recorded
        {
            *page_id = replay.to_owned();
        }
    }
    plan
}

fn validate_follow_up_design_plan(plan: &CreativePlan) -> Result<()> {
    ensure!(
        plan.operations.iter().all(|operation| !matches!(
            operation,
            CreativeOperation::SelectPage { .. }
                | CreativeOperation::PlaceResource { .. }
                | CreativeOperation::InsertComponent { .. }
                | CreativeOperation::AddTemplatePage { .. }
        )),
        "Follow-up Design steps may edit only the current page"
    );
    Ok(())
}

fn validate_assistant_plan(task: TaskKind, plan: &CreativePlan, active_layer: &str) -> Result<()> {
    let encoded = bounded_json(plan, MAX_CREATIVE_PLAN_BYTES, "Assistant plan")?;
    CreativePlan::parse(&encoded)?;
    match task {
        TaskKind::Photo => {
            ensure!(!active_layer.trim().is_empty(), "Photo target is missing");
            ensure!(
                !plan.operations.is_empty()
                    && plan.operations.iter().all(|operation| {
                        matches!(operation, CreativeOperation::AdjustPhoto { layer_id, .. } if layer_id == active_layer)
                    }),
                "Photo workflow plans may adjust only their recorded active layer"
            );
        }
        TaskKind::Caption => ensure!(
            plan.operations.len() == 1
                && matches!(&plan.operations[0], CreativeOperation::SetContent { caption, alt_text }
                    if !caption.trim().is_empty() && !alt_text.trim().is_empty()),
            "Caption workflow plans require one non-empty caption and alt text operation"
        ),
        TaskKind::Design => {}
        _ => anyhow::bail!("Unsupported assistant workflow task"),
    }
    Ok(())
}

fn bounded_json<T: Serialize>(value: &T, max_bytes: usize, label: &str) -> Result<String> {
    struct BoundedWriter {
        bytes: Vec<u8>,
        max_bytes: usize,
    }
    impl Write for BoundedWriter {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            if self.bytes.len().saturating_add(input.len()) > self.max_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "serialized value exceeds its byte limit",
                ));
            }
            self.bytes.extend_from_slice(input);
            Ok(input.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        max_bytes,
    };
    serde_json::to_writer(&mut writer, value)
        .with_context(|| format!("{label} exceeds its bounded JSON format"))?;
    String::from_utf8(writer.bytes).context("Serialized JSON was not UTF-8")
}

fn image_task(intent: &ImageIntent) -> TaskKind {
    match intent {
        ImageIntent::Generate => TaskKind::Generate,
        ImageIntent::Replace => TaskKind::Replace,
        ImageIntent::Background => TaskKind::Background,
        ImageIntent::Expand { .. } => TaskKind::Expand,
    }
}

fn validate_recorded_layer_ids(
    intent: &ImageIntent,
    presentation: &ProductPresentation,
    layer_ids: &[String],
) -> Result<()> {
    ensure!(
        *intent == ImageIntent::Background || presentation.is_empty(),
        "Product presentation is available only for background replacement"
    );
    let expected = 1
        + (*intent == ImageIntent::Background && presentation.shadow.is_some()) as usize
        + (*intent == ImageIntent::Background && presentation.reflection.is_some()) as usize;
    ensure!(
        layer_ids.len() == expected && layer_ids.len() <= MAX_WORKFLOW_LAYER_IDS,
        "Workflow image step recorded an unexpected number of new layers"
    );
    let mut unique = BTreeSet::new();
    for id in layer_ids {
        validate_canonical_uuid(id, "Workflow image layer ID")?;
        ensure!(unique.insert(id), "Workflow image layer IDs must be unique");
    }
    Ok(())
}

fn validate_canonical_uuid(value: &str, label: &str) -> Result<()> {
    let canonical = uuid::Uuid::parse_str(value)
        .with_context(|| format!("{label} is not a UUID"))?
        .to_string()
        .to_uppercase();
    ensure!(canonical == value, "{label} is not canonical");
    Ok(())
}

fn restore_added_layer_ids(
    source: &crate::model::Document,
    result: &mut crate::model::Document,
    recorded: &[String],
) -> Result<()> {
    fn restore<'a>(
        layers: &mut [crate::model::Layer],
        source_ids: &BTreeSet<String>,
        recorded: &mut impl Iterator<Item = &'a String>,
    ) {
        for layer in layers {
            if !source_ids.contains(&layer.id) {
                layer.id = recorded
                    .next()
                    .expect("new-layer count was checked before restoration")
                    .clone();
            }
            restore(&mut layer.children, source_ids, recorded);
        }
    }

    let source_ids = layer_id_set(source);
    ensure!(
        recorded.iter().all(|id| !source_ids.contains(id)),
        "Workflow image layer ID collides with source artwork"
    );

    let added_count = added_layer_ids(source, result).len();
    ensure!(
        added_count == recorded.len(),
        "Workflow image layer count changed during replay"
    );

    // Provider image, shadow and reflection layers contain baked pixels and no
    // layer-ID references. Existing source nodes and their reference metadata
    // are never renamed.
    let mut recorded = recorded.iter();
    restore(&mut result.layers, &source_ids, &mut recorded);
    ensure!(recorded.next().is_none(), "Unused workflow image layer ID");
    Ok(())
}

/// Returns IDs for every result layer absent from the source, in stable
/// depth-first preorder. Callers can retain these IDs with a completed image
/// step so later native plans target the same editable nodes during replay.
pub fn added_layer_ids(
    source: &crate::model::Document,
    result: &crate::model::Document,
) -> Vec<String> {
    fn collect_added(
        layers: &[crate::model::Layer],
        source_ids: &BTreeSet<String>,
        added: &mut Vec<String>,
    ) {
        for layer in layers {
            if !source_ids.contains(&layer.id) {
                added.push(layer.id.clone());
            }
            collect_added(&layer.children, source_ids, added);
        }
    }

    let source_ids = layer_id_set(source);
    let mut added = Vec::new();
    collect_added(&result.layers, &source_ids, &mut added);
    added
}

fn layer_id_set(document: &crate::model::Document) -> BTreeSet<String> {
    fn collect(layers: &[crate::model::Layer], ids: &mut BTreeSet<String>) {
        for layer in layers {
            ids.insert(layer.id.clone());
            collect(&layer.children, ids);
        }
    }

    let mut ids = BTreeSet::new();
    collect(&document.layers, &mut ids);
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        creative_commands::{AnimationPreset, LayoutFit},
        model::Document,
        objects::{LiveShapeKind, LiveShapeStyle},
    };
    use image::Rgba;

    const GENERATED_LAYER_ID: &str = "11111111-1111-4111-8111-111111111111";

    fn design_plan() -> CreativePlan {
        CreativePlan {
            summary: "Place the generated image and add an editable badge".into(),
            operations: vec![
                CreativeOperation::PlaceLayer {
                    layer_id: GENERATED_LAYER_ID.into(),
                    x: 1.0,
                    y: 1.0,
                    width: 6.0,
                    height: 6.0,
                    rotation: 0.0,
                },
                CreativeOperation::AddShape {
                    name: "Workflow badge".into(),
                    x: 1.0,
                    y: 1.0,
                    width: 3,
                    height: 3,
                    style: LiveShapeStyle {
                        kind: LiveShapeKind::Ellipse,
                        red: 0.1,
                        green: 0.6,
                        blue: 0.7,
                        corner_radius: 0.0,
                        line_width: None,
                        start: None,
                        end: None,
                    },
                },
            ],
        }
    }

    fn caption_plan() -> CreativePlan {
        CreativePlan {
            summary: "Add content".into(),
            operations: vec![CreativeOperation::SetContent {
                caption: "A finished workflow".into(),
                alt_text: "A teal badge over a generated image".into(),
            }],
        }
    }

    #[test]
    fn replays_image_design_and_caption_on_one_cloned_candidate() {
        let mut source = Project::new("Source", Document::new(8, 8));
        source.metadata.local_only = true;
        let workflow = WorkflowPlan {
            version: WORKFLOW_PLAN_VERSION,
            source_page_id: None,
            steps: vec![
                WorkflowStep::Image {
                    provider: ProviderId::CodexSubscription,
                    intent: ImageIntent::Generate,
                    product_presentation: ProductPresentation::default(),
                    layer_ids: vec![GENERATED_LAYER_ID.into()],
                },
                WorkflowStep::Assistant {
                    task: TaskKind::Design,
                    provider: ProviderId::ClaudeCode,
                    plan: design_plan(),
                    active_layer: String::new(),
                },
                WorkflowStep::Assistant {
                    task: TaskKind::Caption,
                    provider: ProviderId::ClaudeCode,
                    plan: caption_plan(),
                    active_layer: String::new(),
                },
            ],
        };
        let generated = RgbaImage::from_pixel(8, 8, Rgba([20, 40, 60, 255]));
        let mut candidate = workflow.replay(&source, None, Some(&generated)).unwrap();
        let candidate_document = candidate.active_document().unwrap();

        assert!(candidate_document.layers.iter().any(|layer| {
            layer.id == GENERATED_LAYER_ID
                && layer.name == "Generated image"
                && layer.metadata["omuseGenerated"]["workflow"] == true
        }));
        let badge = candidate_document
            .layers
            .iter()
            .find(|layer| layer.name == "Workflow badge")
            .unwrap();
        assert!(crate::objects::live_shape(badge).unwrap().is_some());
        assert_eq!(
            candidate_document.metadata["omuseContent"]["caption"],
            "A finished workflow"
        );
        let mut original = source.clone();
        assert!(
            original
                .active_document()
                .unwrap()
                .find_layer(GENERATED_LAYER_ID)
                .is_none()
        );
        assert!(candidate.metadata.local_only);
    }

    #[test]
    fn serialize_and_parse_preserve_the_bounded_shape() {
        let workflow = WorkflowPlan {
            version: WORKFLOW_PLAN_VERSION,
            source_page_id: None,
            steps: vec![WorkflowStep::Assistant {
                task: TaskKind::Caption,
                provider: ProviderId::ClaudeCode,
                plan: caption_plan(),
                active_layer: String::new(),
            }],
        };
        let parsed = WorkflowPlan::parse(&workflow.serialize().unwrap()).unwrap();
        assert_eq!(parsed.version, WORKFLOW_PLAN_VERSION);
        assert_eq!(parsed.steps.len(), 1);
        assert!(parsed.source_page_id.is_none());
        assert!(!workflow.serialize().unwrap().contains("source_page_id"));
    }

    #[test]
    fn replay_remaps_a_recorded_wrapper_page_to_a_fresh_wrapper() {
        let recorded = Project::new("Recorded", Document::new(8, 8));
        let recorded_page_id = recorded.active_page_id().to_owned();
        let workflow = WorkflowPlan {
            version: WORKFLOW_PLAN_VERSION,
            source_page_id: Some(recorded_page_id.clone()),
            steps: vec![WorkflowStep::Assistant {
                task: TaskKind::Design,
                provider: ProviderId::ClaudeCode,
                plan: CreativePlan {
                    summary: "Select and recolour the wrapped page".into(),
                    operations: vec![
                        CreativeOperation::SelectPage {
                            page_id: recorded_page_id,
                        },
                        CreativeOperation::SetBackground {
                            color: [12, 34, 56, 255],
                        },
                    ],
                },
                active_layer: String::new(),
            }],
        };
        let mut fresh = Project::new("Fresh", Document::new(8, 8));
        let fresh_page_id = fresh.active_page_id().to_owned();
        assert_ne!(
            workflow.source_page_id.as_deref(),
            Some(fresh_page_id.as_str())
        );

        let mut candidate = workflow.replay(&fresh, None, None).unwrap();
        assert_eq!(candidate.active_page_id(), fresh_page_id);
        let rendered = crate::raster::composite(candidate.active_document().unwrap());
        assert!(rendered.pixels().all(|pixel| pixel.0 == [12, 34, 56, 255]));
        assert_eq!(fresh.active_document().unwrap().layers.len(), 1);

        fresh.add_page("Second", Document::new(8, 8)).unwrap();
        assert!(workflow.replay(&fresh, None, None).is_err());
    }

    #[test]
    fn follow_up_design_is_bounded_to_current_page_operations() {
        let forbidden = vec![
            CreativeOperation::SelectPage {
                page_id: "22222222-2222-4222-8222-222222222222".into(),
            },
            CreativeOperation::PlaceResource {
                resource_id: "33333333-3333-4333-8333-333333333333".into(),
                name: "Saved image".into(),
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
                alt_text: String::new(),
            },
            CreativeOperation::InsertComponent {
                component_id: "44444444-4444-4444-8444-444444444444".into(),
                overrides: Default::default(),
            },
            CreativeOperation::AddTemplatePage {
                template_id: "campaign-square".into(),
                name: "Another page".into(),
                fields: Default::default(),
                caption: String::new(),
                alt_text: String::new(),
            },
        ];
        for operation in forbidden {
            let workflow = WorkflowPlan {
                version: WORKFLOW_PLAN_VERSION,
                source_page_id: None,
                steps: vec![
                    WorkflowStep::Image {
                        provider: ProviderId::CodexSubscription,
                        intent: ImageIntent::Generate,
                        product_presentation: ProductPresentation::default(),
                        layer_ids: vec![GENERATED_LAYER_ID.into()],
                    },
                    WorkflowStep::Assistant {
                        task: TaskKind::Design,
                        provider: ProviderId::ClaudeCode,
                        plan: CreativePlan {
                            summary: "Escape the current page".into(),
                            operations: vec![operation],
                        },
                        active_layer: String::new(),
                    },
                ],
            };
            assert!(
                workflow
                    .validate()
                    .unwrap_err()
                    .to_string()
                    .contains("current page")
            );
        }

        let allowed = WorkflowPlan {
            version: WORKFLOW_PLAN_VERSION,
            source_page_id: None,
            steps: vec![
                WorkflowStep::Image {
                    provider: ProviderId::CodexSubscription,
                    intent: ImageIntent::Generate,
                    product_presentation: ProductPresentation::default(),
                    layer_ids: vec![GENERATED_LAYER_ID.into()],
                },
                WorkflowStep::Assistant {
                    task: TaskKind::Design,
                    provider: ProviderId::ClaudeCode,
                    plan: CreativePlan {
                        summary: "Finish the current page".into(),
                        operations: vec![
                            CreativeOperation::ResizePage {
                                width: 12,
                                height: 12,
                                strategy: LayoutFit::Adapt,
                            },
                            CreativeOperation::AnimatePage {
                                preset: AnimationPreset::Fade,
                                duration_ms: 1_000,
                            },
                            CreativeOperation::SetContent {
                                caption: "Finished".into(),
                                alt_text: "A finished design".into(),
                            },
                        ],
                    },
                    active_layer: String::new(),
                },
            ],
        };
        allowed.validate().unwrap();
    }

    #[test]
    fn rejects_image_after_first_and_reversed_follow_ups() {
        let image = WorkflowStep::Image {
            provider: ProviderId::CodexSubscription,
            intent: ImageIntent::Generate,
            product_presentation: ProductPresentation::default(),
            layer_ids: vec![GENERATED_LAYER_ID.into()],
        };
        let caption = WorkflowStep::Assistant {
            task: TaskKind::Caption,
            provider: ProviderId::ClaudeCode,
            plan: caption_plan(),
            active_layer: String::new(),
        };
        let design = WorkflowStep::Assistant {
            task: TaskKind::Design,
            provider: ProviderId::ClaudeCode,
            plan: design_plan(),
            active_layer: String::new(),
        };

        assert!(
            WorkflowPlan {
                version: WORKFLOW_PLAN_VERSION,
                source_page_id: None,
                steps: vec![caption.clone(), image],
            }
            .validate()
            .is_err()
        );
        assert!(
            WorkflowPlan {
                version: WORKFLOW_PLAN_VERSION,
                source_page_id: None,
                steps: vec![
                    WorkflowStep::Assistant {
                        task: TaskKind::Photo,
                        provider: ProviderId::CodexSubscription,
                        plan: CreativePlan {
                            summary: "Photo".into(),
                            operations: vec![CreativeOperation::AdjustPhoto {
                                layer_id: "photo".into(),
                                exposure_stops: Some(0.5),
                                brightness_percent: None,
                                contrast_percent: None,
                                saturation_percent: None,
                            }],
                        },
                        active_layer: "photo".into(),
                    },
                    caption,
                    design
                ],
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn added_ids_are_preorder_and_source_collisions_fail_replay() {
        let mut source = Project::new("Source", Document::new(8, 8));
        let source_document = source.active_document().unwrap().clone();
        let source_id = source_document.layers[0].id.clone();
        let generated = RgbaImage::from_pixel(8, 8, Rgba([20, 40, 60, 255]));
        let result = ai_edits::prepare_result(
            &source_document,
            None,
            &ImageIntent::Generate,
            &generated,
            json!({"workflow":true}),
        )
        .unwrap();
        let added = added_layer_ids(&source_document, &result);
        assert_eq!(added.len(), 1);
        assert_eq!(result.layers.last().unwrap().id, added[0]);

        let colliding = WorkflowPlan {
            version: WORKFLOW_PLAN_VERSION,
            source_page_id: None,
            steps: vec![WorkflowStep::Image {
                provider: ProviderId::CodexSubscription,
                intent: ImageIntent::Generate,
                product_presentation: ProductPresentation::default(),
                layer_ids: vec![source_id],
            }],
        };
        assert!(
            colliding
                .replay(&source, None, Some(&generated))
                .unwrap_err()
                .to_string()
                .contains("collides")
        );
    }

    #[test]
    fn rejects_oversized_plan_and_photo_target_escape() {
        let oversized = WorkflowPlan {
            version: WORKFLOW_PLAN_VERSION,
            source_page_id: None,
            steps: vec![WorkflowStep::Assistant {
                task: TaskKind::Design,
                provider: ProviderId::ClaudeCode,
                plan: CreativePlan {
                    summary: "Too many".into(),
                    operations: (0..65)
                        .map(|_| CreativeOperation::SetBackground {
                            color: [0, 0, 0, 255],
                        })
                        .collect(),
                },
                active_layer: String::new(),
            }],
        };
        assert!(oversized.validate().is_err());

        let escaped = WorkflowPlan {
            version: WORKFLOW_PLAN_VERSION,
            source_page_id: None,
            steps: vec![WorkflowStep::Assistant {
                task: TaskKind::Photo,
                provider: ProviderId::CodexSubscription,
                plan: CreativePlan {
                    summary: "Wrong target".into(),
                    operations: vec![CreativeOperation::AdjustPhoto {
                        layer_id: "other".into(),
                        exposure_stops: Some(0.5),
                        brightness_percent: None,
                        contrast_percent: None,
                        saturation_percent: None,
                    }],
                },
                active_layer: "active".into(),
            }],
        };
        assert!(escaped.validate().is_err());
    }
}
