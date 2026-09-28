// QtApplication.cpp — thin adapter for QApplication setup (ENG-2).
// The Swift core bootstraps Foundation + runtime, then calls
// compositor_host_run() which creates a QApplication. This file
// isolates the Qt initialization so the Swift side never imports
// Qt headers.

#include <QApplication>
#include <QScreen>

void platform_apply_high_dpi() {
    // Enable high-DPI scaling on supported platforms.
    // Qt handles this automatically via AA_EnableHighDpiScaling.
    QApplication::setAttribute(Qt::AA_EnableHighDpiScaling);
    QApplication::setAttribute(Qt::AA_UseHighDpiPixmaps);
}

QScreen *platform_primary_screen() {
    QApplication *app = QApplication::instance();
    if (app) return app->primaryScreen();
    return nullptr;
}

int platform_start_app(int &argc, char **argv) {
    QApplication app(argc, argv);
    platform_apply_high_dpi();
    return app.exec();
}