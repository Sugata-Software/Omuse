// QtPaths.cpp — platform paths adapter (ENG-2).
// Provides Linux-standard directory paths that the Swift core may query.
// On macOS the core uses Foundation (NSSearchPathForDirectoriesInDomains);
// on Linux we map to XDG Base Directory Convention.

#include "QtPaths.h"
#include <QStandardPaths>
#include <QString>
#include <QDir>

QString platform_home_directory() {
    return QStandardPaths::writableLocation(QStandardPaths::HomeLocation);
}

QString platform_documents_directory() {
    // XDG Documents home — the Flatpak Documents portal grants access
    // to a user-chosen directory; this is the fallback default.
    return QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
}

QString platform_temp_directory() {
    return QStandardPaths::writableLocation(QStandardPaths::TempLocation);
}

bool platform_create_directory(const char *path) {
    if (!path) return false;
    return QDir::fromNativeSeparators(QString::fromUtf8(path)).mkdir();
}

}  // namespace platform