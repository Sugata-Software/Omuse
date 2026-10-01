//! Platform-independent editing model. Pixels are straight-alpha RGBA, bottom layer first.
use crate::shared_image::SharedImage;
use image::RgbaImage;
use serde_json::{Value, json};

pub const MAX_DIMENSION: u32 = 30_000;
pub const MAX_PIXELS: u64 = 100_000_000;
pub const MAX_LAYERS: usize = 10_000;

/// Half-open pixel bounds. Extents are clipped using wide arithmetic so callers
/// can safely pass damage extending beyond the edge of an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub fn clipped(self, width: u32, height: u32) -> Option<Self> {
        self.translated_clipped(0, 0, width, height)
    }

    /// Translate source-image damage to canvas coordinates and intersect it
    /// with the canvas, including layers placed partly outside the canvas.
    pub fn translated_clipped(self, dx: i64, dy: i64, width: u32, height: u32) -> Option<Self> {
        let left = i128::from(self.x) + i128::from(dx);
        let top = i128::from(self.y) + i128::from(dy);
        let right = (left + i128::from(self.width)).clamp(0, i128::from(width));
        let bottom = (top + i128::from(self.height)).clamp(0, i128::from(height));
        let left = left.clamp(0, i128::from(width));
        let top = top.clamp(0, i128::from(height));
        (right > left && bottom > top).then_some(Self {
            x: left as u32,
            y: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        })
    }
}

pub fn valid_dimensions(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_DIMENSION
        && height <= MAX_DIMENSION
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

#[derive(Clone, Debug)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub name: String,
    pub background: [u8; 4],
    pub layers: Vec<Layer>,
    /// Original manifest fields, including fields this editor does not interpret.
    pub metadata: Value,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend_mode: String,
    pub offset_x: f32,
    pub offset_y: f32,
    /// Clockwise degrees around the layer's scaled center.
    pub rotation: f32,
    /// Negative scale represents a flip without moving the unrotated bounds.
    pub scale_x: f32,
    pub scale_y: f32,
    /// Cloning a layer shares pixels; mutable access detaches the edited image.
    pub image: Option<SharedImage>,
    pub mask: Option<SharedImage>,
    /// Editable original, recipe and full-precision cache. Shared by snapshots.
    pub advanced: Option<std::sync::Arc<crate::advanced::LayerState>>,
    /// Editable procedural vector geometry. `image` is its derived RGBA8 cache.
    pub vector_scene: Option<std::sync::Arc<crate::vector_scene::VectorScene>>,
    pub children: Vec<Layer>,
    /// Original layer record; empty groups have `isGroup: true`.
    pub metadata: Value,
}

impl Document {
    pub fn new(width: u32, height: u32) -> Self {
        assert!(
            valid_dimensions(width, height),
            "canvas exceeds supported pixel bounds"
        );
        Self {
            width,
            height,
            name: "Untitled".into(),
            background: [0, 0, 0, 0],
            layers: vec![Layer::paint("Layer 1", width, height)],
            metadata: json!({"documentID": uuid::Uuid::new_v4().to_string().to_uppercase()}),
        }
    }

    pub fn find_layer(&self, id: &str) -> Option<&Layer> {
        fn find<'a>(layers: &'a [Layer], id: &str) -> Option<&'a Layer> {
            for layer in layers {
                if layer.id == id {
                    return Some(layer);
                }
                if let Some(found) = find(&layer.children, id) {
                    return Some(found);
                }
            }
            None
        }
        find(&self.layers, id)
    }

    pub fn find_layer_mut(&mut self, id: &str) -> Option<&mut Layer> {
        fn find<'a>(layers: &'a mut [Layer], id: &str) -> Option<&'a mut Layer> {
            for layer in layers {
                if layer.id == id {
                    return Some(layer);
                }
                if let Some(found) = find(&mut layer.children, id) {
                    return Some(found);
                }
            }
            None
        }
        find(&mut self.layers, id)
    }
}

impl Layer {
    pub fn paint(name: impl Into<String>, width: u32, height: u32) -> Self {
        assert!(
            valid_dimensions(width, height),
            "layer exceeds supported pixel bounds"
        );
        let mut layer = Self::group(name);
        layer.metadata = json!({});
        layer.image = Some(RgbaImage::new(width, height).into());
        layer
    }

    pub fn group(name: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string().to_uppercase(),
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.,
            blend_mode: "Normal".into(),
            offset_x: 0.,
            offset_y: 0.,
            rotation: 0.,
            scale_x: 1.,
            scale_y: 1.,
            image: None,
            mask: None,
            advanced: None,
            vector_scene: None,
            children: vec![],
            metadata: json!({"isGroup": true}),
        }
    }

    pub fn is_group(&self) -> bool {
        self.metadata.get("isGroup").and_then(Value::as_bool) == Some(true)
            || !self.children.is_empty()
    }
}
