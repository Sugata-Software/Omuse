// Composition root (ENG-2): the C++ main owns QCoreApplication::exec and drives the
// Qt Widgets shell. In the full port this is where CMake links the Swift core as a
// static library via the C ABI; on this build host (no Swift toolchain) the shell
// exercises the portable C kernels directly. The Swift @_cdecl seam (ENG-1) plugs
// in here unchanged once built under the Freedesktop runtime extension.

#include "MainWindow.h"
#include <QApplication>

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    app.setApplicationName("Compositor");
    app.setOrganizationName("Compositor");
    MainWindow window;
    window.show();
    return app.exec();
}