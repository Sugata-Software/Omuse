#include "BrushCoverageValidation.h"
#include <algorithm>
#include <limits>

namespace {
float coverage(float distanceSquared, const CompositorBrushUniforms &u) {
    const float distance = std::sqrt(distanceSquared);
    if (u.hardness >= 1) return std::clamp((u.radius - distance) / u.antialias_width + 0.5f, 0.0f, 1.0f);
    const float t = std::clamp((distance / u.radius - u.hardness) / (1 - u.hardness), 0.0f, 1.0f);
    return std::max(0.0f, (std::exp(-2.5f*t*t) - std::exp(-2.5f)) / (1 - std::exp(-2.5f)));
}
float distanceSquared(float x, float y, const CompositorBrushSegment &s) {
    const float dx = s.x1-s.x0, dy = s.y1-s.y0;
    const float t = std::clamp(((x-s.x0)*dx+(y-s.y0)*dy) / std::max(dx*dx+dy*dy,1e-12f),0.0f,1.0f);
    const float px = x-(s.x0+t*dx), py = y-(s.y0+t*dy);
    return px*px+py*py;
}
float density(float distanceSquared, const CompositorBrushUniforms &u) {
    return -std::log(std::max(1 - coverage(distanceSquared,u),0.001f));
}
float segmentDensity(float x, float y, const CompositorBrushSegment &s, const CompositorBrushUniforms &u) {
    const float dx = s.x1-s.x0, dy = s.y1-s.y0, length = std::sqrt(dx*dx+dy*dy);
    const float px = x-s.x0, py = y-s.y0;
    if (length < 1e-6f) return density(px*px+py*py,u);
    const float projection = (px*dx+py*dy)/length;
    const float ex = px-projection*dx/length, ey = py-projection*dy/length;
    const float perpendicular = ex*ex+ey*ey;
    if (perpendicular >= u.radius*u.radius) return 0;
    const float reach = std::sqrt(u.radius*u.radius-perpendicular);
    const float lo = std::max(0.0f,projection-reach), hi = std::min(length,projection+reach);
    if (hi <= lo) return 0;
    const float middle = (lo+hi)*0.5f, half = (hi-lo)*0.5f;
    constexpr float nodes[] = {0.1834346425f,0.5255324099f,0.7966664774f,0.9602898565f};
    constexpr float weights[] = {0.3626837834f,0.3137066459f,0.2223810345f,0.1012285363f};
    float integral = 0;
    for (int i=0;i<4;++i) {
        const float a = middle-half*nodes[i]-projection, b = middle+half*nodes[i]-projection;
        integral += weights[i]*(density(perpendicular+a*a,u)+density(perpendicular+b*b,u));
    }
    return integral*half/u.spacing;
}
}
extern "C" int compositor_brush_cpu(const CompositorBrushUniforms *uniforms,
    const CompositorBrushSegment *segments, size_t count,
    const float *permanent, size_t pixels, float *next, uint8_t *preview) {
    if (!validBrush(uniforms,segments,count,permanent,pixels,next,preview)) return -1;
    const auto &u = *uniforms;
    for (uint32_t y=0;y<u.height;++y) for (uint32_t x=0;x<u.width;++x) {
        const size_t i = size_t(y)*u.width+x;
        const float px = u.origin_x+(float(x)+0.5f)*u.a+(float(y)+0.5f)*u.c;
        const float py = u.origin_y+(float(x)+0.5f)*u.b+(float(y)+0.5f)*u.d;
        next[i] = permanent[i]; preview[i] = 0;
        if (px<0 || py<0 || px>=u.canvas_width || py>=u.canvas_height) continue;
        float value;
        if (u.hardness >= 1) {
            float settled = std::numeric_limits<float>::infinity(), tail = settled;
            for (uint32_t j=0;j<u.settled_count;++j) settled = std::min(settled,distanceSquared(px,py,segments[j]));
            for (uint32_t j=u.settled_count;j<count;++j) tail = std::min(tail,distanceSquared(px,py,segments[j]));
            next[i] = std::max(permanent[i],coverage(settled,u));
            value = std::max(next[i],coverage(tail,u));
        } else {
            float settled = permanent[i], tail = 0;
            for (uint32_t j=0;j<u.settled_count;++j) settled += segmentDensity(px,py,segments[j],u);
            for (uint32_t j=u.settled_count;j<count;++j) tail += segmentDensity(px,py,segments[j],u);
            next[i] = std::min(settled,20.0f);
            value = 1-std::exp(-std::min(settled+tail,20.0f));
        }
        preview[i] = uint8_t(std::clamp(std::round(255*value),0.0f,255.0f));
    }
    return 0;
}
