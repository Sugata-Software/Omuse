#ifndef QtPaths_h
#define QtPaths_h

#include <QString>

// Plan §13: Qt paths adapter. Maps to XDG Base Directory Convention.
// The Swift core queries these for save/load locations, temp files,
// and user document directories. On macOS these come from Foundation;
// on Linux they come from QStandardPaths / freedesktop.org specs.

QString platform_home_directory();
QString platform_documents_directory();
QString platform_temp_directory();
bool platform_create_directory(const char *path);

#endif /* QtPaths_h */