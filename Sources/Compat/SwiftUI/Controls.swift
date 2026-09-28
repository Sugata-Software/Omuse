// The leaf controls. Each is a thin value type carrying exactly what real SwiftUI's initializer takes; the Qt
// renderer turns each into one widget (QLabel, QPushButton, QSlider, ...) — see the architecture plan for the
// kind→widget mapping.

public struct Text: View, PrimitiveView, ExpressibleByStringInterpolation {
    let content: String
    public init(_ content: String) { self.content = content }
    public init(stringLiteral value: String) { content = value }
    public init(verbatim content: String) { self.content = content }
    public init<V: BinaryFloatingPoint>(_ value: V, format: FloatingPointFormatStyle<V>) {
        let pct = Double(value) * 100.0
        let str = String(format: "%.1f%%", pct).replacingOccurrences(of: ".0%", with: "%")
        self.init(str)
    }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Text")
        node.stringParams["text"] = content
        return node
    }
}

public struct FloatingPointFormatStyle<Value: BinaryFloatingPoint>: Sendable {
    public static var percent: FloatingPointFormatStyle<Value> { FloatingPointFormatStyle() }
    public func precision(_ p: Precision) -> FloatingPointFormatStyle<Value> { self }
    public struct Precision: Sendable {
        public static func fractionLength(_ range: ClosedRange<Int>) -> Precision { Precision() }
    }
}


public struct Label<Title: View, Icon: View>: View, PrimitiveView {
    let title: Title, icon: Icon
    public init(@ViewBuilder title: () -> Title, @ViewBuilder icon: () -> Icon) { self.title = title(); self.icon = icon() }
    public var _childViews: [any View] { [title, icon] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Label")
        node.children = children
        return node
    }
}
extension Label where Title == Text, Icon == Image {
    public init(_ title: String, systemImage: String) {
        self.init(title: { Text(title) }, icon: { Image(systemName: systemImage) })
    }
}

public struct Image: View, PrimitiveView {
    let source: String
    public init(systemName: String) { source = "system:\(systemName)" }
    public init(_ name: String) { source = "named:\(name)" }
    public init(decorative cgImage: CGImage, scale: Double) { source = "cgImage" }
    public func resizable() -> Image { self }
    public func scaledToFit() -> some View { self }
    public func scaledToFill() -> some View { self }
    public func aspectRatio(_ aspectRatio: Double? = nil, contentMode: ContentMode) -> some View { self }
    public func aspectRatio(contentMode: ContentMode) -> some View { self }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Image")
        node.stringParams["source"] = source
        return node
    }
}

public struct Button<Label: View>: View, PrimitiveView {
    let label: Label
    let action: () -> Void
    public init(action: @escaping () -> Void, @ViewBuilder label: () -> Label) { self.action = action; self.label = label() }
    public var _childViews: [any View] { [label] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Button")
        node.handlers["action"] = { _ in action() }
        node.children = children
        return node
    }
}
extension Button where Label == Text {
    public init(_ title: String, action: @escaping () -> Void) { self.init(action: action) { Text(title) } }
    /// `role:` doesn't change the render tree's shape (no destructive-red styling here yet); it exists so upstream's
    /// `Button("OK", role: .cancel) { ... }` calls compile.
    public init(_ title: String, role: ButtonRole, action: @escaping () -> Void) { self.init(action: action) { Text(title) } }
}
extension Button {
    public init(role: ButtonRole, action: @escaping () -> Void, @ViewBuilder label: () -> Label) {
        self.init(action: action, label: label)
    }
}
public enum ButtonRole: Sendable { case destructive, cancel }

public struct Toggle<Label: View>: View, PrimitiveView {
    let label: Label
    let isOn: Binding<Bool>
    public init(isOn: Binding<Bool>, @ViewBuilder label: () -> Label) { self.isOn = isOn; self.label = label() }
    public var _childViews: [any View] { [label] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Toggle")
        node.boolParams["isOn"] = isOn.wrappedValue
        node.handlers["isOn"] = { newValue in if let bool = newValue as? Bool { isOn.wrappedValue = bool } }
        node.children = children
        return node
    }
}
extension Toggle where Label == Text {
    public init(_ title: String, isOn: Binding<Bool>) { self.init(isOn: isOn) { Text(title) } }
}

public struct Slider<Value: BinaryFloatingPoint>: View, PrimitiveView {
    let value: Binding<Value>
    let bounds: ClosedRange<Value>
    public init(value: Binding<Value>, in bounds: ClosedRange<Value> = 0...1,
                onEditingChanged: @escaping (Bool) -> Void = { _ in }) {
        self.value = value; self.bounds = bounds
    }
    public init(value: Binding<Value>, in bounds: ClosedRange<Value>, step: Value.Stride,
                onEditingChanged: @escaping (Bool) -> Void = { _ in }) {
        self.value = value; self.bounds = bounds
    }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Slider")
        node.doubleParams["value"] = Double(value.wrappedValue)
        node.doubleParams["lowerBound"] = Double(bounds.lowerBound)
        node.doubleParams["upperBound"] = Double(bounds.upperBound)
        node.handlers["value"] = { newValue in if let double = newValue as? Double { value.wrappedValue = Value(double) } }
        return node
    }
}

public struct ProgressView: View, PrimitiveView {
    let fraction: Double?
    public init() { fraction = nil }
    public init(value: Double?, total: Double = 1) { fraction = value.map { $0 / total } }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "ProgressView")
        if let fraction { node.doubleParams["fraction"] = fraction }
        return node
    }
}

/// `format:`/`text:` text-entry field. Numeric fields (`value:`) round-trip through `Double`; upstream's own
/// `Binding<Double>` closures handle any further unit conversion, matching real SwiftUI's contract.
public struct TextField<Label: View>: View, PrimitiveView {
    let label: Label
    let text: Binding<String>?
    let value: Binding<Double>?
    let prompt: Text?
    public init(_ titleKey: String, text: Binding<String>, prompt: Text? = nil) where Label == Text {
        label = Text(titleKey); self.text = text; value = nil; self.prompt = prompt
    }
    public init(_ titleKey: String, value: Binding<Double>, format: NumberFormat = .number, prompt: Text? = nil) where Label == Text {
        label = Text(titleKey); self.value = value; text = nil; self.prompt = prompt
    }
    /// An integer-valued field: real SwiftUI's `TextField(_:value:format:)` is generic over the format's value
    /// type, so upstream binds it straight to `Int` properties too. Rounds through `Double` internally.
    public init(_ titleKey: String, value intValue: Binding<Int>, format: NumberFormat = .number, prompt: Text? = nil) where Label == Text {
        label = Text(titleKey); text = nil
        value = Binding(get: { Double(intValue.wrappedValue) }, set: { intValue.wrappedValue = Int($0.rounded()) })
        self.prompt = prompt
    }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "TextField")
        node.stringParams["label"] = ViewResolver.resolve(label).stringParams["text"]
        if let text {
            node.stringParams["text"] = text.wrappedValue
            node.handlers["text"] = { newValue in if let string = newValue as? String { text.wrappedValue = string } }
        }
        if let value {
            node.doubleParams["value"] = value.wrappedValue
            node.handlers["value"] = { newValue in if let double = newValue as? Double { value.wrappedValue = double } }
        }
        return node
    }
}

public struct Picker<Label: View, SelectionValue: Hashable, Content: View>: View, PrimitiveView {
    let label: Label
    let selection: Binding<SelectionValue>
    let content: Content
    public init(selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        self.selection = selection; self.content = content(); self.label = label()
    }
    public var _childViews: [any View] { [label, content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Picker")
        node.stringParams["selection"] = "\(selection.wrappedValue)"
        node.children = children
        // Preserve typed tags locally; only the option index crosses the Qt ABI.
        let values = children.dropFirst().map { ($0.tagValue?.base as? SelectionValue) ?? ($0.stringParams["text"] as? SelectionValue) }
        node.handlers["selection"] = { payload in
            guard let index = payload as? Int, values.indices.contains(index), let value = values[index] else { return }
            selection.wrappedValue = value
        }
        return node
    }
}
extension Picker where Label == Text {
    public init(_ titleKey: String, selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content) {
        self.init(selection: selection, content: content) { Text(titleKey) }
    }
}

public struct ColorPicker<Label: View>: View, PrimitiveView {
    let label: Label
    let selection: Binding<Color>
    let supportsOpacity: Bool
    public init(selection: Binding<Color>, supportsOpacity: Bool = true, @ViewBuilder label: () -> Label) {
        self.selection = selection; self.supportsOpacity = supportsOpacity; self.label = label()
    }
    public var _childViews: [any View] { [label] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "ColorPicker")
        node.stringParams["selection"] = selection.wrappedValue.name
        node.boolParams["supportsOpacity"] = supportsOpacity
        node.children = children
        return node
    }
}
extension ColorPicker where Label == Text {
    public init(_ titleKey: String, selection: Binding<Color>, supportsOpacity: Bool = true) {
        self.init(selection: selection, supportsOpacity: supportsOpacity) { Text(titleKey) }
    }
}

public struct Menu<Label: View, Content: View>: View, PrimitiveView {
    let label: Label
    let content: Content
    public init(@ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        self.content = content(); self.label = label()
    }
    public var _childViews: [any View] { [label, content] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Menu")
        node.children = children
        return node
    }
}
extension Menu where Label == Text {
    public init(_ titleKey: String, @ViewBuilder content: () -> Content) {
        self.init(content: content) { Text(titleKey) }
    }
}

public struct Link<Label: View>: View, PrimitiveView {
    let label: Label
    let destination: URL
    public init(destination: URL, @ViewBuilder label: () -> Label) { self.destination = destination; self.label = label() }
    public var _childViews: [any View] { [label] }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Link")
        node.stringParams["destination"] = destination.absoluteString
        node.children = children
        return node
    }
}

/// Immediate-mode drawing surface. The Qt renderer gives it a widget whose `paintEvent` hands upstream's closure a
/// real `GraphicsContext` wrapping the existing `CGContext`→Skia bridge (Sources/Compat/CoreGraphics), so drawing
/// code (histograms, curve graphs, rulers) needs no Linux-specific rewrite.
public struct Canvas: View, PrimitiveView {
    let renderer: (inout GraphicsContext, CGSize) -> Void
    public init(renderersOnly: Bool = false, opaque: Bool = false,
                rendersAsynchronously: Bool = false,
                renderer: @escaping (inout GraphicsContext, CGSize) -> Void) {
        self.renderer = renderer
    }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Canvas")
        node.handlers["draw"] = { boxed in
            guard var box = boxed as? GraphicsContextBox else { return }
            renderer(&box.context, box.size)
        }
        return node
    }
}

/// The boxed pair `Canvas`'s draw handler receives from the Qt side (an `Any` payload, since `RenderNode.handlers`
/// is untyped) — a real `GraphicsContext`/size the CoreGraphics compat bridge already knows how to fill from Skia.
public struct GraphicsContextBox {
    public var context: GraphicsContext
    public var size: CGSize
    public init(context: GraphicsContext, size: CGSize) { self.context = context; self.size = size }
}

public struct LinearGradient: View, PrimitiveView {
    let colors: [Color], startPoint: UnitPoint, endPoint: UnitPoint
    public init(colors: [Color], startPoint: UnitPoint, endPoint: UnitPoint) {
        self.colors = colors; self.startPoint = startPoint; self.endPoint = endPoint
    }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "LinearGradient")
        node.stringParams["colors"] = colors.map(\.name).joined(separator: ",")
        node.stringParams["startPoint"] = startPoint.name
        node.stringParams["endPoint"] = endPoint.name
        return node
    }
}

public struct UnitPoint: Sendable, Equatable {
    let name: String
    public static let top = UnitPoint(name: "top"), bottom = UnitPoint(name: "bottom")
    public static let leading = UnitPoint(name: "leading"), trailing = UnitPoint(name: "trailing")
    public static let center = UnitPoint(name: "center")
    public static let topLeading = UnitPoint(name: "topLeading"), topTrailing = UnitPoint(name: "topTrailing")
    public static let bottomLeading = UnitPoint(name: "bottomLeading"), bottomTrailing = UnitPoint(name: "bottomTrailing")
}

