#pragma once
#include "include/CompositorBrushBackend.h"
#include <cmath>
#include <initializer_list>
inline bool validBrush(const CompositorBrushUniforms *u, const CompositorBrushSegment *segments,
                       size_t count, const float *permanent, size_t pixels, float *next, uint8_t *preview) {
    if (!u || !permanent || !next || !preview || next == permanent || !u->width || !u->height ||
        u->width > 256 || u->height > 256 || pixels != size_t(u->width) * u->height ||
        count != u->segment_count || count > 2048 || u->settled_count > count || (count && !segments)) return false;
    const float values[] = {u->a,u->b,u->c,u->d,u->origin_x,u->origin_y,u->radius,u->hardness,
        u->canvas_width,u->canvas_height,u->antialias_width,u->spacing};
    for (float v : values) if (!std::isfinite(v) || std::abs(v) > 1e9f) return false;
    if (u->radius <= 0 || u->radius > 1000 || u->hardness < 0 || u->hardness > 1 ||
        u->canvas_width <= 0 || u->canvas_height <= 0 || u->antialias_width < 0.001f || u->spacing < 0.25f) return false;
    for (size_t i = 0; i < count; ++i) {
        const auto &s = segments[i];
        for (float v : {s.x0,s.y0,s.x1,s.y1}) if (!std::isfinite(v) || std::abs(v) > 1e7f) return false;
    }
    for (size_t i = 0; i < pixels; ++i)
        if (!std::isfinite(permanent[i]) || permanent[i] < 0 || permanent[i] > (u->hardness >= 1 ? 1 : 20)) return false;
    return true;
}
