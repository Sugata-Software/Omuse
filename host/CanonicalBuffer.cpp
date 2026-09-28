#include "CanonicalBuffer.h"
#include <cstring>

namespace compositor {

std::vector<uint8_t> canonical_from_qimage(const QImage &img) {
    QImage src = img.convertToFormat(QImage::Format_RGBA8888); // unpremult R,G,B,A
    const int w = src.width(), h = src.height();
    std::vector<uint8_t> out((size_t)w * h * 4);
    for (int y = 0; y < h; ++y) {
        const uint8_t *s = src.constScanLine(y);
        uint8_t *d = out.data() + (size_t)y * w * 4;
        for (int x = 0; x < w; ++x) {
            unsigned r = s[x * 4 + 0], g = s[x * 4 + 1], b = s[x * 4 + 2], a = s[x * 4 + 3];
            d[x * 4 + 0] = (uint8_t)((r * a + 127u) / 255u); // premultiply
            d[x * 4 + 1] = (uint8_t)((g * a + 127u) / 255u);
            d[x * 4 + 2] = (uint8_t)((b * a + 127u) / 255u);
            d[x * 4 + 3] = (uint8_t)a;
        }
    }
    return out;
}

QImage qimage_from_canonical(const uint8_t *rgba, int width, int height) {
    QImage img(width, height, QImage::Format_RGBA8888_Premultiplied);
    for (int y = 0; y < height; ++y)
        std::memcpy(img.scanLine(y), rgba + (size_t)y * width * 4, (size_t)width * 4);
    return img;
}

}