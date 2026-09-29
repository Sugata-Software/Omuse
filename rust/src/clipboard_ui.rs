//! Keep editable trees only while Omuse owns the matching clipboard item.
//! The public clipboard also contains PNG for other applications. A private,
//! process-local metadata token prevents an unrelated (even identical) PNG
//! from resurrecting old layer structure.
use super::*;

#[derive(Default)]
pub(super) struct LayerClipboardStore(Option<(String, omuse::editor::LayerClipboard)>);
impl gpui_kit::Global for LayerClipboardStore {}

pub(super) fn clipboard_identity(cx: &App) -> Option<String> {
    cx.try_global::<LayerClipboardStore>()
        .and_then(|s| s.0.as_ref())
        .map(|(token, _)| token.clone())
}

impl EditorView {
    pub(super) fn copy_layer_forest(&mut self, cut: bool, cx: &mut Context<Self>) {
        let ids = self.selected_layer_ids();
        let result = (|| -> anyhow::Result<_> {
            let payload = self.editor.copy_layers(&ids)?;
            let preview = payload.preview()?;
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(preview)
                .write_to(&mut bytes, image::ImageFormat::Png)?;
            Ok((payload, bytes.into_inner()))
        })();
        match result {
            Ok((payload, bytes)) => {
                if cut && !self.editor.delete_layers(&ids) {
                    self.status = "Cut needs unlocked layers with no external mask dependents; nothing was removed or copied".into();
                    cx.notify();
                    return;
                }
                let count = payload.root_count();
                let token = format!("omuse-layers:{}", uuid::Uuid::new_v4());
                let mut item =
                    ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, bytes));
                item.entries.extend(
                    ClipboardItem::new_string_with_metadata(String::new(), token.clone()).entries,
                );
                cx.set_global(LayerClipboardStore(Some((token, payload))));
                cx.write_to_clipboard(item);
                self.clipboard_origin = None;
                if cut {
                    self.changed(cx);
                }
                self.status = format!(
                    "{} {count} editable layer{} · Other apps receive PNG",
                    if cut { "Cut" } else { "Copied" },
                    if count == 1 { "" } else { "s" }
                );
            }
            Err(e) => self.status = format!("Layer copy: {e:#}"),
        }
        cx.notify();
    }

    pub(super) fn paste_layer_forest(
        &mut self,
        item: &ClipboardItem,
        cx: &mut Context<Self>,
    ) -> bool {
        let token = item.entries.iter().find_map(|entry| match entry {
            ClipboardEntry::String(s) => s
                .metadata
                .as_deref()
                .filter(|s| s.starts_with("omuse-layers:")),
            _ => None,
        });
        let payload = cx
            .try_global::<LayerClipboardStore>()
            .and_then(|store| store.0.as_ref())
            .filter(|(stored, _)| Some(stored.as_str()) == token)
            .map(|(_, payload)| payload.clone());
        let Some(payload) = payload else {
            return false;
        };
        match self.editor.paste_layers(&payload) {
            Ok(ids) => {
                let count = ids.len();
                self.select_layer_ids(ids);
                self.paint_mask = false;
                self.changed(cx);
                self.status = format!(
                    "Pasted {count} editable layer{} · Ctrl+Z to undo",
                    if count == 1 { "" } else { "s" }
                );
            }
            Err(e) => self.status = format!("Layer paste: {e:#}"),
        }
        cx.notify();
        true
    }
}
