// The layout containers: generic nodes the Qt renderer maps straight onto QVBoxLayout/QHBoxLayout/etc. Adding a
// new one here benefits every panel that uses it — never a per-panel Qt reimplementation.

public struct VStack<Content: View>: View, PrimitiveView {
    let alignment: HorizontalAlignment, spacing: Double?
    let content: Content
    public init(alignment: HorizontalAlignment = .center, spacing: Double? = nil, @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.spacing = spacing; self.content = content()
    }
    public var _childViews: [any View] { [content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "VStack")
        node.stringParams["alignment"] = alignment.name
        if let spacing { node.doubleParams["spacing"] = spacing }
        node.children = children
        return node
    }
}

public struct HStack<Content: View>: View, PrimitiveView {
    let alignment: VerticalAlignment, spacing: Double?
    let content: Content
    public init(alignment: VerticalAlignment = .center, spacing: Double? = nil, @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.spacing = spacing; self.content = content()
    }
    public var _childViews: [any View] { [content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "HStack")
        node.stringParams["alignment"] = alignment.name
        if let spacing { node.doubleParams["spacing"] = spacing }
        node.children = children
        return node
    }
}

public typealias LazyVStack<Content: View> = VStack<Content>
public typealias LazyHStack<Content: View> = HStack<Content>

public struct ZStack<Content: View>: View, PrimitiveView {
    let alignment: Alignment
    let content: Content
    public init(alignment: Alignment = .center, @ViewBuilder content: () -> Content) {
        self.alignment = alignment; self.content = content()
    }
    public var _childViews: [any View] { [content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "ZStack")
        node.stringParams["alignment"] = alignment.name
        node.children = children
        return node
    }
}

public struct Spacer: View, PrimitiveView {
    let minLength: Double?
    public init(minLength: Double? = nil) { self.minLength = minLength }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Spacer")
        if let minLength { node.doubleParams["minLength"] = minLength }
        return node
    }
}

public struct Divider: View, PrimitiveView {
    public init() {}
    public func _makeNode(children: [RenderNode]) -> RenderNode { RenderNode(kind: "Divider") }
}

/// A scrollable, row-per-element list — the Qt renderer treats it exactly like `ScrollView` + `VStack` (a
/// `QScrollArea` of stacked rows); real SwiftUI's selection/row-styling behavior isn't modeled yet.
public struct List<Data: RandomAccessCollection, ID: Hashable, Content: View>: View, PrimitiveView {
    let data: Data
    let content: (Data.Element) -> Content
    public init(_ data: Data, id: KeyPath<Data.Element, ID>, @ViewBuilder content: @escaping (Data.Element) -> Content) {
        self.data = data; self.content = content
    }
    public var _childViews: [any View] { data.map { content($0) } }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "ScrollView")
        node.boolParams["vertical"] = true
        var stack = RenderNode(kind: "VStack")
        stack.children = children
        node.children = [stack]
        return node
    }
}
extension List where Data.Element: Identifiable, ID == Data.Element.ID {
    public init(_ data: Data, @ViewBuilder content: @escaping (Data.Element) -> Content) {
        self.init(data, id: \.id, content: content)
    }
}

public struct ScrollView<Content: View>: View, PrimitiveView {
    let axes: Axis.Set
    let content: Content
    public init(_ axes: Axis.Set = .vertical, @ViewBuilder content: () -> Content) {
        self.axes = axes; self.content = content()
    }
    public var _childViews: [any View] { [content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "ScrollView")
        node.boolParams["horizontal"] = axes.contains(.horizontal)
        node.boolParams["vertical"] = axes.contains(.vertical)
        node.children = children
        return node
    }
}

public struct ScrollViewProxy {
    public func scrollTo(_ id: AnyHashable, anchor: UnitPoint? = nil) {}
}

public struct ScrollViewReader<Content: View>: View, PrimitiveView {
    let content: (ScrollViewProxy) -> Content
    public init(@ViewBuilder content: @escaping (ScrollViewProxy) -> Content) {
        self.content = content
    }
    public var _childViews: [any View] { [content(ScrollViewProxy())] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "ScrollViewReader")
        node.children = children
        return node
    }
}

public struct Axis: Sendable {
    public struct Set: OptionSet, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let horizontal = Set(rawValue: 1 << 0), vertical = Set(rawValue: 1 << 1)
    }
}

public struct GeometryProxy: Sendable {
    public var size: CGSize
    public init(size: CGSize) { self.size = size }
    /// The Qt renderer doesn't feed real per-frame layout geometry back into Swift yet (see the note below), so
    /// this returns a zero-origin rect the size of `size` regardless of `space` — structurally real, not accurate.
    public func frame(in space: CoordinateSpace) -> CGRect { CGRect(origin: .zero, size: size) }
}

/// `GeometryReader` cannot know the real Qt layout size at Swift-tree-build time; it resolves its content with a
/// zero-size proxy today (the Qt renderer re-lays-out the resolved children itself once mounted). Panels using it
/// for frame math will need a real measurement round-trip in a later phase.
public struct Grid<Content: View>: View, PrimitiveView {
    let content: Content
    public init(alignment: Alignment = .center, horizontalSpacing: Double? = nil, verticalSpacing: Double? = nil,
                @ViewBuilder content: () -> Content) {
        self.content = content()
    }
    public var _childViews: [any View] { [content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "VStack") // A Grid's rows read top-to-bottom, same as a VStack of HStacks.
        node.children = children
        return node
    }
}

public struct GridRow<Content: View>: View, PrimitiveView {
    let content: Content
    public init(alignment: VerticalAlignment = .center, @ViewBuilder content: () -> Content) { self.content = content() }
    public var _childViews: [any View] { [content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "HStack")
        node.children = children
        return node
    }
}

public struct GeometryReader<Content: View>: View, PrimitiveView {
    let content: (GeometryProxy) -> Content
    public init(@ViewBuilder content: @escaping (GeometryProxy) -> Content) { self.content = content }
    public var _childViews: [any View] { [content(GeometryProxy(size: .zero))] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "GeometryReader")
        node.children = children
        return node
    }
}
