/*
 * C reference implementation of compositor_composite_over — TEST STAND-IN ONLY.
 *
 * The production implementation is the Swift @_cdecl export in
 * Sources/CompositorCore/CompositeOver.swift, which compiles under the
 * Freedesktop Swift runtime extension (not on this build host). This C file
 * mirrors that exact math and signature so the C++ shim + unit tests can link
 * and run here, proving the ABI header is a valid contract and the compositing
 * math is correct. It is compiled ONLY into the test target (CMakeLists.txt),
 * never linked into the production Swift core. If the two ever diverge, the
 * parity fixture (ENG-10 pattern) catches it.
 */

#include "CompositorCore.h"

static uint8_t clampu8(float v) {
    if (v != v) return 0;            /* NaN */
    if (v <= 0.0f) return 0;
    if (v >= 255.0f) return 255;
    return (uint8_t)(v + 0.5f);      /* round to nearest */
}

int compositor_composite_over(uint8_t *dst_rgba,
                              const uint8_t *src_rgba,
                              const uint8_t *coverage,
                              size_t width,
                              size_t height,
                              size_t stride,
                              float opacity) {
    if (width == 0 || height == 0 || stride < width * 4) return -1;
    if (dst_rgba == NULL || src_rgba == NULL) return -1;

    float op = opacity < 0.0f ? 0.0f : (opacity > 1.0f ? 1.0f : opacity);

    for (size_t y = 0; y < height; y++) {
        size_t row_offset = y * stride;
        size_t coverage_row_offset = y * width;
        for (size_t x = 0; x < width; x++) {
            size_t d = row_offset + x * 4;
            float sr = (float)src_rgba[d];
            float sg = (float)src_rgba[d + 1];
            float sb = (float)src_rgba[d + 2];
            float sa = (float)src_rgba[d + 3];
            float dr = (float)dst_rgba[d];
            float dg = (float)dst_rgba[d + 1];
            float db = (float)dst_rgba[d + 2];
            float da = (float)dst_rgba[d + 3];

            float cov = coverage ? (float)coverage[coverage_row_offset + x] / 255.0f : 1.0f;
            float effective_alpha = (sa / 255.0f) * cov * op;
            float inverse = 1.0f - effective_alpha;

            dst_rgba[d]     = clampu8(sr * cov * op + dr * inverse);
            dst_rgba[d + 1] = clampu8(sg * cov * op + dg * inverse);
            dst_rgba[d + 2] = clampu8(sb * cov * op + db * inverse);
            dst_rgba[d + 3] = clampu8(sa * cov * op + da * inverse);
        }
    }
    return 0;
}