//! Typed, bounded vector commands shared by native controls and assistant plans.
//! Results are prepared without mutating the source scene.
use crate::{
    vector_boolean::{self, BooleanOperation},
    vector_geometry as geometry,
    vector_path::VectorPath,
    vector_scene::{MAX_SCENE_OBJECTS, VectorObject, VectorScene},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum VectorCommand {
    Simplify { tolerance: f32 },
    Offset { distance: f32 },
    OutlineStroke,
    Unite,
    Subtract,
    Intersect,
    Exclude,
    Divide,
}

impl<'de> Deserialize<'de> for VectorCommand {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Internally tagged unit variants ignore extra fields in Serde. Empty
        // struct variants enforce the same strict shape as parameterized ones.
        #[derive(Deserialize)]
        #[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Simplify { tolerance: f32 },
            Offset { distance: f32 },
            OutlineStroke {},
            Unite {},
            Subtract {},
            Intersect {},
            Exclude {},
            Divide {},
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Simplify { tolerance } => Self::Simplify { tolerance },
            Wire::Offset { distance } => Self::Offset { distance },
            Wire::OutlineStroke {} => Self::OutlineStroke,
            Wire::Unite {} => Self::Unite,
            Wire::Subtract {} => Self::Subtract,
            Wire::Intersect {} => Self::Intersect,
            Wire::Exclude {} => Self::Exclude,
            Wire::Divide {} => Self::Divide,
        })
    }
}

impl VectorCommand {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Simplify { tolerance } => ensure!(
                tolerance.is_finite() && (0.01..=100.).contains(tolerance),
                "Simplify tolerance must be 0.01–100 pixels"
            ),
            Self::Offset { distance } => ensure!(
                distance.is_finite() && distance.abs() <= 4096.,
                "Offset distance must be between -4096 and 4096 pixels"
            ),
            _ => {}
        }
        Ok(())
    }
}

pub struct VectorResult {
    pub scene: VectorScene,
    pub selected: BTreeSet<usize>,
    pub anchors_before: usize,
    pub anchors_after: usize,
}

pub fn apply(
    source: &VectorScene,
    ids: &[String],
    command: &VectorCommand,
    cancel: &AtomicBool,
) -> Result<VectorResult> {
    source.validate()?;
    command.validate()?;
    ensure!(!cancel.load(Ordering::Relaxed), "Vector command cancelled");
    ensure!(
        !ids.is_empty() && ids.len() <= 64,
        "Choose 1–64 vector objects"
    );
    let unique: BTreeSet<_> = ids.iter().collect();
    ensure!(unique.len() == ids.len(), "Duplicate vector object IDs");
    let selected: BTreeSet<_> = source
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| unique.contains(&o.id))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        selected.len() == ids.len(),
        "A selected vector object no longer exists"
    );
    let before = selected
        .iter()
        .flat_map(|i| &source.objects[*i].path.subpaths)
        .map(|s| s.anchors.len())
        .sum::<usize>();
    let cap = if matches!(command, VectorCommand::Simplify { .. }) {
        crate::vector_scene::MAX_SCENE_ANCHORS
    } else {
        geometry::MAX_GEOMETRY_SEGMENTS
    };
    ensure!(
        before <= cap,
        "Selection exceeds the path-operation work limit; choose fewer paths"
    );
    let boolean = match command {
        VectorCommand::Unite => Some(BooleanOperation::Union),
        VectorCommand::Subtract => Some(BooleanOperation::Subtract),
        VectorCommand::Intersect => Some(BooleanOperation::Intersect),
        VectorCommand::Exclude => Some(BooleanOperation::Exclude),
        VectorCommand::Divide => Some(BooleanOperation::Divide),
        _ => None,
    };
    let mut objects = Vec::new();
    let mut chosen = BTreeSet::new();
    if let Some(operation) = boolean {
        ensure!(selected.len() >= 2, "Select at least two filled objects");
        let input: Vec<_> = selected
            .iter()
            .map(|i| source.objects[*i].clone())
            .collect();
        let mut result = vector_boolean::combine(&input, operation, cancel)?;
        let mut groups = input[0].groups.clone();
        for object in &input[1..] {
            let common = groups
                .iter()
                .zip(&object.groups)
                .take_while(|(a, b)| a == b)
                .count();
            groups.truncate(common);
        }
        for object in &mut result {
            object.groups = groups.clone();
        }
        let insertion = selected.last().unwrap() + 1 - selected.len();
        objects = source
            .objects
            .iter()
            .enumerate()
            .filter(|(i, _)| !selected.contains(i))
            .map(|(_, o)| o.clone())
            .collect();
        chosen.extend(insertion..insertion + result.len());
        objects.splice(insertion..insertion, result);
    } else {
        for (index, object) in source.objects.iter().enumerate() {
            ensure!(!cancel.load(Ordering::Relaxed), "Vector command cancelled");
            if !selected.contains(&index) {
                objects.push(object.clone());
                continue;
            }
            let mut result = match command {
                VectorCommand::Simplify { tolerance } => {
                    vec![geometry::simplify(object, *tolerance, cancel)?]
                }
                VectorCommand::Offset { distance } => {
                    geometry::offset_path(object, *distance, cancel)?
                }
                VectorCommand::OutlineStroke => {
                    ensure!(
                        object.opacity == 1.
                            || (object.fill.is_none() && object.fill_gradient.is_none()),
                        "A filled stroke below 100% object opacity needs grouped transparency; set object opacity to 100% first"
                    );
                    let mut paths = geometry::outline_stroke(object, cancel)?;
                    if object.fill.is_some() || object.fill_gradient.is_some() {
                        let mut fill = object.clone();
                        fill.stroke = None;
                        fill.stroke_options = None;
                        paths.insert(0, fill);
                    }
                    paths
                }
                _ => unreachable!(),
            };
            for item in &mut result {
                item.groups = object.groups.clone();
            }
            chosen.extend(objects.len()..objects.len() + result.len());
            objects.extend(result);
            ensure!(
                objects.len() <= MAX_SCENE_OBJECTS,
                "Path operation exceeds object limit"
            );
        }
    }
    let after = chosen
        .iter()
        .flat_map(|i| &objects[*i].path.subpaths)
        .map(|s| s.anchors.len())
        .sum();
    if objects.is_empty() {
        objects.push(VectorObject::new(
            "Path 1",
            VectorPath::default(),
            Some([0; 4]),
            None,
        ));
    }
    let scene = VectorScene {
        objects,
        ..source.clone()
    };
    scene.validate()?;
    ensure!(!cancel.load(Ordering::Relaxed), "Vector command cancelled");
    Ok(VectorResult {
        scene,
        selected: chosen,
        anchors_before: before,
        anchors_after: after,
    })
}
