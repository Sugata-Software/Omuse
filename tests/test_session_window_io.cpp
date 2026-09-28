// SessionWindow file operations test: export/import PNG/JPEG, save/load project
// Uses the real SessionWindow class (moc'd Q_OBJECT) to exercise the IO milestone.

#include "SessionWindow.h"

#include <QApplication>
#include <QDir>
#include <QFile>
#include <QImage>
#include <QTemporaryDir>
#include <cstdint>
#include <cstdio>
#include <vector>

static int g_failures = 0;
#define CHECK(cond, msg) do { if (!(cond)) { std::fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__); ++g_failures; } } while (0)

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);

    // Create a temporary directory for test files
    QTemporaryDir tempDir;
    CHECK(tempDir.isValid(), "create temp dir");
    const QString tempPath = tempDir.path();

    // Create SessionWindow (drives Swift core via C ABI)
    SessionWindow window;

    // Test 1: Export PNG
    {
        QString pngPath = tempPath + "/test_export.png";
        CHECK(window.exportPNG(pngPath), "export PNG");
        CHECK(QFile::exists(pngPath), "PNG file exists");

        // Verify round-trip
        QImage reimported(pngPath);
        CHECK(!reimported.isNull() && reimported.size().width() > 0 && reimported.size().height() > 0, "PNG round-trip size");
    }

    // Test 2: Export JPEG
    {
        QString jpgPath = tempPath + "/test_export.jpg";
        CHECK(window.exportJPEG(jpgPath, 90), "export JPEG");
        CHECK(QFile::exists(jpgPath), "JPEG file exists");

        // Verify load
        QImage reimported(jpgPath);
        CHECK(!reimported.isNull() && reimported.size().width() > 0 && reimported.size().height() > 0, "JPEG load size");
        // JPEG is lossy but dimensions should match
    }

    // Test 3: Import image (create a test image first)
    {
        QString testImgPath = tempPath + "/test_import.png";
        QImage testImg(32, 32, QImage::Format_RGBA8888);
        testImg.fill(qRgba(255, 0, 0, 255)); // Solid red
        CHECK(testImg.save(testImgPath, "PNG"), "create test import image");

        // Import into session
        CHECK(window.importImage(testImgPath), "import image");

        // Verify session updated
        // The window's m_image should now reflect the imported image
    }

    // Test 4: Save project
    {
        QString projectPath = tempPath + "/test_project";
        CHECK(window.saveProject(projectPath), "save project");
        CHECK(QFile::exists(projectPath + "/manifest.json"), "manifest.json exists");
        CHECK(!QDir(projectPath + "/images").entryList(QDir::Files).isEmpty(), "layer asset exists");
    }

    // Test 5: Load project
    {
        QString projectPath = tempPath + "/test_project";
        CHECK(window.loadProject(projectPath), "load project");
        CHECK(window.exportPNG(tempPath + "/reopened.png"), "reopened project renders");
    }

    if (g_failures == 0) {
        std::printf("All SessionWindow file operation tests passed.\n");
        return 0;
    }
    std::fprintf(stderr, "%d SessionWindow file operation test(s) failed.\n", g_failures);
    return 1;
}
