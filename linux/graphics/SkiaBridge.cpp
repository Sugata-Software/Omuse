// SkiaBridge.cpp — C ABI bridge between the Swift core and Skia render backends
// (plan §9, Stage 5/8). Implements CompRenderer for Raster (CPU) and Vulkan backends.
//
// The Swift core owns document pixels (PortableImage/PixelBuffer, CPU-addressable).
// The Skia backends own the SkCanvas/SkSurface and the GPU device.
// Swift calls through this C ABI to render a CPU RGBA8 buffer into a destination
// buffer of the canonical contract (premultiplied RGBA8, stride == width*4).
//
// Backend contract (plan §6):
//   RendererDeviceFactory.create() tries Vulkan, falls back to Raster.
//   Both backends implement the same render contract (LSP):
//     - compositor_render_rgba(src, dst, w, h) composites src over dst
//       at opacity 1.0, source-over, returning the result in dst.
//     - A backend that cannot initialize returns NULL; the caller uses
//       the existing pure-Swift DocumentRenderer as the ultimate fallback.
//
// Status codes: 0 success, -1 invalid argument, -2 backend lost (try Raster).

#include "SkiaBridge.h"
#include "CompositorEffectsBackend.h"
#include <stdlib.h>
#include <string.h>
#include <algorithm>
#include <array>
#include <iostream>
#include <vector>

// Include Skia headers — search in Skia source include/core layout or system /app layout.
#if defined(__has_include)
#if __has_include("include/core/SkCanvas.h")
#define COMPOSITOR_HAS_SKIA 1
#include "include/core/SkCanvas.h"
#include "include/core/SkSurface.h"
#include "include/core/SkImage.h"
#include "include/core/SkPixmap.h"
#include "include/core/SkPaint.h"
#include "include/core/SkBlendMode.h"
#include "include/core/SkSamplingOptions.h"
#include "include/core/SkPath.h"
#include "include/core/SkPathBuilder.h"
#include "include/core/SkShader.h"
#include "include/core/SkMatrix.h"
#include "include/core/SkColor.h"
#include "include/core/SkPathUtils.h"
#include "include/core/SkPathIter.h"
#include "include/core/SkRRect.h"
#include "include/core/SkRegion.h"
#include "include/core/SkColorFilter.h"
#include "include/effects/SkImageFilters.h"
#include "include/effects/SkColorMatrix.h"
#include "include/pathops/SkPathOps.h"
#include "include/effects/SkGradient.h"
#elif __has_include(<skia/core/SkCanvas.h>)
#define COMPOSITOR_HAS_SKIA 1
#include <skia/core/SkCanvas.h>
#include <skia/core/SkSurface.h>
#include <skia/core/SkImage.h>
#include <skia/core/SkPixmap.h>
#include <skia/core/SkPaint.h>
#include <skia/core/SkBlendMode.h>
#include <skia/core/SkSamplingOptions.h>
#include <skia/core/SkPath.h>
#include <skia/core/SkPathBuilder.h>
#include <skia/core/SkShader.h>
#include <skia/core/SkMatrix.h>
#include <skia/core/SkColor.h>
#include <skia/core/SkPathUtils.h>
#include <skia/core/SkPathIter.h>
#include <skia/core/SkRRect.h>
#include <skia/core/SkRegion.h>
#include <skia/core/SkColorFilter.h>
#include <skia/effects/SkImageFilters.h>
#include <skia/effects/SkColorMatrix.h>
#include <skia/pathops/SkPathOps.h>
#include <skia/effects/SkGradient.h>
#endif
#if __has_include(<vulkan/vulkan.h>)
#define COMPOSITOR_HAS_VULKAN 1
#include <vulkan/vulkan.h>
#endif
#endif

// ── CompRenderer ────────────────────────────────────────────────────────

// Opaque handle owning one Skia backend context.
struct CompRenderer {
    void* ctx = nullptr;

#if defined(COMPOSITOR_HAS_VULKAN)
    VkInstance vk_instance = VK_NULL_HANDLE;
    VkPhysicalDevice vk_physical_device = VK_NULL_HANDLE;
    VkDevice vk_device = VK_NULL_HANDLE;
    VkQueue vk_queue = VK_NULL_HANDLE;
    uint32_t vk_queue_family = 0;
    VkFormat vk_format = VK_FORMAT_R8G8B8A8_UNORM;
#endif

    bool is_raster = false;
    bool is_device_lost = false;
    CompRendererKind last_executed = COMP_RENDERER_RASTER;

    CompRenderer() = default;
    ~CompRenderer() = default;
};

// ── Forward declarations ────────────────────────────────────────────────

static void raster_device_create(CompRenderer* r, int width, int height);
static void raster_device_destroy(CompRenderer* r);
static int raster_render_rgba(CompRenderer* r,
                              const uint8_t* src_rgba,
                              uint8_t* dst_rgba,
                              size_t width, size_t height);

static void vulkan_device_create(CompRenderer* r, int force_raster);
static void vulkan_device_destroy(CompRenderer* r);
static int vulkan_render_rgba(CompRenderer* r,
                              const uint8_t* src_rgba,
                              uint8_t* dst_rgba,
                              size_t width, size_t height);

// ── Availability and Kind Queries ──────────────────────────────────────

int compositor_skia_available(void) {
#if defined(COMPOSITOR_HAS_SKIA)
    return 1;
#else
    return 0;
#endif
}

CompRendererKind compositor_renderer_last_executed(const CompRenderer *renderer) {
    if (!renderer) return COMP_RENDERER_RASTER;
    return renderer->last_executed;
}

// ── compositor_renderer_create ─────────────────────────────────────────

CompRenderer *compositor_renderer_create(int force_raster, CompRendererKind *kind_out) {
    CompRenderer* r = new (std::nothrow) CompRenderer();
    if (!r) return nullptr;

    if (force_raster) {
        raster_device_create(r, 0, 0);
        if (kind_out) *kind_out = COMP_RENDERER_RASTER;
        return r;
    }

    // Try Vulkan first.
    vulkan_device_create(r, force_raster);
    if (r->vk_device != VK_NULL_HANDLE) {
        if (kind_out) *kind_out = COMP_RENDERER_VULKAN;
        return r;
    }

    // Vulkan failed — fall back to Raster.
    raster_device_create(r, 0, 0);
    if (kind_out) *kind_out = COMP_RENDERER_RASTER;
    return r;
}

// ── compositor_renderer_close ──────────────────────────────────────────

void compositor_renderer_close(CompRenderer *renderer) {
    if (!renderer) return;
    CompRenderer* r = (CompRenderer*)renderer;
    if (r->is_raster) {
        raster_device_destroy(r);
    } else {
        vulkan_device_destroy(r);
    }
    delete r;
}

// ── compositor_renderer_kind ───────────────────────────────────────────

CompRendererKind compositor_renderer_kind(const CompRenderer *renderer) {
    if (!renderer) return COMP_RENDERER_RASTER;
    const CompRenderer* r = (const CompRenderer*)renderer;
    return r->is_raster ? COMP_RENDERER_RASTER : COMP_RENDERER_VULKAN;
}

void compositor_renderer_simulate_device_lost(CompRenderer *renderer) {
    if (renderer) {
        renderer->is_device_lost = true;
    }
}

// ── compositor_renderer_activate / deactivate ──────────────────────────

void compositor_renderer_activate(CompRenderer *renderer) {
    if (!renderer) return;
    // No-op on the C side; the Swift shim (CGContextCompat) checks
    // compositor_compat_set_render_fn which the host sets once at startup.
    // Activation is a signal for the Swift side to route through Skia.
    (void)renderer;
}

void compositor_renderer_deactivate(void) {
    // Swift side will fall back to pure-Swift LayerRenderer on device loss.
    (void)0;
}

// ── compositor_render_rgba ─────────────────────────────────────────────

int compositor_render_rgba(CompRenderer *renderer,
                           const uint8_t *src_rgba,
                           uint8_t *dst_rgba,
                           size_t width, size_t height) {
    if (!renderer || !src_rgba || !dst_rgba) return -1;
    if (width == 0 || height == 0) return -1;

#if !defined(COMPOSITOR_HAS_SKIA)
    return -3;
#else
    CompRenderer* r = (CompRenderer*)renderer;
    if (r->is_raster) {
        return raster_render_rgba(r, src_rgba, dst_rgba, width, height);
    } else {
        int rc = vulkan_render_rgba(r, src_rgba, dst_rgba, width, height);
        if (rc == -2) {
            // Plan §6 runtime loss failsafe: automatic dynamic fallback to Raster CPU
            vulkan_device_destroy(r);
            raster_device_create(r, static_cast<int>(width), static_cast<int>(height));
            rc = raster_render_rgba(r, src_rgba, dst_rgba, width, height);
        }
        return rc;
    }
#endif
}

// ── compositor_skia_raster_surface ─────────────────────────────────────

int compositor_skia_raster_surface(const uint8_t *src_rgba,
                                   uint8_t *dst_rgba,
                                   size_t width, size_t height) {
    if (!src_rgba || !dst_rgba) return -1;
    if (width == 0 || height == 0) return -1;

#if !defined(COMPOSITOR_HAS_SKIA)
    return -3;
#else
    const size_t stride = width * 4;
    SkImageInfo info = SkImageInfo::Make(static_cast<int>(width),
                                         static_cast<int>(height),
                                         kRGBA_8888_SkColorType,
                                         kPremul_SkAlphaType);

    SkPixmap srcPixmap(info, src_rgba, stride);
    sk_sp<SkImage> srcImage = SkImages::RasterFromPixmap(srcPixmap, nullptr, nullptr);
    if (!srcImage) return -1;

    sk_sp<SkSurface> surface = SkSurfaces::WrapPixels(info, dst_rgba, stride);
    if (!surface) return -1;

    SkCanvas* canvas = surface->getCanvas();
    if (!canvas) return -1;

    SkPaint paint;
    paint.setBlendMode(SkBlendMode::kSrcOver);
    canvas->drawImage(srcImage, 0, 0, SkSamplingOptions(), &paint);
    return 0;
#endif
}

// ── compositor_vulkan_enumerate_devices ────────────────────────────────

int compositor_vulkan_enumerate_devices(void) {
#if defined(COMPOSITOR_HAS_VULKAN)
    VkApplicationInfo appInfo{};
    appInfo.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    appInfo.pApplicationName = "Compositor";
    appInfo.applicationVersion = VK_MAKE_VERSION(1, 0, 0);
    appInfo.apiVersion = VK_API_VERSION_1_0;

    VkInstanceCreateInfo createInfo{};
    createInfo.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    createInfo.pApplicationInfo = &appInfo;

    VkInstance instance = VK_NULL_HANDLE;
    VkResult res = vkCreateInstance(&createInfo, nullptr, &instance);
    if (res != VK_SUCCESS || instance == VK_NULL_HANDLE) {
        return -1;
    }

    uint32_t device_count = 0;
    res = vkEnumeratePhysicalDevices(instance, &device_count, nullptr);
    vkDestroyInstance(instance, nullptr);

    if (res == VK_SUCCESS) {
        return (int)device_count;
    }
    return -1;
#else
    return -1;
#endif
}

// ── Internal: raster device ────────────────────────────────────────────

static void raster_device_create(CompRenderer* r, int width, int height) {
    (void)width; (void)height;
    r->is_raster = true;
    r->last_executed = COMP_RENDERER_RASTER;
}

static void raster_device_destroy(CompRenderer* r) {
    r->is_raster = false;
}

static int raster_render_rgba(CompRenderer* r,
                              const uint8_t* src_rgba,
                              uint8_t* dst_rgba,
                              size_t width, size_t height) {
    if (!r || !r->is_raster) return -1;
    if (!src_rgba || !dst_rgba) return -1;
    if (width == 0 || height == 0) return -1;

#if defined(COMPOSITOR_HAS_SKIA)
    r->last_executed = COMP_RENDERER_RASTER;
    const size_t stride = width * 4;
    SkImageInfo info = SkImageInfo::Make(static_cast<int>(width),
                                         static_cast<int>(height),
                                         kRGBA_8888_SkColorType,
                                         kPremul_SkAlphaType);

    SkPixmap srcPixmap(info, src_rgba, stride);
    sk_sp<SkImage> srcImage = SkImages::RasterFromPixmap(srcPixmap, nullptr, nullptr);
    if (!srcImage) return -1;

    sk_sp<SkSurface> surface = SkSurfaces::WrapPixels(info, dst_rgba, stride);
    if (!surface) return -1;

    SkCanvas* canvas = surface->getCanvas();
    if (!canvas) return -1;

    SkPaint paint;
    paint.setBlendMode(SkBlendMode::kSrcOver);
    canvas->drawImage(srcImage, 0, 0, SkSamplingOptions(), &paint);
    return 0;
#else
    return -3;
#endif
}

// ── Internal: Vulkan device ────────────────────────────────────────────

static void vulkan_device_create(CompRenderer* r, int force_raster) {
    if (force_raster) {
        r->is_raster = true;
        r->last_executed = COMP_RENDERER_RASTER;
        return;
    }
#if defined(COMPOSITOR_HAS_VULKAN)
    r->is_raster = false;
    r->is_device_lost = false;
    r->last_executed = COMP_RENDERER_VULKAN;

    VkApplicationInfo appInfo{};
    appInfo.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    appInfo.pApplicationName = "Compositor";
    appInfo.applicationVersion = VK_MAKE_VERSION(1, 0, 0);
    appInfo.apiVersion = VK_API_VERSION_1_0;

    VkInstanceCreateInfo createInfo{};
    createInfo.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    createInfo.pApplicationInfo = &appInfo;

    VkInstance instance = VK_NULL_HANDLE;
    VkResult res = vkCreateInstance(&createInfo, nullptr, &instance);
    if (res != VK_SUCCESS || instance == VK_NULL_HANDLE) {
        r->vk_device = VK_NULL_HANDLE;
        r->is_raster = true;
        r->last_executed = COMP_RENDERER_RASTER;
        return;
    }

    uint32_t device_count = 0;
    res = vkEnumeratePhysicalDevices(instance, &device_count, nullptr);
    if (res != VK_SUCCESS || device_count == 0) {
        vkDestroyInstance(instance, nullptr);
        r->vk_device = VK_NULL_HANDLE;
        r->is_raster = true;
        r->last_executed = COMP_RENDERER_RASTER;
        return;
    }

    std::vector<VkPhysicalDevice> devices(device_count);
    vkEnumeratePhysicalDevices(instance, &device_count, devices.data());

    VkPhysicalDevice chosenDevice = VK_NULL_HANDLE;
    uint32_t chosenQueueFamily = 0;
    bool foundQueue = false;

    for (const auto& dev : devices) {
        uint32_t qfCount = 0;
        vkGetPhysicalDeviceQueueFamilyProperties(dev, &qfCount, nullptr);
        std::vector<VkQueueFamilyProperties> qfProps(qfCount);
        vkGetPhysicalDeviceQueueFamilyProperties(dev, &qfCount, qfProps.data());

        for (uint32_t i = 0; i < qfCount; ++i) {
            if (qfProps[i].queueFlags & VK_QUEUE_GRAPHICS_BIT) {
                chosenDevice = dev;
                chosenQueueFamily = i;
                foundQueue = true;
                break;
            }
        }
        if (foundQueue) break;
    }

    if (!foundQueue) {
        vkDestroyInstance(instance, nullptr);
        r->vk_device = VK_NULL_HANDLE;
        r->is_raster = true;
        r->last_executed = COMP_RENDERER_RASTER;
        return;
    }

    float priority = 1.0f;
    VkDeviceQueueCreateInfo qci{};
    qci.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO;
    qci.queueFamilyIndex = chosenQueueFamily;
    qci.queueCount = 1;
    qci.pQueuePriorities = &priority;

    VkDeviceCreateInfo dci{};
    dci.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO;
    dci.queueCreateInfoCount = 1;
    dci.pQueueCreateInfos = &qci;

    VkDevice device = VK_NULL_HANDLE;
    res = vkCreateDevice(chosenDevice, &dci, nullptr, &device);
    if (res != VK_SUCCESS || device == VK_NULL_HANDLE) {
        vkDestroyInstance(instance, nullptr);
        r->vk_device = VK_NULL_HANDLE;
        r->is_raster = true;
        r->last_executed = COMP_RENDERER_RASTER;
        return;
    }

    r->vk_instance = instance;
    r->vk_physical_device = chosenDevice;
    r->vk_device = device;
    r->vk_queue_family = chosenQueueFamily;
    vkGetDeviceQueue(device, chosenQueueFamily, 0, &r->vk_queue);
    r->is_raster = false;
    r->last_executed = COMP_RENDERER_VULKAN;
#else
    r->is_raster = true;
    r->last_executed = COMP_RENDERER_RASTER;
    (void)r; (void)force_raster;
#endif
}

static void vulkan_device_destroy(CompRenderer* r) {
#if defined(COMPOSITOR_HAS_VULKAN)
    if (r->vk_device != VK_NULL_HANDLE) {
        vkDestroyDevice(r->vk_device, nullptr);
        r->vk_device = VK_NULL_HANDLE;
    }
    if (r->vk_instance != VK_NULL_HANDLE) {
        vkDestroyInstance(r->vk_instance, nullptr);
        r->vk_instance = VK_NULL_HANDLE;
    }
    r->vk_queue = VK_NULL_HANDLE;
    r->vk_physical_device = VK_NULL_HANDLE;
#else
    (void)r;
#endif
}

static int vulkan_render_rgba(CompRenderer* r,
                              const uint8_t* src_rgba,
                              uint8_t* dst_rgba,
                              size_t width, size_t height) {
    (void)r; (void)src_rgba; (void)dst_rgba; (void)width; (void)height;
    // Stage 8 will implement Vulkan rendering via Skia GrDirectContext.
    // Until then, return -2 to trigger dynamic fallback to Raster.
    return -2;
}

// ── Stage 4/5: Canvas C ABI (CoreGraphics-shaped Skia Bridge) ────────────

#if defined(COMPOSITOR_HAS_SKIA)
static SkBlendMode map_cg_blend_mode(int cg_mode) {
    switch (cg_mode) {
        case 0:  return SkBlendMode::kSrcOver;     // normal
        case 1:  return SkBlendMode::kMultiply;    // multiply
        case 2:  return SkBlendMode::kScreen;      // screen
        case 3:  return SkBlendMode::kOverlay;     // overlay
        case 4:  return SkBlendMode::kDarken;      // darken
        case 5:  return SkBlendMode::kLighten;     // lighten
        case 6:  return SkBlendMode::kColorDodge;  // colorDodge
        case 7:  return SkBlendMode::kColorBurn;   // colorBurn
        case 8:  return SkBlendMode::kSoftLight;   // softLight
        case 9:  return SkBlendMode::kHardLight;   // hardLight
        case 10: return SkBlendMode::kDifference;  // difference
        case 11: return SkBlendMode::kExclusion;   // exclusion
        case 12: return SkBlendMode::kHue;         // hue
        case 13: return SkBlendMode::kSaturation;  // saturation
        case 14: return SkBlendMode::kColor;       // color
        case 15: return SkBlendMode::kLuminosity;  // luminosity
        case 16: return SkBlendMode::kClear;       // clear
        case 17: return SkBlendMode::kSrc;         // copy
        case 18: return SkBlendMode::kSrcIn;       // sourceIn
        case 19: return SkBlendMode::kSrcOut;      // sourceOut
        case 20: return SkBlendMode::kSrcATop;     // sourceAtop
        case 21: return SkBlendMode::kDstOver;     // destinationOver
        case 22: return SkBlendMode::kDstIn;       // destinationIn
        case 23: return SkBlendMode::kDstOut;      // destinationOut
        case 24: return SkBlendMode::kDstATop;     // destinationAtop
        case 25: return SkBlendMode::kXor;         // xor
        case 26: return SkBlendMode::kDarken;      // plusDarker
        case 27: return SkBlendMode::kPlus;        // plusLighter
        default: return SkBlendMode::kSrcOver;
    }
}

static SkSamplingOptions map_cg_sampling(int quality) {
    switch (quality) {
        case 1:  // none / nearest
            return SkSamplingOptions(SkFilterMode::kNearest, SkMipmapMode::kNone);
        case 2:  // low
            return SkSamplingOptions(SkFilterMode::kLinear, SkMipmapMode::kNone);
        case 3:  // medium
            return SkSamplingOptions(SkFilterMode::kLinear, SkMipmapMode::kNearest);
        case 4:  // high
            return SkSamplingOptions(SkCubicResampler::CatmullRom());
        default: // default / low
            return SkSamplingOptions(SkFilterMode::kLinear, SkMipmapMode::kNone);
    }
}

struct CompCanvasState {
    float alpha = 1.0f;
    SkBlendMode blendMode = SkBlendMode::kSrcOver;
    SkSamplingOptions sampling = SkSamplingOptions(SkFilterMode::kLinear);
    bool antialias = true;
};

struct CompCanvas {
    sk_sp<SkSurface> surface;
    SkCanvas* canvas = nullptr;
    std::vector<CompCanvasState> states;
    uint8_t* pixels = nullptr;
    size_t width = 0;
    size_t height = 0;
    size_t stride = 0;

    CompCanvasState& current_state() {
        if (states.empty()) states.push_back(CompCanvasState{});
        return states.back();
    }
};

struct CompPath {
    SkPathBuilder builder;
    SkPath get_path(bool even_odd) const {
        SkPath p = builder.snapshot();
        p.setFillType(even_odd ? SkPathFillType::kEvenOdd : SkPathFillType::kWinding);
        return p;
    }
};
#else
struct CompCanvas {};
struct CompPath {};
#endif

CompCanvas *compositor_canvas_create(uint8_t *pixels, size_t width, size_t height, size_t stride) {
    return compositor_canvas_create_ex(pixels, width, height, stride, 0);
}

CompCanvas *compositor_canvas_create_ex(uint8_t *pixels, size_t width, size_t height, size_t stride, int format) {
    if (!pixels || width == 0 || height == 0) return nullptr;
#if defined(COMPOSITOR_HAS_SKIA)
    const bool gray = format == 1;
    if (stride == 0) stride = width * (gray ? 1 : 4);
    SkImageInfo info = gray ? SkImageInfo::Make(static_cast<int>(width), static_cast<int>(height), kGray_8_SkColorType, kOpaque_SkAlphaType)
                            : SkImageInfo::Make(static_cast<int>(width), static_cast<int>(height), kRGBA_8888_SkColorType, kPremul_SkAlphaType);
    sk_sp<SkSurface> surface = SkSurfaces::WrapPixels(info, pixels, stride);
    if (!surface) return nullptr;
    SkCanvas* canvas = surface->getCanvas();
    if (!canvas) return nullptr;

    CompCanvas* c = new (std::nothrow) CompCanvas();
    if (!c) return nullptr;
    c->surface = surface;
    c->canvas = canvas;
    c->pixels = pixels;
    c->width = width;
    c->height = height;
    c->stride = stride;
    c->states.push_back(CompCanvasState{});
    return c;
#else
    (void)stride; (void)format;
    return nullptr;
#endif
}

void compositor_canvas_destroy(CompCanvas *canvas) {
    if (!canvas) return;
    delete canvas;
}

void compositor_canvas_save(CompCanvas *canvas) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    canvas->canvas->save();
    canvas->states.push_back(canvas->current_state());
#else
    (void)canvas;
#endif
}

void compositor_canvas_restore(CompCanvas *canvas) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    canvas->canvas->restore();
    if (canvas->states.size() > 1) {
        canvas->states.pop_back();
    }
#else
    (void)canvas;
#endif
}

void compositor_canvas_translate(CompCanvas *canvas, float dx, float dy) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    canvas->canvas->translate(dx, dy);
#else
    (void)canvas; (void)dx; (void)dy;
#endif
}

void compositor_canvas_scale(CompCanvas *canvas, float sx, float sy) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    canvas->canvas->scale(sx, sy);
#else
    (void)canvas; (void)sx; (void)sy;
#endif
}

void compositor_canvas_rotate(CompCanvas *canvas, float radians) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    float degrees = radians * (180.0f / 3.14159265358979323846f);
    canvas->canvas->rotate(degrees);
#else
    (void)canvas; (void)radians;
#endif
}

void compositor_canvas_concat(CompCanvas *canvas, float a, float b, float c, float d, float tx, float ty) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    SkMatrix m;
    m.setAll(a, c, tx, b, d, ty, 0.0f, 0.0f, 1.0f);
    canvas->canvas->concat(m);
#else
    (void)canvas; (void)a; (void)b; (void)c; (void)d; (void)tx; (void)ty;
#endif
}

void compositor_canvas_get_ctm(const CompCanvas *canvas, float *a, float *b, float *c, float *d, float *tx, float *ty) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) {
        if (a) *a = 1; if (b) *b = 0; if (c) *c = 0; if (d) *d = 1; if (tx) *tx = 0; if (ty) *ty = 0;
        return;
    }
    SkMatrix m = canvas->canvas->getTotalMatrix();
    if (a) *a = m.getScaleX();
    if (b) *b = m.getSkewY();
    if (c) *c = m.getSkewX();
    if (d) *d = m.getScaleY();
    if (tx) *tx = m.getTranslateX();
    if (ty) *ty = m.getTranslateY();
#else
    if (a) *a = 1; if (b) *b = 0; if (c) *c = 0; if (d) *d = 1; if (tx) *tx = 0; if (ty) *ty = 0;
    (void)canvas;
#endif
}

void compositor_canvas_clip_rect(CompCanvas *canvas, float x, float y, float w, float h, int antialias) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    canvas->canvas->clipRect(SkRect::MakeXYWH(x, y, w, h), SkClipOp::kIntersect, antialias != 0);
#else
    (void)canvas; (void)x; (void)y; (void)w; (void)h; (void)antialias;
#endif
}

void compositor_canvas_clip_rect_difference(CompCanvas *canvas, float x, float y, float w, float h) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    canvas->canvas->clipRect(SkRect::MakeXYWH(x, y, w, h), SkClipOp::kDifference, false);
#else
    (void)canvas; (void)x; (void)y; (void)w; (void)h;
#endif
}

void compositor_canvas_clip_path(CompCanvas *canvas, const CompPath *path, int even_odd, int antialias) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas || !path) return;
    SkPath p = path->get_path(even_odd != 0);
    const SkMatrix m = canvas->canvas->getTotalMatrix();
    if (antialias == 0 && !m.rectStaysRect()) {
        // A hard-edged clip under a rotation or skew is rasterized once, against the whole surface, and applied as a
        // pixel region. Left to Skia, the pixels on an edge depend on what is already clipped (it trims the edge to
        // the current clip first, which shifts its fixed-point rounding), so the same path clips a different pixel
        // at a tie the second time. Core Graphics has one answer per path; this gives Skia one too.
        SkPath device = p.makeTransform(m);
        SkRegion whole(SkIRect::MakeWH(static_cast<int>(canvas->width), static_cast<int>(canvas->height)));
        SkRegion region;
        region.setPath(device, whole);
        canvas->canvas->clipRegion(region, SkClipOp::kIntersect);
        return;
    }
    canvas->canvas->clipPath(p, SkClipOp::kIntersect, antialias != 0);
#else
    (void)canvas; (void)path; (void)even_odd; (void)antialias;
#endif
}

void compositor_canvas_clip_mask(CompCanvas *canvas, const uint8_t *mask_pixels, size_t mask_w, size_t mask_h, size_t mask_stride, float x, float y, float w, float h, int is_alpha_only) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas || !mask_pixels || mask_w == 0 || mask_h == 0) return;
    if (mask_stride == 0) mask_stride = is_alpha_only ? mask_w : mask_w * 4;

    SkColorType ct = is_alpha_only ? kAlpha_8_SkColorType : kRGBA_8888_SkColorType;
    SkAlphaType at = is_alpha_only ? kPremul_SkAlphaType : kPremul_SkAlphaType;
    SkImageInfo maskInfo = SkImageInfo::Make(static_cast<int>(mask_w), static_cast<int>(mask_h), ct, at);
    SkPixmap maskPixmap(maskInfo, mask_pixels, mask_stride);
    sk_sp<SkImage> maskImg = SkImages::RasterFromPixmapCopy(maskPixmap);
    if (!maskImg) return;

    SkMatrix m;
    m.setRectToRect(SkRect::MakeWH(mask_w, mask_h), SkRect::MakeXYWH(x, y, w, h), SkMatrix::kFill_ScaleToFit);
    // Core Graphics confines the mask to its rect and holds its edge pixels there, so a 1x1 mask stays uniform.
    sk_sp<SkShader> shader = maskImg->makeShader(SkTileMode::kClamp, SkTileMode::kClamp, canvas->current_state().sampling, m);
    if (shader) {
        canvas->canvas->clipRect(SkRect::MakeXYWH(x, y, w, h), SkClipOp::kIntersect, true);
        canvas->canvas->clipShader(shader, SkClipOp::kIntersect);
    }
#else
    (void)canvas; (void)mask_pixels; (void)mask_w; (void)mask_h; (void)mask_stride; (void)x; (void)y; (void)w; (void)h; (void)is_alpha_only;
#endif
}

void compositor_canvas_get_clip_bounds(const CompCanvas *canvas, float *x, float *y, float *w, float *h) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) {
        if (x) *x = 0; if (y) *y = 0; if (w) *w = 0; if (h) *h = 0;
        return;
    }
    SkIRect devBounds = canvas->canvas->getDeviceClipBounds();
    SkMatrix inv;
    SkRect localBounds;
    if (canvas->canvas->getTotalMatrix().invert(&inv)) {
        localBounds = inv.mapRect(SkRect::Make(devBounds));
    } else {
        localBounds = SkRect::Make(devBounds);
    }
    if (x) *x = localBounds.fLeft;
    if (y) *y = localBounds.fTop;
    if (w) *w = localBounds.width();
    if (h) *h = localBounds.height();
#else
    if (x) *x = 0; if (y) *y = 0; if (w) *w = 0; if (h) *h = 0;
    (void)canvas;
#endif
}

void compositor_canvas_set_alpha(CompCanvas *canvas, float alpha) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas) return;
    canvas->current_state().alpha = alpha;
#else
    (void)canvas; (void)alpha;
#endif
}

void compositor_canvas_set_blend_mode(CompCanvas *canvas, int cg_blend_mode) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas) return;
    canvas->current_state().blendMode = map_cg_blend_mode(cg_blend_mode);
#else
    (void)canvas; (void)cg_blend_mode;
#endif
}

void compositor_canvas_set_interpolation_quality(CompCanvas *canvas, int cg_quality) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas) return;
    canvas->current_state().sampling = map_cg_sampling(cg_quality);
#else
    (void)canvas; (void)cg_quality;
#endif
}

void compositor_canvas_set_antialias(CompCanvas *canvas, int antialias) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas) return;
    canvas->current_state().antialias = antialias != 0;
#else
    (void)canvas; (void)antialias;
#endif
}

void compositor_canvas_fill_rect(CompCanvas *canvas, float x, float y, float w, float h, float r, float g, float b, float a) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    SkPaint paint;
    float finalAlpha = a * canvas->current_state().alpha;
    paint.setColor4f(SkColor4f{r, g, b, finalAlpha});
    paint.setBlendMode(canvas->current_state().blendMode);
    paint.setAntiAlias(canvas->current_state().antialias);
    canvas->canvas->drawRect(SkRect::MakeXYWH(x, y, w, h), paint);
#else
    (void)canvas; (void)x; (void)y; (void)w; (void)h; (void)r; (void)g; (void)b; (void)a;
#endif
}

void compositor_canvas_clear(CompCanvas *canvas, float x, float y, float w, float h) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    SkPaint paint;
    paint.setBlendMode(SkBlendMode::kClear);
    canvas->canvas->drawRect(SkRect::MakeXYWH(x, y, w, h), paint);
#else
    (void)canvas; (void)x; (void)y; (void)w; (void)h;
#endif
}

void compositor_canvas_draw_image_rect(CompCanvas *canvas, const uint8_t *src_pixels, size_t src_w, size_t src_h, size_t src_stride, float dx, float dy, float dw, float dh, float opacity, int cg_blend_mode, int cg_sampling_quality) {
    compositor_canvas_draw_image_rect_ex(canvas, src_pixels, src_w, src_h, src_stride, 0, dx, dy, dw, dh, opacity, cg_blend_mode, cg_sampling_quality);
}

void compositor_canvas_draw_image_rect_ex(CompCanvas *canvas, const uint8_t *src_pixels, size_t src_w, size_t src_h, size_t src_stride, int src_format, float dx, float dy, float dw, float dh, float opacity, int cg_blend_mode, int cg_sampling_quality) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas || !src_pixels || src_w == 0 || src_h == 0) return;
    const bool srcGray = src_format == 1;
    if (src_stride == 0) src_stride = src_w * (srcGray ? 1 : 4);

    SkImageInfo srcInfo = srcGray ? SkImageInfo::Make(static_cast<int>(src_w), static_cast<int>(src_h), kGray_8_SkColorType, kOpaque_SkAlphaType)
                                  : SkImageInfo::Make(static_cast<int>(src_w), static_cast<int>(src_h), kRGBA_8888_SkColorType, kPremul_SkAlphaType);
    SkPixmap srcPixmap(srcInfo, src_pixels, src_stride);
    sk_sp<SkImage> img = SkImages::RasterFromPixmap(srcPixmap, nullptr, nullptr);
    if (!img) return;

    SkPaint paint;
    float finalAlpha = opacity * canvas->current_state().alpha;
    paint.setAlphaf(finalAlpha);
    paint.setBlendMode(cg_blend_mode >= 0 ? map_cg_blend_mode(cg_blend_mode) : canvas->current_state().blendMode);
    paint.setAntiAlias(canvas->current_state().antialias);

    SkSamplingOptions sampling = (cg_sampling_quality >= 0) ? map_cg_sampling(cg_sampling_quality) : canvas->current_state().sampling;
    canvas->canvas->drawImageRect(img, SkRect::MakeXYWH(dx, dy, dw, dh), sampling, &paint);
#else
    (void)canvas; (void)src_pixels; (void)src_w; (void)src_h; (void)src_stride; (void)src_format; (void)dx; (void)dy; (void)dw; (void)dh; (void)opacity; (void)cg_blend_mode; (void)cg_sampling_quality;
#endif
}

void compositor_canvas_begin_transparency_layer(CompCanvas *canvas, float opacity) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    canvas->canvas->saveLayerAlphaf(nullptr, opacity);
    canvas->states.push_back(canvas->current_state());
#else
    (void)canvas; (void)opacity;
#endif
}

void compositor_canvas_end_transparency_layer(CompCanvas *canvas) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas) return;
    canvas->canvas->restore();
    if (canvas->states.size() > 1) {
        canvas->states.pop_back();
    }
#else
    (void)canvas;
#endif
}

/* Path Handle */
CompPath *compositor_path_create(void) {
#if defined(COMPOSITOR_HAS_SKIA)
    return new (std::nothrow) CompPath();
#else
    return nullptr;
#endif
}

void compositor_path_destroy(CompPath *path) {
    if (!path) return;
    delete path;
}

void compositor_path_move_to(CompPath *path, float x, float y) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.moveTo(x, y);
#else
    (void)path; (void)x; (void)y;
#endif
}

void compositor_path_line_to(CompPath *path, float x, float y) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.lineTo(x, y);
#else
    (void)path; (void)x; (void)y;
#endif
}

void compositor_path_add_rect(CompPath *path, float x, float y, float w, float h) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.addRect(SkRect::MakeXYWH(x, y, w, h));
#else
    (void)path; (void)x; (void)y; (void)w; (void)h;
#endif
}

void compositor_path_add_ellipse(CompPath *path, float cx, float cy, float rx, float ry) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.addOval(SkRect::MakeXYWH(cx - rx, cy - ry, rx * 2, ry * 2));
#else
    (void)path; (void)cx; (void)cy; (void)rx; (void)ry;
#endif
}

void compositor_path_close(CompPath *path) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.close();
#else
    (void)path;
#endif
}

void compositor_path_reset(CompPath *path) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.reset();
#else
    (void)path;
#endif
}

#if defined(COMPOSITOR_HAS_SKIA)
// SkPathOps' debug helpers (src/pathops/SkPathOpsDebug.cpp) reference SkPath::dump, which this Skia build
// leaves out (it is compiled only with SK_DUMP_ENABLED). Nothing here dumps paths, so a no-op keeps the
// pathops archive self-contained.
void SkPath::dump(SkWStream *, bool) const {}
#endif

/* ── CoreGraphics path surface ───────────────────────────────────────────── */

void compositor_path_quad_to(CompPath *path, float cx, float cy, float x, float y) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.quadTo(cx, cy, x, y);
#else
    (void)path; (void)cx; (void)cy; (void)x; (void)y;
#endif
}

void compositor_path_cubic_to(CompPath *path, float c1x, float c1y, float c2x, float c2y, float x, float y) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.cubicTo(SkPoint::Make(c1x, c1y), SkPoint::Make(c2x, c2y), SkPoint::Make(x, y));
#else
    (void)path; (void)c1x; (void)c1y; (void)c2x; (void)c2y; (void)x; (void)y;
#endif
}

void compositor_path_add_round_rect(CompPath *path, float x, float y, float w, float h, float rx, float ry) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (path) path->builder.addRRect(SkRRect::MakeRectXY(SkRect::MakeXYWH(x, y, w, h), rx, ry));
#else
    (void)path; (void)x; (void)y; (void)w; (void)h; (void)rx; (void)ry;
#endif
}

CompPath *compositor_path_copy(const CompPath *path) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!path) return nullptr;
    CompPath *copy = new (std::nothrow) CompPath();
    if (copy) copy->builder = path->builder;
    return copy;
#else
    (void)path; return nullptr;
#endif
}

CompPath *compositor_path_op(const CompPath *a, const CompPath *b, int op, int even_odd_a, int even_odd_b) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!a || !b) return nullptr;
    static const SkPathOp ops[4] = {kDifference_SkPathOp, kIntersect_SkPathOp, kUnion_SkPathOp, kXOR_SkPathOp};
    if (op < 0 || op > 3) return nullptr;
    std::optional<SkPath> result = Op(a->get_path(even_odd_a != 0), b->get_path(even_odd_b != 0), ops[op]);
    if (!result) return nullptr;
    // The Swift side keeps no fill rule, so hand back a winding-filled path (XOR arrives even-odd).
    if (result->getFillType() != SkPathFillType::kWinding) {
        if (auto winding = AsWinding(*result)) result = winding;
    }
    CompPath *out = new (std::nothrow) CompPath();
    if (out) out->builder = SkPathBuilder(*result);
    return out;
#else
    (void)a; (void)b; (void)op; (void)even_odd_a; (void)even_odd_b; return nullptr;
#endif
}

CompPath *compositor_path_stroke(const CompPath *path, float width, int cap, int join, float miter_limit) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!path || width < 0) return nullptr;
    SkPaint paint;
    paint.setStyle(SkPaint::kStroke_Style);
    paint.setStrokeWidth(width);
    paint.setStrokeMiter(miter_limit > 0 ? miter_limit : 10.0f);
    paint.setStrokeCap(cap == 1 ? SkPaint::kRound_Cap : cap == 2 ? SkPaint::kSquare_Cap : SkPaint::kButt_Cap);
    paint.setStrokeJoin(join == 1 ? SkPaint::kRound_Join : join == 2 ? SkPaint::kBevel_Join : SkPaint::kMiter_Join);
    SkPath stroked = skpathutils::FillPathWithPaint(path->get_path(false), paint);
    CompPath *out = new (std::nothrow) CompPath();
    if (out) out->builder = SkPathBuilder(stroked);
    return out;
#else
    (void)path; (void)width; (void)cap; (void)join; (void)miter_limit; return nullptr;
#endif
}

int compositor_path_bounds(const CompPath *path, float out_rect[4]) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!path || !out_rect) return 0;
    SkPath p = path->get_path(false);
    if (p.isEmpty()) return 0;
    SkRect r = p.computeTightBounds();
    out_rect[0] = r.x(); out_rect[1] = r.y(); out_rect[2] = r.width(); out_rect[3] = r.height();
    return 1;
#else
    (void)path; (void)out_rect; return 0;
#endif
}

size_t compositor_path_elements(const CompPath *path, uint8_t *out_types, size_t type_capacity,
                                float *out_points, size_t point_float_capacity, size_t *points_written) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (points_written) *points_written = 0;
    if (!path) return 0;
    SkPath p = path->get_path(false);
    SkPathIter iter = p.iter();
    size_t count = 0, floats = 0;
    while (auto rec = iter.next()) {
        uint8_t type = 0; size_t first = 0, n = 0;
        std::vector<SkPoint> converted;
        switch (rec->fVerb) {
        case SkPathVerb::kMove:  type = 0; first = 0; n = 1; break;
        case SkPathVerb::kLine:  type = 1; first = 1; n = 1; break;
        case SkPathVerb::kQuad:  type = 2; first = 1; n = 2; break;
        case SkPathVerb::kCubic: type = 3; first = 1; n = 3; break;
        case SkPathVerb::kConic: {
            // Approximate the conic with quads; the point spans `pts[0..2]`.
            SkPoint quads[1 + 2 * 4];
            int q = SkPath::ConvertConicToQuads(rec->fPoints[0], rec->fPoints[1], rec->fPoints[2], rec->conicWeight(), quads, 2);
            for (int i = 0; i < q; ++i) {
                if (out_types && count < type_capacity) out_types[count] = 2;
                if (out_points && floats + 4 <= point_float_capacity) {
                    out_points[floats] = quads[1 + 2 * i].x(); out_points[floats + 1] = quads[1 + 2 * i].y();
                    out_points[floats + 2] = quads[2 + 2 * i].x(); out_points[floats + 3] = quads[2 + 2 * i].y();
                }
                ++count; floats += 4;
            }
            continue;
        }
        case SkPathVerb::kClose: type = 4; n = 0; break;
        }
        if (out_types && count < type_capacity) out_types[count] = type;
        if (out_points) {
            for (size_t i = 0; i < n; ++i) {
                if (floats + 2 <= point_float_capacity) {
                    out_points[floats] = rec->fPoints[first + i].x();
                    out_points[floats + 1] = rec->fPoints[first + i].y();
                }
                floats += 2;
            }
        } else floats += n * 2;
        ++count;
    }
    if (points_written) *points_written = floats;
    return count;
#else
    (void)path; (void)out_types; (void)type_capacity; (void)out_points; (void)point_float_capacity;
    if (points_written) *points_written = 0;
    return 0;
#endif
}

void compositor_canvas_fill_path(CompCanvas *canvas, const CompPath *path, int even_odd,
                                 float r, float g, float b, float a) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas || !path) return;
    SkPaint paint;
    paint.setColor4f(SkColor4f{r, g, b, a * canvas->current_state().alpha});
    paint.setBlendMode(canvas->current_state().blendMode);
    paint.setAntiAlias(canvas->current_state().antialias);
    canvas->canvas->drawPath(path->get_path(even_odd != 0), paint);
#else
    (void)canvas; (void)path; (void)even_odd; (void)r; (void)g; (void)b; (void)a;
#endif
}

void compositor_canvas_stroke_path(CompCanvas *canvas, const CompPath *path, float width, int cap, int join,
                                   float miter_limit, float r, float g, float b, float a) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas || !path) return;
    SkPaint paint;
    paint.setStyle(SkPaint::kStroke_Style);
    paint.setStrokeWidth(width);
    paint.setStrokeMiter(miter_limit > 0 ? miter_limit : 10.0f);
    paint.setStrokeCap(cap == 1 ? SkPaint::kRound_Cap : cap == 2 ? SkPaint::kSquare_Cap : SkPaint::kButt_Cap);
    paint.setStrokeJoin(join == 1 ? SkPaint::kRound_Join : join == 2 ? SkPaint::kBevel_Join : SkPaint::kMiter_Join);
    paint.setColor4f(SkColor4f{r, g, b, a * canvas->current_state().alpha});
    paint.setBlendMode(canvas->current_state().blendMode);
    paint.setAntiAlias(canvas->current_state().antialias);
    canvas->canvas->drawPath(path->get_path(false), paint);
#else
    (void)canvas; (void)path; (void)width; (void)cap; (void)join; (void)miter_limit; (void)r; (void)g; (void)b; (void)a;
#endif
}

#if defined(COMPOSITOR_HAS_SKIA)
static void draw_gradient_shader(CompCanvas *canvas, sk_sp<SkShader> shader, const SkRect &clipBounds, int options,
                                 bool extendStart, bool extendEnd, bool radial) {
    (void)options; (void)extendStart; (void)extendEnd; (void)radial;
    if (!shader) return;
    SkPaint paint;
    paint.setShader(shader);
    paint.setAlphaf(canvas->current_state().alpha);
    paint.setBlendMode(canvas->current_state().blendMode);
    paint.setAntiAlias(canvas->current_state().antialias);
    canvas->canvas->drawRect(clipBounds, paint);
}
#endif

void compositor_canvas_draw_linear_gradient(CompCanvas *canvas, float x0, float y0, float x1, float y1,
                                            const float *colors, const float *locations, size_t count, int options) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas || !colors || !locations || count < 2) return;
    std::vector<SkColor4f> cs(count);
    for (size_t i = 0; i < count; ++i) cs[i] = SkColor4f{colors[i * 4], colors[i * 4 + 1], colors[i * 4 + 2], colors[i * 4 + 3]};
    // CoreGraphics only paints beyond the end points when the extend options ask for it; otherwise the
    // outside stays untouched. A clamp tile mode plus an explicit limiting clip reproduces that.
    const bool before = options & 1, after = options & 2;
    SkPoint pts[2] = {SkPoint::Make(x0, y0), SkPoint::Make(x1, y1)};
    SkGradient grad(SkGradient::Colors(cs, SkSpan<const float>(locations, count), SkTileMode::kClamp), SkGradient::Interpolation());
    sk_sp<SkShader> shader = SkShaders::LinearGradient(pts, grad);
    SkRect bounds = SkRect::Make(canvas->canvas->getDeviceClipBounds());
    canvas->canvas->save();
    if (!before || !after) {
        // Limit to the half-planes the options allow (perpendicular to the gradient axis).
        SkPath keep;
        SkVector axis = pts[1] - pts[0];
        float len = axis.length();
        if (len > 0) {
            axis.scale(1.0f / len);
            SkVector normal = SkVector::Make(-axis.fY, axis.fX);
            float big = 1e5f;
            SkPoint a = pts[0] - (before ? axis * big : SkVector::Make(0, 0));
            SkPoint b = pts[1] + (after ? axis * big : SkVector::Make(0, 0));
            SkPathBuilder pb;
            pb.moveTo(a + normal * big); pb.lineTo(b + normal * big); pb.lineTo(b - normal * big); pb.lineTo(a - normal * big); pb.close();
            canvas->canvas->clipPath(pb.detach(), SkClipOp::kIntersect, false);
        }
    }
    draw_gradient_shader(canvas, shader, bounds, options, before, after, false);
    canvas->canvas->restore();
#else
    (void)canvas; (void)x0; (void)y0; (void)x1; (void)y1; (void)colors; (void)locations; (void)count; (void)options;
#endif
}

void compositor_canvas_draw_radial_gradient(CompCanvas *canvas, float x0, float y0, float r0, float x1, float y1,
                                            float r1, const float *colors, const float *locations, size_t count,
                                            int options) {
#if defined(COMPOSITOR_HAS_SKIA)
    if (!canvas || !canvas->canvas || !colors || !locations || count < 2) return;
    std::vector<SkColor4f> cs(count);
    for (size_t i = 0; i < count; ++i) cs[i] = SkColor4f{colors[i * 4], colors[i * 4 + 1], colors[i * 4 + 2], colors[i * 4 + 3]};
    const bool before = options & 1, after = options & 2;
    SkGradient grad(SkGradient::Colors(cs, SkSpan<const float>(locations, count), SkTileMode::kClamp), SkGradient::Interpolation());
    sk_sp<SkShader> shader = SkShaders::TwoPointConicalGradient(SkPoint::Make(x0, y0), r0, SkPoint::Make(x1, y1), r1, grad);
    SkRect bounds = SkRect::Make(canvas->canvas->getDeviceClipBounds());
    canvas->canvas->save();
    if (!after) {
        SkPathBuilder pb;
        pb.addOval(SkRect::MakeXYWH(x1 - r1, y1 - r1, r1 * 2, r1 * 2));
        canvas->canvas->clipPath(pb.detach(), SkClipOp::kIntersect, canvas->current_state().antialias);
    }
    (void)before;  // Before-start extension is empty for the concentric case used by upstream (r0 = 0).
    draw_gradient_shader(canvas, shader, bounds, options, before, after, true);
    canvas->canvas->restore();
#else
    (void)canvas; (void)x0; (void)y0; (void)r0; (void)x1; (void)y1; (void)r1; (void)colors; (void)locations; (void)count; (void)options;
#endif
}


// ── Layer effects: the Skia tier ───────────────────────────────────────────────────────────────────────────────
//
// The nine passes as Skia image filters over float (RGBA F32) surfaces, so nothing is rounded to 8 bits until the end:
// dilate/erode for the stroke's reach, a bilinear translate for the shadow's offset, a clamped Gaussian for its blur,
// arithmetic blends for the ring and the inner shadow's coverage, and colour-matrix filters to tint coverage before
// the layers are composited with source-over in the order upstream's Metal compose kernel uses. Approximate where
// Skia's kernels differ from the reference C++ tier (Gaussian support and edge blending of the shift), by a few levels.

#if defined(COMPOSITOR_HAS_SKIA)
namespace {

sk_sp<SkImage> effects_apply(const sk_sp<SkImage> &input, sk_sp<SkImageFilter> filter, int w, int h) {
    sk_sp<SkSurface> surface = SkSurfaces::Raster(SkImageInfo::Make(w, h, kRGBA_F32_SkColorType, kPremul_SkAlphaType));
    if (!surface) return nullptr;
    surface->getCanvas()->clear(SK_ColorTRANSPARENT);
    SkPaint paint;
    paint.setImageFilter(std::move(filter));
    surface->getCanvas()->drawImage(input, 0, 0, SkSamplingOptions(), &paint);
    return surface->makeImageSnapshot();
}

// Coverage held in alpha, tinted: colour * coverage * opacity, premultiplied.
sk_sp<SkImageFilter> effects_tint(const CompositorEffectColor &c) {
    SkColorMatrix m;
    m.setRowMajor(std::array<float, 20>{0, 0, 0, 0, c.r,
                                        0, 0, 0, 0, c.g,
                                        0, 0, 0, 0, c.b,
                                        0, 0, 0, c.opacity, 0}.data());
    return SkImageFilters::ColorFilter(SkColorFilters::Matrix(m), nullptr);
}

sk_sp<SkImageFilter> effects_blur(float sigma) { return SkImageFilters::Blur(sigma, sigma, SkTileMode::kClamp, nullptr); }
sk_sp<SkImageFilter> effects_shift(float dx, float dy) {
    return SkImageFilters::MatrixTransform(SkMatrix::Translate(dx, dy), SkSamplingOptions(SkFilterMode::kLinear), nullptr);
}

}  // namespace
#endif

int compositor_skia_effects_render(const CompositorEffectsParams *p, const uint8_t *pixels, uint8_t *out) {
    if (!p || !pixels || !out || p->width == 0 || p->height == 0) return -1;
#if defined(COMPOSITOR_HAS_SKIA)
    const int w = static_cast<int>(p->width), h = static_cast<int>(p->height);
    if (static_cast<size_t>(w) * h > 100000000) return -1;
    try {
        SkPixmap sourcePixmap(SkImageInfo::Make(w, h, kRGBA_8888_SkColorType, kPremul_SkAlphaType), pixels, static_cast<size_t>(w) * 4);
        sk_sp<SkImage> source = SkImages::RasterFromPixmapCopy(sourcePixmap);
        if (!source) return -2;
        // The shape's own coverage, in alpha.
        SkColorMatrix alphaOnly;
        alphaOnly.setRowMajor(std::array<float, 20>{0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0}.data());
        sk_sp<SkImage> shape = effects_apply(source, SkImageFilters::ColorFilter(SkColorFilters::Matrix(alphaOnly), nullptr), w, h);
        if (!shape) return -2;

        sk_sp<SkSurface> result = SkSurfaces::Raster(SkImageInfo::Make(w, h, kRGBA_F32_SkColorType, kPremul_SkAlphaType));
        if (!result) return -2;
        SkCanvas *canvas = result->getCanvas();
        canvas->clear(SK_ColorTRANSPARENT);

        sk_sp<SkImage> shadow, ring, inner, outer;
        if (p->has_stroke) {
            const float reach = static_cast<float>(std::max(1u, p->stroke_reach));
            sk_sp<SkImage> moved = effects_apply(shape, p->stroke_inside ? SkImageFilters::Erode(reach, reach, nullptr)
                                                                          : SkImageFilters::Dilate(reach, reach, nullptr), w, h);
            if (!moved) return -2;
            // outside: moved - shape; inside: shape - moved (both clamped to 0...1 by the blend).
            sk_sp<SkImage> difference = effects_apply(shape,
                p->stroke_inside ? SkImageFilters::Arithmetic(0, 1, -1, 0, false, SkImageFilters::Image(moved, SkSamplingOptions()), SkImageFilters::Image(shape, SkSamplingOptions()))
                                 : SkImageFilters::Arithmetic(0, 1, -1, 0, false, SkImageFilters::Image(shape, SkSamplingOptions()), SkImageFilters::Image(moved, SkSamplingOptions())), w, h);
            ring = difference ? effects_apply(difference, effects_tint(p->stroke), w, h) : nullptr;
            if (!ring) return -2;
        }
        if (p->has_shadow) {
            sk_sp<SkImage> moved = effects_apply(shape, effects_shift(p->shadow_dx, p->shadow_dy), w, h);
            if (moved && p->shadow_sigma > 0.01f) moved = effects_apply(moved, effects_blur(p->shadow_sigma), w, h);
            shadow = moved ? effects_apply(moved, effects_tint(p->shadow), w, h) : nullptr;
            if (!shadow) return -2;
        }
        if (p->has_inner) {
            sk_sp<SkImage> moved = effects_apply(shape, effects_shift(p->inner_dx, p->inner_dy), w, h);
            if (moved && p->inner_sigma > 0.01f) moved = effects_apply(moved, effects_blur(p->inner_sigma), w, h);
            // inside: shape * (1 - moved)
            sk_sp<SkImage> inside = moved ? effects_apply(shape,
                SkImageFilters::Arithmetic(-1, 1, 0, 0, false, SkImageFilters::Image(moved, SkSamplingOptions()), SkImageFilters::Image(shape, SkSamplingOptions())), w, h) : nullptr;
            inner = inside ? effects_apply(inside, effects_tint(p->inner), w, h) : nullptr;
            if (!inner) return -2;
        }
        if (p->has_outer) {
            // The shape softened omnidirectionally (no offset), with the shape's own interior excluded: moved * (1 - shape).
            sk_sp<SkImage> moved = p->outer_sigma > 0.01f ? effects_apply(shape, effects_blur(p->outer_sigma), w, h) : shape;
            sk_sp<SkImage> excluded = moved ? effects_apply(shape,
                SkImageFilters::Arithmetic(-1, 1, 0, 0, false, SkImageFilters::Image(shape, SkSamplingOptions()), SkImageFilters::Image(moved, SkSamplingOptions())), w, h) : nullptr;
            outer = excluded ? effects_apply(excluded, effects_tint(p->outer), w, h) : nullptr;
            if (!outer) return -2;
        }
        // Shadow behind, the glow around it, outside stroke over that, the layer's pixels over that, then a colour
        // overlay, an inner shadow and an inside stroke on top.
        if (shadow) canvas->drawImage(shadow, 0, 0);
        if (outer) canvas->drawImage(outer, 0, 0);
        if (ring && !p->stroke_inside) canvas->drawImage(ring, 0, 0);
        canvas->drawImage(source, 0, 0);
        if (p->has_overlay) canvas->drawImage(effects_apply(shape, effects_tint(p->overlay), w, h), 0, 0);
        if (inner) canvas->drawImage(inner, 0, 0);
        if (ring && p->stroke_inside) canvas->drawImage(ring, 0, 0);

        SkPixmap target(SkImageInfo::Make(w, h, kRGBA_8888_SkColorType, kPremul_SkAlphaType), out, static_cast<size_t>(w) * 4);
        return result->readPixels(target, 0, 0) ? 0 : -2;
    } catch (...) {
        return -2;
    }
#else
    return -2;
#endif
}
