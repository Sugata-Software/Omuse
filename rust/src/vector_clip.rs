//! Retained, editable filled clipping silhouettes. Masks are ordinary scene
//! objects; clipping changes neither their geometry nor their contents.
use crate::vector_scene::{VECTOR_SCENE_CLIP_VERSION, VectorObject, VectorScene};
use anyhow::{Result, ensure};
use std::collections::{HashMap, HashSet};

pub(crate) fn validate(scene: &VectorScene) -> Result<()> {
    let mut used = HashSet::new();
    for object in &scene.objects {
        ensure!(
            scene.version >= VECTOR_SCENE_CLIP_VERSION
                || (!object.is_clip && object.clip_to.is_none()),
            "Editable vector clipping requires scene version 5"
        );
        if object.is_clip {
            ensure!(
                object.clip_to.is_none(),
                "Nested vector clipping is unsupported; release the existing clip first"
            );
            ensure!(
                object.text_path.is_none(),
                "Outline live text before using it as a clipping mask"
            );
            ensure!(
                !object.path.subpaths.is_empty()
                    && object
                        .path
                        .subpaths
                        .iter()
                        .all(|s| s.closed && s.anchors.len() >= 3),
                "A clipping mask must contain closed paths with at least three points"
            );
        }
        if let Some(id) = &object.clip_to {
            ensure!(id != &object.id, "An object cannot clip itself");
            let source = scene
                .objects
                .iter()
                .find(|source| &source.id == id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Clipping mask is missing; release the clip before removing its mask"
                    )
                })?;
            ensure!(
                source.is_clip && source.clip_to.is_none(),
                "Clipping reference must identify an independent mask"
            );
            used.insert(id.as_str());
        }
    }
    ensure!(
        scene
            .objects
            .iter()
            .all(|object| !object.is_clip || used.contains(object.id.as_str())),
        "Clipping mask has no contents; release the clip before removing its contents"
    );
    Ok(())
}

/// Use the topmost selected object as a retained mask for the other selected
/// objects. Geometry and styles are preserved for subsequent Release.
pub fn make(scene: &VectorScene, selected: &[usize]) -> Result<(VectorScene, Vec<usize>)> {
    scene.validate()?;
    let selected: std::collections::BTreeSet<_> = selected.iter().copied().collect();
    ensure!(
        selected.len() >= 2 && selected.iter().all(|i| *i < scene.objects.len()),
        "Select contents and a closed shape above them to make a clip"
    );
    let mask = *selected.last().unwrap();
    ensure!(
        selected
            .iter()
            .all(|i| !scene.objects[*i].is_clip && scene.objects[*i].clip_to.is_none()),
        "Release existing clipping before making another clip"
    );
    let id = scene.objects[mask].id.clone();
    let mut result = scene.clone();
    result.version = result.version.max(VECTOR_SCENE_CLIP_VERSION);
    result.objects[mask].is_clip = true;
    for index in &selected {
        if *index != mask {
            result.objects[*index].clip_to = Some(id.clone());
        }
    }
    result.validate()?;
    Ok((result, selected.into_iter().collect()))
}

/// Release each selected clipping relationship, restoring the original mask
/// as a painted editable shape once no remaining content uses it.
pub fn release(scene: &VectorScene, selected: &[usize]) -> Result<(VectorScene, Vec<usize>)> {
    scene.validate()?;
    ensure!(
        selected.iter().all(|i| *i < scene.objects.len()),
        "Clipping selection is outside the scene"
    );
    let mut result = scene.clone();
    let masks: HashSet<String> = selected
        .iter()
        .filter_map(|i| {
            let object = &scene.objects[*i];
            if object.is_clip {
                Some(object.id.clone())
            } else {
                object.clip_to.clone()
            }
        })
        .collect();
    ensure!(
        !masks.is_empty(),
        "Select clipped artwork or its mask to release clipping"
    );
    let mut selection = std::collections::BTreeSet::new();
    for (index, object) in result.objects.iter_mut().enumerate() {
        if object.clip_to.as_ref().is_some_and(|id| masks.contains(id)) {
            object.clip_to = None;
            selection.insert(index);
        }
        if masks.contains(&object.id) {
            object.is_clip = false;
            selection.insert(index);
        }
    }
    result.validate()?;
    Ok((result, selection.into_iter().collect()))
}

/// Repair references after a bounded clone operation has assigned new IDs.
/// A copied mask with no copied content becomes an ordinary visible shape.
pub fn repair_clones(
    scene: &VectorScene,
    originals: &[usize],
    copies: &mut [VectorObject],
) -> Result<()> {
    ensure!(
        originals.len() == copies.len() && originals.iter().all(|i| *i < scene.objects.len()),
        "Invalid vector clipping copy mapping"
    );
    let ids: HashMap<_, _> = originals
        .iter()
        .zip(copies.iter())
        .map(|(i, object)| (scene.objects[*i].id.as_str(), object.id.clone()))
        .collect();
    for copy in copies.iter_mut() {
        if let Some(id) = &mut copy.clip_to {
            if let Some(new) = ids.get(id.as_str()) {
                *id = new.clone();
            }
        }
    }
    let used: HashSet<_> = copies.iter().filter_map(|o| o.clip_to.clone()).collect();
    for copy in copies {
        if copy.is_clip && !used.contains(&copy.id) {
            copy.is_clip = false;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector_path::Point;
    use std::sync::atomic::AtomicBool;
    fn scene() -> VectorScene {
        VectorScene {
            version: 1,
            width: 32,
            height: 32,
            objects: vec![
                VectorObject::rectangle("Art", 0., 0., 32., 32., Some([255, 0, 0, 255]), None)
                    .unwrap(),
                VectorObject::ellipse("Mask", 8., 8., 16., 16., Some([0, 0, 255, 255]), None)
                    .unwrap(),
            ],
        }
    }
    #[test]
    fn clip_renders_hits_and_releases_without_changing_geometry() {
        let source = scene();
        let (clip, _) = make(&source, &[0, 1]).unwrap();
        let image = clip.render(&AtomicBool::new(false)).unwrap();
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert_eq!(image.get_pixel(16, 16).0, [255, 0, 0, 255]);
        assert_eq!(clip.hit_test(Point { x: 0., y: 0. }, 2.).unwrap(), None);
        assert_eq!(
            clip.hit_test(Point { x: 16., y: 16. }, 0.).unwrap(),
            Some(0)
        );
        let encoded = serde_json::to_vec(&clip).unwrap();
        let reopened: VectorScene = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(reopened.render(&AtomicBool::new(false)).unwrap(), image);
        let (mut released, _) = release(&reopened, &[0]).unwrap();
        released.version = source.version;
        assert_eq!(released, source);
    }
    #[test]
    fn edited_transformed_mask_is_live_and_region_render_matches() {
        let (mut clip, _) = make(&scene(), &[0, 1]).unwrap();
        clip.objects[1].transform[4] = 8.;
        let cancel = AtomicBool::new(false);
        let full = clip.render(&cancel).unwrap();
        assert_eq!(full.get_pixel(9, 16)[3], 0);
        assert_eq!(full.get_pixel(24, 16)[3], 255);
        let region = clip
            .render_region(
                crate::model::PixelRect {
                    x: 16,
                    y: 8,
                    width: 16,
                    height: 16,
                },
                1.,
                &cancel,
            )
            .unwrap();
        assert_eq!(
            region,
            image::imageops::crop_imm(&full, 16, 8, 16, 16).to_image()
        );
    }
    #[test]
    fn invalid_references_nesting_open_masks_and_old_versions_are_rejected() {
        let (clip, _) = make(&scene(), &[0, 1]).unwrap();
        for kind in 0..5 {
            let mut invalid = clip.clone();
            match kind {
                0 => invalid.objects[0].clip_to = Some(uuid::Uuid::new_v4().to_string()),
                1 => invalid.objects[1].clip_to = Some(invalid.objects[0].id.clone()),
                2 => invalid.objects[1].path.subpaths[0].closed = false,
                3 => invalid.version = 4,
                _ => invalid.objects[0].clip_to = None,
            }
            assert!(invalid.validate().is_err());
        }
    }
    #[test]
    fn cloned_masks_are_independent_and_single_content_copies_keep_mask() {
        let (clip, _) = make(&scene(), &[0, 1]).unwrap();
        let mut copies = clip.objects.clone();
        for copy in &mut copies {
            copy.id = uuid::Uuid::new_v4().to_string();
        }
        repair_clones(&clip, &[0, 1], &mut copies).unwrap();
        assert_eq!(copies[0].clip_to.as_deref(), Some(copies[1].id.as_str()));
        let mut one = vec![clip.objects[0].clone()];
        one[0].id = uuid::Uuid::new_v4().to_string();
        repair_clones(&clip, &[0], &mut one).unwrap();
        assert_eq!(one[0].clip_to, clip.objects[0].clip_to);
    }
}
