// The platform-agnostic tree a resolved SwiftUI view collapses into. The Qt host walks this generically (one
// interpreter for every panel) instead of Linux code hand-reimplementing each upstream panel — see
// docs/platform-abstraction.md and linux/upstream-parity.json.

/// A resolved node: a container/control kind, its parameters, and its already-resolved children. Leaf controls
/// (Text, Button, Slider, ...) carry their state in `params`/`stringParams`/`handlers`; containers (VStack, HStack,
/// ...) carry theirs the same way and additionally have `children`.
public struct RenderNode {
    /// A stable dot-path ("0.2.1") identifying this node's position in the tree — empty until `assignIDs()` runs.
    /// Not needed to resolve or unit-test the tree; only needed once a tree crosses into `RenderNodeWire` for the
    /// Qt bridge, where a later phase's action dispatch (button taps, slider drags) will look nodes up by it.
    public var id: String = ""
    public var tagValue: AnyHashable?
    public var kind: String
    public var stringParams: [String: String] = [:]
    public var doubleParams: [String: Double] = [:]
    public var boolParams: [String: Bool] = [:]
    /// Action/value-changed closures a control fires back into Swift (button taps, slider drags, text edits, ...).
    public var handlers: [String: (Any) -> Void] = [:]
    public var modifiers: [RenderModifier] = []
    public var children: [RenderNode] = []

    public init(kind: String) { self.kind = kind }
}

extension RenderNode {
    /// Assigns dot-path ids to this node and every descendant, in place. Call once, on the root, after resolving.
    public mutating func assignIDs(prefix: String = "0") {
        id = prefix
        for i in children.indices { children[i].assignIDs(prefix: "\(prefix).\(i)") }
    }

    /// Flattens this (already `assignIDs()`-ed) node and its descendants' `handlers` into a lookup keyed by node
    /// id, then handler key (`"action"`, `"value"`, `"isOn"`, `"text"`, ...). A resolved tree's closures only live
    /// as long as the `RenderNode` value itself; the Qt bridge keeps this registry around instead so a later
    /// button tap / slider drag can still reach the right one by id (see `Sources/LinuxBridge/SwiftUIBridge.swift`).
    public func collectHandlers(into registry: inout [String: [String: (Any) -> Void]]) {
        if !handlers.isEmpty { registry[id] = handlers }
        for child in children { child.collectHandlers(into: &registry) }
    }
}

/// A style/layout annotation attached to a resolved node by a SwiftUI modifier (`.frame`, `.padding`, ...). Each
/// case mirrors one thing the Qt renderer needs to apply; unhandled cases are simply ignored by older renderers,
/// so adding a new modifier here never breaks existing panels.
public enum RenderModifier {
    case frame(width: Double?, height: Double?, minWidth: Double?, minHeight: Double?, maxWidth: Double?, maxHeight: Double?, alignment: String)
    case padding(top: Double, leading: Double, bottom: Double, trailing: Double)
    case font(String)
    case foregroundStyle(String)
    case background(String)
    case opacity(Double)
    case disabled(Bool)
    case fixedSize(horizontal: Bool, vertical: Bool)
    case buttonStyle(String)
    case toggleStyle(String)
    case controlSize(String)
    case textFieldStyle(String)
    case multilineTextAlignment(String)
    case contentShape(String)
    case clipShape(String)
    case overlay(RenderNode, alignment: String)
    case offset(x: Double, y: Double)
    case cornerRadius(Double)
    case shadow(radius: Double)
    case scaleEffect(Double)
    case rotationEffect(Double)
    case help(String)
    case accessibilityLabel(String)
    case accessibilityIdentifier(String)
    case identifier(String)
    case scrollIndicators(String)
    case keyboardShortcut(key: String, modifiers: Int)
    case tag(String)
    case onAppear(() -> Void)
    case onSubmit(() -> Void)
    case onExitCommand(() -> Void)
    /// `.onChange`/`.focused` etc. read/write through a boxed value the Qt side polls or pushes into; keyed by an
    /// opaque tag so a node can carry more than one.
    case sink(String, (Any) -> Void)
}

/// A view whose node in the render tree is produced directly (no `body` to recurse into) — SwiftUI's real
/// mechanism uses `Body == Never` for exactly this; every leaf control and every layout container in this compat
/// layer conforms here instead of composing further `View`s at resolve time.
public protocol PrimitiveView: View where Body == Never {
    /// Other views this one hosts (a container's stack content); leaves return `[]`.
    var _childViews: [any View] { get }
    /// Builds this view's node, given its children already resolved.
    func _makeNode(children: [RenderNode]) -> RenderNode
}

extension PrimitiveView {
    public var _childViews: [any View] { [] }
    public var body: Never { fatalError("\(Self.self) is a primitive view; it has no body") }
}

/// Views that stand for zero-or-more sibling views rather than one node (`TupleView`, `ForEach`, `Group`,
/// `EmptyView`, `if`/`if let` branches, arrays from `for` loops in a `@ViewBuilder`). The resolver splices these
/// into their parent's child list instead of giving them their own render node. Refines `PrimitiveView` purely to
/// inherit its default `body: Never` — `_makeNode` is never actually invoked for these (the resolver intercepts
/// `_ViewListProviding` first) so it gets a throwaway default too.
public protocol _ViewListProviding: PrimitiveView {
    var _viewList: [any View] { get }
}

extension _ViewListProviding {
    public func _makeNode(children: [RenderNode]) -> RenderNode { RenderNode(kind: "_ViewList") }
}

public enum ViewResolver {
    /// Resolves a full view (composite or primitive) into its render tree, recursing through `body` for composite
    /// (upstream) views and calling `_makeNode` once a primitive/leaf is reached.
    public static func resolve(_ view: any View) -> RenderNode {
        resolveList(view).first ?? RenderNode(kind: "_Empty")
    }

    static func resolveList(_ view: any View) -> [RenderNode] {
        if let list = view as? any _ViewListProviding {
            return list._viewList.flatMap(resolveList)
        }
        if let primitive = view as? any PrimitiveView {
            let children = primitive._childViews.flatMap(resolveList)
            return [primitive._makeNode(children: children)]
        }
        return resolveList(view.body)
    }
}
