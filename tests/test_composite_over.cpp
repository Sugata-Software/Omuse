// Unit tests for the Compositor C ABI seam (C++ side).
// Builds and runs on this host with g++; links against the C reference
// implementation (tests/ref/compositor_composite_over.c), which mirrors the
// Swift @_cdecl export. The Swift export itself is verified under the
// Freedesktop Swift runtime extension (not on this host).
//
// Build: see CMakeLists.txt (cmake --build) or the g++ one-liner in README.

#include "CompositorCore.h"
#include "../shim/CompositorCoreShim.hpp"

#include <cassert>
#include <cstdio>
#include <cstring>

static int g_failures = 0;

#define CHECK(cond, msg) do { \
    if (!(cond)) { \
        std::fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__); \
        g_failures++; \
    } \
} while (0)

// Premultiplied source-over: opaque red over transparent black -> opaque red.
static void test_opaque_over_transparent() {
    uint8_t dst[4] = {0, 0, 0, 0};
    uint8_t src[4] = {255, 0, 0, 255};
    int rc = compositor_composite_over(dst, src, nullptr, 1, 1, 4, 1.0f);
    CHECK(rc == 0, "opaque-over-transparent rc");
    CHECK(dst[0] == 255 && dst[1] == 0 && dst[2] == 0 && dst[3] == 255,
          "opaque-over-transparent result");
}

// Premultiplied source-over: semi-transparent red over opaque blue.
//   src=(128,0,0,128) premult, dst=(0,0,255,255), op=1.
//   effective_alpha = 128/255 = 0.50196, inverse = 0.49804.
//   out_r = 128*1 + 0*0.498 = 128.
//   out_g = 0.
//   out_b = 0*1 + 255*0.498 = 126.99 -> 127.
//   out_a = 128*1 + 255*0.498 = 254.99 -> 255.
static void test_semitransparent_over_opaque() {
    uint8_t dst[4] = {0, 0, 255, 255};
    uint8_t src[4] = {128, 0, 0, 128};
    int rc = compositor_composite_over(dst, src, nullptr, 1, 1, 4, 1.0f);
    CHECK(rc == 0, "semi-over-opaque rc");
    CHECK(dst[0] == 128, "semi-over-opaque r");
    CHECK(dst[1] == 0, "semi-over-opaque g");
    CHECK(dst[2] == 127, "semi-over-opaque b");
    CHECK(dst[3] == 255, "semi-over-opaque a");
}

// Coverage mask halves the source contribution.
//   src=(128,0,0,128) premult, dst=(0,0,255,255), coverage=128, op=1.
//   cov = 128/255 = 0.50196.
//   effective_alpha = (128/255) * 0.50196 = 0.25195, inverse = 0.74805.
//   out_r = 128*0.50196 + 0 = 64.25 -> 64.
//   out_b = 0 + 255*0.74805 = 190.75 -> 191.
//   out_a = 128*0.50196 + 255*0.74805 = 64.25 + 190.75 = 255.
static void test_coverage_mask() {
    uint8_t dst[4] = {0, 0, 255, 255};
    uint8_t src[4] = {128, 0, 0, 128};
    uint8_t coverage[1] = {128};
    int rc = compositor_composite_over(dst, src, coverage, 1, 1, 4, 1.0f);
    CHECK(rc == 0, "coverage rc");
    CHECK(dst[0] == 64, "coverage r");
    CHECK(dst[1] == 0, "coverage g");
    CHECK(dst[2] == 191, "coverage b");
    CHECK(dst[3] == 255, "coverage a");
}

// Opacity scales the effective source alpha.
//   src=(255,0,0,255) over dst=(0,0,255,255), op=0.5.
//   effective_alpha = 0.5, inverse = 0.5.
//   out_r = 255*0.5 + 0 = 127.5 -> 128.
//   out_b = 0 + 255*0.5 = 127.5 -> 128.
//   out_a = 255*0.5 + 255*0.5 = 255.
static void test_opacity() {
    uint8_t dst[4] = {0, 0, 255, 255};
    uint8_t src[4] = {255, 0, 0, 255};
    int rc = compositor_composite_over(dst, src, nullptr, 1, 1, 4, 0.5f);
    CHECK(rc == 0, "opacity rc");
    CHECK(dst[0] == 128, "opacity r");
    CHECK(dst[2] == 128, "opacity b");
    CHECK(dst[3] == 255, "opacity a");
}

// Stride padding: a 1-pixel-wide tile with stride 8 still composites correctly.
static void test_stride_padding() {
    uint8_t dst[8] = {0, 0, 0, 0, 0, 0, 0, 0};
    uint8_t src[8] = {10, 20, 30, 255, 0, 0, 0, 0};
    int rc = compositor_composite_over(dst, src, nullptr, 1, 1, 8, 1.0f);
    CHECK(rc == 0, "stride-padding rc");
    CHECK(dst[0] == 10 && dst[1] == 20 && dst[2] == 30 && dst[3] == 255,
          "stride-padding result");
}

// ENG-15: invalid geometry is rejected, not passed to the kernel.
static void test_invalid_geometry() {
    uint8_t buf[4] = {0};
    // zero width
    CHECK(compositor_composite_over(buf, buf, nullptr, 0, 1, 4, 1.0f) == -1, "zero width");
    // zero height
    CHECK(compositor_composite_over(buf, buf, nullptr, 1, 0, 4, 1.0f) == -1, "zero height");
    // stride too small
    CHECK(compositor_composite_over(buf, buf, nullptr, 2, 1, 4, 1.0f) == -1, "stride < width*4");
    // NULL dst
    CHECK(compositor_composite_over(nullptr, buf, nullptr, 1, 1, 4, 1.0f) == -1, "null dst");
    // NULL src
    CHECK(compositor_composite_over(buf, nullptr, nullptr, 1, 1, 4, 1.0f) == -1, "null src");
}

// The C++ shim validates and forwards, returning the typed Status.
static void test_shim_wrapper() {
    using namespace compositor;
    uint8_t dst[4] = {0, 0, 0, 0};
    uint8_t src[4] = {255, 0, 0, 255};
    Status rc = composite_over_checked(BufferView{dst}, ConstBufferView{src},
                                       ConstCoverage{nullptr}, 1, 1, 4, 1.0f);
    CHECK(rc == Status::Ok, "shim ok");
    CHECK(dst[0] == 255, "shim result");
    // shim rejects bad geometry too (defense in depth)
    Status bad = composite_over_checked(BufferView{dst}, ConstBufferView{src},
                                          ConstCoverage{nullptr}, 2, 1, 4, 1.0f);
    CHECK(bad == Status::InvalidArgument, "shim rejects bad geometry");
}

// Opacity out of range is clamped, not rejected.
static void test_opacity_clamp() {
    uint8_t dst[4] = {0, 0, 0, 0};
    uint8_t src[4] = {255, 0, 0, 255};
    CHECK(compositor_composite_over(dst, src, nullptr, 1, 1, 4, 5.0f) == 0, "opacity>1 clamped");
    CHECK(dst[0] == 255, "opacity>1 clamped result");
}

int main() {
    test_opaque_over_transparent();
    test_semitransparent_over_opaque();
    test_coverage_mask();
    test_opacity();
    test_stride_padding();
    test_invalid_geometry();
    test_shim_wrapper();
    test_opacity_clamp();

    if (g_failures == 0) {
        std::printf("All CompositorCore ABI tests passed.\n");
        return 0;
    }
    std::fprintf(stderr, "%d test(s) failed.\n", g_failures);
    return 1;
}