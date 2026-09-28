#pragma once

// Qt Widgets / XDG-portal implementations of the platform seams. Header-only and
// Q_OBJECT-free (SwiftPM cannot run moc). A macOS shell would provide AppKit-backed
// implementations of the same interfaces; nothing above this file would change.

#include "interfaces/IPlatformServices.h"
#include "ColorPickerDialog.h"

#include <QApplication>
#include <QClipboard>
#include <QDir>
#include <QFileDialog>
#include <QGuiApplication>
#include <QMessageBox>
#include <QMimeData>
#include <QStandardPaths>

namespace qtplatform {

class FileDialogs final : public IFileDialogService {
public:
    QString chooseImageToOpen() override {
        return QFileDialog::getOpenFileName(QApplication::activeWindow(), QObject::tr("Open Image"), QString(),
            QObject::tr("Images (*.png *.jpg *.jpeg *.bmp *.tiff *.webp);;All files (*)"));
    }
    QString chooseProjectToOpen() override {
        return QFileDialog::getExistingDirectory(QApplication::activeWindow(), QObject::tr("Open Project"));
    }
    QString chooseProjectSavePath() override {
        return QFileDialog::getSaveFileName(QApplication::activeWindow(), QObject::tr("Save Project"), QString(),
            QObject::tr("Compositor project (*.comp);;All files (*)"));
    }
    QString chooseExportPath(const QString &formatName, const QString &filter) override {
        return QFileDialog::getSaveFileName(QApplication::activeWindow(), QObject::tr("Export %1").arg(formatName), QString(), filter);
    }
};

class Clipboard final : public IClipboardService {
public:
    void setImage(const QImage &image) override {
        if (auto *clipboard = QGuiApplication::clipboard()) clipboard->setImage(image);
    }
    QImage image() const override {
        const QClipboard *clipboard = QGuiApplication::clipboard();
        const QMimeData *mime = clipboard ? clipboard->mimeData() : nullptr;
        return (mime && mime->hasImage()) ? qvariant_cast<QImage>(mime->imageData()) : QImage();
    }
};

class ColorPicker final : public IColorPickerService {
public:
    QColor pick(const QColor &initial, const QString &title) override {
        return ColorPickerDialog::getColor(initial, QApplication::activeWindow(), title);
    }
};

class Notifier final : public IUserNotifier {
public:
    void warn(const QString &title, const QString &text) override {
        QMessageBox::warning(QApplication::activeWindow(), title, text);
    }
};

class Storage final : public IStorageLocator {
public:
    QString appDataDirectory() const override {
        QString dir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
        return dir.isEmpty() ? QDir::tempPath() + "/Compositor" : dir;
    }
};

}  // namespace qtplatform

#include "FlatpakUpdateService.h"

inline PlatformServices PlatformServices::qtDefaults() {
    PlatformServices services;
    services.files = std::make_shared<qtplatform::FileDialogs>();
    services.clipboard = std::make_shared<qtplatform::Clipboard>();
    services.colors = std::make_shared<qtplatform::ColorPicker>();
    services.notifier = std::make_shared<qtplatform::Notifier>();
    services.storage = std::make_shared<qtplatform::Storage>();
    services.updates = std::make_shared<qtplatform::FlatpakUpdateService>();
    return services;
}

inline PlatformServices PlatformServices::withDefaults() const {
    const PlatformServices fallback = qtDefaults();
    PlatformServices merged = *this;
    if (!merged.files) merged.files = fallback.files;
    if (!merged.clipboard) merged.clipboard = fallback.clipboard;
    if (!merged.colors) merged.colors = fallback.colors;
    if (!merged.notifier) merged.notifier = fallback.notifier;
    if (!merged.storage) merged.storage = fallback.storage;
    if (!merged.updates) merged.updates = fallback.updates;
    return merged;
}
