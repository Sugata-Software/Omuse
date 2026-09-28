#ifndef QtFileDialog_h
#define QtFileDialog_h

#include <QString>
#include <QStringList>

// Plan §8.8: Qt file dialog adapter. Uses QFileDialog with
// DontUseNativeDialog so Flatpak xdg-desktop-portal is employed
// in sandboxed environments. On unsandboxed desktops this falls
// through to native OS file choosers.

QString native_open_file(const QString &default_filter = "Compositor Files (*.comp *.rgba)");
QString native_save_file(const QString &default_filter = "Compositor Files (*.comp *.rgba)",
                         QString *selected_filter_out = nullptr);
QStringList native_open_files();

#endif /* QtFileDialog_h */