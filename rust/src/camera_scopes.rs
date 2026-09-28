//! Theme-aware Camera Raw scopes. Analysis is cached by the preview worker;
//! painting never walks artwork pixels or allocates an image-sized buffer.
use gpui_kit::{
    App, Bounds, Pixels, Rgba, Window, canvas, div, fill, point, prelude::*, px, rgba, size,
};
use gpui_omarchy::ActiveTheme;
use omuse::photo_scopes::{PhotoScopes, VECTOR_SIZE};
use std::sync::Arc;

const HISTOGRAM_WIDTH: f32 = 292.;
const HEIGHT: f32 = 112.;
const CHANNELS: [u32; 3] = [0xf7768eaa, 0x9ece6aaa, 0x7aa2f7aa];

pub fn panel(
    scopes: Option<Arc<PhotoScopes>>,
    caption: &'static str,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.omarchy().clone();
    let histogram = scopes.clone();
    let background = theme.inset;
    let grid = theme.border;
    let signal = theme.accent;
    div()
        .id("camera-scopes")
        .debug_selector(|| "camera-scopes".into())
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().text_color(theme.secondary).child(caption))
        .child(
            div()
                .flex()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_xs().child("RGB histogram"))
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    window.paint_quad(fill(bounds, background));
                                    for fraction in [0.25, 0.5, 0.75] {
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(
                                                    bounds.origin.x + bounds.size.width * fraction,
                                                    bounds.origin.y,
                                                ),
                                                size(px(1.), bounds.size.height),
                                            ),
                                            grid,
                                        ));
                                    }
                                    if let Some(scopes) = &histogram {
                                        let peak = scopes.histogram_peak().max(1) as f32;
                                        let width = bounds.size.width / 256.;
                                        for (channel, colour) in CHANNELS.iter().enumerate() {
                                            for (bin, &count) in
                                                scopes.rgb[channel].iter().enumerate()
                                            {
                                                if count == 0 {
                                                    continue;
                                                }
                                                let height =
                                                    bounds.size.height * (count as f32 / peak);
                                                window.paint_quad(fill(
                                                    Bounds::new(
                                                        point(
                                                            bounds.origin.x + width * bin as f32,
                                                            bounds.origin.y + bounds.size.height
                                                                - height,
                                                        ),
                                                        size(width, height.max(px(1.))),
                                                    ),
                                                    rgba(*colour),
                                                ));
                                            }
                                        }
                                    }
                                },
                            )
                            .w(px(HISTOGRAM_WIDTH))
                            .h(px(HEIGHT)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_xs().child("Vectorscope"))
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    window.paint_quad(fill(bounds, background));
                                    let centre = bounds.center();
                                    let radius = bounds.size.width / 2. - px(2.);
                                    for i in 0..96 {
                                        let angle = i as f32 * std::f32::consts::TAU / 96.;
                                        dot(
                                            window,
                                            centre.x + radius * angle.cos(),
                                            centre.y - radius * angle.sin(),
                                            px(1.),
                                            grid.into(),
                                        );
                                    }
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(centre.x, bounds.origin.y),
                                            size(px(1.), bounds.size.height),
                                        ),
                                        grid,
                                    ));
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(bounds.origin.x, centre.y),
                                            size(bounds.size.width, px(1.)),
                                        ),
                                        grid,
                                    ));
                                    if let Some(scopes) = &scopes {
                                        let scale = (scopes.vector_peak() as f32).ln_1p().max(1.);
                                        let step = (radius * 2.) / (VECTOR_SIZE - 1) as f32;
                                        for (index, &weight) in scopes.vectors.iter().enumerate() {
                                            if weight == 0 {
                                                continue;
                                            }
                                            let mut colour: Rgba = signal.into();
                                            colour.a = (weight as f32).ln_1p() / scale;
                                            dot(
                                                window,
                                                centre.x - radius
                                                    + step * (index % VECTOR_SIZE) as f32,
                                                centre.y - radius
                                                    + step * (index / VECTOR_SIZE) as f32,
                                                step.max(px(1.)),
                                                colour,
                                            );
                                        }
                                    }
                                    // Primary colour targets explain the hue orientation at a glance.
                                    for (i, colour) in CHANNELS.iter().enumerate() {
                                        let angle = i as f32 * std::f32::consts::TAU / 3.;
                                        dot(
                                            window,
                                            centre.x + radius * angle.cos(),
                                            centre.y - radius * angle.sin(),
                                            px(3.),
                                            rgba(*colour),
                                        );
                                    }
                                },
                            )
                            .w(px(HEIGHT))
                            .h(px(HEIGHT)),
                        ),
                ),
        )
        .child(
            div().text_xs().text_color(theme.secondary).child(
                "Selected layer · sampled RGB levels and hue/saturation · update with Preview",
            ),
        )
}

fn dot(window: &mut Window, x: Pixels, y: Pixels, width: Pixels, colour: Rgba) {
    window.paint_quad(fill(
        Bounds::new(point(x - width / 2., y - width / 2.), size(width, width)),
        colour,
    ));
}
