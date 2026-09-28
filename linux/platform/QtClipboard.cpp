// QtClipboard.cpp — platform clipboard adapter (ENG-2).
// Qt provides QClipboard + QMimeData which covers the full clipboard
// contract: text, images, color values, custom formats.
// The Swift core uses NSPasteboard on macOS; on Linux this adapter
// bridges to Qt's system clipboard.

#include "QtClipboard.h"
#include <QApplication>
#include <QClipboard>
#include <QMimeData>

// Map Swift clipboard types to Qt MIME types.
static QString swift_type_to_mime(const char *swift_type) {
    if (swift_type == nullptr) return {};
    // Swift uses uniform type identifiers; we map the most common ones.
    if (strcmp(swift_type, "public.text") == 0 || strcmp(swift_type, "public.plain-text") == 0)
        return "text/plain";
    if (strcmp(swift_type, "public.image") == 0)
        return "image/*";
    if (strcmp(swift_type, "public.png") == 0)
        return "image/png";
    if (strcmp(swift_type, "public.jpeg") == 0)
        return "image/jpeg";
    return {};
}

void platform_set_clipboard_text(const char *text, size_t count) {
    if (!text) return;
    QClipboard *clipboard = QApplication::clipboard();
    if (!clipboard) return;
    clipboard->setText(QString::fromUtf8(text, count));
}

std::string platform_get_clipboard_text() {
    QClipboard *clipboard = QApplication::clipboard();
    if (!clipboard) return {};
    QString qtxt = clipboard->text();
    return qtxt.toUtf8().constData();
}

void platform_set_clipboard_image(const uint8_t *pixels, size_t count, int width, int height) {
    if (!pixels || count < (size_t)(width * height * 4)) return;
    QClipboard *clipboard = QApplication::clipboard();
    if (!clipboard) return;

    // Create QImage in premultiplied RGBA8 (canonical contract).
    // QImage takes ownership of the data copy.
    QImage img(pixels, width, height, width * 4, QImage::Format_RGBA8888);
    // Premultiplication: QImage::Format_RGBA8888 is already premultiplied
    // if the source is premultiplied. We assume the canonical buffer is.
    clipboard->setImage(img);
}

bool platform_has_clipboard_image() {
    QClipboard *clipboard = QApplication::clipboard();
    if (!clipboard) return false;
    return clipboard->image().format() == QImage::Format_RGBA8888 ||
           clipboard->image().format() == QImage::Format_RGB888;
}