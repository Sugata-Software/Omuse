// Host integration test: the open -> canonicalize -> filter -> save -> reopen
// journey using the real C kernels, no GUI. This is the host-verifiable slice of
// the file-map's "First packaged open/paint/undo/save/reopen/export journey"
// workstream. Qt does the codec decode (the ImageImporter/ImageExporter Swift
// adapters are the SDK-side replacement); CanonicalBuffer canonicalizes pixels
// to the kernel contract; the reused macOS C kernels do the pixel work.

#include "CanonicalBuffer.h"

extern "C" {
#include "NoisePixels.h"
#include "LensPixels.h"
#include "AdjustPixels.h"
}

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QImage>
#include <QTemporaryFile>
#include <cstdint>
#include <cstring>
#include <vector>

static int g_failures = 0;
#define CHECK(cond, msg) do { if (!(cond)) { std::fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__); ++g_failures; } } while (0)

int main(int argc, char *argv[]) {
    QCoreApplication app(argc, argv);

    // 1. Open: synthesize a 4x4 opaque image and round-trip it through a PNG file,
    //    the way QImage would load a real asset.
    QImage src(4, 4, QImage::Format_RGB32);
    src.fill(qRgba(100, 150, 200, 255));
    QTemporaryFile tmp(QDir::tempPath() + "/comp_XXXXXX.png");
    CHECK(tmp.open(), "open temp file");
    const QString path = tmp.fileName();
    CHECK(src.save(path, "PNG"), "save source PNG");

    // 2. Reopen (the "open" step of the journey).
    QImage loaded(path);
    CHECK(!loaded.isNull() && loaded.width() == 4 && loaded.height() == 4, "reopen PNG");

    // 3. Canonicalize to the kernel contract (premultiplied RGBA, stride == w*4).
    std::vector<uint8_t> a = compositor::canonical_from_qimage(loaded);
    CHECK(a.size() == (size_t)4 * 4 * 4, "canonical size");

    // Opaque image: premultiplied == straight, so canonical == (R,G,B,255).
    CHECK(a[0] == 100 && a[1] == 150 && a[2] == 200 && a[3] == 255, "canonical premultiplied values");

    // 4. Filter: apply noise (in place) — pixels must change but alpha must not.
    std::vector<uint8_t> b = a;
    noise_add(b.data(), 4, 4, 16, 80.0f, 0, 0, 42);
    CHECK(std::memcmp(a.data(), b.data(), b.size()) != 0, "noise changes pixels");
    for (size_t i = 0; i < b.size(); i += 4) CHECK(b[i + 3] == 255, "noise preserves alpha");

    // 5. Filter: lens_distort k=0 must be identity (separate src/dst path).
    std::vector<uint8_t> srcC = a, outC(a.size());
    lens_distort(srcC.data(), outC.data(), 4, 4, 16, 0.0);
    CHECK(std::memcmp(a.data(), outC.data(), a.size()) == 0, "lens k=0 identity through host buffer");

    // 6. Save the filtered image and reopen it.
    QImage display = compositor::qimage_from_canonical(b.data(), 4, 4);
    CHECK(!display.isNull(), "build display image from canonical");
    const QString outPath = path + ".out.png";
    CHECK(display.convertToFormat(QImage::Format_RGBA8888).save(outPath, "PNG"), "save filtered PNG");
    QImage reopened(outPath);
    CHECK(!reopened.isNull() && reopened.width() == 4 && reopened.height() == 4, "reopen filtered PNG");
    QFile::remove(outPath);

    if (g_failures == 0) { std::printf("All host journey tests passed.\n"); return 0; }
    std::fprintf(stderr, "%d host journey test(s) failed.\n", g_failures);
    return 1;
}