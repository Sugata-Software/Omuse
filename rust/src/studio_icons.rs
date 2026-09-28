//! Curated Lucide line icons used by the Studio workspace.
//!
//! The application registers [`StudioAssets`] as its GPUI asset source. This
//! keeps the Studio set local and deterministic while retaining the default
//! `gpui-kit-assets` bundle for other components.

use std::borrow::Cow;

use gpui_kit::base::StyledExt;
use gpui_kit::{
    App, AssetSource, IntoElement, RenderOnce, Result, SharedString, StyleRefinement, Styled,
    Window, px, svg,
};

macro_rules! studio_icons {
    ($(($name:literal, $file:literal)),+ $(,)?) => {
        const STUDIO_ASSETS: &[(&str, &[u8])] = &[
            $(
                (
                    concat!("studio-icons/", $name, ".svg"),
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/assets/studio-icons/",
                        $file,
                        ".svg"
                    )) as &[u8],
                ),
            )+
        ];

        fn studio_path(name: &'static str) -> &'static str {
            match name {
                $($name => concat!("studio-icons/", $name, ".svg"),)+
                // Unknown names should still produce a valid, theme-colored
                // glyph rather than an asset lookup error in a release build.
                _ => "studio-icons/sparkles.svg",
            }
        }
    };
}

// The four aliases below correspond to names requested by Studio. The
// installed gpui-kit-assets release does not ship those exact Lucide paths:
// `waves` uses `waves-horizontal`, `swirl` uses `orbit`, `file-plus-2` uses
// `file-plus`, and `trash-2` uses `trash`.
studio_icons!(
    ("omuse", "omuse"),
    ("layers", "layers"),
    ("move", "move"),
    ("hand", "hand"),
    ("square-dashed", "square-dashed"),
    ("circle-dashed", "circle-dashed"),
    ("lasso", "lasso"),
    ("wand-sparkles", "wand-sparkles"),
    ("scan", "scan"),
    ("brush", "brush"),
    ("pencil", "pencil"),
    ("eraser", "eraser"),
    ("paint-bucket", "paint-bucket"),
    ("blend", "blend"),
    ("pipette", "pipette"),
    ("stamp", "stamp"),
    ("bandage", "bandage"),
    ("sparkles", "sparkles"),
    ("droplet", "droplet"),
    ("waves", "waves"),
    ("swirl", "swirl"),
    ("square", "square"),
    ("circle", "circle"),
    ("minus", "minus"),
    ("type", "type"),
    ("sliders-horizontal", "sliders-horizontal"),
    ("focus", "focus"),
    ("save", "save"),
    ("upload", "upload"),
    ("folder-open", "folder-open"),
    ("file-plus-2", "file-plus-2"),
    ("keyboard", "keyboard"),
    ("undo-2", "undo-2"),
    ("redo-2", "redo-2"),
    ("eye", "eye"),
    ("eye-off", "eye-off"),
    ("lock-keyhole", "lock-keyhole"),
    ("plus", "plus"),
    ("trash-2", "trash-2"),
    ("copy", "copy"),
    ("chevron-down", "chevron-down"),
    ("chevron-right", "chevron-right"),
    ("crop", "crop"),
    ("maximize", "maximize"),
    ("settings-2", "settings-2"),
    ("panel-right-close", "panel-right-close"),
    ("panel-right-open", "panel-right-open"),
    ("check", "check"),
    ("image", "image"),
    ("sun-moon", "sun-moon"),
    ("rotate-cw", "rotate-cw"),
    ("arrows-up-from-line", "arrows-up-from-line"),
);

/// Asset source for the Studio icon set plus the default GPUI Kit catalog.
#[derive(Clone, Copy, Debug, Default)]
pub struct StudioAssets;

impl AssetSource for StudioAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = STUDIO_ASSETS
            .iter()
            .find(|(asset_path, _)| *asset_path == path)
        {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }

        <gpui_kit::assets::Assets as AssetSource>::load(&gpui_kit::assets::Assets, path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths =
            <gpui_kit::assets::Assets as AssetSource>::list(&gpui_kit::assets::Assets, path)?;
        paths.extend(
            STUDIO_ASSETS
                .iter()
                .filter(|(asset_path, _)| (*asset_path).starts_with(path))
                .map(|(asset_path, _)| SharedString::from(*asset_path)),
        );
        Ok(paths)
    }
}

#[derive(IntoElement)]
pub struct Glyph {
    name: &'static str,
    style: StyleRefinement,
}

impl Styled for Glyph {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Glyph {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        // Svg requires an explicit color. Resolve the surrounding theme at paint
        // time so active tools and light themes use their actual inherited color.
        svg()
            .path(studio_path(self.name))
            .text_color(window.text_style().color)
            .refine_style(&self.style)
    }
}

/// An 18 px line icon that resolves its color from the surrounding control.
pub fn glyph(name: &'static str) -> Glyph {
    Glyph {
        name,
        style: StyleRefinement::default(),
    }
    .size(px(18.))
    .flex_shrink_0()
}
