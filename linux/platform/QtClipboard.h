#ifndef QtClipboard_h
#define QtClipboard_h

#include <string>

// Plan §8.9: Qt clipboard adapter. Bridges Swift NSPasteboard interactions
// to Qt QClipboard / QMimeData. No custom portal needed — Qt already
// integrates with the system clipboard and xdg-desktop-portal.

void platform_set_clipboard_text(const char *text, size_t count);
std::string platform_get_clipboard_text();
void platform_set_clipboard_image(const uint8_t *pixels, size_t count, int width, int height);
bool platform_has_clipboard_image();

#endif /* QtClipboard_h */