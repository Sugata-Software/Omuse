use image::{Rgba, RgbaImage};
use omuse::{
    model::{Document, Layer},
    raster,
};
use serde_json::json;

fn clipped_group_with_independent_child_mask(mask_alpha: u8) -> Document {
    let mut base = Layer::paint("Translucent blue base", 1, 1);
    base.image = Some(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 255, 128])).into());
    let mut source = Layer::paint("Independent hidden mask source", 1, 1);
    source.image = Some(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, mask_alpha])).into());
    source.visible = false;
    let mut child = Layer::paint("Masked red child", 1, 1);
    child.image = Some(RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255])).into());
    child.metadata["maskSourceID"] = json!(source.id);
    let mut group = Layer::group("Clipped group");
    group.metadata["maskSourceID"] = json!(base.id);
    group.children = vec![child];
    let mut document = Document::new(1, 1);
    document.layers = vec![base, group, source];
    document
}

#[test]
fn raster_clipped_group_keeps_its_descendants_independent_live_masks() {
    for alpha in [0, 64, 128, 255] {
        let document = clipped_group_with_independent_child_mask(alpha);
        assert!(raster::validate(&document).is_empty());
        assert_eq!(
            raster::composite(&document).get_pixel(0, 0).0,
            [alpha, 0, 255 - alpha, 128],
            "a clipped group must not unmask its child at mask alpha {alpha}"
        );
    }
}

#[test]
fn precision_clipped_group_keeps_its_descendants_independent_live_masks() {
    for alpha in [0, 64, 128, 255] {
        let document = clipped_group_with_independent_child_mask(alpha);
        let covered = u16::from(alpha) * 257;
        assert_eq!(
            raster::composite16(&document).unwrap().get_pixel(0, 0).0,
            [covered, 0, 65_535 - covered, 128 * 257],
            "16-bit clipping must not unmask its child at mask alpha {alpha}"
        );
    }
}
