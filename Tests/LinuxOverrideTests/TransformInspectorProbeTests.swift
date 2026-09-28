import Testing
@testable import Compositor
import SwiftUI

/// Sanity check for build-order step 3 (interactive controls + action dispatch): resolves one real, unmodified
/// `Compositor/UI/TransformInspector.swift` (wired into the Linux build via Sources/UpstreamCore/UI), then drives
/// its "Auto Select" `Toggle` through the exact id-keyed handler-registry mechanism
/// `Sources/LinuxBridge/SwiftUIBridge.swift`'s `compositor_session_dispatch_swiftui_action` uses, and confirms the
/// mutation really lands on `EditorSession` and is visible in the next resolved tree.
///
/// This goes through `RenderNode.collectHandlers`/the resolved handler closures directly rather than the raw
/// `@_cdecl` C ABI functions: those assume they're called from the process's real main thread (`onMain`'s
/// `Thread.isMainThread` / `DispatchQueue.main.sync` fallback in SessionABI.swift), which a `swift test` process
/// does not guarantee for an async `@MainActor` test body on Linux — calling them directly here reproducibly
/// crashed (`SIGILL`, a Swift Concurrency blocking-call trap), a real but separate pre-existing threading
/// assumption, not a bug in the SwiftUI compat layer itself. The actual ABI round-trip is proven for real by
/// `host_run.cpp`'s `COMPOSITOR_GRAB_SWIFTUI_TREE` path, which genuinely runs on the process main thread.
@MainActor
struct TransformInspectorProbeTests {
    @Test func resolvesWithToggleAndButtons() {
        let session = EditorSession()
        let node = ViewResolver.resolve(TransformInspector(session: session))
        func flatten(_ node: RenderNode) -> [RenderNode] { [node] + node.children.flatMap(flatten) }
        let all = flatten(node)
        #expect(all.contains { $0.kind == "Toggle" })
        #expect(all.contains { $0.kind == "Button" && $0.handlers["action"] != nil })
    }

    @Test func dispatchingAToggleHandlerFlipsSessionStateForTheNextResolve() throws {
        let session = EditorSession()
        let initial = session.transformAutoSelect

        var before = ViewResolver.resolve(TransformInspector(session: session))
        before.assignIDs()
        var registry: [String: [String: (Any) -> Void]] = [:]
        before.collectHandlers(into: &registry)

        func flatten(_ node: RenderNode) -> [RenderNode] { [node] + node.children.flatMap(flatten) }
        // Identified by its label child (`Toggle("Auto Select", isOn: ...)`) — the tree has three Toggles.
        let toggleNode = try #require(flatten(before).first {
            $0.kind == "Toggle" && $0.children.first?.stringParams["text"] == "Auto Select"
        })
        #expect(toggleNode.boolParams["isOn"] == initial)
        let isOnHandler = try #require(registry[toggleNode.id]?["isOn"])

        // The exact call `compositor_session_dispatch_swiftui_action` makes once it looks a handler up by id.
        isOnHandler(!initial)
        #expect(session.transformAutoSelect == !initial)

        // Re-resolving the same session (what the *next* compositor_session_render_tree call does) reflects it —
        // no separate "commit" step, since the handler mutated `EditorSession` directly (real Observation).
        let after = ViewResolver.resolve(TransformInspector(session: session))
        let toggleAfter = try #require(flatten(after).first {
            $0.kind == "Toggle" && $0.children.first?.stringParams["text"] == "Auto Select"
        })
        #expect(toggleAfter.boolParams["isOn"] == !initial)
    }

    @Test func unknownNodeIDIsAbsentFromTheRegistry() {
        var node = ViewResolver.resolve(TransformInspector(session: EditorSession()))
        node.assignIDs()
        var registry: [String: [String: (Any) -> Void]] = [:]
        node.collectHandlers(into: &registry)
        #expect(registry["not-a-real-id"] == nil)
    }
}
