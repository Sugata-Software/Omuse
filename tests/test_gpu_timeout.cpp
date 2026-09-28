#include "CompositorBrushBackend.h"
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <vector>

#define REQUIRE(condition) do { \
    if (!(condition)) { std::fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #condition); return 1; } \
} while (false)

int main() {
    auto waits = reinterpret_cast<int(*)()>(dlsym(RTLD_DEFAULT, "compositor_test_waits"));
    auto destroys = reinterpret_cast<int(*)()>(dlsym(RTLD_DEFAULT, "compositor_test_destroys"));
    auto idleWaits = reinterpret_cast<int(*)()>(dlsym(RTLD_DEFAULT, "compositor_test_idle_waits"));
    REQUIRE(waits && destroys && idleWaits);
    auto *gpu = compositor_vulkan_brush_create();
    if (!gpu) return 77;

    CompositorBrushUniforms uniforms{1, 0, 0, 1, 0, 0, 8, .5f, 32, 32, 1, .4f, 32, 32, 0, 1};
    CompositorBrushSegment segment{8, 8, 24, 24};
    std::vector<float> input(1024), next(1024, -123);
    std::vector<uint8_t> preview(1024, 17);
    auto render = [&] {
        return compositor_vulkan_brush_render(gpu, &uniforms, &segment, 1,
            input.data(), input.size(), next.data(), preview.data());
    };
    REQUIRE(render() == -2);
    REQUIRE(std::all_of(next.begin(), next.end(), [](float value) { return value == -123; }));
    REQUIRE(std::all_of(preview.begin(), preview.end(), [](uint8_t value) { return value == 17; }));
    REQUIRE(waits() == 1);
    // A failed context stays disabled; it must not submit a second tile.
    REQUIRE(render() == -2 && waits() == 1);
    REQUIRE(compositor_brush_cpu(&uniforms, &segment, 1, input.data(), input.size(),
                                  next.data(), preview.data()) == 0);
    REQUIRE(*std::max_element(preview.begin(), preview.end()) > 0);

    compositor_vulkan_brush_destroy(gpu);
    const char *outcome = std::getenv("COMPOSITOR_TEST_FENCE");
    const int expected = outcome && !std::strcmp(outcome, "completed") ? 1 : 0;
    REQUIRE(destroys() == expected);
    REQUIRE(idleWaits() == 0);
    std::puts("GPU timeout: unchanged outputs, disabled retry, CPU fallback, safe teardown");
}
