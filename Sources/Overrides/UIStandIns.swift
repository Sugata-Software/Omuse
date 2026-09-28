// STAND-INS for UI types the unmodified canvas view refers to (Compositor/UI is not compiled on Linux):
//   CanvasRulerNSView (UI/CanvasRulers.swift)          — the rulers NSView; the canvas only checks `view is CanvasRulerNSView`.
//     The real file's `draw(_:)` needs NSString.draw/size, NSFont.monospacedDigitSystemFont, NSAffineTransform.concat
//     — real AppKit text-drawing compat, a separate sub-project from SwiftUI compat; tried and reverted, see the plan.
// `FloatingPanelController`/`ColorPickerPanelController` are wired in for real (Compositor/UI/FloatingPanel.swift,
// ColorPickerSheet.swift) — their `NSPanel`s never actually become visible without a real display, so `show`/
// `makeKeyAndOrderFront`/etc. all still end up as no-ops in practice, same effect the old stand-ins had.

import Foundation
import AppKit
import SwiftUI

// `ShortcutSettings`/`configuredNativeShortcut`/`configuredKeyboardShortcut` are real now
// (Sources/Overrides/KeyboardShortcuts.swift, a faithful override of Compositor/UI/KeyboardShortcuts.swift).

struct BlendModePicker: View {
    let session: EditorSession
    var body: some View {
        Picker("Blend mode", selection: Binding(
            get: { session.activeLayer?.blendMode ?? .normal },
            set: { session.setLayerBlendMode($0); session.refreshCanvasPreview?() }
        )) {
            ForEach(LayerBlendMode.allCases, id: \.self) { mode in
                Text(mode.rawValue).tag(mode)
            }
        }
    }
}

// `LayersPanel` is wired in for real (Compositor/UI/LayersPanel.swift); it uses `NativeLayerList`, which is
// overridden (`Sources/Overrides/NativeLayerListOverride.swift` — a real functional backend swap, not a stand-in;
// see that file for why the actual NSTableView-based `NativeLayerList.swift` is blocked).

/// STAND-IN for the `NSTableView` subclass in UI/NativeLayerList.swift, whose key handling (tool keys, nudges while the
/// layer list has focus) lives in the macOS layers panel. `CompositorTests/TransformTests.swift` (upstream's own,
/// unmodified) constructs this directly, so it stays even though `NativeLayerListOverride.swift`'s `List`-based
/// replacement no longer hosts it via `HostedNativeContent`.
@MainActor final class LayerTableView: NSTableView {
    weak var session: EditorSession?
    override func keyDown(with event: NSEvent) {
        let plain = event.modifierFlags.intersection([.command, .control, .option]).isEmpty
        if event.keyCode == 53, session?.transformEdit != nil {
            session?.cancelTransform()
        } else if [36, 76].contains(event.keyCode), session?.transformEdit != nil {
            session?.commitTransform()
        } else if plain, event.keyCode == 48 {
            session?.cycleToolMode()
        } else if plain, event.charactersIgnoringModifiers?.lowercased() == "x" {
            session?.swapPaletteColors()
        } else if plain, event.charactersIgnoringModifiers?.lowercased() == "d" {
            session?.resetPaletteColors()
        } else if plain, event.charactersIgnoringModifiers?.lowercased() == "t" {
            session?.selectTool(.type)
        } else if plain, ["a", "v", "h", "z", "b", "e", "g", "l", "m", "w", "j", "s", "u", "r", "i", "c"].contains(event.charactersIgnoringModifiers?.lowercased() ?? "") {
            let key = event.charactersIgnoringModifiers?.lowercased()
            if key == "m" { if !event.isARepeat { session?.pressMarqueeKey() } }
            else if key == "l" { if !event.isARepeat { session?.pressLassoKey() } }
            else if key == "b" || key == "e" {
                session?.selectTool(.brush)
                session?.brushMode = key == "e" ? .erase : .paint
            }
            else if key == "w" { if !event.isARepeat { session?.pressWandKey() } }
            else { session?.selectTool(key == "a" ? .idle : key == "i" ? .eyedropper : key == "c" ? .crop : key == "r" ? .blur : key == "b" ? .brush : key == "g" ? .gradient : key == "l" ? .lasso : key == "m" ? .marquee : key == "j" ? .spotHealing : key == "s" ? .cloneStamp : key == "u" ? .shape : key == "v" ? .move : key == "h" ? .hand : .zoom) }
        } else if plain, let digit = Int(event.charactersIgnoringModifiers ?? ""), session?.usesOpacityKeys == true {
            session?.typeOpacityDigit(digit)
        // With the Move tool the arrows move the layer, as on the canvas, rather than changing the row selection.
        } else if plain, session?.transformEdit != nil || session?.tool == .move, [123, 124, 125, 126].contains(event.keyCode) {
            let step: CGFloat = event.modifierFlags.contains(.shift) ? 10 : 1
            session?.nudgeLayer(dx: event.keyCode == 123 ? -step : event.keyCode == 124 ? step : 0,
                                dy: event.keyCode == 126 ? -step : event.keyCode == 125 ? step : 0)
        } else if [51, 117].contains(event.keyCode), plain {
            session?.deleteKeyPressed()
        } else { super.keyDown(with: event) }
    }
}

