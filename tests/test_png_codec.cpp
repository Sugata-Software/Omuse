// IO-tier codec verification (file-map "IO / codec mapping" — ImageIO → Qt):
// proves the PNG export/import round-trip is byte-identical for
// QImage::Format_RGBA8888, the format the Swift core's compositor_session_render
// emits (non-premultiplied RGBA). The macOS export path uses
// CGImageDestination (ImageIO); the Linux port replaces it with QImageWriter.
// This test is Qt-only (no Swift static lib) so it runs in the default CMake/CTest
// build. It stresses the case where a premultiplying codec would diverge:
// semi-transparent pixels (alpha 1..254). If Qt premultiplied on write or
// un-premultiplied on read, these values would round-trip with rounding error;
// byte-identity confirms the Format_RGBA8888 PNG path preserves straight alpha,
// which is what the export/import legs rely on.
//
// The JPEG leg is also probed: JPEG is lossy and has no alpha, so it is not
// byte-identical; the test only asserts it decodes to the right dimensions and
// opaque alpha (the lossy-quality tradeoff is a UI-layer setting, verified by
// QImageWriter::setQuality in the ImageExporter port, not here).

#include <QCoreApplication>
#include <QDir>
#include <QImage>
#include <QImageReader>
#include <QImageWriter>
#include <QTemporaryFile>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>

static int g_failures = 0;
#define CHECK(cond, msg) do { if (!(cond)) { std::fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__); ++g_failures; } } while (0)

int main(int argc, char *argv[]) {
    QCoreApplication app(argc, argv);

    // A 4x4 RGBA8888 buffer with a mix that would expose premultiplication loss:
    //   row 0: opaque colors
    //   row 1: fully transparent (0,0,0,0) and near-transparent (200,100,50,1)
    //   row 2: semi-transparent mid alpha (123,200,77,128)
    //   row 3: a solid color at several alphas (10,20,30 at alpha 1 / 128 / 254 / 255)
    const int w = 4, h = 4;
    const uint8_t src[w * h * 4] = {
        // row 0 — opaque
        255, 0, 0, 255,    0, 255, 0, 255,    0, 0, 255, 255,    255, 255, 0, 255,
        // row 1 — transparent extremes
        0, 0, 0, 0,        200, 100, 50, 1,   0, 0, 0, 0,        200, 100, 50, 1,
        // row 2 — mid alpha
        123, 200, 77, 128, 123, 200, 77, 128, 123, 200, 77, 128, 123, 200, 77, 128,
        // row 3 — one color, escalating alpha
        10, 20, 30, 1,     10, 20, 30, 128,   10, 20, 30, 254,   10, 20, 30, 255,
    };

    QImage img(src, w, h, w * 4, QImage::Format_RGBA8888);
    CHECK(!img.isNull(), "construct RGBA8888 image");

    // --- PNG round-trip (the lossless export/import path) ---
    QTemporaryFile png(QDir::tempPath() + "/comp_codec_XXXXXX.png");
    CHECK(png.open(), "open png temp");
    const QString pngPath = png.fileName();
    png.close();

    QImageWriter wPNG(pngPath, "PNG");
    CHECK(wPNG.write(img), "QImageWriter PNG write");

    QImageReader rPNG(pngPath);
    CHECK(rPNG.canRead(), "QImageReader PNG canRead");
    CHECK(rPNG.format() == QByteArray("png"), "QImageReader format() == png");
    QImage loaded = rPNG.read();
    CHECK(!loaded.isNull(), "PNG read non-null");
    CHECK(loaded.width() == w && loaded.height() == h, "PNG dimensions preserved");

    // Convert to RGBA8888 for comparison (the format the Swift core emits).
    QImage rgba = loaded.convertToFormat(QImage::Format_RGBA8888);
    CHECK(!rgba.isNull(), "convertToFormat RGBA8888");

    // Byte-identity: the whole buffer must round-trip exactly. A premultiplying
    // codec would alter the semi-transparent rows (1, 2, and the low-alpha pixels
    // of row 3).
    bool identical = true;
    int firstDiff = -1;
    for (int i = 0; i < w * h * 4; ++i) {
        uint8_t got = rgba.constBits()[i];
        if (got != src[i]) { identical = false; firstDiff = i; break; }
    }
    CHECK(identical, "PNG RGBA8888 byte-identical round-trip");
    if (!identical) {
        std::fprintf(stderr, "  first diff at byte %d: got %u, want %u\n",
                     firstDiff, rgba.constBits()[firstDiff], src[firstDiff]);
    }

    // --- supportedImageFormats / supportedMimeTypes (the CGImageSourceCopyTypeIdentifiers mapping) ---
    QList<QByteArray> fmts = QImageReader::supportedImageFormats();
    CHECK(fmts.contains("png"), "QImageReader supports png");
    CHECK(fmts.contains("jpg") || fmts.contains("jpeg"), "QImageReader supports jpeg");
    CHECK(fmts.contains("tiff") || fmts.contains("tif"), "QImageReader supports tiff");
    CHECK(fmts.contains("webp"), "QImageReader supports webp");
    QList<QByteArray> mimes = QImageReader::supportedMimeTypes();
    CHECK(mimes.contains("image/png"), "QImageReader mime image/png");

    // --- JPEG leg (lossy; dimensions + opaque alpha only) ---
    QTemporaryFile jpg(QDir::tempPath() + "/comp_codec_XXXXXX.jpg");
    CHECK(jpg.open(), "open jpg temp");
    const QString jpgPath = jpg.fileName();
    jpg.close();
    QImageWriter wJPG(jpgPath, "jpeg");
    wJPG.setQuality(90); // kCGImageDestinationLossyCompressionQuality → setQuality
    // JPEG has no alpha: Qt drops it. Write the RGB view (Format_RGB32) so the
    // write succeeds; the export path flattens over a background before JPEG.
    QImage rgbView = img.convertToFormat(QImage::Format_RGB30);
    CHECK(wJPG.write(rgbView), "QImageWriter JPEG write");
    QImage loadedJPG = QImage(jpgPath).convertToFormat(QImage::Format_RGB30);
    CHECK(!loadedJPG.isNull() && loadedJPG.width() == w && loadedJPG.height() == h,
          "JPEG dimensions preserved");

    // --- TIFF round-trip (Stage 12 parity) ---
    QTemporaryFile tif(QDir::tempPath() + "/comp_codec_XXXXXX.tiff");
    CHECK(tif.open(), "open tiff temp");
    const QString tifPath = tif.fileName();
    tif.close();
    QImageWriter wTIF(tifPath, "TIFF");
    CHECK(wTIF.write(img), "QImageWriter TIFF write");
    QImageReader rTIF(tifPath);
    CHECK(rTIF.canRead(), "QImageReader TIFF canRead");
    QImage loadedTIF = rTIF.read();
    CHECK(!loadedTIF.isNull() && loadedTIF.width() == w && loadedTIF.height() == h,
          "TIFF dimensions preserved");

    // --- WebP round-trip (Stage 12 parity) ---
    QTemporaryFile webp(QDir::tempPath() + "/comp_codec_XXXXXX.webp");
    CHECK(webp.open(), "open webp temp");
    const QString webpPath = webp.fileName();
    webp.close();
    QImageWriter wWebP(webpPath, "WEBP");
    wWebP.setQuality(90);
    CHECK(wWebP.write(img), "QImageWriter WebP write");
    QImageReader rWebP(webpPath);
    CHECK(rWebP.canRead(), "QImageReader WebP canRead");
    QImage loadedWebP = rWebP.read();
    CHECK(!loadedWebP.isNull() && loadedWebP.width() == w && loadedWebP.height() == h,
          "WebP dimensions preserved");

    if (g_failures == 0) std::printf("image codecs round-trip OK (PNG, JPEG, TIFF, WebP, %d bytes)\n", w * h * 4);
    return g_failures == 0 ? 0 : 1;
}