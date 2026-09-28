// View modifiers. Each one wraps its content in `ModifiedContent`, a `PrimitiveView` whose `_makeNode` just tags
// the already-resolved child node with a `RenderModifier` — so adding a modifier here is a small, mechanical
// addition that every panel using it picks up automatically, with no Qt-side per-panel code.

import Combine
import UniformTypeIdentifiers

/// Deliberately NOT generic over its content (real SwiftUI's `ModifiedContent<Content, Modifier>` is, but pays for
/// it with compiler-privileged fast paths this compat layer doesn't have). A generic `ModifiedContent<Content>`
/// nests one new type layer per modifier in a chain — `ModifiedContent<ModifiedContent<ModifiedContent<...>>>` for
/// a 15-modifier chain — which is exactly what makes Swift's constraint solver time out on long real-world modifier
/// chains (confirmed: `Compositor/ContentView.swift`'s `editorChrome`, ~15 modifiers deep, timed out on the generic
/// version and compiles instantly with this one). Erasing `content` to `any View` makes every `modified(_:)` call
/// return the *same* concrete type, so chain length no longer grows the type the solver has to explore.
public struct ModifiedContent: View, PrimitiveView {
    let content: any View
    let apply: (inout RenderNode) -> Void
    public var _childViews: [any View] { [content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = children.first ?? RenderNode(kind: "_Empty")
        apply(&node)
        return node
    }
}

extension View {
    private func modified(_ apply: @escaping (inout RenderNode) -> Void) -> ModifiedContent {
        ModifiedContent(content: self, apply: apply)
    }

    public func frame(width: Double? = nil, height: Double? = nil, alignment: Alignment = .center) -> some View {
        modified { $0.modifiers.append(.frame(width: width, height: height, minWidth: nil, minHeight: nil, maxWidth: nil, maxHeight: nil, alignment: alignment.name)) }
    }
    public func frame(minWidth: Double? = nil, idealWidth: Double? = nil, maxWidth: Double? = nil,
                       minHeight: Double? = nil, idealHeight: Double? = nil, maxHeight: Double? = nil,
                       alignment: Alignment = .center) -> some View {
        modified { $0.modifiers.append(.frame(width: idealWidth, height: idealHeight, minWidth: minWidth, minHeight: minHeight, maxWidth: maxWidth, maxHeight: maxHeight, alignment: alignment.name)) }
    }
    public func padding(_ edges: Edge.Set = .all, _ length: Double? = nil) -> some View {
        let amount = length ?? 8
        return modified {
            $0.modifiers.append(.padding(
                top: edges.contains(.top) ? amount : 0, leading: edges.contains(.leading) ? amount : 0,
                bottom: edges.contains(.bottom) ? amount : 0, trailing: edges.contains(.trailing) ? amount : 0))
        }
    }
    public func padding(_ length: Double) -> some View { padding(.all, length) }
    public func font(_ font: Font) -> some View { modified { $0.modifiers.append(.font(font.name)) } }
    public func foregroundStyle(_ style: ShapeStyle) -> some View {
        modified { $0.modifiers.append(.foregroundStyle(style.name)) }
    }
    public func foregroundColor(_ color: Color?) -> some View {
        modified { $0.modifiers.append(.foregroundStyle(color?.name ?? "primary")) }
    }
    public func background(_ style: ShapeStyle) -> some View {
        modified { $0.modifiers.append(.background(style.name)) }
    }
    public func background<V: View>(@ViewBuilder _ content: () -> V) -> some View { self }
    public func background<V: View>(_ content: V, alignment: Alignment = .center) -> some View { self }
    public func sheet<V: View>(isPresented: Binding<Bool>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: () -> V) -> some View { self }
    public func sheet<Item: Identifiable, V: View>(item: Binding<Item?>, onDismiss: (() -> Void)? = nil,
                                                    @ViewBuilder content: (Item) -> V) -> some View { self }
    public func background<S: Shape>(_ style: ShapeStyle, in shape: S) -> some View {
        modified { $0.modifiers.append(.background(style.name)) }
    }
    public func opacity(_ value: Double) -> some View { modified { $0.modifiers.append(.opacity(value)) } }
    public func disabled(_ value: Bool) -> some View { modified { $0.modifiers.append(.disabled(value)) } }
    public func fixedSize(horizontal: Bool = true, vertical: Bool = true) -> some View {
        modified { $0.modifiers.append(.fixedSize(horizontal: horizontal, vertical: vertical)) }
    }
    public func fixedSize() -> some View { fixedSize(horizontal: true, vertical: true) }
    public func buttonStyle(_ style: StyleToken) -> some View { modified { $0.modifiers.append(.buttonStyle(style.name)) } }
    public func toggleStyle(_ style: StyleToken) -> some View { modified { $0.modifiers.append(.toggleStyle(style.name)) } }
    public func controlSize(_ size: StyleToken) -> some View { modified { $0.modifiers.append(.controlSize(size.name)) } }
    public func textFieldStyle(_ style: StyleToken) -> some View { modified { $0.modifiers.append(.textFieldStyle(style.name)) } }
    public func multilineTextAlignment(_ alignment: TextAlignment) -> some View {
        modified { $0.modifiers.append(.multilineTextAlignment(alignment.name)) }
    }
    public func contentShape(_ shape: StyleToken) -> some View { modified { $0.modifiers.append(.contentShape(shape.name)) } }
    public func contentShape<S: Shape>(_ shape: S) -> some View { modified { $0.modifiers.append(.contentShape("\(S.self)")) } }
    public func monospacedDigit() -> some View { modified { $0.modifiers.append(.font("monospacedDigit")) } }
    public func position(x: Double = 0, y: Double = 0) -> some View { self }
    public func coordinateSpace(name: some Hashable) -> some View { self }
    public func tint(_ color: Color?) -> some View { modified { $0.modifiers.append(.foregroundStyle(color?.name ?? "accentColor")) } }
    public func clipShape(_ shape: StyleToken) -> some View { modified { $0.modifiers.append(.clipShape(shape.name)) } }
    public func clipShape<S: Shape>(_ shape: S) -> some View { modified { $0.modifiers.append(.clipShape("\(S.self)")) } }
    public func labelsHidden() -> some View { self }
    public func cornerRadius(_ radius: Double) -> some View { modified { $0.modifiers.append(.cornerRadius(radius)) } }
    public func offset(x: Double = 0, y: Double = 0) -> some View { modified { $0.modifiers.append(.offset(x: x, y: y)) } }
    public func shadow(radius: Double = 1) -> some View { modified { $0.modifiers.append(.shadow(radius: radius)) } }
    public func shadow(color: Color = .black, radius: Double, x: Double = 0, y: Double = 0) -> some View {
        modified { $0.modifiers.append(.shadow(radius: radius)) }
    }
    public func scaleEffect(_ scale: Double) -> some View { modified { $0.modifiers.append(.scaleEffect(scale)) } }
    public func scaleEffect(x: Double = 1, y: Double = 1, anchor: UnitPoint = .center) -> some View {
        modified { $0.modifiers.append(.scaleEffect(max(x, y))) }
    }
    public func rotationEffect(_ radians: Double) -> some View { modified { $0.modifiers.append(.rotationEffect(radians)) } }
    public func rotationEffect(_ angle: Angle) -> some View { modified { $0.modifiers.append(.rotationEffect(angle.radians)) } }
    public func overlay<V: View>(alignment: Alignment = .center, @ViewBuilder _ content: () -> V) -> some View {
        let overlayNode = ViewResolver.resolve(content())
        return modified { $0.modifiers.append(.overlay(overlayNode, alignment: alignment.name)) }
    }
    public func overlay<V: View>(_ overlayView: V, alignment: Alignment = .center) -> some View {
        overlay(alignment: alignment) { overlayView }
    }
    public func help(_ text: String) -> some View { modified { $0.modifiers.append(.help(text)) } }
    public func accessibilityLabel(_ text: String) -> some View { modified { $0.modifiers.append(.accessibilityLabel(text)) } }
    public func accessibilityIdentifier(_ text: String) -> some View {
        modified { $0.modifiers.append(.accessibilityIdentifier(text)) }
    }
    public func accessibilityHidden(_ hidden: Bool) -> some View { self }
    public func lineLimit(_ number: Int?) -> some View { self }
    public func accessibilityValue(_ value: String) -> some View { self }
    public func accessibilityAddTraits(_ traits: AccessibilityTraits) -> some View {
        modified {
            if traits.contains(.isSelected) {
                $0.boolParams["isSelected"] = true
            }
        }
    }
    public func buttonBorderShape(_ shape: StyleToken) -> some View { self }
    public func pickerStyle(_ style: StyleToken) -> some View { self }
    public func menuStyle(_ style: StyleToken) -> some View { self }
    public func onDisappear(perform action: @escaping () -> Void = {}) -> some View { self }
    public func task(id: some Equatable, priority: TaskPriority = .userInitiated, _ action: @escaping () async -> Void) -> some View { self }
    public func task(priority: TaskPriority = .userInitiated, _ action: @escaping () async -> Void) -> some View { self }
    public func id<ID: Hashable>(_ id: ID) -> some View { modified { $0.modifiers.append(.identifier("\(id)")) } }
    public func tag<V: Hashable>(_ tag: V) -> some View { modified { $0.tagValue = AnyHashable(tag); $0.modifiers.append(.tag("\(tag)")) } }
    public func scrollIndicators(_ visibility: StyleToken, axes: Axis.Set = [.horizontal, .vertical]) -> some View {
        modified { $0.modifiers.append(.scrollIndicators(visibility.name)) }
    }
    public func keyboardShortcut(_ key: KeyEquivalent, modifiers: EventModifiers = .command) -> some View {
        modified { $0.modifiers.append(.keyboardShortcut(key: String(key.character), modifiers: modifiers.rawValue)) }
    }
    public func onAppear(perform action: @escaping () -> Void = {}) -> some View {
        modified { $0.modifiers.append(.onAppear(action)) }
    }
    public func onSubmit(_ action: @escaping () -> Void) -> some View { modified { $0.handlers["submit"] = { _ in action() } } }
    public func onExitCommand(perform action: @escaping () -> Void) -> some View {
        modified { $0.modifiers.append(.onExitCommand(action)) }
    }
    public func focused(_ condition: FocusState<Bool>) -> some View {
        modified { $0.modifiers.append(.sink("focused", { newValue in
            if let bool = newValue as? Bool { condition.wrappedValue = bool }
        })) }
    }
    /// `.focused($field, equals: .someCase)` — sets `field` to `value` while this view holds focus, `nil` otherwise.
    public func focused<Value: Hashable>(_ binding: FocusState<Value?>, equals value: Value) -> some View {
        modified { $0.modifiers.append(.sink("focused", { newValue in
            if let isFocused = newValue as? Bool { binding.wrappedValue = isFocused ? value : nil }
        })) }
    }
    public func onChange<V: Equatable>(of value: V, initial: Bool = false, _ action: @escaping (V, V) -> Void) -> some View {
        modified { $0.modifiers.append(.sink("onChange", { newValue in
            if let typed = newValue as? V { action(value, typed) }
        })) }
    }
    /// `@_disfavoredOverload`: real, load-bearing fix, not decoration. Having both this and the 2-arg overload
    /// above visible as equally-good candidates is what made Swift's constraint solver time out on
    /// `Compositor/ContentView.swift`'s `body` (11 chained `.onChange` calls, all 2-arg) — confirmed by removing
    /// this overload entirely, which made the exact same "unable to type-check in reasonable time" error vanish
    /// (25s build vs. 2+ minutes at a 1000x-raised solver threshold that still failed). `@_disfavoredOverload`
    /// tells the solver to only consider this one after every other candidate fails, so the 11 two-argument call
    /// sites resolve immediately without it in the running, while `TransformInspector.swift`'s three genuine
    /// zero-argument calls still resolve correctly (just as the last candidate tried, not the first).
    @_disfavoredOverload
    public func onChange<V: Equatable>(of value: V, initial: Bool = false, _ action: @escaping () -> Void) -> some View {
        modified { $0.modifiers.append(.sink("onChange", { _ in action() })) }
    }
    public func gesture<G>(_ gesture: G) -> some View { self }
    public func onTapGesture(count: Int = 1, perform action: @escaping () -> Void) -> some View {
        modified { $0.handlers[count == 2 ? "doubleTap" : "tap"] = { _ in action() } }
    }
    /// Structurally real (stores the subscription intent), but inert: firing it needs a live run loop tied to Qt's
    /// event loop, which this compat layer doesn't wire up yet. See `Combine.swift`.
    public func onReceive<P: CombinePublisher>(_ publisher: P, perform action: @escaping (P.Output) -> Void) -> some View { self }
    public func mask<V: View>(alignment: Alignment = .center, @ViewBuilder _ mask: () -> V) -> some View { self }
    public func onScrollGeometryChange<T: Equatable>(for type: T.Type, of transform: @escaping (ScrollGeometry) -> T,
                                                      action: @escaping (T, T) -> Void) -> some View { self }
    /// Structurally real; actually recognising a drag-and-drop gesture needs a Qt-side drag backend this compat
    /// layer doesn't have yet, so `delegate`'s callbacks never fire. Same honesty as `.onReceive` above.
    public func onDrop(of types: [String], delegate: any DropDelegate) -> some View { self }
    public func onDrop(of types: [String], isTargeted: Binding<Bool>? = nil,
                       perform action: @escaping ([NSItemProvider], CGPoint) -> Bool) -> some View { self }
    public func animation<V: Equatable>(_ animation: StyleToken?, value: V) -> some View { self }
    public func aspectRatio(_ aspectRatio: Double? = nil, contentMode: ContentMode) -> some View { self }
    public func aspectRatio(contentMode: ContentMode) -> some View { self }
    public func popover<V: View>(isPresented: Binding<Bool>, @ViewBuilder content: () -> V) -> some View { self }
    /// Preserve menu nodes and their action handlers for Qt's native context-menu event.
    /// The renderer omits them from the normal layout and presents them on request.
    public func contextMenu<M: View>(@ViewBuilder menuItems: () -> M) -> some View {
        let menu = ViewResolver.resolve(menuItems())
        return modified { node in
            var wrapper = RenderNode(kind: "ContextMenu")
            wrapper.children = [menu]
            node.children.append(wrapper)
        }
    }
    public func simultaneousGesture<G>(_ gesture: G) -> some View { self }
    public func clipped() -> some View { self }
    public func scrollBounceBehavior(_ behavior: ScrollBounceBehavior, axes: Axis.Set = [.vertical]) -> some View { self }
    public func allowsHitTesting(_ enabled: Bool) -> some View { self }
    public func preferredColorScheme(_ colorScheme: ColorScheme?) -> some View { self }
    public func navigationTitle(_ title: String) -> some View { self }
    public func sharedBackgroundVisibility(_ visibility: StyleToken) -> some View { self }
    public func pointerStyle(_ style: PointerStyle) -> some View { self }
    /// The general form (`GeometryProxy`), distinct from `.onScrollGeometryChange` above. The Qt renderer doesn't
    /// feed real per-frame layout back into Swift yet, so `transform` only ever sees `GeometryProxy`'s zero-origin
    /// placeholder rect — structurally real, not yet live, the same honesty as `.onReceive`/`.onDrop`.
    public func onGeometryChange<T: Equatable>(for type: T.Type, of transform: @escaping (GeometryProxy) -> T,
                                                action: @escaping (T) -> Void) -> some View { self }
    public func toolbar<Content: View>(@ViewBuilder content: () -> Content) -> some View { self }
    public func fileImporter(isPresented: Binding<Bool>, allowedContentTypes: [UTType], allowsMultipleSelection: Bool = false,
                              onCompletion: @escaping (Result<[URL], any Error>) -> Void) -> some View { self }
    public func alert<A: View>(_ title: String, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A) -> some View { self }
    public func alert<A: View, M: View>(_ title: String, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A,
                                         @ViewBuilder message: () -> M) -> some View { self }


    /// Applies a custom `ViewModifier` (the mechanism `Compositor/UI`'s own modifiers — e.g. `NewProjectDropTarget`
    /// — use). Delegates to the modifier's own `body`, so it composes normally instead of needing a `RenderModifier`
    /// case of its own.
    public func modifier<M: ViewModifier>(_ modifier: M) -> some View {
        _CustomModified(content: self, modifier: modifier)
    }
}

public protocol ViewModifier {
    associatedtype Body: View
    typealias Content = _ViewModifierContent
    @ViewBuilder func body(content: Content) -> Body
}

public struct _ViewModifierContent: View, _ViewListProviding {
    let view: any View
    public var _viewList: [any View] { [view] }
}

struct _CustomModified<Content: View, M: ViewModifier>: View {
    let content: Content
    let modifier: M
    var body: some View { modifier.body(content: _ViewModifierContent(view: content)) }
}

extension View {
    public func accessibilityElement(children: AccessibilityChildBehavior = .ignore) -> some View { self }
}

public enum AccessibilityChildBehavior: Sendable {
    case ignore, contain, combine
}

public struct ScrollGeometry: Sendable {
    public var contentOffset: CGPoint
    public var contentSize: CGSize
    public init(contentOffset: CGPoint = .zero, contentSize: CGSize = .zero) {
        self.contentOffset = contentOffset
        self.contentSize = contentSize
    }
}

public struct DropProposal {
    public enum Operation { case cancel, move, copy, forbidden }
    public var operation: Operation
    public init(operation: Operation) { self.operation = operation }
}

public protocol DropDelegate {
    func validateDrop(info: DropInfo) -> Bool
    func dropEntered(info: DropInfo)
    func dropUpdated(info: DropInfo) -> DropProposal?
    func dropExited(info: DropInfo)
    func performDrop(info: DropInfo) -> Bool
}

public extension DropDelegate {
    func validateDrop(info: DropInfo) -> Bool { true }
    func dropEntered(info: DropInfo) {}
    func dropUpdated(info: DropInfo) -> DropProposal? { nil }
    func dropExited(info: DropInfo) {}
}

public struct DropInfo {
    public let location: CGPoint
    public init(location: CGPoint = .zero) { self.location = location }
    public func hasItemsConforming(to types: [String]) -> Bool { false }
    public func itemProviders(for types: [String]) -> [NSItemProvider] { [] }
}

/// `.toolbar { }`'s content: the Qt renderer doesn't host a native toolbar yet (the panels this unblocks —
/// `ContentView.swift`'s New/zoom/tab-strip controls — render fine as ordinary tree nodes; only their placement
/// in a system toolbar bar is inert), so these resolve their content like `Group` and carry no placement of their
/// own. Structurally real, matching the rest of this file's honesty about what's live vs. what only type-checks.
public struct ToolbarItemPlacement: Sendable, Equatable {
    let name: String
    public init(_ name: String) { self.name = name }
    public static let navigation = ToolbarItemPlacement("navigation")
    public static let primaryAction = ToolbarItemPlacement("primaryAction")
    public static let automatic = ToolbarItemPlacement("automatic")
}

public struct ToolbarItem<Content: View>: View, _ViewListProviding {
    let content: Content
    public init(placement: ToolbarItemPlacement = .automatic, @ViewBuilder content: () -> Content) { self.content = content() }
    public var _viewList: [any View] { [content] }
}

public struct ToolbarItemGroup<Content: View>: View, _ViewListProviding {
    let content: Content
    public init(placement: ToolbarItemPlacement = .automatic, @ViewBuilder content: () -> Content) { self.content = content() }
    public var _viewList: [any View] { [content] }
}

public struct ToolbarSpacer: View, PrimitiveView {
    public enum Sizing { case fixed, flexible }
    public init(_ sizing: Sizing = .flexible, placement: ToolbarItemPlacement = .automatic) {}
    public func _makeNode(children: [RenderNode]) -> RenderNode { RenderNode(kind: "Spacer") }
}

public enum ColorScheme: Sendable { case light, dark }

/// `NSCursor`-shaped pointer hints (`.pointerStyle(.columnResize)` on a resize handle) — inert: the Qt renderer
/// doesn't wire cursor changes to hover state yet.
public struct PointerStyle: Sendable {
    public static let columnResize = PointerStyle()
    public static let horizontalResize = PointerStyle()
    public static let verticalResize = PointerStyle()
}

