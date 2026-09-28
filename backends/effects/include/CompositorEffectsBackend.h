#ifndef COMPOSITOR_EFFECTS_BACKEND_H
#define COMPOSITOR_EFFECTS_BACKEND_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

// Layer effects (stroke, drop shadow, colour overlay, inner shadow) over one premultiplied RGBA8 image: the nine passes
// of upstream's Metal renderer (alpha, spread rows/columns, ring, shift, blur rows/columns, inside, compose), with no
// platform types. `color` is straight rgb in 0...1 and `opacity` is the effect's own opacity.
typedef struct { float r, g, b, opacity; } CompositorEffectColor;

typedef struct CompositorEffectsParams {
    uint32_t width, height;
    int32_t has_stroke, stroke_inside;
    uint32_t stroke_reach;                      // whole pixels, at least 1
    CompositorEffectColor stroke;
    int32_t has_shadow;
    float shadow_dx, shadow_dy, shadow_sigma;   // sigma <= 0.01 means no blur
    CompositorEffectColor shadow;
    int32_t has_overlay;
    CompositorEffectColor overlay;
    int32_t has_inner;
    float inner_dx, inner_dy, inner_sigma;
    CompositorEffectColor inner;
    int32_t has_outer;
    float outer_sigma;                          // outer glow's blur; upstream: OuterGlowEffect.size / 2
    CompositorEffectColor outer;
} CompositorEffectsParams;

// The OpenCV tier: the heavy passes as OpenCV operators on float planes. -2 when the build has no OpenCV.
int compositor_effects_opencv(const CompositorEffectsParams *params, const uint8_t *pixels, uint8_t *out);

typedef struct CompositorVulkanEffects CompositorVulkanEffects;

// `pixels` and `out` are width*height*4 bytes, disjoint. Return 0 on success, -1 invalid input, -2 backend
// unavailable or failed at run time (`out` is then untouched, so the caller can try the next backend).
int compositor_effects_cpu(const CompositorEffectsParams *params, const uint8_t *pixels, uint8_t *out);

CompositorVulkanEffects *compositor_vulkan_effects_create(void);
void compositor_vulkan_effects_destroy(CompositorVulkanEffects *context);
int compositor_vulkan_effects_render(CompositorVulkanEffects *context, const CompositorEffectsParams *params,
                                     const uint8_t *pixels, uint8_t *out);
const char *compositor_vulkan_effects_device(CompositorVulkanEffects *context);
// VkPhysicalDeviceType: 1 integrated, 2 discrete, 3 virtual, 4 CPU (a software rasterizer such as llvmpipe).
uint32_t compositor_vulkan_effects_device_type(CompositorVulkanEffects *context);

#ifdef __cplusplus
}
#endif
#endif
