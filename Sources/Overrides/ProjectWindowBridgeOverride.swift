// OVERRIDE for `Compositor/UI/ProjectWindowBridge.swift`. The real file is blocked at the compiler level, not by
// missing compat coverage: its `ProjectWindowDelegate` writes `override func responds(to:)`/`forwardingTarget(for:)`
// to forward unhandled `NSWindowDelegate` calls to a `previous` delegate that was already installed — real
// Objective-C runtime message-forwarding, which the Swift compiler has no code path for on Linux at all (confirmed:
// `swiftc -enable-objc-interop` returns "unknown argument" — this isn't a missing library or a flag to flip).
//
// Not a lossy simplification: the forwarding path this override drops is provably dead on this platform. Nothing
// else in the codebase ever sets a delegate on a project's main `NSWindow` before `ProjectWindowBridge` does (only
// `FloatingPanel.swift` sets a delegate, and that's on a *different* window — its own floating panel). `previous`
// would always be `nil` here, so forwarding to it is a no-op regardless of how it's implemented. What's preserved,
// faithfully: intercepting close approval and routing it to `workspace.closeWindow`/`controller.close`, and keeping
// `controller.window`/`workspace.window` in sync — the actual observable behavior.

import SwiftUI

struct ProjectWindowBridge: View, PrimitiveView {
    let controller: ProjectController
    func _makeNode(children: [RenderNode]) -> RenderNode { RenderNode(kind: "_Native") }
}

extension ProjectWindowBridge: HostedNativeContent {
    func makeNativeView() -> NSView? {
        let view = ProjectWindowView(controller: controller)
        return view
    }
}

final class ProjectWindowView: NSView {
    var controller: ProjectController
    private let proxy = ProjectWindowDelegate()
    init(controller: ProjectController) {
        self.controller = controller
        super.init(frame: .zero)
        proxy.controller = controller
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    func update(_ controller: ProjectController) {
        self.controller = controller
        proxy.controller = controller
        controller.window = window
        controller.workspace?.window = window
    }
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window else { return }
        controller.window = window
        controller.workspace?.window = window
        window.representedURL = controller.session.projectURL
        window.isDocumentEdited = controller.session.isModified
        if window.delegate !== proxy { window.delegate = proxy }
    }
}

private final class ProjectWindowDelegate: NSWindowDelegate {
    weak var controller: ProjectController?
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard let controller else { return true }
        Task {
            if let workspace = controller.workspace { await workspace.closeWindow(sender) }
            else { await controller.close(sender) }
        }
        return false
    }
}
