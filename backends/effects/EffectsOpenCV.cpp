// OpenCV tier of the layer-effects chain (Vulkan -> Skia -> OpenCV -> C): the heavy passes are OpenCV's own operators on
// float planes (rectangular dilate/erode for the stroke's reach, warpAffine for the shadow's offset, a Gaussian with edge
// replication for its blur), so a build with OpenCV's optimised kernels (SIMD, threads) runs them well. Coverage
// arithmetic and compose are the reference's, in the same order. Built only when OpenCV's headers are present
// (COMPOSITOR_HAS_OPENCV, set by the package manifest / CMake); otherwise every call reports "unavailable" (-2).
#include "CompositorEffectsBackend.h"

#if defined(COMPOSITOR_HAS_OPENCV)
#include <opencv2/core.hpp>
#include <opencv2/imgproc.hpp>
#include <algorithm>
#include <cmath>

namespace {
float clamp01(float v) { return std::min(std::max(v, 0.0f), 1.0f); }
uint8_t byteOf(float v) { return static_cast<uint8_t>(clamp01(v) * 255.0f + 0.5f); }

// Past the edge there is nothing: the rectangle of the reference's separable max/min, with zero borders.
cv::Mat spread(const cv::Mat &plane, int reach, bool smallest) {
    cv::Mat out;
    const cv::Mat kernel = cv::getStructuringElement(cv::MORPH_RECT, cv::Size(2 * reach + 1, 2 * reach + 1));
    if (smallest) cv::erode(plane, out, kernel, cv::Point(-1, -1), 1, cv::BORDER_CONSTANT, cv::Scalar(0));
    else cv::dilate(plane, out, kernel, cv::Point(-1, -1), 1, cv::BORDER_CONSTANT, cv::Scalar(0));
    return out;
}

cv::Mat shift(const cv::Mat &plane, float dx, float dy) {
    const cv::Matx23f m(1, 0, dx, 0, 1, dy);
    cv::Mat out;
    cv::warpAffine(plane, out, m, plane.size(), cv::INTER_LINEAR, cv::BORDER_CONSTANT, cv::Scalar(0));
    return out;
}

cv::Mat blur(const cv::Mat &plane, float sigma) {
    const int radius = std::max(1, static_cast<int>(std::lround(sigma * 3.0f)));
    cv::Mat out;
    cv::GaussianBlur(plane, out, cv::Size(2 * radius + 1, 2 * radius + 1), sigma, sigma, cv::BORDER_REPLICATE);
    return out;
}
}  // namespace

extern "C" int compositor_effects_opencv(const CompositorEffectsParams *p, const uint8_t *pixels, uint8_t *out) {
    if (!p || !pixels || !out || p->width == 0 || p->height == 0) return -1;
    const int w = static_cast<int>(p->width), h = static_cast<int>(p->height);
    if (static_cast<size_t>(w) * h > 100000000) return -1;
    try {
        const cv::Mat rgba(h, w, CV_8UC4, const_cast<uint8_t *>(pixels));
        cv::Mat first;
        cv::extractChannel(rgba, first, 3);
        first.convertTo(first, CV_32F, 1.0 / 255.0);
        cv::Mat ring, shadow, inner, outer;
        if (p->has_stroke) {
            const cv::Mat moved = spread(first, static_cast<int>(std::max(1u, p->stroke_reach)), p->stroke_inside != 0);
            cv::Mat difference = p->stroke_inside ? first - moved : moved - first;
            cv::max(difference, 0.0, difference);
            cv::min(difference, 1.0, ring);
        }
        if (p->has_shadow) {
            shadow = shift(first, p->shadow_dx, p->shadow_dy);
            if (p->shadow_sigma > 0.01f) shadow = blur(shadow, p->shadow_sigma);
        }
        if (p->has_inner) {
            cv::Mat moved = shift(first, p->inner_dx, p->inner_dy);
            if (p->inner_sigma > 0.01f) moved = blur(moved, p->inner_sigma);
            inner = first.mul(1.0 - moved);
            cv::max(inner, 0.0, inner);
            cv::min(inner, 1.0, inner);
        }
        if (p->has_outer) {
            // The shape softened omnidirectionally (no offset), with the shape's own interior excluded.
            cv::Mat moved = p->outer_sigma > 0.01f ? blur(first, p->outer_sigma) : first.clone();
            outer = moved.mul(1.0 - first);
            cv::max(outer, 0.0, outer);
            cv::min(outer, 1.0, outer);
        }
        // Shadow behind, the glow around it, outside stroke over that, the layer's pixels over that, then a colour
        // overlay, an inner shadow and an inside stroke on top.
        cv::parallel_for_(cv::Range(0, h), [&](const cv::Range &rows) {
            for (int y = rows.start; y < rows.end; ++y) {
                const uint8_t *src = pixels + static_cast<size_t>(y) * w * 4;
                uint8_t *dst = out + static_cast<size_t>(y) * w * 4;
                for (int x = 0; x < w; ++x) {
                    float cr = 0, cg = 0, cb = 0, alpha = 0;
                    auto over = [&](const CompositorEffectColor &c, float coverage) {
                        cr = c.r * coverage + cr * (1.0f - coverage);
                        cg = c.g * coverage + cg * (1.0f - coverage);
                        cb = c.b * coverage + cb * (1.0f - coverage);
                        alpha = coverage + alpha * (1.0f - coverage);
                    };
                    if (p->has_shadow) {
                        const float coverage = clamp01(shadow.at<float>(y, x) * p->shadow.opacity);
                        cr = p->shadow.r * coverage; cg = p->shadow.g * coverage; cb = p->shadow.b * coverage; alpha = coverage;
                    }
                    if (p->has_outer) over(p->outer, clamp01(outer.at<float>(y, x) * p->outer.opacity));
                    const float strokeCoverage = p->has_stroke ? clamp01(ring.at<float>(y, x) * p->stroke.opacity) : 0.0f;
                    if (p->has_stroke && !p->stroke_inside) over(p->stroke, strokeCoverage);
                    const float sa = src[x * 4 + 3] / 255.0f;
                    cr = src[x * 4] / 255.0f + cr * (1.0f - sa);
                    cg = src[x * 4 + 1] / 255.0f + cg * (1.0f - sa);
                    cb = src[x * 4 + 2] / 255.0f + cb * (1.0f - sa);
                    alpha = sa + alpha * (1.0f - sa);
                    if (p->has_overlay) over(p->overlay, clamp01(first.at<float>(y, x) * p->overlay.opacity));
                    if (p->has_inner) over(p->inner, clamp01(inner.at<float>(y, x) * p->inner.opacity));
                    if (p->has_stroke && p->stroke_inside) over(p->stroke, strokeCoverage);
                    dst[x * 4] = byteOf(cr); dst[x * 4 + 1] = byteOf(cg); dst[x * 4 + 2] = byteOf(cb); dst[x * 4 + 3] = byteOf(alpha);
                }
            }
        });
    } catch (...) {
        return -2;
    }
    return 0;
}
#else
extern "C" int compositor_effects_opencv(const CompositorEffectsParams *, const uint8_t *, uint8_t *) { return -2; }
#endif
