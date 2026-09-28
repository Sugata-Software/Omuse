// The portable tier of the layer-effects chain (Vulkan -> Skia -> OpenCV -> C): the same nine passes as upstream's
// Metal kernels, in plain C++ with float arithmetic in the same order, rows spread over the available cores. It is
// the reference the GPU backends are checked against and what runs when there is no device.

#include "CompositorEffectsBackend.h"
#include <algorithm>
#include <cmath>
#include <functional>
#include <thread>
#include <vector>

namespace {

void parallelRows(uint32_t rows, const std::function<void(uint32_t, uint32_t)> &body) {
    unsigned workers = std::max(1u, std::min(std::thread::hardware_concurrency(), 16u));
    if (rows < 64 || workers == 1) { body(0, rows); return; }
    workers = std::min<unsigned>(workers, rows);
    std::vector<std::thread> pool;
    const uint32_t chunk = (rows + workers - 1) / workers;
    for (unsigned i = 0; i < workers; ++i) {
        const uint32_t begin = i * chunk, end = std::min(rows, begin + chunk);
        if (begin >= end) break;
        pool.emplace_back(body, begin, end);
    }
    for (auto &t : pool) t.join();
}

using Plane = std::vector<float>;

// The largest (or smallest) value within reach along a row or column; past the edge there is nothing (0).
void spread(const Plane &source, Plane &result, uint32_t w, uint32_t h, int reach, bool smallest, bool rows) {
    parallelRows(h, [&](uint32_t y0, uint32_t y1) {
        for (uint32_t y = y0; y < y1; ++y) {
            for (uint32_t x = 0; x < w; ++x) {
                float best = smallest ? 1.0f : 0.0f;
                for (int offset = -reach; offset <= reach; ++offset) {
                    const int sample = static_cast<int>(rows ? x : y) + offset;
                    const int limit = static_cast<int>(rows ? w : h);
                    const float value = (sample < 0 || sample >= limit) ? 0.0f
                        : source[rows ? static_cast<size_t>(y) * w + static_cast<uint32_t>(sample)
                                      : static_cast<size_t>(sample) * w + x];
                    best = smallest ? std::min(best, value) : std::max(best, value);
                }
                result[static_cast<size_t>(y) * w + x] = best;
            }
        }
    });
}

// The shape moved by (dx, dy), bilinearly, so a shadow moves smoothly rather than in whole steps.
void shift(const Plane &source, Plane &result, uint32_t w, uint32_t h, float dx, float dy) {
    parallelRows(h, [&](uint32_t y0, uint32_t y1) {
        for (uint32_t y = y0; y < y1; ++y) {
            for (uint32_t x = 0; x < w; ++x) {
                const float sx = static_cast<float>(x) - dx, sy = static_cast<float>(y) - dy;
                float value = 0.0f;
                if (sx >= 0.0f && sy >= 0.0f && sx <= static_cast<float>(w - 1) && sy <= static_cast<float>(h - 1)) {
                    const uint32_t x0 = static_cast<uint32_t>(std::floor(sx)), yy0 = static_cast<uint32_t>(std::floor(sy));
                    const uint32_t x1 = std::min(x0 + 1, w - 1), yy1 = std::min(yy0 + 1, h - 1);
                    const float fx = sx - static_cast<float>(x0), fy = sy - static_cast<float>(yy0);
                    auto at = [&](uint32_t px, uint32_t py) { return source[static_cast<size_t>(py) * w + px]; };
                    const float top = at(x0, yy0) + (at(x1, yy0) - at(x0, yy0)) * fx;
                    const float bottom = at(x0, yy1) + (at(x1, yy1) - at(x0, yy1)) * fx;
                    value = top + (bottom - top) * fy;
                }
                result[static_cast<size_t>(y) * w + x] = value;
            }
        }
    });
}

// A Gaussian pass along rows or columns; samples past the edge repeat the edge.
void blur(const Plane &source, Plane &result, uint32_t w, uint32_t h, float sigma, int radius, bool rows) {
    std::vector<float> weights(static_cast<size_t>(2 * radius + 1));
    for (int offset = -radius; offset <= radius; ++offset)
        weights[static_cast<size_t>(offset + radius)] = std::exp(-static_cast<float>(offset * offset) / (2.0f * sigma * sigma));
    parallelRows(h, [&](uint32_t y0, uint32_t y1) {
        for (uint32_t y = y0; y < y1; ++y) {
            for (uint32_t x = 0; x < w; ++x) {
                float total = 0.0f, weightSum = 0.0f;
                for (int offset = -radius; offset <= radius; ++offset) {
                    const float weight = weights[static_cast<size_t>(offset + radius)];
                    const int limit = static_cast<int>(rows ? w : h);
                    const int sample = std::min(std::max(static_cast<int>(rows ? x : y) + offset, 0), limit - 1);
                    total += weight * source[rows ? static_cast<size_t>(y) * w + static_cast<uint32_t>(sample)
                                                  : static_cast<size_t>(sample) * w + x];
                    weightSum += weight;
                }
                result[static_cast<size_t>(y) * w + x] = total / weightSum;
            }
        }
    });
}

float clamp01(float v) { return std::min(std::max(v, 0.0f), 1.0f); }
uint8_t byteOf(float v) { return static_cast<uint8_t>(clamp01(v) * 255.0f + 0.5f); }

}  // namespace

extern "C" int compositor_effects_cpu(const CompositorEffectsParams *p, const uint8_t *pixels, uint8_t *out) {
    if (!p || !pixels || !out || p->width == 0 || p->height == 0) return -1;
    const uint32_t w = p->width, h = p->height;
    const size_t count = static_cast<size_t>(w) * h;
    if (count > 100000000) return -1;
    try {
        Plane first(count), second(count), third(count);
        // first: the shape's own coverage.
        parallelRows(h, [&](uint32_t y0, uint32_t y1) {
            for (size_t i = static_cast<size_t>(y0) * w; i < static_cast<size_t>(y1) * w; ++i) first[i] = static_cast<float>(pixels[i * 4 + 3]) / 255.0f;
        });
        if (p->has_stroke) {
            // second: the shape reached out (or pulled in) by the stroke's size; third: the ring between them.
            const int reach = static_cast<int>(std::max(1u, p->stroke_reach));
            spread(first, third, w, h, reach, p->stroke_inside != 0, true);
            spread(third, second, w, h, reach, p->stroke_inside != 0, false);
            for (size_t i = 0; i < count; ++i)
                third[i] = clamp01(p->stroke_inside ? first[i] - second[i] : second[i] - first[i]);
        }
        if (p->has_shadow) {
            shift(first, second, w, h, p->shadow_dx, p->shadow_dy);
            if (p->shadow_sigma > 0.01f) {
                Plane scratch(count);
                const int radius = std::max(1, static_cast<int>(std::lround(p->shadow_sigma * 3.0f)));
                blur(second, scratch, w, h, p->shadow_sigma, radius, true);
                blur(scratch, second, w, h, p->shadow_sigma, radius, false);
            }
        }
        Plane outer;
        if (p->has_outer) {
            // The shape softened omnidirectionally, with the shape's own interior excluded (no offset, unlike a shadow).
            outer.resize(count);
            if (p->outer_sigma > 0.01f) {
                Plane scratch(count);
                const int radius = std::max(1, static_cast<int>(std::lround(p->outer_sigma * 3.0f)));
                blur(first, scratch, w, h, p->outer_sigma, radius, true);
                blur(scratch, outer, w, h, p->outer_sigma, radius, false);
            } else {
                outer = first;
            }
            for (size_t i = 0; i < count; ++i) outer[i] = clamp01(outer[i] * (1.0f - first[i]));
        }
        Plane inner;
        if (p->has_inner) {
            Plane moved(count), softened(count);
            shift(first, moved, w, h, p->inner_dx, p->inner_dy);
            if (p->inner_sigma > 0.01f) {
                const int radius = std::max(1, static_cast<int>(std::lround(p->inner_sigma * 3.0f)));
                blur(moved, softened, w, h, p->inner_sigma, radius, true);
                blur(softened, moved, w, h, p->inner_sigma, radius, false);
            }
            inner.resize(count);
            for (size_t i = 0; i < count; ++i) inner[i] = clamp01(first[i] * (1.0f - moved[i]));
        }
        // Shadow behind, outside stroke over it, the layer's pixels over that, then a colour overlay, an inner shadow
        // and an inside stroke on top.
        parallelRows(h, [&](uint32_t y0, uint32_t y1) {
            for (size_t i = static_cast<size_t>(y0) * w; i < static_cast<size_t>(y1) * w; ++i) {
                float cr = 0, cg = 0, cb = 0, alpha = 0;
                if (p->has_shadow) {
                    const float coverage = clamp01(second[i] * p->shadow.opacity);
                    cr = p->shadow.r * coverage; cg = p->shadow.g * coverage; cb = p->shadow.b * coverage;
                    alpha = coverage;
                }
                const float strokeCoverage = p->has_stroke ? clamp01(third[i] * p->stroke.opacity) : 0.0f;
                auto over = [&](const CompositorEffectColor &c, float coverage) {
                    cr = c.r * coverage + cr * (1.0f - coverage);
                    cg = c.g * coverage + cg * (1.0f - coverage);
                    cb = c.b * coverage + cb * (1.0f - coverage);
                    alpha = coverage + alpha * (1.0f - coverage);
                };
                if (p->has_outer) over(p->outer, clamp01(outer[i] * p->outer.opacity));
                if (p->has_stroke && !p->stroke_inside) over(p->stroke, strokeCoverage);
                const float sr = pixels[i * 4] / 255.0f, sg = pixels[i * 4 + 1] / 255.0f, sb = pixels[i * 4 + 2] / 255.0f,
                            sa = pixels[i * 4 + 3] / 255.0f;
                cr = sr + cr * (1.0f - sa); cg = sg + cg * (1.0f - sa); cb = sb + cb * (1.0f - sa);
                alpha = sa + alpha * (1.0f - sa);
                if (p->has_overlay) over(p->overlay, clamp01(first[i] * p->overlay.opacity));
                if (p->has_inner) over(p->inner, clamp01(inner[i] * p->inner.opacity));
                if (p->has_stroke && p->stroke_inside) over(p->stroke, strokeCoverage);
                out[i * 4] = byteOf(cr); out[i * 4 + 1] = byteOf(cg); out[i * 4 + 2] = byteOf(cb); out[i * 4 + 3] = byteOf(alpha);
            }
        });
    } catch (...) {
        return -2;
    }
    return 0;
}
