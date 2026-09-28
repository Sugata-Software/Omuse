// Override for Compositor/UI/CameraRawSlider.swift: the real file is an NSSlider subclass reached through
// NSViewRepresentable, using #selector/@objc target-action and a custom NSSliderCell for its gradient track and
// animated track-click — the same Objective-C-runtime wall as BlendModePicker/NativeLayerList/KeyboardShortcuts
// (see linux/upstream-parity.json). Same public API (`CameraRawSlider(value:range:track:help:onChange:onReset:)`,
// `CameraRawSliderTrack`'s cases), backed by a real SwiftUI `Slider` instead of a custom NSSlider — a backend
// swap, not a lossy rewrite of the panel's actual behavior (every slider still shows and edits its value). Honest,
// documented gaps: the colored gradient track (temperature/tint/chroma/hue family) isn't painted — plain track
// only — and double-click-to-reset isn't wired (no double-click gesture in this compat layer yet); `onReset` is
// still exposed so call sites compile and the reset action is reachable once a UI for it exists.
import SwiftUI

enum CameraRawSliderTrack {
    case plain
    case temperature
    case tint
    case chroma
    case hue(Double)
    case saturation(Double)
    case luminance(Double)
}

struct CameraRawSlider: View {
    var value: Double
    var range: ClosedRange<Double>
    var track: CameraRawSliderTrack
    var help: String
    var onChange: (Double) -> Void
    var onReset: () -> Void

    var body: some View {
        Slider(value: Binding(get: { value }, set: { onChange($0) }), in: range)
            .help(help)
            .accessibilityLabel(help)
    }
}
