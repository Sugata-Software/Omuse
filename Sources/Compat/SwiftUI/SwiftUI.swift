// SwiftUI compat: upstream's model code imports SwiftUI only for a few value-level conveniences (and, on Apple
// platforms, for the Observation re-export). The views themselves are the Qt shell's job.

@_exported import Foundation
@_exported import CoreGraphics
@_exported import Observation
@_exported import Combine
@_exported import FoundationCompat
// Real macOS SwiftUI transitively exposes AppKit types (`NSColor`, ...) to any file that only `import SwiftUI` —
// several upstream panels rely on exactly this and never `import AppKit` themselves.
@_exported import AppKit

// `View` itself, `@ViewBuilder`, and the render-tree resolver now live in ViewBuilder.swift/RenderTree.swift —
// `Compositor/UI/*.swift` compiles and resolves through them instead of being excluded from the Linux build.

/// A SwiftUI view whose real content is a native NSView (e.g. a layers panel that wraps an NSTableView).
@MainActor public protocol HostedNativeContent { func makeNativeView() -> NSView? }

/// Hosts a view tree as an NSView (the Qt shell renders the real UI).
@MainActor public final class NSHostingView<Root: View>: NSView {
    public var rootView: Root
    public init(rootView: Root) {
        self.rootView = rootView
        super.init(frame: .zero)
        // A root view that stands in for native content (a list, a table) puts that view in the hosted tree.
        if let content = (rootView as? any HostedNativeContent)?.makeNativeView() { addSubview(content) }
    }
    public required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
    /// The size the hosted view would take up. No real layout engine runs the resolved tree yet, so this is a
    /// fixed placeholder rather than a measured value — enough for callers that only need *a* nonzero size.
    public var fittingSize: CGSize { CGSize(width: 320, height: 240) }
    /// Real SwiftUI's `NSHostingSizingOptions` — controls whether AppKit measures the hosted view's intrinsic
    /// size. No real layout engine runs here, so this is inert; it exists so `host.sizingOptions = []` compiles.
    public var sizingOptions: NSHostingSizingOptions = .standardBounds
}

public struct NSHostingSizingOptions: OptionSet, Sendable {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }
    public static let standardBounds = NSHostingSizingOptions(rawValue: 1 << 0)
    public static let minSize = NSHostingSizingOptions(rawValue: 1 << 1)
    public static let maxSize = NSHostingSizingOptions(rawValue: 1 << 2)
    public static let intrinsicContentSize = NSHostingSizingOptions(rawValue: 1 << 3)
}

/// Erases any `View` to a single concrete type, the same way real SwiftUI's `AnyView` does — needed wherever
/// upstream code stores a heterogeneous view (e.g. a floating panel's content).
public struct AnyView: View, PrimitiveView {
    let base: any View
    public init(_ view: some View) { base = view }
    public var _childViews: [any View] { [base] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "_ViewList")
        node.children = children
        return node
    }
}

/// Carries a root view for `NSWindow.beginSheet`; the Qt shell (or a headless resolver) decides what to show.
@MainActor public final class NSHostingController<Root: View>: NSViewController {
    public var rootView: Root { didSet { representedRootView = rootView } }
    public init(rootView: Root) { self.rootView = rootView; super.init(); representedRootView = rootView }
}

extension MutableCollection where Self: RangeReplaceableCollection {
    /// SwiftUI's `move(fromOffsets:toOffset:)` used by list reordering (`onMove`): moves the elements at `source`
    /// so that they end up before the element originally at `destination` (or at the end).
    public mutating func move(fromOffsets source: IndexSet, toOffset destination: Int) {
        let moving = source.map { self[index(startIndex, offsetBy: $0)] }
        var remaining: [Element] = []
        var insertAt = 0
        for (offset, element) in self.enumerated() {
            if offset == destination { insertAt = remaining.count }
            if !source.contains(offset) { remaining.append(element) }
        }
        if destination >= count { insertAt = remaining.count }
        remaining.insert(contentsOf: moving, at: insertAt)
        self = Self(remaining)
    }
}

public struct EventModifiers: OptionSet, Sendable {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }
    public static let capsLock = EventModifiers(rawValue: 1 << 0), shift = EventModifiers(rawValue: 1 << 1)
    public static let control = EventModifiers(rawValue: 1 << 2), option = EventModifiers(rawValue: 1 << 3)
    public static let command = EventModifiers(rawValue: 1 << 4), numericPad = EventModifiers(rawValue: 1 << 5)
    public static let function = EventModifiers(rawValue: 1 << 6)
    public static let all: EventModifiers = [.capsLock, .shift, .control, .option, .command, .numericPad, .function]
}

/// The bridge protocol between SwiftUI and AppKit views. This is the sanctioned escape hatch for the handful of
/// upstream views wrapping a real native control (an NSPopUpButton-backed picker, a table-backed list, ...) that
/// the "generic compat" approach explicitly doesn't try to reinterpret — the Qt renderer sees a `"_Native"` node
/// and slots in `makeNSView`'s real widget directly. Refines `PrimitiveView` so it plugs into the render tree like
/// every other view, but its content is opaque to the resolver.
@MainActor public protocol NSViewRepresentable: PrimitiveView {
    associatedtype NSViewType: NSView
    associatedtype Coordinator = Void
    typealias Context = NSViewRepresentableContext<Self>
    func makeNSView(context: Context) -> NSViewType
    func updateNSView(_ nsView: NSViewType, context: Context)
    func makeCoordinator() -> Coordinator
    static func dismantleNSView(_ nsView: NSViewType, coordinator: Coordinator)
}
extension NSViewRepresentable {
    /// The Qt bridge (a later phase) recognises `"_Native"` and calls `makeNSView`/`updateNSView` itself, rather
    /// than interpreting this node generically.
    public func _makeNode(children: [RenderNode]) -> RenderNode { RenderNode(kind: "_Native") }
}
public struct NSViewRepresentableContext<Representable: NSViewRepresentable> {
    public var coordinator: Representable.Coordinator
    public init(coordinator: Representable.Coordinator) { self.coordinator = coordinator }
}
extension NSViewRepresentable where Coordinator == Void {
    public func makeCoordinator() -> Void { () }
}
extension NSViewRepresentable {
    public static func dismantleNSView(_ nsView: NSViewType, coordinator: Coordinator) {}
}
