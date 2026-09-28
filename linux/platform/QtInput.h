#ifndef QtInput_h
#define QtInput_h

// Plan §8.10: Qt input adapter. Minimal — the Swift core drives input
// through its own command-graph (compositor_session_command, brush strokes,
// etc.). The Qt shell only needs to keep the window alive and process
// OS events so the event loop does not block.

void platform_process_key_event(bool is_key_down, int key_code, int unicode);
void platform_process_mouse_move(int x, int y);
void platform_process_mouse_click(int x, int y, bool left, bool pressed);

#endif /* QtInput_h */