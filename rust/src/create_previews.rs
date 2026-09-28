//! Bounded, background thumbnails for the native template browser.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(super) struct TemplatePreviews {
    pub images: BTreeMap<String, Arc<RenderImage>>,
    failed: BTreeSet<String>,
    loading: bool,
}
impl EditorView {
    pub(super) fn load_template_previews(&mut self, cx: &mut Context<Self>) {
        if self.create.template_previews.loading || !self.create.template_previews.images.is_empty()
        {
            return;
        }
        self.create.template_previews.loading = true;
        cx.spawn(async move |view, cx| {
            for template in omuse::create::templates() {
                let id = template.id;
                let pixels = cx
                    .background_executor()
                    .spawn(async move {
                        let document = omuse::create::instantiate_template(id, None).ok()?;
                        let pixels = raster::composite(&document);
                        (pixels.width() > 0 && pixels.height() > 0)
                            .then(|| image::imageops::thumbnail(&pixels, 288, 240))
                    })
                    .await;
                // Publish as each thumbnail finishes. The first card no longer
                // waits behind the entire catalogue, and closing the window
                // prevents the remaining work from being scheduled.
                if view
                    .update(cx, |this, cx| {
                        if let Some(pixels) = pixels {
                            this.create
                                .template_previews
                                .images
                                .insert(id.to_owned(), render_image(&pixels));
                        } else {
                            this.create.template_previews.failed.insert(id.to_owned());
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            let _ = view.update(cx, |this, cx| {
                this.create.template_previews.loading = false;
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn create_template_preview(&self, id: &str, cx: &App) -> AnyElement {
        let t = cx.omarchy();
        let frame = div()
            .h(px(152.))
            .w_full()
            .overflow_hidden()
            .rounded(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .bg(t.background);
        if let Some(image) = self.create.template_previews.images.get(id) {
            frame
                .child(
                    gpui_kit::img(image.clone())
                        .h_full()
                        .max_w_full()
                        .object_fit(gpui_kit::ObjectFit::Contain),
                )
                .into_any_element()
        } else {
            frame
                .child(div().text_size(px(11.)).text_color(t.secondary).child(
                    if self.create.template_previews.failed.contains(id) {
                        "Preview unavailable"
                    } else {
                        "Preparing preview…"
                    },
                ))
                .into_any_element()
        }
    }
}
