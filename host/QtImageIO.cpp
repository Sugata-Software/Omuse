// Qt image plugins as the ImageIO compat backend. Registered into the Swift core at startup so
// CGImageSource / CGImageDestination (used by upstream's importer, exporter and project store) decode and encode
// every format Qt supports (PNG, JPEG, TIFF, WebP, BMP, GIF, ...), not just the portable PNG codec.

#include <QBuffer>
#include <QByteArray>
#include <QImage>
#include <QImageReader>
#include <QImageWriter>
#include <QPainter>
#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <cstdint>
#include <vector>

// HEIF/HEIC/AVIF go through libheif directly: the Qt plugin for them is not dependable (it swaps the dimensions of a
// 64x32 image on a round trip). libheif is optional; without it those formats are simply not handled here.
#if defined(__has_include)
#if __has_include(<libheif/heif.h>)
#define COMPOSITOR_HAS_LIBHEIF 1
#include <libheif/heif.h>
#endif
#endif

extern "C" {
typedef int32_t (*CompositorImageDecodeFn)(const uint8_t *, size_t, uint8_t **, int32_t *, int32_t *, int32_t *, double *,
                                           int32_t *, char *, size_t);
typedef int32_t (*CompositorImageEncodeFn)(const uint8_t *, int32_t, int32_t, int32_t, const char *, double, double,
                                           int32_t, uint8_t **, size_t *);
// Provided by the Swift core; weak so C++-only builds (no Swift) still link.
void compositor_imageio_register(CompositorImageDecodeFn, CompositorImageEncodeFn) __attribute__((weak));
}

namespace {

bool isHeifContainer(const uint8_t *d, size_t n) {
    if (n < 12 || memcmp(d + 4, "ftyp", 4) != 0) return false;
    static const char *brands[] = {"heic", "heix", "hevc", "hevx", "mif1", "msf1", "avif", "avis"};
    for (const char *b : brands) if (memcmp(d + 8, b, 4) == 0) return true;
    return false;
}

#if defined(COMPOSITOR_HAS_LIBHEIF)
int32_t decodeHeif(const uint8_t *data, size_t length, uint8_t **out, int32_t *width, int32_t *height, int32_t *channels,
                   int32_t *orientation, char *uti, size_t utiCapacity) {
    heif_context *ctx = heif_context_alloc();
    heif_image_handle *handle = nullptr;
    heif_image *img = nullptr;
    int32_t rc = -3;
    if (heif_context_read_from_memory_without_copy(ctx, data, length, nullptr).code == heif_error_Ok &&
        heif_context_get_primary_image_handle(ctx, &handle).code == heif_error_Ok) {
        heif_decoding_options *opts = heif_decoding_options_alloc();
        // libheif applies the file's rotation/mirroring itself, so the pixels come back upright.
        if (heif_decode_image(handle, &img, heif_colorspace_RGB, heif_chroma_interleaved_RGBA, opts).code == heif_error_Ok) {
            int stride = 0;
            const uint8_t *src = heif_image_get_plane_readonly(img, heif_channel_interleaved, &stride);
            const int w = heif_image_get_width(img, heif_channel_interleaved), h = heif_image_get_height(img, heif_channel_interleaved);
            auto *pixels = static_cast<uint8_t *>(malloc(static_cast<size_t>(w) * h * 4));
            if (src && pixels && w > 0 && h > 0) {
                for (int y = 0; y < h; ++y) {
                    const uint8_t *row = src + static_cast<size_t>(y) * stride;
                    uint8_t *dst = pixels + static_cast<size_t>(y) * w * 4;
                    for (int x = 0; x < w; ++x) {  // straight -> premultiplied
                        const int a = row[x * 4 + 3];
                        dst[x * 4] = static_cast<uint8_t>((row[x * 4] * a + 127) / 255);
                        dst[x * 4 + 1] = static_cast<uint8_t>((row[x * 4 + 1] * a + 127) / 255);
                        dst[x * 4 + 2] = static_cast<uint8_t>((row[x * 4 + 2] * a + 127) / 255);
                        dst[x * 4 + 3] = static_cast<uint8_t>(a);
                    }
                }
                *out = pixels;
                if (width) *width = w;
                if (height) *height = h;
                if (channels) *channels = 4;
                if (orientation) *orientation = 1;
                const bool avif = length >= 12 && (memcmp(data + 8, "avif", 4) == 0 || memcmp(data + 8, "avis", 4) == 0);
                if (uti && utiCapacity) { strncpy(uti, avif ? "public.avif" : "public.heic", utiCapacity - 1); uti[utiCapacity - 1] = 0; }
                rc = 0;
            } else {
                free(pixels);
            }
        }
        heif_decoding_options_free(opts);
    }
    if (img) heif_image_release(img);
    if (handle) heif_image_handle_release(handle);
    heif_context_free(ctx);
    return rc;
}

int32_t encodeHeif(const uint8_t *pixels, int32_t width, int32_t height, int32_t channels, double quality,
                   uint8_t **out, size_t *outLength) {
    if (channels != 4) return -1;
    heif_context *ctx = heif_context_alloc();
    heif_encoder *encoder = nullptr;
    // HEVC is what a .heic normally holds; where only an AV1 encoder is installed the same HEIF container carries AV1.
    if (heif_context_get_encoder_for_format(ctx, heif_compression_HEVC, &encoder).code != heif_error_Ok)
        heif_context_get_encoder_for_format(ctx, heif_compression_AV1, &encoder);
    int32_t rc = -2;
    heif_image *img = nullptr;
    if (encoder && heif_image_create(width, height, heif_colorspace_RGB, heif_chroma_interleaved_RGBA, &img).code == heif_error_Ok &&
        heif_image_add_plane(img, heif_channel_interleaved, width, height, 8).code == heif_error_Ok) {
        int stride = 0;
        uint8_t *dst = heif_image_get_plane(img, heif_channel_interleaved, &stride);
        for (int y = 0; y < height; ++y)
            for (int x = 0; x < width; ++x) {  // premultiplied -> straight
                const uint8_t *p = pixels + (static_cast<size_t>(y) * width + x) * 4;
                uint8_t *q = dst + static_cast<size_t>(y) * stride + x * 4;
                const int a = p[3];
                for (int c = 0; c < 3; ++c) q[c] = a == 0 ? 0 : static_cast<uint8_t>(std::min(255, (p[c] * 255 + a / 2) / a));
                q[3] = static_cast<uint8_t>(a);
            }
        if (quality >= 0) heif_encoder_set_lossy_quality(encoder, static_cast<int>(quality * 100 + 0.5));
        else heif_encoder_set_lossless(encoder, 1);
        heif_encoding_options *opts = heif_encoding_options_alloc();
        heif_image_handle *handle = nullptr;
        if (heif_context_encode_image(ctx, img, encoder, opts, &handle).code == heif_error_Ok) {
            struct Writer { std::vector<uint8_t> bytes; } writer;
            heif_writer w{};
            w.writer_api_version = 1;
            w.write = [](heif_context *, const void *d, size_t n, void *ud) -> heif_error {
                auto *wr = static_cast<Writer *>(ud);
                wr->bytes.insert(wr->bytes.end(), static_cast<const uint8_t *>(d), static_cast<const uint8_t *>(d) + n);
                return {heif_error_Ok, heif_suberror_Unspecified, "ok"};
            };
            if (heif_context_write(ctx, &w, &writer).code == heif_error_Ok && !writer.bytes.empty()) {
                auto *copy = static_cast<uint8_t *>(malloc(writer.bytes.size()));
                if (copy) {
                    memcpy(copy, writer.bytes.data(), writer.bytes.size());
                    *out = copy;
                    *outLength = writer.bytes.size();
                    rc = 0;
                }
            }
            heif_image_handle_release(handle);
        }
        heif_encoding_options_free(opts);
    }
    if (img) heif_image_release(img);
    if (encoder) heif_encoder_release(encoder);
    heif_context_free(ctx);
    return rc;
}
#endif

const char *utiForFormat(const QByteArray &format) {
    const QByteArray f = format.toLower();
    if (f == "png") return "public.png";
    if (f == "jpg" || f == "jpeg") return "public.jpeg";
    if (f == "tif" || f == "tiff") return "public.tiff";
    if (f == "webp") return "org.webmproject.webp";
    if (f == "bmp") return "com.microsoft.bmp";
    if (f == "gif") return "com.compuserve.gif";
    if (f == "heic" || f == "heif") return "public.heic";
    return nullptr;
}

const char *formatForUti(const char *uti) {
    if (!uti) return nullptr;
    if (!strcmp(uti, "public.png")) return "png";
    if (!strcmp(uti, "public.jpeg")) return "jpeg";
    if (!strcmp(uti, "public.tiff")) return "tiff";
    if (!strcmp(uti, "org.webmproject.webp")) return "webp";
    if (!strcmp(uti, "com.microsoft.bmp")) return "bmp";
    if (!strcmp(uti, "com.compuserve.gif")) return "gif";
    if (!strcmp(uti, "public.heic") || !strcmp(uti, "public.heif")) return "heic";
    return nullptr;
}

// Qt's transformation flags -> EXIF orientation 1...8.
int exifOrientation(QImageIOHandler::Transformations t) {
    const bool mirror = t.testFlag(QImageIOHandler::TransformationMirror);
    const bool flip = t.testFlag(QImageIOHandler::TransformationFlip);
    const bool rotate = t.testFlag(QImageIOHandler::TransformationRotate90);
    if (!rotate) return mirror && flip ? 3 : mirror ? 2 : flip ? 4 : 1;
    return mirror && flip ? 7 : mirror ? 5 : flip ? 7 : 6;
}

// EXIF orientation 1...8 -> Qt's transformation flags (inverse of exifOrientation).
QImageIOHandler::Transformations transformationForExif(int o) {
    using T = QImageIOHandler;
    switch (o) {
        case 2: return T::TransformationMirror;
        case 3: return T::TransformationRotate180;
        case 4: return T::TransformationFlip;
        case 5: return T::TransformationFlipAndRotate90;
        case 6: return T::TransformationRotate90;
        case 7: return T::TransformationMirrorAndRotate90;
        case 8: return T::TransformationRotate270;
        default: return T::TransformationNone;
    }
}

int32_t decodeImage(const uint8_t *data, size_t length, uint8_t **out, int32_t *width, int32_t *height,
                    int32_t *channels, double *dpi, int32_t *orientation, char *uti, size_t utiCapacity) {
    if (!data || !length || !out) return -1;
#if defined(COMPOSITOR_HAS_LIBHEIF)
    if (isHeifContainer(data, length)) return decodeHeif(data, length, out, width, height, channels, orientation, uti, utiCapacity);
#endif
    QByteArray bytes = QByteArray::fromRawData(reinterpret_cast<const char *>(data), static_cast<qsizetype>(length));
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::ReadOnly);
    QImageReader reader(&buffer);
    reader.setAutoTransform(false);
    const char *type = utiForFormat(reader.format());
    if (!type) return -2;
    const auto transform = reader.transformation();
    QImage image = reader.read();
    if (image.isNull()) return -3;

    // Gray only when the file itself is gray; an RGB file whose pixels happen to be neutral stays colour.
    const bool gray = image.format() == QImage::Format_Grayscale8 || image.format() == QImage::Format_Grayscale16;
    QImage converted = gray ? image.convertToFormat(QImage::Format_Grayscale8)
                            : image.convertToFormat(QImage::Format_RGBA8888_Premultiplied);
    const int ch = gray ? 1 : 4;
    const size_t bytesPerLine = static_cast<size_t>(converted.width()) * ch;
    auto *pixels = static_cast<uint8_t *>(malloc(bytesPerLine * converted.height()));
    if (!pixels) return -4;
    for (int y = 0; y < converted.height(); ++y) memcpy(pixels + y * bytesPerLine, converted.constScanLine(y), bytesPerLine);
    *out = pixels;
    if (width) *width = converted.width();
    if (height) *height = converted.height();
    if (channels) *channels = ch;
    if (dpi) *dpi = image.dotsPerMeterX() > 0 ? image.dotsPerMeterX() * 0.0254 : 0.0;
    if (orientation) *orientation = exifOrientation(transform);
    if (uti && utiCapacity) { strncpy(uti, type, utiCapacity - 1); uti[utiCapacity - 1] = 0; }
    return 0;
}

int32_t encodeImage(const uint8_t *pixels, int32_t width, int32_t height, int32_t channels, const char *uti,
                    double quality, double dpi, int32_t orientation, uint8_t **out, size_t *outLength) {
#if defined(COMPOSITOR_HAS_LIBHEIF)
    if (uti && pixels && width > 0 && height > 0 && out && outLength &&
        (!strcmp(uti, "public.heic") || !strcmp(uti, "public.heif") || !strcmp(uti, "public.avif")))
        return encodeHeif(pixels, width, height, channels, quality, out, outLength);
#endif
    const char *format = formatForUti(uti);
    if (!pixels || width <= 0 || height <= 0 || !format || !out || !outLength) return -1;
    QImage image;
    if (channels == 1) {
        image = QImage(pixels, width, height, width, QImage::Format_Grayscale8).copy();
    } else if (channels == 4) {
        image = QImage(pixels, width, height, width * 4, QImage::Format_RGBA8888_Premultiplied).copy();
    } else {
        return -1;
    }
    if (dpi > 0) {
        const int dpm = static_cast<int>(dpi / 0.0254 + 0.5);
        image.setDotsPerMeterX(dpm);
        image.setDotsPerMeterY(dpm);
    }
    QByteArray bytes;
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::WriteOnly);
    QImageWriter writer(&buffer, format);
    if (quality >= 0) writer.setQuality(static_cast<int>(quality * 100 + 0.5));
    if (orientation > 1 && orientation <= 8) writer.setTransformation(transformationForExif(orientation));
    // JPEG has no alpha: flatten onto white like ImageIO does for opaque destinations.
    if (!strcmp(format, "jpeg") && image.hasAlphaChannel()) {
        QImage flat(image.size(), QImage::Format_RGB32);
        flat.fill(Qt::white);
        QPainter p(&flat);
        p.drawImage(0, 0, image);
        p.end();
        image = flat;
    }
    if (!writer.write(image)) return -2;
    auto *copy = static_cast<uint8_t *>(malloc(static_cast<size_t>(bytes.size())));
    if (!copy) return -3;
    memcpy(copy, bytes.constData(), static_cast<size_t>(bytes.size()));
    *out = copy;
    *outLength = static_cast<size_t>(bytes.size());
    return 0;
}

}  // namespace

extern "C" int compositor_qt_imageio_install(void) {
    if (!compositor_imageio_register) return 0;
    compositor_imageio_register(decodeImage, encodeImage);
    return 1;
}

// For loaders that cannot be called back into (a test process that dlopens this library): hand out the callbacks.
extern "C" int compositor_qt_imageio_functions(CompositorImageDecodeFn *decode, CompositorImageEncodeFn *encode) {
    if (!decode || !encode) return 0;
    *decode = decodeImage;
    *encode = encodeImage;
    return 1;
}

// Round-trips through the same callbacks the Swift core uses. Returns 0 when every supported format behaves.
extern "C" int compositor_qt_imageio_selftest(void) {
    const int w = 16, h = 12;
    std::vector<uint8_t> rgba(static_cast<size_t>(w) * h * 4);
    for (int y = 0; y < h; ++y)
        for (int x = 0; x < w; ++x) {
            const uint8_t a = 255;
            uint8_t *p = &rgba[(static_cast<size_t>(y) * w + x) * 4];
            p[0] = static_cast<uint8_t>(x * 15); p[1] = static_cast<uint8_t>(y * 20); p[2] = 90; p[3] = a;
        }
    struct Case { const char *uti; bool lossless; };
    for (const Case &c : {Case{"public.png", true}, Case{"public.tiff", true}, Case{"public.jpeg", false}}) {
        uint8_t *encoded = nullptr; size_t length = 0;
        if (encodeImage(rgba.data(), w, h, 4, c.uti, c.lossless ? -1 : 0.95, 144.0, 1, &encoded, &length) != 0) return 1;
        uint8_t *decoded = nullptr; int32_t dw = 0, dh = 0, ch = 0, orient = 0; double dpi = 0; char uti[96] = {0};
        const int32_t rc = decodeImage(encoded, length, &decoded, &dw, &dh, &ch, &dpi, &orient, uti, sizeof(uti));
        free(encoded);
        if (rc != 0 || dw != w || dh != h || ch != 4 || strcmp(uti, c.uti) != 0) { free(decoded); return 2; }
        if (c.lossless) {
            if (memcmp(decoded, rgba.data(), rgba.size()) != 0) { free(decoded); return 3; }
            if (dpi < 143.0 || dpi > 145.0) { free(decoded); return 4; }
        } else {
            long diff = 0;
            for (size_t i = 0; i < rgba.size(); ++i) diff += std::abs(int(decoded[i]) - int(rgba[i]));
            if (diff / static_cast<long>(rgba.size()) > 12) { free(decoded); return 5; }
        }
        free(decoded);
    }
    // A gray plane stays a one-channel mask through PNG.
    std::vector<uint8_t> plane(static_cast<size_t>(w) * h);
    for (size_t i = 0; i < plane.size(); ++i) plane[i] = static_cast<uint8_t>(i);
    uint8_t *encoded = nullptr; size_t length = 0;
    if (encodeImage(plane.data(), w, h, 1, "public.png", -1, 0, 1, &encoded, &length) != 0) return 6;
    uint8_t *decoded = nullptr; int32_t dw = 0, dh = 0, ch = 0, orient = 0; double dpi = 0; char uti[96] = {0};
    const int32_t rc = decodeImage(encoded, length, &decoded, &dw, &dh, &ch, &dpi, &orient, uti, sizeof(uti));
    free(encoded);
    const bool ok = rc == 0 && ch == 1 && dw == w && memcmp(decoded, plane.data(), plane.size()) == 0;
    free(decoded);
    return ok ? 0 : 7;
}
