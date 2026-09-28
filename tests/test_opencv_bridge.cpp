// test_opencv_bridge.cpp — Stage 10 verification test.
// Verifies deterministic execution of content-aware inpaint and guided filter.

#include "OpenCVBridge.h"
#include <cassert>
#include <cstdio>
#include <cstring>
#include <vector>
#include <cmath>

static int g_failures = 0;
#define CHECK(cond, msg) do { if (!(cond)) { std::fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__); ++g_failures; } } while (0)

static void test_invalid_arguments() {
    int rc = compositor_content_aware_fill(nullptr, 0, nullptr, 0, 0, 0);
    CHECK(rc == -1, "reject null and zero sizes in content_aware_fill");

    rc = compositor_guided_filter(nullptr, nullptr, nullptr, 0, 0, 0, 1e-4f);
    CHECK(rc == -1, "reject null and zero sizes in guided_filter");
}

static void test_content_aware_fill_basic() {
    // 16x16 image with an opaque pattern and a 2x2 hole in the center
    const int w = 16, h = 16;
    std::vector<uint8_t> rgba(w * h * 4, 0);
    std::vector<uint8_t> mask(w * h, 0);

    for (int y = 0; y < h; ++y) {
        for (int x = 0; x < w; ++x) {
            int idx = (y * w + x) * 4;
            rgba[idx + 0] = 200; // Reddish pattern
            rgba[idx + 1] = 50;
            rgba[idx + 2] = 50;
            rgba[idx + 3] = 255;
        }
    }

    // Set center 2x2 hole in mask
    for (int y = 7; y <= 8; ++y) {
        for (int x = 7; x <= 8; ++x) {
            mask[y * w + x] = 255;
            int idx = (y * w + x) * 4;
            rgba[idx + 0] = 0;
            rgba[idx + 1] = 0;
            rgba[idx + 2] = 0;
            rgba[idx + 3] = 0;
        }
    }

    int rc = compositor_content_aware_fill(rgba.data(), w * 4, mask.data(), w, w, h);
    CHECK(rc == 1, "content aware fill succeeded");

    // Check that center hole now has non-zero pixels
    for (int y = 7; y <= 8; ++y) {
        for (int x = 7; x <= 8; ++x) {
            int idx = (y * w + x) * 4;
            CHECK(rgba[idx + 3] > 0, "inpainted pixel alpha restored");
            CHECK(rgba[idx + 0] > 0, "inpainted pixel color synthesized");
        }
    }
}

static void test_guided_filter_identity_and_bounds() {
    const int w = 4, h = 4;
    std::vector<float> guide = {
        0.1f, 0.2f, 0.3f, 0.4f,
        0.5f, 0.6f, 0.7f, 0.8f,
        0.2f, 0.3f, 0.4f, 0.5f,
        0.6f, 0.7f, 0.8f, 0.9f
    };
    std::vector<float> source = guide;
    std::vector<float> output(w * h, 0.0f);

    // Radius 0 is exact identity
    int rc = compositor_guided_filter(guide.data(), source.data(), output.data(), w, h, 0, 1e-4f);
    CHECK(rc == 0, "guided filter radius 0 rc == 0");
    for (size_t i = 0; i < guide.size(); ++i) {
        CHECK(std::abs(output[i] - source[i]) < 1e-6f, "radius 0 identity");
    }

    // Radius 1 with uniform source produces uniform output
    std::vector<float> uniform_source(w * h, 0.5f);
    rc = compositor_guided_filter(guide.data(), uniform_source.data(), output.data(), w, h, 1, 1e-4f);
    CHECK(rc == 0, "guided filter uniform rc == 0");
    for (size_t i = 0; i < guide.size(); ++i) {
        CHECK(std::abs(output[i] - 0.5f) < 1e-4f, "uniform output remains uniform");
        CHECK(output[i] >= 0.0f && output[i] <= 1.0f, "clamped in [0, 1]");
    }
}

int main() {
    std::printf("OpenCV bridge active=%d\n", compositor_has_opencv());
    test_invalid_arguments();
    test_content_aware_fill_basic();
    test_guided_filter_identity_and_bounds();

    if (g_failures != 0) {
        std::fprintf(stderr, "test_opencv_bridge: %d FAILURE(S)\n", g_failures);
        return 1;
    }
    std::printf("test_opencv_bridge: ALL TESTS PASSED\n");
    return 0;
}
