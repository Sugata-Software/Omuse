/*
 * SkiaBridge.h — C ABI seam between the Swift core and the Skia render
 * backends (plan §9, Stage 5/8). The Swift core owns document pixels
 * (PortableImage/PixelBuffer, CPU-addressable); the Skia backends own
 * the SkCanvas/SkSurface and the GPU device. Swift calls through this
 * header to render a CPU RGBA8 buffer into a destination buffer of the
 * same canonical contract (premultiplied RGBA8, stride == width*4).
 *
 * Backend contract (plan §6):
 *   RendererDeviceFactory.create() tries Vulkan, falls back to Raster.
 *   Both backends implement the same render contract (LSP):
 *     - compositor_render_rgba(src, dst, w, h) composites src over dst
 *       at opacity 1.0, source-over, returning the result in dst.
 *     - A backend that cannot initialize returns NULL; the caller uses
 *       the existing pure-Swift DocumentRenderer as the ultimate fallback.
 *
 * Status codes: 0 success, -1 invalid argument, -2 backend lost (try Raster), -3 no Skia compiled in.
 *
 * This header is the only Skia-facing C surface. Swift never imports Skia
 * or Qt headers; C++ never imports Swift types. The boundary is POD + C.
 */
#ifndef SKIABRIDGE_H
#define SKIABRIDGE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque render device handle. One handle owns one Skia backend context
 * (GrDirectContext for Vulkan, or a CPU raster context). Thread-unsafe;
 * the caller serializes. Close releases all GPU resources. */
typedef struct CompRenderer CompRenderer;

/* Backend kind, queried for diagnostics and failsafe decisions. */
typedef enum {
    COMP_RENDERER_RASTER = 0,
    COMP_RENDERER_VULKAN = 1,
} CompRendererKind;

/*
 * Query whether Skia is compiled in. Returns 1 if Skia is available, 0 if not.
 * When 0, render calls return status -3.
 */
int compositor_skia_available(void);

/* Layer effects (stroke, drop shadow, colour overlay, inner shadow) with Skia image filters: the Skia tier of the
 * effects chain. `pixels` and `out` are width*height*4 bytes (premultiplied RGBA8), disjoint. Returns 0 on success,
 * -1 invalid input, -2 when Skia is not built in. See backends/effects/include/CompositorEffectsBackend.h. */
struct CompositorEffectsParams;
int compositor_skia_effects_render(const struct CompositorEffectsParams *params, const uint8_t *pixels, uint8_t *out);

/*
 * Create a render device. Try Vulkan first (plan §6 startup). On any
 * failure (no device, no queue family, bad ICD, device lost, no /dev/dri),
 * fall back to Raster. kind_out receives the chosen backend. Returns NULL
 * only if both backends fail to initialize (extremely rare; the Swift
 * DocumentRenderer remains the ultimate fallback in that case).
 *
 * force_raster: if nonzero, skip Vulkan and create a Raster device. Used
 * by tests and the forced-CPU mode (plan §6 DoD).
 */
CompRenderer *compositor_renderer_create(int force_raster, CompRendererKind *kind_out);
void compositor_renderer_close(CompRenderer *renderer);
CompRendererKind compositor_renderer_kind(const CompRenderer *renderer);
CompRendererKind compositor_renderer_last_executed(const CompRenderer *renderer);
void compositor_renderer_simulate_device_lost(CompRenderer *renderer);

/*
 * Activate `renderer` as the Swift CoreGraphicsCompat backend. After this
 * call, CGCompat draws route through Skia until the renderer is closed or
 * a device-lost forces a fallback (plan §6 runtime loss). Paired with
 * compositor_renderer_deactivate. */
void compositor_renderer_activate(CompRenderer *renderer);
void compositor_renderer_deactivate(void);

/*
 * Composite a source RGBA8 tile over a destination RGBA8 tile in place,
 * premultiplied source-over, opacity 1.0, via the backend's SkCanvas.
 * Both buffers are canonical contract (stride == width*4, premultiplied).
 *
 * This is the Stage 5 headless raster surface smoke (plan §8.10 DoD):
 * prove Skia can ingest and emit the canonical buffer format. The full
 * LayerRenderer draw pipeline (transforms, masks, blend modes) stays in
 * Swift for now; this call validates the Skia bridge end-to-end.
 *
 * Returns 0 on success, -1 invalid argument, -2 device lost (caller should
 * close, recreate as Raster, and retry — plan §6 runtime loss).
 */
int compositor_render_rgba(CompRenderer *renderer,
                           const uint8_t *src_rgba,
                           uint8_t *dst_rgba,
                           size_t width, size_t height);

/*
 * Create a raster surface of the given size and fill it with the src pixels,
 * returning the result in dst. Equivalent to compositor_render_rgba for the
 * Raster backend; for Vulkan it exercises the GPU upload + readback path.
 * Used by the Stage 0 DoD smoke test (plan §8.10): create a Skia Raster
 * surface, write into it, read back, verify.
 */
int compositor_skia_raster_surface(const uint8_t *src_rgba,
                                   uint8_t *dst_rgba,
                                   size_t width, size_t height);

/*
 * Enumerate Vulkan physical devices. Returns the count, or -1 if the Vulkan
 * loader is unavailable (no ICD). Does not create a device. Used by the
 * Stage 0 DoD smoke (plan §8.10): "find the Vulkan loader and try to
 * enumerate devices." Also returns -1 if the Skia build has no Vulkan.
 */
int compositor_vulkan_enumerate_devices(void);

/*
 * Register a render function with the Swift CoreGraphicsCompat shim. The
 * Swift CGContextCompat draws route through this function when set; when
 * NULL (or on non-zero return), draws fall back to the pure-Swift
 * LayerRenderer (plan §6 failsafe). The host calls this once at startup
 * after creating a CompRenderer.
 *
 * fn signature matches compositor_render_rgba minus the renderer handle:
 *   fn(src, dst, width, height) -> int32_t
 * The C++ side wraps a CompRenderer* into this signature via a static
 * trampoline (one active renderer at a time; the factory replaces it on
 * device loss).
 */
typedef int32_t (*CompRenderFn)(const uint8_t *src, uint8_t *dst,
                                size_t width, size_t height);
void compositor_compat_set_render_fn(CompRenderFn fn);

/* ── Stage 4/5: Canvas C ABI (CoreGraphics-shaped Skia Bridge) ──────────── */

typedef struct CompCanvas CompCanvas;
typedef struct CompPath CompPath;

/* Canvas Lifecycle */
CompCanvas *compositor_canvas_create(uint8_t *pixels, size_t width, size_t height, size_t stride);
/* Pixel formats: 0 = RGBA8888 premultiplied, 1 = 8-bit gray (opaque, one byte per pixel). */
CompCanvas *compositor_canvas_create_ex(uint8_t *pixels, size_t width, size_t height, size_t stride, int format);
void compositor_canvas_destroy(CompCanvas *canvas);

/* State Management */
void compositor_canvas_save(CompCanvas *canvas);
void compositor_canvas_restore(CompCanvas *canvas);
void compositor_canvas_translate(CompCanvas *canvas, float dx, float dy);
void compositor_canvas_scale(CompCanvas *canvas, float sx, float sy);
void compositor_canvas_rotate(CompCanvas *canvas, float radians);
void compositor_canvas_concat(CompCanvas *canvas, float a, float b, float c, float d, float tx, float ty);
void compositor_canvas_get_ctm(const CompCanvas *canvas, float *a, float *b, float *c, float *d, float *tx, float *ty);

/* Clipping */
void compositor_canvas_clip_rect(CompCanvas *canvas, float x, float y, float w, float h, int antialias);
void compositor_canvas_clip_rect_difference(CompCanvas *canvas, float x, float y, float w, float h);
void compositor_canvas_clip_path(CompCanvas *canvas, const CompPath *path, int even_odd, int antialias);
void compositor_canvas_clip_mask(CompCanvas *canvas, const uint8_t *mask_pixels, size_t mask_w, size_t mask_h, size_t mask_stride, float x, float y, float w, float h, int is_alpha_only);
void compositor_canvas_get_clip_bounds(const CompCanvas *canvas, float *x, float *y, float *w, float *h);

/* Style & Attributes */
void compositor_canvas_set_alpha(CompCanvas *canvas, float alpha);
void compositor_canvas_set_blend_mode(CompCanvas *canvas, int cg_blend_mode);
void compositor_canvas_set_interpolation_quality(CompCanvas *canvas, int cg_quality);
void compositor_canvas_set_antialias(CompCanvas *canvas, int antialias);

/* Drawing Operations */
void compositor_canvas_fill_rect(CompCanvas *canvas, float x, float y, float w, float h, float r, float g, float b, float a);
void compositor_canvas_clear(CompCanvas *canvas, float x, float y, float w, float h);
void compositor_canvas_draw_image_rect(CompCanvas *canvas, const uint8_t *src_pixels, size_t src_w, size_t src_h, size_t src_stride, float dx, float dy, float dw, float dh, float opacity, int cg_blend_mode, int cg_sampling_quality);
/* Same, with the source pixel format (0 = RGBA8888 premultiplied, 1 = 8-bit gray, drawn as opaque gray). */
void compositor_canvas_draw_image_rect_ex(CompCanvas *canvas, const uint8_t *src_pixels, size_t src_w, size_t src_h, size_t src_stride, int src_format, float dx, float dy, float dw, float dh, float opacity, int cg_blend_mode, int cg_sampling_quality);
void compositor_canvas_begin_transparency_layer(CompCanvas *canvas, float opacity);
void compositor_canvas_end_transparency_layer(CompCanvas *canvas);

/* Path Handle */
CompPath *compositor_path_create(void);
void compositor_path_destroy(CompPath *path);
void compositor_path_move_to(CompPath *path, float x, float y);
void compositor_path_line_to(CompPath *path, float x, float y);
void compositor_path_add_rect(CompPath *path, float x, float y, float w, float h);
void compositor_path_add_ellipse(CompPath *path, float cx, float cy, float rx, float ry);
void compositor_path_close(CompPath *path);
void compositor_path_reset(CompPath *path);

/* CoreGraphics path surface (Apple CGPath/CGContext compat). Curves, rounded rects and copies. */
void compositor_path_quad_to(CompPath *path, float cx, float cy, float x, float y);
void compositor_path_cubic_to(CompPath *path, float c1x, float c1y, float c2x, float c2y, float x, float y);
void compositor_path_add_round_rect(CompPath *path, float x, float y, float w, float h, float rx, float ry);
CompPath *compositor_path_copy(const CompPath *path);

/* Boolean operations (Skia PathOps): op 0 = difference (a - b), 1 = intersect, 2 = union, 3 = xor.
 * even_odd_a/b select each operand's fill rule. Returns a new path, or NULL if the operation fails. */
CompPath *compositor_path_op(const CompPath *a, const CompPath *b, int op, int even_odd_a, int even_odd_b);

/* Stroked outline as a fillable path. cap: 0 butt, 1 round, 2 square. join: 0 miter, 1 round, 2 bevel. */
CompPath *compositor_path_stroke(const CompPath *path, float width, int cap, int join, float miter_limit);

/* Tight bounds of the path (out = x, y, w, h). Returns 0 when the path is empty. */
int compositor_path_bounds(const CompPath *path, float out_rect[4]);

/* Element enumeration: type 0 move (1 pt), 1 line (1 pt), 2 quad (2 pts), 3 cubic (3 pts), 4 close (0 pts).
 * Conics are converted to quads. Returns the element count when out_types is NULL, otherwise the number of
 * elements written; points are packed x,y in order into out_points (capacity in floats). */
size_t compositor_path_elements(const CompPath *path, uint8_t *out_types, size_t type_capacity,
                                float *out_points, size_t point_float_capacity, size_t *points_written);

/* Painting a path on a canvas. */
void compositor_canvas_fill_path(CompCanvas *canvas, const CompPath *path, int even_odd,
                                 float r, float g, float b, float a);
void compositor_canvas_stroke_path(CompCanvas *canvas, const CompPath *path, float width, int cap, int join,
                                   float miter_limit, float r, float g, float b, float a);

/* Gradients painted through the current clip. colors are packed r,g,b,a; locations has `count` entries in 0...1.
 * options bit0: extend before start, bit1: extend after end (CGGradientDrawingOptions). */
void compositor_canvas_draw_linear_gradient(CompCanvas *canvas, float x0, float y0, float x1, float y1,
                                            const float *colors, const float *locations, size_t count, int options);
void compositor_canvas_draw_radial_gradient(CompCanvas *canvas, float x0, float y0, float r0, float x1, float y1,
                                            float r1, const float *colors, const float *locations, size_t count,
                                            int options);

#ifdef __cplusplus
}
#endif

#endif /* SKIABRIDGE_H */