// compositor_host_run.h — the Qt host C entry called by the Swift composition root.
//
// The Linux port's composition root is inverted: a Swift `@main`
// (Sources/CompositorHostBootstrap/hostMain.swift) bootstraps the Swift runtime
// + Foundation (which a C++ main cannot on the Freedesktop Swift 6.3 SDK), then
// calls compositor_host_run to run the Qt Widgets shell. The Qt host calls back
// into the Swift core through the compositor_session_* C ABI (ENG-1).

#ifndef COMPOSITOR_HOST_RUN_H
#define COMPOSITOR_HOST_RUN_H

#ifdef __cplusplus
extern "C" {
#endif

int compositor_host_ui_smoke(int argc, char **argv);
int compositor_host_run(int argc, char **argv);
int compositor_host_dialog_smoke(int argc, char **argv);
int compositor_host_io_smoke(int argc, char **argv);
int compositor_host_layers_smoke(int argc, char **argv);
int compositor_host_brush_smoke(int argc, char **argv);

#ifdef __cplusplus
}
#endif

#endif /* COMPOSITOR_HOST_RUN_H */
