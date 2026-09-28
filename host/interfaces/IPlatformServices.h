#pragma once

// Platform seams for the editor shell (SOLID).
//
//  S  Each interface has one reason to change: files, clipboard, colour choice,
//     user notification, storage location.
//  O  New platforms (macOS AppKit, a test double, a portal-only sandbox) are added
//     by writing a new implementation; SessionWindow and the dialogs do not change.
//  L  Every implementation honours the same contract: an empty QString / null
//     QImage / invalid QColor means "user cancelled" or "nothing available".
//  I  Five narrow roles instead of one "Platform" god-interface, so a dialog that
//     only needs a colour picker never sees file or clipboard APIs.
//  D  High-level editor code depends on these abstractions, never on QFileDialog,
//     QClipboard, QMessageBox, QStandardPaths or (on macOS) NSOpenPanel/NSPasteboard.
//
// The interfaces use value types only (QString/QImage/QColor): no widget parents
// leak through, so a non-Qt implementation does not need Qt Widgets at all.

#include <QColor>
#include <QImage>
#include <QString>
#include <memory>

class IFileDialogService {
public:
    virtual ~IFileDialogService() = default;
    virtual QString chooseImageToOpen() = 0;
    virtual QString chooseProjectToOpen() = 0;
    virtual QString chooseProjectSavePath() = 0;
    /// `formatName` is "PNG", "JPEG", ...; `filter` a Qt-style name-filter string.
    virtual QString chooseExportPath(const QString &formatName, const QString &filter) = 0;
};

class IClipboardService {
public:
    virtual ~IClipboardService() = default;
    virtual void setImage(const QImage &image) = 0;
    /// Null image when the clipboard holds no image.
    virtual QImage image() const = 0;
};

class IColorPickerService {
public:
    virtual ~IColorPickerService() = default;
    /// Invalid colour when cancelled.
    virtual QColor pick(const QColor &initial, const QString &title) = 0;
};

class IUserNotifier {
public:
    virtual ~IUserNotifier() = default;
    virtual void warn(const QString &title, const QString &text) = 0;
};

#include "IUpdateService.h"

class IStorageLocator {
public:
    virtual ~IStorageLocator() = default;
    /// Per-user writable application data directory (no trailing slash).
    virtual QString appDataDirectory() const = 0;
};

/// Composition-root bundle handed to the shell. Any member may be replaced
/// independently (e.g. a fake file dialog in tests, NSPasteboard on macOS).
struct PlatformServices {
    std::shared_ptr<IFileDialogService> files;
    std::shared_ptr<IClipboardService> clipboard;
    std::shared_ptr<IColorPickerService> colors;
    std::shared_ptr<IUserNotifier> notifier;
    std::shared_ptr<IStorageLocator> storage;
    std::shared_ptr<IUpdateService> updates;

    /// Qt Widgets implementations (defined in QtPlatformServices.h).
    static PlatformServices qtDefaults();
    /// Fills any missing member with the Qt default.
    PlatformServices withDefaults() const;
};
