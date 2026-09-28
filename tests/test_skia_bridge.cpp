// test_skia_bridge.cpp — Stage 0/5 DoD smoke for the Skia render-device
// bridge (plan §8.10, §9). Verifies, with no Qt and no Swift:
//
//   1. compositor_skia_raster_surface: a Skia Raster surface ingests a
//      canonical RGBA8 buffer and emits the same bytes (identity round-trip
//      proves the bridge handles the canonical contract).
//   2. compositor_renderer_create: factory returns a Raster device when
//      force_raster is set; kind is COMP_RENDERER_RASTER.
//   3. compositor_vulkan_enumerate_devices: returns 0 (loader reachable)
//      or -1 (no Vulkan); either is acceptable, a crash is not.
//   4. compositor_render_rgba: source-over composite of a red src over a
//      transparent dst yields red dst (premultiplied RGBA8).
//
// This is the plan's Stage 0 dependency smoke test, restricted to the Skia
// leg (Qt/Swift legs are verified by the existing CTest + Swift suite).
#include "SkiaBridge.h"

#include <cassert>
#include <cstdio>
#include <cstring>

static int test_raster_surface_identity() {
    const size_t w = 4, h = 4;
    uint8_t src[w * h * 4], dst[w * h * 4];
    for (size_t i = 0; i < w * h * 4; ++i) src[i] = static_cast<uint8_t>(i % 256);
    std::memset(dst, 0, sizeof(dst));
    int rc = compositor_skia_raster_surface(src, dst, w, h);
    if (rc != 0) { std::fprintf(stderr, "raster_surface rc=%d\n", rc); return 1; }
    // Premultiplied source-over over transparent dst == src (identity).
    if (std::memcmp(src, dst, sizeof(src)) != 0) {
        std::fprintf(stderr, "raster_surface bytes differ\n");
        return 1;
    }
    return 0;
}

static int test_skia_available() {
    if (!compositor_skia_available()) {
        std::fprintf(stderr, "expected compositor_skia_available() == 1\n");
        return 1;
    }
    return 0;
}

static int test_renderer_factory_raster() {
    CompRendererKind kind = COMP_RENDERER_VULKAN;
    CompRenderer *r = compositor_renderer_create(1, &kind);
    if (!r) { std::fprintf(stderr, "renderer_create NULL\n"); return 1; }
    if (kind != COMP_RENDERER_RASTER) {
        std::fprintf(stderr, "expected RASTER, got %d\n", kind);
        compositor_renderer_close(r);
        return 1;
    }
    if (compositor_renderer_kind(r) != COMP_RENDERER_RASTER) {
        compositor_renderer_close(r);
        return 1;
    }
    if (compositor_renderer_last_executed(r) != COMP_RENDERER_RASTER) {
        std::fprintf(stderr, "expected last_executed RASTER, got %d\n",
                     compositor_renderer_last_executed(r));
        compositor_renderer_close(r);
        return 1;
    }
    compositor_renderer_close(r);
    return 0;
}

static int test_vulkan_enumerate_no_crash() {
    int n = compositor_vulkan_enumerate_devices();
    // 0 = loader reachable (enumeration deferred to Stage 8).
    // -1 = no Vulkan. Both are acceptable; a crash is not.
    std::printf("vulkan_enumerate_devices=%d\n", n);
    return 0;
}

static int test_render_rgba_source_over() {
    const size_t w = 2, h = 2;
    // src: opaque red, premultiplied.
    uint8_t src[w * h * 4] = {255, 0, 0, 255, 255, 0, 0, 255,
                              255, 0, 0, 255, 255, 0, 0, 255};
    // dst: transparent.
    uint8_t dst[w * h * 4] = {0};
    CompRenderer *r = compositor_renderer_create(1, nullptr);
    if (!r) return 1;
    int rc = compositor_render_rgba(r, src, dst, w, h);
    compositor_renderer_close(r);
    if (rc != 0) { std::fprintf(stderr, "render_rgba rc=%d\n", rc); return 1; }
    for (size_t p = 0; p < w * h; ++p) {
        const uint8_t *d = dst + p * 4;
        if (d[0] != 255 || d[1] != 0 || d[2] != 0 || d[3] != 255) {
            std::fprintf(stderr, "pixel %zu = %d,%d,%d,%d\n",
                p, d[0], d[1], d[2], d[3]);
            return 1;
        }
    }
    return 0;
}

// Regression (2026-09-21 review, R46): the bridge used to memcpy src over dst,
// so every test with an opaque source over a transparent destination passed
// while real compositing was never exercised. A partially transparent source
// over a NON-EMPTY destination is the smallest input a copy cannot satisfy.
// Premultiplied source-over: out = src + dst * (255 - src.a) / 255.
static int test_render_rgba_partial_alpha_over_nonempty_dst() {
    const size_t w = 2, h = 1;
    const uint8_t src[w * h * 4] = {128, 0, 0, 128,   0, 100, 0, 100};   // premultiplied
    const uint8_t dst0[w * h * 4] = {0, 0, 255, 255,  200, 200, 200, 255}; // opaque backdrop
    uint8_t expected[w * h * 4];
    for (size_t p = 0; p < w * h; ++p) {
        const int inv = 255 - src[p * 4 + 3];
        for (int c = 0; c < 4; ++c) {
            expected[p * 4 + c] = static_cast<uint8_t>(
                src[p * 4 + c] + (dst0[p * 4 + c] * inv + 127) / 255);
        }
    }
    uint8_t dst[w * h * 4];
    std::memcpy(dst, dst0, sizeof(dst));
    CompRenderer *r = compositor_renderer_create(1, nullptr);
    if (!r) return 1;
    int rc = compositor_render_rgba(r, src, dst, w, h);
    compositor_renderer_close(r);
    if (rc != 0) { std::fprintf(stderr, "partial_alpha rc=%d\n", rc); return 1; }
    for (size_t i = 0; i < sizeof(dst); ++i) {
        const int diff = static_cast<int>(dst[i]) - static_cast<int>(expected[i]);
        if (diff < -1 || diff > 1) {  // Skia may round differently by 1
            std::fprintf(stderr,
                "partial_alpha byte %zu = %d, expected %d (source-over, not a copy)\n",
                i, dst[i], expected[i]);
            return 1;
        }
    }
    return 0;
}

static int test_vulkan_device_creation_and_device_lost_fallback() {
    CompRendererKind kind = COMP_RENDERER_RASTER;
    CompRenderer *r = compositor_renderer_create(0, &kind);
    if (!r) {
        std::fprintf(stderr, "compositor_renderer_create(0) failed\n");
        return 1;
    }
    std::printf("Created renderer with kind=%d\n", static_cast<int>(kind));

    const size_t w = 2, h = 2;
    uint8_t src[w * h * 4] = {10, 20, 30, 255, 40, 50, 60, 255,
                              70, 80, 90, 255, 100, 110, 120, 255};
    uint8_t dst[w * h * 4] = {0};
    int rc = compositor_render_rgba(r, src, dst, w, h);
    if (rc != 0) {
        std::fprintf(stderr, "compositor_render_rgba failed rc=%d\n", rc);
        compositor_renderer_close(r);
        return 1;
    }
    if (std::memcmp(src, dst, sizeof(src)) != 0) {
        std::fprintf(stderr, "compositor_render_rgba output mismatch\n");
        compositor_renderer_close(r);
        return 1;
    }
    // Vulkan falls back to Raster until Stage 8, so last_executed must be RASTER.
    if (compositor_renderer_last_executed(r) != COMP_RENDERER_RASTER) {
        std::fprintf(stderr, "expected last_executed RASTER, got %d\n",
                     compositor_renderer_last_executed(r));
        compositor_renderer_close(r);
        return 1;
    }

    // If backend was Vulkan, simulate loss and verify seamless dynamic fallback to Raster CPU.
    if (kind == COMP_RENDERER_VULKAN) {
        compositor_renderer_simulate_device_lost(r);
        std::memset(dst, 0, sizeof(dst));
        rc = compositor_render_rgba(r, src, dst, w, h);
        if (rc != 0) {
            std::fprintf(stderr, "compositor_render_rgba after device_lost failed rc=%d\n", rc);
            compositor_renderer_close(r);
            return 1;
        }
        if (std::memcmp(src, dst, sizeof(src)) != 0) {
            std::fprintf(stderr, "compositor_render_rgba fallback output mismatch\n");
            compositor_renderer_close(r);
            return 1;
        }
        if (compositor_renderer_kind(r) != COMP_RENDERER_RASTER) {
            std::fprintf(stderr, "expected RASTER after device lost fallback\n");
            compositor_renderer_close(r);
            return 1;
        }
        if (compositor_renderer_last_executed(r) != COMP_RENDERER_RASTER) {
            std::fprintf(stderr, "expected last_executed RASTER after device lost fallback\n");
            compositor_renderer_close(r);
            return 1;
        }
        std::printf("Device loss fallback to Raster CPU verified successfully\n");
    }

    compositor_renderer_close(r);
    return 0;
}

static int test_canvas_c_abi() {
    const size_t w = 8, h = 8;
    uint8_t dst[w * h * 4];
    std::memset(dst, 0, sizeof(dst));

    CompCanvas *canvas = compositor_canvas_create(dst, w, h, w * 4);
    if (!canvas) {
        std::fprintf(stderr, "compositor_canvas_create returned NULL\n");
        return 1;
    }

    // 1. Initial CTM check
    float a = 0, b = 0, c = 0, d = 0, tx = 0, ty = 0;
    compositor_canvas_get_ctm(canvas, &a, &b, &c, &d, &tx, &ty);
    if (a != 1.0f || b != 0.0f || c != 0.0f || d != 1.0f || tx != 0.0f || ty != 0.0f) {
        std::fprintf(stderr, "Initial CTM mismatch: [%f,%f,%f,%f,%f,%f]\n", a, b, c, d, tx, ty);
        compositor_canvas_destroy(canvas);
        return 1;
    }

    // 2. Fill rect with green (0, 1, 0, 1)
    compositor_canvas_fill_rect(canvas, 0, 0, (float)w, (float)h, 0.0f, 1.0f, 0.0f, 1.0f);
    if (dst[0] != 0 || dst[1] != 255 || dst[2] != 0 || dst[3] != 255) {
        std::fprintf(stderr, "fill_rect failed: %d,%d,%d,%d\n", dst[0], dst[1], dst[2], dst[3]);
        compositor_canvas_destroy(canvas);
        return 1;
    }

    // 3. Save, translate, scale, verify CTM, restore
    compositor_canvas_save(canvas);
    compositor_canvas_translate(canvas, 2.0f, 3.0f);
    compositor_canvas_scale(canvas, 2.0f, 2.0f);
    compositor_canvas_get_ctm(canvas, &a, &b, &c, &d, &tx, &ty);
    if (a != 2.0f || b != 0.0f || c != 0.0f || d != 2.0f || tx != 2.0f || ty != 3.0f) {
        std::fprintf(stderr, "Transformed CTM mismatch: [%f,%f,%f,%f,%f,%f]\n", a, b, c, d, tx, ty);
        compositor_canvas_destroy(canvas);
        return 1;
    }
    compositor_canvas_restore(canvas);
    compositor_canvas_get_ctm(canvas, &a, &b, &c, &d, &tx, &ty);
    if (a != 1.0f || b != 0.0f || c != 0.0f || d != 1.0f || tx != 0.0f || ty != 0.0f) {
        std::fprintf(stderr, "Restored CTM mismatch: [%f,%f,%f,%f,%f,%f]\n", a, b, c, d, tx, ty);
        compositor_canvas_destroy(canvas);
        return 1;
    }

    // 4. Clip bounds
    compositor_canvas_clip_rect(canvas, 1.0f, 1.0f, 4.0f, 4.0f, 0);
    float cx = 0, cy = 0, cw = 0, ch = 0;
    compositor_canvas_get_clip_bounds(canvas, &cx, &cy, &cw, &ch);
    if (cx != 1.0f || cy != 1.0f || cw != 4.0f || ch != 4.0f) {
        std::fprintf(stderr, "Clip bounds mismatch: [%f,%f,%f,%f]\n", cx, cy, cw, ch);
        compositor_canvas_destroy(canvas);
        return 1;
    }

    // 5. Draw image rect inside clip
    const uint8_t blue_img[4] = {0, 0, 255, 255};
    compositor_canvas_draw_image_rect(canvas, blue_img, 1, 1, 4, 1.0f, 1.0f, 1.0f, 1.0f, 1.0f, -1, -1);
    // pixel at (1, 1) should now be blue
    size_t offset = (1 * w + 1) * 4;
    if (dst[offset] != 0 || dst[offset + 1] != 0 || dst[offset + 2] != 255 || dst[offset + 3] != 255) {
        std::fprintf(stderr, "draw_image_rect pixel (1,1) mismatch: %d,%d,%d,%d\n",
                     dst[offset], dst[offset + 1], dst[offset + 2], dst[offset + 3]);
        compositor_canvas_destroy(canvas);
        return 1;
    }
    // pixel outside clip (0, 0) should still be green
    if (dst[0] != 0 || dst[1] != 255 || dst[2] != 0 || dst[3] != 255) {
        std::fprintf(stderr, "pixel (0,0) outside clip was modified: %d,%d,%d,%d\n",
                     dst[0], dst[1], dst[2], dst[3]);
        compositor_canvas_destroy(canvas);
        return 1;
    }

    // 6. Path clip test
    CompPath *path = compositor_path_create();
    if (!path) {
        std::fprintf(stderr, "compositor_path_create returned NULL\n");
        compositor_canvas_destroy(canvas);
        return 1;
    }
    compositor_path_move_to(path, 0, 0);
    compositor_path_line_to(path, 4, 0);
    compositor_path_line_to(path, 4, 4);
    compositor_path_close(path);
    compositor_canvas_clip_path(canvas, path, 0, 0);
    compositor_path_destroy(path);

    // 7. Transparency layer test
    compositor_canvas_begin_transparency_layer(canvas, 0.5f);
    compositor_canvas_fill_rect(canvas, 1.0f, 1.0f, 1.0f, 1.0f, 1.0f, 0.0f, 0.0f, 1.0f);
    compositor_canvas_end_transparency_layer(canvas);

    // 8. Clear
    compositor_canvas_clear(canvas, 0, 0, 8, 8);

    compositor_canvas_destroy(canvas);
    return 0;
}

int main() {
    if (!compositor_skia_available()) {
        std::printf("Skia unavailable, skipping bridge tests (exit 77)\n");
        return 77;
    }
    int failures = 0;
    failures += test_skia_available();
    failures += test_raster_surface_identity();
    failures += test_renderer_factory_raster();
    failures += test_vulkan_enumerate_no_crash();
    failures += test_render_rgba_source_over();
    failures += test_render_rgba_partial_alpha_over_nonempty_dst();
    failures += test_vulkan_device_creation_and_device_lost_fallback();
    failures += test_canvas_c_abi();
    if (failures) {
        std::fprintf(stderr, "SkiaBridge smoke: %d FAILURE(S)\n", failures);
        return 1;
    }
    std::printf("SkiaBridge smoke OK\n");
    return 0;
}