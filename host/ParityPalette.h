#pragma once

// ParityPalette.h — Canonical UI/UX color and visual state tokens from macOS Compositor.
// Source: MACOS_UI_PARITY_RULES.md Section 5.

#include <QColor>

namespace ParityPalette {

// Editor Backgrounds
inline const QColor &editorBackground() {
    static const QColor c(0x24, 0x24, 0x24); // white 0.14 nominal
    return c;
}

// Ruler Colors
inline const QColor &rulerBackground() {
    static const QColor c(0x33, 0x33, 0x33); // white 0.20
    return c;
}
inline const QColor &rulerTick() {
    static const QColor c(0x9E, 0x9E, 0x9E); // white 0.62
    return c;
}
inline const QColor &rulerLabel() {
    static const QColor c(0xC7, 0xC7, 0xC7); // white 0.78
    return c;
}
inline const QColor &rulerBoundary() {
    static const QColor c(0x14, 0x14, 0x14); // white 0.08
    return c;
}

// Interactive Tool States
inline const QColor &toolSelectedFill() {
    static const QColor c(255, 255, 255, 31); // white 12%
    return c;
}
inline const QColor &toolSelectedBorder() {
    static const QColor c(255, 255, 255, 36); // white 14%
    return c;
}
inline const QColor &toolHoverFill() {
    static const QColor c(255, 255, 255, 15); // white 6%
    return c;
}

// Project Tab Visuals
inline const QColor &tabActiveFill() {
    static const QColor c(255, 255, 255, 31); // white 12%
    return c;
}
inline const QColor &tabInactiveFill() {
    static const QColor c(255, 255, 255, 9);  // white 3.5%
    return c;
}
inline const QColor &tabActiveBorder() {
    static const QColor c(255, 255, 255, 56); // white 22%
    return c;
}
inline const QColor &tabInactiveBorder() {
    static const QColor c(255, 255, 255, 20); // white 8%
    return c;
}

// Layer Table Hairline
inline const QColor &layerRowHairline() {
    static const QColor c(255, 255, 255, 15); // white 6%
    return c;
}

// Swatch Palette Defaults
inline const QColor &defaultForeground() {
    static const QColor c(0x00, 0x00, 0x00); // Black
    return c;
}
inline const QColor &defaultBackground() {
    static const QColor c(0xFF, 0xFF, 0xFF); // White
    return c;
}

} // namespace ParityPalette
