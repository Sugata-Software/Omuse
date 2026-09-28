// The `View` protocol and `@ViewBuilder`. Upstream `Compositor/UI/*.swift` panels declare `var body: some View { ... }`
// exactly as they do on macOS; this file is what makes that compile and resolve into a `RenderNode` tree (see
// RenderTree.swift) instead of needing a hand-written Qt mirror per panel.

public protocol View {
    associatedtype Body: View
    @ViewBuilder var body: Body { get }
}

extension Never: View {
    public var body: Never { fatalError("Never has no body") }
}

public struct EmptyView: View, _ViewListProviding {
    public init() {}
    public var _viewList: [any View] { [] }
}

/// Erases the static arity SwiftUI's real `TupleView<T>` preserves; nothing in this codebase inspects a tuple
/// view's shape, so a flat, type-erased element list is enough to resolve correctly.
public struct TupleView: View, _ViewListProviding {
    let elements: [any View]
    public var _viewList: [any View] { elements }
}

/// Keeps both `if`/`else` branch types generic all the way through (matching real SwiftUI's `_ConditionalContent`)
/// — erasing to a fixed wrapper type here breaks the compiler's generic inference for the surrounding `buildBlock`
/// call whenever a branch itself involves further opaque `some View` chains (exactly what real panels do).
public enum _ConditionalContent<TrueContent: View, FalseContent: View>: View, _ViewListProviding {
    case trueContent(TrueContent)
    case falseContent(FalseContent)
    public var _viewList: [any View] {
        switch self {
        case .trueContent(let view): return [view]
        case .falseContent(let view): return [view]
        }
    }
}

extension Optional: View, PrimitiveView, _ViewListProviding where Wrapped: View {
    public var _viewList: [any View] { map { [$0] } ?? [] }
}

extension Array: View, PrimitiveView, _ViewListProviding where Element: View {
    public var _viewList: [any View] { map { $0 } }
}

public struct Group<Content: View>: View, _ViewListProviding {
    let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    public var _viewList: [any View] { [content] }
}

/// A static/identifiable data-driven list of views. This compat layer resolves it eagerly at render time (it
/// flattens into its element views) rather than diffing row-by-row; that's enough for the mostly-small collections
/// (layer lists, palette swatches, tool pickers) `Compositor/UI` builds `ForEach` over.
public struct ForEach<Data: RandomAccessCollection, ID: Hashable, Content: View>: View, _ViewListProviding {
    let data: Data
    let content: (Data.Element) -> Content
    public init(_ data: Data, id: KeyPath<Data.Element, ID>, @ViewBuilder content: @escaping (Data.Element) -> Content) {
        self.data = data; self.content = content
    }
    public var _viewList: [any View] { data.map { content($0) } }
    /// Registers a reorder handler with SwiftUI's own `(IndexSet, Int) -> Void` shape. Stored for the day the Qt
    /// renderer implements list drag-and-drop; until then this compiles and does nothing, same as `.gesture`.
    public func onMove(perform action: @escaping (IndexSet, Int) -> Void) -> ForEach { self }
    public func onDelete(perform action: @escaping (IndexSet) -> Void) -> ForEach { self }
}

extension ForEach where Data.Element: Identifiable, ID == Data.Element.ID {
    public init(_ data: Data, @ViewBuilder content: @escaping (Data.Element) -> Content) {
        self.init(data, id: \.id, content: content)
    }
}

extension ForEach where Data == Range<Int>, ID == Int {
    public init(_ data: Range<Int>, @ViewBuilder content: @escaping (Int) -> Content) {
        self.init(data, id: \.self, content: content)
    }
}

@resultBuilder
public enum ViewBuilder {
    public static func buildBlock() -> EmptyView { EmptyView() }

    // `buildPartialBlock` (unlimited, left-folded pairwise) rather than a fixed `buildBlock<C0...C9>` arity cap —
    // real panels have bodies with more than 10 top-level statements (LevelsSheet.swift has 12), and real SwiftUI
    // itself moved to this exact mechanism for the same reason.
    public static func buildPartialBlock<Content: View>(first content: Content) -> Content { content }
    public static func buildPartialBlock<C0: View, C1: View>(accumulated: C0, next: C1) -> TupleView {
        let existing = (accumulated as? TupleView)?.elements ?? [accumulated]
        return TupleView(elements: existing + [next])
    }

    public static func buildOptional<Content: View>(_ component: Content?) -> Content? { component }
    public static func buildEither<TrueContent: View, FalseContent: View>(
        first component: TrueContent
    ) -> _ConditionalContent<TrueContent, FalseContent> { .trueContent(component) }
    public static func buildEither<TrueContent: View, FalseContent: View>(
        second component: FalseContent
    ) -> _ConditionalContent<TrueContent, FalseContent> { .falseContent(component) }
    public static func buildArray<Content: View>(_ components: [Content]) -> [Content] { components }
    public static func buildExpression<Content: View>(_ expression: Content) -> Content { expression }
    public static func buildLimitedAvailability<Content: View>(_ component: Content) -> Content { component }
}
