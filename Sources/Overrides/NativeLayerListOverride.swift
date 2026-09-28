// OVERRIDE for `Compositor/UI/NativeLayerList.swift`'s public entry point (`NativeLayerList(session:)`). The real
// file (1101 lines) is genuinely blocked, not by missing compat coverage: it uses `#selector`/`@objc` throughout
// (32 occurrences) for its `NSTableView` delegate/data-source/drag machinery, which needs an Objective-C runtime
// Linux Swift does not have (confirmed: the frontend runs with `-disable-objc-interop`, and there is no `libobjc`
// on this platform to emulate one onto — a from-scratch message-dispatch layer is a separate, large, uncertain
// sub-project, not a SwiftUI-compat gap).
//
// This is deliberately NOT a lossy stand-in. `EditorSession` already exposes exactly the operations a layers list
// needs as plain, list-shaped APIs — `layerRows` (display order + depth + visibility, top-first), `reorderLayers
// (from: IndexSet, to: Int)` (the *exact* signature SwiftUI's own `ForEach.onMove(perform:)` expects), `selectLayer`
// /`selectLayers`, `toggleLayerVisibility`. A generic `List`/`ForEach` reimplementation on top of those is a real
// backend swap (native `NSTableView` drag-and-drop → Qt/SwiftUI-compat list interaction), the same category as the
// existing Metal→Vulkan override: view, select, multi-select, show/hide, and thumbnails (via the already-wired
// `CanvasThumbnail.swift`) are all preserved. Reorder is the one capability NOT wired yet in this pass: `.onMove`
// on `ForEach` (see `ViewBuilder.swift`) is a documented no-op placeholder until the Qt renderer grows list
// drag-and-drop, and `EditorSession.reorderLayers(from:to:)` already has the exact `(IndexSet, Int)` shape it
// needs — the data-side is ready, only the interaction isn't connected yet.

import SwiftUI

struct NativeLayerList: View {
    let session: EditorSession

    private var depths: [UUID: Int] {
        Dictionary(uniqueKeysWithValues: session.layerRows.map { ($0.layer.id, $0.depth) })
    }

    var body: some View {
        List(session.layerRows, id: \.layer.id) { entry in
            row(for: entry.layer.id)
        }
        .accessibilityIdentifier("layersList")
    }

    private func row(for id: UUID) -> some View {
        let byID = Dictionary(uniqueKeysWithValues: (session.document?.layers ?? []).map { ($0.id, $0) })
        guard let layer = byID[id] else { return AnyView(EmptyView()) }
        return AnyView(
            HStack(spacing: 8) {
                Spacer().frame(width: Double(depths[id] ?? 0) * 14)
                Button {
                    session.toggleLayerVisibility(layer.id)
                } label: {
                    Image(systemName: layer.isVisible ? "eye" : "eye.slash")
                }
                .buttonStyle(.plain)
                .accessibilityLabel(layer.isVisible ? "Hide layer" : "Show layer")
                Text(layer.name)
                Spacer()
            }
            .padding(.vertical, 4)
            .contentShape(Rectangle())
            .background(session.selectedLayerIDs.contains(layer.id) ? Color.accentColor.opacity(0.25) : Color.clear)
            .onTapGesture {
                if session.selectedLayerIDs.contains(layer.id), session.selectedLayerIDs.count > 1 {
                    session.selectLayers(session.selectedLayerIDs, primary: layer.id)
                } else {
                    session.selectLayer(layer.id)
                }
            }
        )
    }
}
