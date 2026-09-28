#ifndef OpenCVBridge_h
#define OpenCVBridge_h

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Content-aware inpainting / fill bridge (plan Stage 10).
 * Inpaints masked pixels (mask != 0) using exemplar patch synthesis or OpenCV cv::inpaint.
 *
 * Parameters:
 *   rgba         premultiplied RGBA8 image buffer, modified in place.
 *   rgba_stride  bytes per row of rgba (must be >= width * 4).
 *   mask         grayscale mask where non-zero indicates pixels to inpaint.
 *   mask_stride  bytes per row of mask (must be >= width).
 *   width, height dimensions in pixels.
 *
 * Returns 1 on success, 0 when no source patch exists, -1 on invalid argument or memory failure.
 */
int compositor_content_aware_fill(uint8_t *rgba, size_t rgba_stride,
                                  const uint8_t *mask, size_t mask_stride,
                                  int width, int height);

/*
 * Guided filter for edge-preserving matte refinement (plan Stage 10).
 * Refines a coarse matte against a guide image (He, Sun & Tang).
 *
 * Parameters:
 *   guide        normalized grayscale guide values [0.0, 1.0], width * height floats.
 *   source       input matte values [0.0, 1.0], width * height floats.
 *   output       filtered output matte values [0.0, 1.0], width * height floats.
 *   width, height dimensions.
 *   radius       window radius for local linear model.
 *   eps          regularization parameter (e.g. 1e-4).
 *
 * Returns 0 on success, -1 on error.
 */
int compositor_guided_filter(const float *guide, const float *source, float *output,
                             int width, int height, int radius, float eps);

/*
 * Returns 1 if native OpenCV is linked and active, 0 if using deterministic C/CPU fallback.
 */
int compositor_has_opencv(void);

#ifdef __cplusplus
}
#endif

#endif /* OpenCVBridge_h */
