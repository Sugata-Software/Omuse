import Testing
@testable import Compositor
import SwiftUI

/// Sanity check for the Sources/Compat/SwiftUI compat layer: resolves one real, unmodified
/// `Compositor/UI/NavigationToolHeader.swift` (wired into the Linux build via Sources/UpstreamCore/UI, see
/// Package.swift) all the way to a `RenderNode` tree, proving the `View`/`@ViewBuilder`/modifier machinery works
/// end to end on real upstream code, not just synthetic examples.
@MainActor
struct NavigationToolHeaderProbeTests {
    @Test func resolvesToAnHStackWithASpacer() {
        let session = EditorSession()
        let node = ViewResolver.resolve(NavigationToolHeader(session: session))
        #expect(node.kind == "HStack")
        #expect(node.children.contains { $0.kind == "Spacer" })
    }

    @Test func zoomModeShowsATextFieldWithTheZoomFieldsToolHeaderBarModifiers() {
        let session = EditorSession()
        session.tool = .zoom
        let node = ViewResolver.resolve(NavigationToolHeader(session: session))
        // `.toolHeaderBar()` (a real upstream `View` extension) applies `.font`/`.frame`/`.fixedSize`; proves
        // upstream's own generic View-extension modifiers compose through the compat layer unmodified.
        #expect(node.modifiers.contains { if case .font = $0 { return true } else { return false } })
        #expect(node.modifiers.contains { if case .frame = $0 { return true } else { return false } })
        func flatten(_ node: RenderNode) -> [RenderNode] { [node] + node.children.flatMap(flatten) }
        let all = flatten(node)
        #expect(all.contains { $0.kind == "TextField" })
        // `.unitSuffix("%")` wraps the field in an HStack with a "%" Text — another real upstream View extension.
        #expect(all.contains { $0.kind == "Text" && $0.stringParams["text"] == "%" })
    }
}
