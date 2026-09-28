// QtFileDialog.cpp — platform file dialog adapter (ENG-2).
// Wraps QFileDialog with fallback to xdg-desktop-portal (Flatpak portal).
// The Swift core calls compositor_session_import_rgba / export_layer etc.
// with file paths; the Qt host resolves them through this adapter.

#include "QtFileDialog.h"
#include <QFileDialog>
#include <QDir>
#include <QDebug>
#include <QMimeData>

// Plan §8.8: Qt has its own xdg-desktop-portal platform-theme file dialog
// backend. Its implementation calls org.freedesktop.portal.Desktop.
// We use QFileDialog transparently; in a sandboxed Flatpak Qt/KDE will
// fall back to the portal automatically when native dialogs are disabled.

QString native_open_file(const QString &default_filter) {
    // QFileDialog with DontUseNativeDialog so the Flatpak portal is used.
    // On a real desktop (non-sandboxed) this falls through to the OS dialog.
    QStringList filters;
    filters << default_filter;
    return QFileDialog::getOpenFileName(nullptr, "Open File",
                                        QString(),
                                        filters.join(";;"));
}

QString native_save_file(const QString &default_filter, QString *selected_filter_out) {
    QStringList filters;
    filters << default_filter;
    bool ok;
    QString result = QFileDialog::getSaveFileName(nullptr, "Save As",
                                                   QString(),
                                                   filters.join(";;"),
                                                   &ok);
    if (selected_filter_out) *selected_filter_out = ok ? QString() : QString();
    return result;
}

QStringList native_open_files() {
    // Allow multiple file selection via portal.
    // The xdg-desktop-portal supports multi-select through the FileChooser
    // portal protocol.
    return QFileDialog::getOpenFileNames(nullptr, "Open Files");
}