// Linux smoke tests for the shared upstream C pixel kernels, including their
// padded-row stride contract. These do not re-derive each kernel's full numerics.

// The C kernel headers are pure C (no extern "C" guards — that C++-safety
// belongs to ENG-2's Qt-host integration). Wrap them here so C++ calls bind
// to the C linkage of libCompositorKernels.
extern "C" {
#include "AdjustPixels.h"
#include "BrushPixels.h"
#include "ContentFill.h"
#include "HealPixels.h"
#include "LensPixels.h"
#include "LevelsPixels.h"
#include "NoisePixels.h"
#include "WandPixels.h"
}

#include <cassert>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>

static int g_failures = 0;

#define CHECK(cond, msg) do { \
    if (!(cond)) { \
        std::fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__); \
        g_failures++; \
    } \
} while (0)

// ---- lens_distort: k=0 copies the source exactly (bilinear of an identical grid). ----
static void test_lens_identity() {
    uint8_t src[12] = {10, 20, 30, 255,  40, 50, 60, 255,  70, 80, 90, 255};
    uint8_t dst[12] = {0};
    lens_distort(src, dst, 3, 1, 12, 0.0);
    CHECK(std::memcmp(src, dst, 12) == 0, "lens k=0 identity");
}

// ---- noise_add: same seed reproduces, alpha untouched, transparent pixels untouched. ----
static void test_noise_determinism() {
    uint8_t a[16] = {100, 110, 120, 200,  0, 0, 0, 0,  50, 60, 70, 255,  10, 20, 30, 128};
    uint8_t b[16];
    std::memcpy(b, a, 16);
    uint8_t alpha_a[4] = {a[3], a[7], a[11], a[15]};
    noise_add(a, 4, 1, 16, 50.0f, 0, 0, 42);
    noise_add(b, 4, 1, 16, 50.0f, 0, 0, 42);
    CHECK(std::memcmp(a, b, 16) == 0, "noise same seed reproduces");
    CHECK(a[3] == alpha_a[0] && a[11] == alpha_a[2] && a[15] == alpha_a[3], "noise preserves alpha");
    CHECK(a[4] == 0 && a[5] == 0 && a[6] == 0 && a[7] == 0, "noise leaves transparent pixel untouched");
}

// ---- brush_alpha_bounds: half-open bounds of nonzero alpha. ----
static void test_brush_alpha_bounds() {
    // 4x4, opaque 2x2 block at x in [1,2], y in [1,2].
    uint8_t px[64] = {0};
    for (size_t y = 1; y <= 2; ++y)
        for (size_t x = 1; x <= 2; ++x)
            px[y * 16 + x * 4 + 3] = 255;
    size_t b[4];
    brush_alpha_bounds(px, 4, 4, 16, b);
    CHECK(b[0] == 1 && b[1] == 1 && b[2] == 3 && b[3] == 3, "brush_alpha_bounds half-open block");
}

// ---- wand_mask: contiguous flood fill over a solid-color image selects all. ----
static void test_wand_contiguous() {
    // 3x3 opaque white, premultiplied.
    uint8_t px[36];
    for (int i = 0; i < 9; ++i) { px[i*4]=255; px[i*4+1]=255; px[i*4+2]=255; px[i*4+3]=255; }
    uint8_t mask[9] = {0};
    long count = wand_mask(px, 3, 3, 12, 1, 1, 0, 255, 1, mask);
    CHECK(count == 9, "wand contiguous selects all 9");
    for (int i = 0; i < 9; ++i) CHECK(mask[i] == 255, "wand mask fully set");
}

// ---- levels_apply: identity table (table[c][i] = i/255) preserves opaque pixels. ----
static void test_levels_identity() {
    float tables[3 * 256];
    for (int c = 0; c < 3; ++c)
        for (int i = 0; i < 256; ++i) tables[c * 256 + i] = (float)i / 255.0f;
    uint8_t px[4] = {10, 200, 77, 255};
    levels_apply(px, 1, tables);
    CHECK(px[0] == 10 && px[1] == 200 && px[2] == 77 && px[3] == 255, "levels identity preserves opaque");
}

// ---- heal_coverage_bounds: half-open bounds of nonzero gray. ----
static void test_heal_coverage_bounds() {
    uint8_t gray[25] = {0};
    gray[1 * 5 + 1] = 128; gray[1 * 5 + 2] = 200; gray[2 * 5 + 1] = 10; gray[2 * 5 + 2] = 255;
    long b[4];
    heal_coverage_bounds(gray, 5, 5, 5, b);
    CHECK(b[0] == 1 && b[1] == 1 && b[2] == 3 && b[3] == 3, "heal_coverage_bounds half-open");
}

// ---- rgba_clamp_premultiplied: channel > alpha is clamped to alpha. ----
static void test_clamp_premultiplied() {
    uint8_t px[4] = {200, 100, 50, 128};
    rgba_clamp_premultiplied(px, 1);
    CHECK(px[0] == 128, "clamp r to alpha");
    CHECK(px[1] == 100, "g unchanged");
    CHECK(px[2] == 50, "b unchanged");
    CHECK(px[3] == 128, "alpha unchanged");
}

// ---- layer_extract_alpha / layer_restore_alpha round-trip the alpha channel. ----
static void test_layer_alpha_roundtrip() {
    uint8_t rgba[16] = {10, 20, 30, 128,  40, 50, 60, 200,  70, 80, 90, 255,  1, 2, 3, 0};
    uint8_t alpha[4] = {0};
    uint8_t restored[16];
    std::memcpy(restored, rgba, 16);
    layer_extract_alpha(rgba, 16, alpha, 4, 4, 1);
    CHECK(alpha[0] == 128 && alpha[1] == 200 && alpha[2] == 255 && alpha[3] == 0, "extract alpha");
    // layer_restore_alpha premultiplies by the extracted alpha; for a pixel whose
    // stored alpha already equals the extracted alpha, opaque pixels are preserved.
    uint8_t opq[8] = {10, 20, 30, 255, 40, 50, 60, 255};
    uint8_t opa[2] = {255, 255};
    layer_restore_alpha(opq, 8, opa, 2, 2, 1);
    CHECK(opq[0] == 10 && opq[3] == 255, "restore alpha opaque preserved");
}

// The shared upstream kernels support padded rows. Check their public stride
// contract rather than the removed Linux-only canonical-stride assertion.
static void test_padded_stride_preserves_pixels_and_padding() {
    const uint8_t source[24] = {
        10, 20, 30, 255, 40, 50, 60, 255, 99, 99, 99, 99,
        70, 80, 90, 255, 11, 22, 33, 255, 99, 99, 99, 99
    };
    uint8_t output[24];
    std::memset(output, 0xA5, sizeof(output));
    lens_distort(source, output, 2, 2, 12, 0.0);
    for (int row = 0; row < 2; ++row) {
        CHECK(std::memcmp(source + row * 12, output + row * 12, 8) == 0,
              "padded identity preserves visible pixels");
        for (int column = 8; column < 12; ++column)
            CHECK(output[row * 12 + column] == 0xA5, "padded identity leaves padding untouched");
    }
}

int main() {
    test_lens_identity();
    test_noise_determinism();
    test_brush_alpha_bounds();
    test_wand_contiguous();
    test_levels_identity();
    test_heal_coverage_bounds();
    test_clamp_premultiplied();
    test_layer_alpha_roundtrip();
    test_padded_stride_preserves_pixels_and_padding();

    if (g_failures == 0) {
        std::printf("All Compositor kernel tests passed.\n");
        return 0;
    }
    std::fprintf(stderr, "%d kernel test(s) failed.\n", g_failures);
    return 1;
}