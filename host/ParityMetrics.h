#pragma once

// ParityMetrics.h — Canonical UI/UX geometry tokens derived from macOS Compositor.
// Source: MACOS_UI_PARITY_RULES.md, Compositor/ContentView.swift, ToolHeaderStyle.swift,
// LayersPanel.swift, CanvasRulers.swift, ColorPaletteControls.swift.
//
// All measurements are in logical points/pixels. Use hairline(dpr) for single-device-pixel borders.

#include <QtGlobal>
#include <QSize>

namespace ParityMetrics {

// 1. Top-level Window
constexpr int WindowDefaultWidth = 1180;
constexpr int WindowDefaultHeight = 780;
constexpr int WindowMinWidth = 800;
constexpr int WindowMinHeight = 520;

// 2. Tool Rail (Left)
constexpr int ToolRailWidth = 56;
constexpr int ToolVisualFrame = 36;       // 36x36
constexpr int ToolStackSpacing = 10;
constexpr int ToolStackPaddingTop = 16;
constexpr int ToolStackPaddingBottom = 12;
constexpr int ToolCornerRadius = 7;
constexpr int ToolIconNominal = 18;       // Bounds: 17 system, 18 custom

// 3. Tool Options Header (Top)
constexpr int ToolHeaderHeight = 42;
constexpr int ToolHeaderPaddingH = 18;
constexpr int ToolHeaderSpacing = 12;
constexpr int CropHeaderSpacing = 14;

// 4. Status Bar (Bottom)
constexpr int StatusBarHeight = 30;
constexpr int StatusBarPaddingH = 18;
constexpr int StatusBarSpacing = 16;
constexpr int StatusBarZoomWidth = 62;

// 5. Layers Panel (Right)
constexpr int LayersPanelDefaultWidth = 252;
constexpr int LayersPanelMinWidth = 202;
constexpr int LayersPanelMaxWidth = 352;
constexpr int PanelResizeHitTarget = 8;
constexpr int LayerBaseRowHeight = 52;
constexpr int LayerEffectSubrowHeight = 24;
constexpr int LayerRowIntercellVertical = 2;
constexpr int LayersHeaderPadding = 18;
constexpr int LayersAppearancePadding = 12;
constexpr int LayersFooterPaddingH = 8;
constexpr int LayersFooterPaddingV = 4;

// 6. Project Tab Strip
constexpr int TabStripHeight = 34;
constexpr int TabHeight = 28;
constexpr int TabGap = 6;
constexpr int TabTextMin = 35;
constexpr int TabTextMax = 155;
constexpr int TabCloseSize = 16;

// 7. Rulers & Guides
constexpr int RulerThickness = 18;
constexpr int RulerMajorTickTarget = 70;

// 8. Palette Swatches
constexpr int PaletteFrameWidth = 36;
constexpr int PaletteFrameHeight = 36;
constexpr int SwatchSize = 24;
constexpr int SwatchCornerRadius = 6;
constexpr int SwapIconSize = 12;
constexpr int ResetIconSize = 12;

// Hairline helper: returns the logical thickness of 1 physical device pixel at DPR.
inline qreal hairline(qreal devicePixelRatio) {
    return (devicePixelRatio > 0.0) ? (1.0 / devicePixelRatio) : 1.0;
}

} // namespace ParityMetrics
