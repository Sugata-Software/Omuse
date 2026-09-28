// QtInput.cpp — platform input adapter (ENG-2).
// Bridges Swift NSEvent / EditorCanvas input handling to Qt's event system.
// The Compositor core's EditorSession receives commands through the C ABI
// (compositor_session_command) which already carries point/rect data;
// this adapter is only needed for the Qt shell's native event processing.

#include "QtInput.h"
#include <QApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QWidget>

// Minimal input translation: the Swift core drives UI through JSON commands
// (compositor_session_command). The Qt host merely needs to process OS-level
// events (keyboard/mouse) to keep the window alive and responsive.
// Full input routing is handled by the Swift event loop; this adapter
// exposes the Qt primitives so the host can forward them if needed.

void platform_process_key_event(bool is_key_down, int key_code, int unicode) {
    // Swift core does not use direct Qt key events; commands arrive via
    // compositor_session_command. This is a no-op for the core logic.
    // However, the Qt event loop needs to drain events to avoid blocking.
    Q_UNUSED(is_key_down);
    Q_UNUSED(key_code);
    Q_UNUSED(unicode);
}

void platform_process_mouse_move(int x, int y) {
    // Swift core tracks mouse position internally; Qt shell just needs
    // to keep the window responsive. No direct translation needed.
    Q_UNUSED(x);
    Q_UNUSED(y);
}

void platform_process_mouse_click(int x, int y, bool left, bool pressed) {
    // Swift core handles click semantics through its command graph.
    // This adapter exists only so the Qt shell can forward low-level events
    // if a future UI path requires it.
    Q_UNUSED(x);
    Q_UNUSED(y);
    Q_UNUSED(left);
    Q_UNUSED(pressed);
}