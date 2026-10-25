//! Read-only admission estimates for already resident Create state. No disk
//! access, lazy loads, cache mutation, pixel conversion or eager validation.
//! Aliases are deliberately counted for every owner. These are conservative
//! owned-data estimates, not allocator/process/GPU RSS measurements or caps.
use super::*;
use std::{mem::size_of, sync::TryLockError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResidentMemoryEstimate {
    pub bytes: u64,
    pub loaded_documents: usize,
    pub cached_lazy_documents: usize,
    pub checked_out_pages: usize,
    pub loaded_resources: usize,
    pub cached_lazy_resources: usize,
}
fn cap<T>(values: &Vec<T>) -> u64 {
    (values.capacity() as u64).saturating_mul(size_of::<T>() as u64)
}
fn path_bytes(path: &PathBuf) -> u64 {
    (path.capacity() as u64)
        .saturating_mul(4)
        .saturating_add(64)
}
fn json_bytes(value: &serde_json::Value, depth: usize) -> Result<u64> {
    ensure!(
        depth <= 128,
        "Create metadata is too deeply nested to estimate safely"
    );
    let own = size_of::<serde_json::Value>() as u64;
    Ok(match value {
        serde_json::Value::String(s) => own.saturating_add(s.capacity() as u64),
        serde_json::Value::Array(a) => {
            a.iter().try_fold(own.saturating_add(cap(a)), |bytes, v| {
                Ok::<_, anyhow::Error>(bytes.saturating_add(json_bytes(v, depth + 1)?))
            })?
        }
        serde_json::Value::Object(map) => map.iter().try_fold(own, |bytes, (k, v)| {
            Ok::<_, anyhow::Error>(
                bytes
                    .saturating_add(1024)
                    .saturating_add(k.capacity() as u64)
                    .saturating_add(json_bytes(v, depth + 1)?),
            )
        })?,
        _ => own,
    })
}
fn path_heap(path: &crate::vector_path::VectorPath) -> u64 {
    path.subpaths.iter().fold(cap(&path.subpaths), |n, p| {
        n.saturating_add(cap(&p.anchors))
    })
}
fn advanced_bytes(state: &crate::advanced::LayerState) -> u64 {
    let recipe = &state.recipe;
    let mut bytes = (size_of::<crate::advanced::LayerState>() as u64)
        .saturating_add(state.retained_bytes() as u64)
        .saturating_add(recipe.source_id.capacity() as u64)
        .saturating_add(recipe.source_name.capacity() as u64)
        .saturating_add(
            recipe
                .linked_path
                .as_ref()
                .map_or(0, |s| s.capacity() as u64),
        )
        .saturating_add(
            recipe
                .raw_extension
                .as_ref()
                .map_or(0, |s| s.capacity() as u64),
        )
        .saturating_add(cap(&recipe.nodes));
    if let Some(raw) = &state.raw_bytes {
        bytes = bytes.saturating_add(raw.capacity().saturating_sub(raw.len()) as u64);
    }
    // Tiled storage's public payload count covers channel capacities. Reserve
    // additional tile owners/vector/allocator bookkeeping without deduplicating
    // source/result aliases or invoking a conversion.
    for image in [&state.source, &state.result] {
        let (w, h) = image.dimensions();
        let tile = image.tile_size();
        bytes =
            bytes.saturating_add(u64::from(w.div_ceil(tile)) * u64::from(h.div_ceil(tile)) * 64);
    }
    if let Some(path) = &recipe.vector {
        bytes = bytes.saturating_add(path_heap(path));
    }
    for node in &recipe.nodes {
        bytes = bytes
            .saturating_add(node.id.capacity() as u64)
            .saturating_add(node.name.capacity() as u64);
        use crate::advanced_ops::AdvancedOperation as Op;
        bytes = bytes.saturating_add(match &node.operation {
            Op::CameraRaw(s) => [&s.curve.rgb, &s.curve.red, &s.curve.green, &s.curve.blue]
                .into_iter()
                .fold(0u64, |n, v| n.saturating_add(cap(v)))
                .saturating_add(cap(&s.mixer.hue))
                .saturating_add(cap(&s.mixer.saturation))
                .saturating_add(cap(&s.mixer.luminance))
                .saturating_add(cap(&s.mixer.points))
                .saturating_add(cap(&s.geometry.guides)),
            Op::Warp(w) => cap(&w.points).saturating_add(cap(&w.pins)),
            Op::Filter(crate::filters::Filter::Curves { points }) => cap(points),
            Op::Filter(crate::filters::Filter::Dither(s)) => s.characters.capacity() as u64,
            _ => 0,
        });
    }
    bytes
}
/// Estimate one layer tree, retaining each shared image/source owner separately.
pub fn layer_resident_bytes(layer: &Layer, depth: usize) -> Result<u64> {
    ensure!(
        depth <= 64,
        "Create layer nesting is too deep to estimate safely"
    );
    let mut bytes = (size_of::<Layer>() as u64)
        .saturating_add(layer.id.capacity() as u64)
        .saturating_add(layer.name.capacity() as u64)
        .saturating_add(layer.blend_mode.capacity() as u64)
        .saturating_add(cap(&layer.children))
        .saturating_add(json_bytes(&layer.metadata, 0)?);
    for image in [&layer.image, &layer.mask].into_iter().flatten() {
        bytes = bytes
            .saturating_add(image.as_raw().capacity() as u64)
            .saturating_add(64);
    }
    if let Some(state) = &layer.advanced {
        bytes = bytes.saturating_add(advanced_bytes(state));
    }
    if let Some(scene) = &layer.vector_scene {
        bytes = bytes
            .saturating_add(scene.retained_bytes() as u64)
            .saturating_add(64);
    }
    for child in &layer.children {
        bytes = bytes.saturating_add(layer_resident_bytes(child, depth + 1)?);
    }
    Ok(bytes)
}
/// No serialization buffer is built; JSON container/string capacities and
/// retained RAW/16-bit/vector/mask owners are included in the traversal.
pub fn document_resident_bytes(document: &Document) -> Result<u64> {
    let mut bytes = (size_of::<Document>() as u64)
        .saturating_add(document.name.capacity() as u64)
        .saturating_add(cap(&document.layers))
        .saturating_add(json_bytes(&document.metadata, 0)?);
    for layer in &document.layers {
        bytes = bytes.saturating_add(layer_resident_bytes(layer, 0)?);
    }
    Ok(bytes)
}
fn brand_bytes(brand: &BrandKit) -> u64 {
    let mut bytes = (size_of::<BrandKit>() as u64)
        .saturating_add(brand.id.capacity() as u64)
        .saturating_add(brand.name.capacity() as u64)
        .saturating_add(brand.fonts.heading.capacity() as u64)
        .saturating_add(brand.fonts.body.capacity() as u64)
        .saturating_add(cap(&brand.logo_resource_ids));
    for id in &brand.logo_resource_ids {
        bytes = bytes.saturating_add(id.capacity() as u64);
    }
    for key in brand.colors.keys().chain(brand.spacing.keys()) {
        bytes = bytes
            .saturating_add(1024)
            .saturating_add(key.capacity() as u64);
    }
    for (key, style) in &brand.text_styles {
        bytes = bytes
            .saturating_add(1024)
            .saturating_add(key.capacity() as u64)
            .saturating_add(style.font.capacity() as u64);
    }
    bytes
}
impl ResidentMemoryEstimate {
    fn document(&mut self, storage: &DocumentStorage, page: bool) -> Result<()> {
        match storage {
            DocumentStorage::Loaded(document) => {
                self.loaded_documents = self.loaded_documents.saturating_add(1);
                self.bytes = self
                    .bytes
                    .saturating_add(document_resident_bytes(document)?);
            }
            DocumentStorage::CheckedOut => {
                if page {
                    self.checked_out_pages = self.checked_out_pages.saturating_add(1);
                }
            }
            DocumentStorage::Lazy { path, cache } => {
                self.bytes = self
                    .bytes
                    .saturating_add(path_bytes(path))
                    .saturating_add(128);
                let guard = match cache.try_lock() {
                    Ok(g) => g,
                    Err(TryLockError::WouldBlock) => anyhow::bail!(
                        "A Create document cache is loading; wait for that background operation before starting another image job"
                    ),
                    Err(TryLockError::Poisoned(_)) => anyhow::bail!(
                        "A Create document cache is unavailable; resident memory cannot be estimated safely"
                    ),
                };
                if let Some(document) = guard.as_ref() {
                    self.cached_lazy_documents = self.cached_lazy_documents.saturating_add(1);
                    self.bytes = self
                        .bytes
                        .saturating_add(document_resident_bytes(document)?);
                }
            }
        }
        Ok(())
    }
}
impl Project {
    pub fn resident_memory_estimate(&self) -> Result<ResidentMemoryEstimate> {
        let mut estimate = ResidentMemoryEstimate {
            bytes: (size_of::<Self>() as u64)
                .saturating_add(self.id.capacity() as u64)
                .saturating_add(self.title.capacity() as u64)
                .saturating_add(self.active_page_id.capacity() as u64)
                .saturating_add(
                    self.active_brand_id
                        .as_ref()
                        .map_or(0, |s| s.capacity() as u64),
                )
                .saturating_add(
                    self.metadata
                        .shared_background_component_id
                        .as_ref()
                        .map_or(0, |s| s.capacity() as u64),
                )
                .saturating_add(cap(&self.pages))
                .saturating_add(cap(&self.components))
                .saturating_add(cap(&self.resources))
                .saturating_add(cap(&self.brand_kits)),
            ..Default::default()
        };
        for brand in &self.brand_kits {
            estimate.bytes = estimate.bytes.saturating_add(brand_bytes(brand));
        }
        if let Some(source) = &self.source {
            estimate.bytes = estimate
                .bytes
                .saturating_add(path_bytes(&source.root))
                .saturating_add(128);
            if let Some(retained) = &source._retained {
                estimate.bytes = estimate
                    .bytes
                    .saturating_add(path_bytes(&retained.0))
                    .saturating_add(128);
            }
        }
        for page in &self.pages {
            estimate.bytes = estimate
                .bytes
                .saturating_add(page.id.capacity() as u64)
                .saturating_add(page.name.capacity() as u64)
                .saturating_add(page.template_id.as_ref().map_or(0, |s| s.capacity() as u64));
            estimate.document(&page.storage, true)?;
        }
        for component in &self.components {
            estimate.bytes = estimate
                .bytes
                .saturating_add(component.id.capacity() as u64)
                .saturating_add(component.name.capacity() as u64);
            estimate.document(&component.storage, false)?;
        }
        for resource in &self.resources {
            estimate.bytes = estimate
                .bytes
                .saturating_add(resource.id.capacity() as u64)
                .saturating_add(resource.name.capacity() as u64)
                .saturating_add(resource.media_type.capacity() as u64);
            match &resource.storage {
                ResourceStorage::Loaded(bytes) => {
                    estimate.loaded_resources = estimate.loaded_resources.saturating_add(1);
                    estimate.bytes = estimate.bytes.saturating_add(bytes.capacity() as u64);
                }
                ResourceStorage::Lazy { path, cache } => {
                    estimate.bytes = estimate
                        .bytes
                        .saturating_add(path_bytes(path))
                        .saturating_add(128);
                    let guard = match cache.try_lock() {
                        Ok(g) => g,
                        Err(TryLockError::WouldBlock) => anyhow::bail!(
                            "A Create resource cache is loading; wait for that background operation before starting another image job"
                        ),
                        Err(TryLockError::Poisoned(_)) => anyhow::bail!(
                            "A Create resource cache is unavailable; resident memory cannot be estimated safely"
                        ),
                    };
                    if let Some(bytes) = guard.as_ref() {
                        estimate.cached_lazy_resources =
                            estimate.cached_lazy_resources.saturating_add(1);
                        estimate.bytes = estimate.bytes.saturating_add(bytes.capacity() as u64);
                    }
                }
            }
        }
        Ok(estimate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba, RgbaImage};
    use std::sync::Arc;
    fn project() -> Project {
        Project::new("Memory fixture", Document::new(8, 6))
    }
    #[test]
    fn loaded_pages_components_raw_words_masks_and_reserved_capacity_are_charged() {
        let mut p = project();
        let empty = p.resident_memory_estimate().unwrap();
        let pixels = ImageBuffer::from_pixel(8, 6, Rgba([12345u16, 23456, 34567, 65535]));
        let mut state =
            crate::advanced::LayerState::from_rgba16(&pixels, "Retained RAW-shaped fixture")
                .unwrap();
        let mut raw = Vec::with_capacity(8192);
        raw.extend_from_slice(b"owned fixture");
        state.raw_bytes = Some(Arc::new(raw));
        state.recipe.raw_extension = Some("dng".into());
        state.recipe.raw_settings = Some(Default::default());
        let source_bytes = state.source.memory_bytes() as u64;
        let mut doc = Document::new(8, 6);
        doc.layers[0].advanced = Some(Arc::new(state));
        doc.layers[0].mask = Some(RgbaImage::from_pixel(8, 6, Rgba([255; 4])).into());
        p.components.push(ReusableComponent {
            id: new_id(),
            name: "Prototype artwork".into(),
            revision: 1,
            storage: DocumentStorage::Loaded(doc),
        });
        let result = p.resident_memory_estimate().unwrap();
        assert_eq!(result.loaded_documents, 2);
        assert!(result.bytes >= empty.bytes + 8192 + source_bytes * 2 + 8 * 6 * 4);
    }
    #[test]
    fn alias_owners_are_counted_multiple_times_without_reading_or_cloning_pixels() {
        let mut p = project();
        let before = p.resident_memory_estimate().unwrap();
        let DocumentStorage::Loaded(doc) = &p.pages[0].storage else {
            panic!()
        };
        let image = doc.layers[0].image.clone().unwrap();
        let duplicate = doc.clone();
        p.pages.push(Page::from_document("Alias".into(), duplicate));
        let after = p.resident_memory_estimate().unwrap();
        assert_eq!(after.loaded_documents, 2);
        assert!(after.bytes >= before.bytes + image.as_raw().capacity() as u64);
        let DocumentStorage::Loaded(doc) = &p.pages[1].storage else {
            panic!()
        };
        assert!(
            doc.layers[0]
                .image
                .as_ref()
                .unwrap()
                .shares_pixels_with(&image)
        );
    }
    #[test]
    fn lazy_none_is_not_loaded_and_populated_lazy_page_component_resource_are_counted() {
        let mut p = project();
        let doc = Document::new(8, 6);
        let cache = Arc::new(Mutex::new(None));
        p.pages[0].storage = DocumentStorage::Lazy {
            path: PathBuf::from("/definitely-not-present/never-read-page.omuse"),
            cache: cache.clone(),
        };
        let empty = p.resident_memory_estimate().unwrap();
        assert_eq!(empty.loaded_documents, 0);
        assert_eq!(empty.cached_lazy_documents, 0);
        assert!(cache.lock().unwrap().is_none());
        *cache.lock().unwrap() = Some(doc.clone());
        p.components.push(ReusableComponent {
            id: new_id(),
            name: "Lazy prototype".into(),
            revision: 1,
            storage: DocumentStorage::Lazy {
                path: PathBuf::from("/never-read-component.omuse"),
                cache: Arc::new(Mutex::new(Some(doc))),
            },
        });
        let mut bytes = Vec::with_capacity(4096);
        bytes.extend_from_slice(b"abc");
        p.resources.push(SharedResource {
            id: new_id(),
            name: "Cached bytes".into(),
            media_type: "image/png".into(),
            byte_len: 3,
            storage: ResourceStorage::Lazy {
                path: PathBuf::from("/never-read-resource.png"),
                cache: Arc::new(Mutex::new(Some(bytes))),
            },
        });
        let after = p.resident_memory_estimate().unwrap();
        assert_eq!(after.cached_lazy_documents, 2);
        assert_eq!(after.cached_lazy_resources, 1);
        assert!(after.bytes >= empty.bytes + 4096 + 8 * 6 * 4 * 2);
        assert!(matches!(p.pages[0].storage, DocumentStorage::Lazy { .. }));
    }
    #[test]
    fn unloaded_resource_declared_size_is_not_mistaken_for_resident_bytes() {
        let mut p = project();
        p.resources.push(SharedResource {
            id: new_id(),
            name: "Unloaded".into(),
            media_type: "image/png".into(),
            byte_len: 3,
            storage: ResourceStorage::Lazy {
                path: PathBuf::from("/never-read-resource"),
                cache: Arc::new(Mutex::new(None)),
            },
        });
        let small = p.resident_memory_estimate().unwrap();
        p.resources[0].byte_len = 64 * 1024 * 1024;
        let declared_large = p.resident_memory_estimate().unwrap();
        assert_eq!(declared_large, small);
        let mut data = Vec::with_capacity(8192);
        data.extend_from_slice(b"abc");
        p.resources[0].storage = ResourceStorage::Loaded(data);
        let loaded = p.resident_memory_estimate().unwrap();
        assert_eq!(loaded.loaded_resources, 1);
        assert!(loaded.bytes >= small.bytes + 7000);
    }
    #[test]
    fn busy_or_poisoned_cache_refuses_admission_without_waiting_or_loading() {
        let mut p = project();
        let cache = Arc::new(Mutex::new(None));
        p.pages[0].storage = DocumentStorage::Lazy {
            path: PathBuf::from("/never-read"),
            cache: cache.clone(),
        };
        let guard = cache.lock().unwrap();
        assert!(
            p.resident_memory_estimate()
                .unwrap_err()
                .to_string()
                .contains("cache is loading")
        );
        drop(guard);
        assert!(p.resident_memory_estimate().is_ok());
        let poisoned = cache.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("owned cache poison fixture")
        })
        .join();
        assert!(
            p.resident_memory_estimate()
                .unwrap_err()
                .to_string()
                .contains("cache is unavailable")
        );
    }
    #[test]
    fn metadata_reserved_strings_and_array_capacity_are_counted_without_serializing() {
        let mut doc = Document::new(1, 1);
        let before = document_resident_bytes(&doc).unwrap();
        let mut text = String::with_capacity(16384);
        text.push('x');
        let mut values = Vec::with_capacity(128);
        values.push(serde_json::Value::String(text));
        doc.metadata = serde_json::Value::Array(values);
        assert!(document_resident_bytes(&doc).unwrap() >= before + 16000);
    }
}
