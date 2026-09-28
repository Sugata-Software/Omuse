#ifndef COMPOSITOR_BRUSH_BACKEND_H
#define COMPOSITOR_BRUSH_BACKEND_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
// Matches the shader's four 16-byte push-constant groups; no platform types.
typedef struct {
    float a, b, c, d;
    float origin_x, origin_y, radius, hardness;
    float canvas_width, canvas_height, antialias_width, spacing;
    uint32_t width, height, settled_count, segment_count;
} CompositorBrushUniforms;
typedef struct { float x0, y0, x1, y1; } CompositorBrushSegment;
typedef struct CompositorVulkanBrush CompositorVulkanBrush;
// Each call processes one <=256x256 tile and <=2048 centerline segments.
// Input/output permanent arrays have width*height floats; preview has that many
// bytes. Outputs are disjoint from input. Return 0 on success, -1 invalid input,
// -2 unavailable/runtime failure. Failure leaves caller output untouched.
int compositor_brush_cpu(const CompositorBrushUniforms *uniforms,
    const CompositorBrushSegment *segments, size_t segment_count,
    const float *permanent, size_t pixel_count, float *next, uint8_t *preview);
CompositorVulkanBrush *compositor_vulkan_brush_create(void);
void compositor_vulkan_brush_destroy(CompositorVulkanBrush *context);
int compositor_vulkan_brush_render(CompositorVulkanBrush *context,
    const CompositorBrushUniforms *uniforms,
    const CompositorBrushSegment *segments, size_t segment_count,
    const float *permanent, size_t pixel_count, float *next, uint8_t *preview);
// Immutable diagnostics, valid until context destruction.
const char *compositor_vulkan_brush_device(CompositorVulkanBrush *context);
uint32_t compositor_vulkan_brush_device_type(CompositorVulkanBrush *context);
uint32_t compositor_vulkan_brush_driver_version(CompositorVulkanBrush *context);
#ifdef __cplusplus
}
#endif
#endif
