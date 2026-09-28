// Canonical-buffer adapters between Qt images and the portable C kernels.
//
// Canonical tile contract (from the port plan): 8-bit premultiplied RGBA sRGB,
// R,G,B,A byte order, stride == width*4. The C kernels assert this at entry
// (ENG-17). These adapters produce and consume that layout.
//
// File-map role: ImageImporter/ImageExporter are "Apple replacement / adaptation"
// Swift files ("Replace Apple decoding/color conversion; bounded codec adapter to
// canonical pixels") that run under the Freedesktop Swift runtime extension. On
// this build host (no Swift toolchain) Qt does the codec decode and these C++
// adapters canonicalize the pixels — a host stand-in for the Swift adapters, not a
// production replacement. The byte math mirrors what the Swift core will do.

#pragma once
#include <QImage>
#include <cstdint>
#include <vector>

namespace compositor {

// Any QImage format -> tightly packed premultiplied RGBA (stride == width*4).
std::vector<uint8_t> canonical_from_qimage(const QImage &img);

// Tightly packed premultiplied RGBA -> a displayable QImage (RGBA8888_Premultiplied).
QImage qimage_from_canonical(const uint8_t *rgba, int width, int height);

}