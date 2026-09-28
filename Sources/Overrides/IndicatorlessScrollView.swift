// Override for Compositor/UI/IndicatorlessScrollView.swift: the real file is a custom NSScrollView subclass
// (NSViewRepresentable) purely so the scroll indicator never shows, even when the system setting always shows
// scrollers — no #selector/@objc, but it needs a real NSScrollView compat class this layer doesn't have yet.
// Same public API (`IndicatorlessScrollView(content:)`), backed by the existing `ScrollView` + `.scrollIndicators
// (.hidden)` compat surface — the exact combination `ContentView.swift`'s own `toolRail` already uses for the
// identical effect. Not a lossy rewrite: hiding the indicator is the file's entire job.
import SwiftUI

struct IndicatorlessScrollView<Content: View>: View {
    @ViewBuilder let content: () -> Content
    var body: some View {
        ScrollView(.vertical) { content() }
            .scrollIndicators(.hidden)
    }
}
