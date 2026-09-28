// Minimal value types upstream `Compositor/UI/*.swift` names directly (`.leading`, `.horizontal`, `.callout`,
// `.secondary`, ...). They carry just enough information (a description string, mostly) for `RenderNode` to record
// what the Qt renderer should do; they are not a rendering engine of their own.

import AppKit

public struct Edge: Hashable, Sendable {
    public struct Set: OptionSet, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let top = Set(rawValue: 1 << 0), leading = Set(rawValue: 1 << 1)
        public static let bottom = Set(rawValue: 1 << 2), trailing = Set(rawValue: 1 << 3)
        public static let horizontal: Set = [.leading, .trailing]
        public static let vertical: Set = [.top, .bottom]
        public static let all: Set = [.top, .leading, .bottom, .trailing]
    }
}

public struct EdgeInsets: Sendable {
    public var top: Double, leading: Double, bottom: Double, trailing: Double
    public init(top: Double = 0, leading: Double = 0, bottom: Double = 0, trailing: Double = 0) {
        self.top = top; self.leading = leading; self.bottom = bottom; self.trailing = trailing
    }
}

public struct ScrollBounceBehavior: Sendable, Equatable {
    public let name: String
    public static let automatic = ScrollBounceBehavior(name: "automatic")
    public static let always = ScrollBounceBehavior(name: "always")
    public static let basedOnSize = ScrollBounceBehavior(name: "basedOnSize")
}

public enum ContentMode: Sendable {
    case fit
    case fill
}

public struct AccessibilityTraits: OptionSet, Sendable {
    public let rawValue: UInt
    public init(rawValue: UInt) { self.rawValue = rawValue }
    public static let isSelected = AccessibilityTraits(rawValue: 1)
    public static let isButton = AccessibilityTraits(rawValue: 2)
    public static let isHeader = AccessibilityTraits(rawValue: 4)
}



public struct HorizontalAlignment: Sendable, Equatable { public let name: String
    public static let leading = HorizontalAlignment(name: "leading")
    public static let center = HorizontalAlignment(name: "center")
    public static let trailing = HorizontalAlignment(name: "trailing")
}
public struct VerticalAlignment: Sendable, Equatable { public let name: String
    public static let top = VerticalAlignment(name: "top")
    public static let center = VerticalAlignment(name: "center")
    public static let bottom = VerticalAlignment(name: "bottom")
    public static let firstTextBaseline = VerticalAlignment(name: "firstTextBaseline")
}
public struct Alignment: Sendable, Equatable {
    public let name: String
    public init(name: String) { self.name = name }
    public init(horizontal: HorizontalAlignment, vertical: VerticalAlignment) { name = "\(horizontal.name)-\(vertical.name)" }
    public static let center = Alignment(name: "center"), leading = Alignment(name: "leading")
    public static let trailing = Alignment(name: "trailing"), top = Alignment(name: "top")
    public static let bottom = Alignment(name: "bottom"), topLeading = Alignment(name: "topLeading")
    public static let topTrailing = Alignment(name: "topTrailing"), bottomLeading = Alignment(name: "bottomLeading")
    public static let bottomTrailing = Alignment(name: "bottomTrailing")
}

public struct TextAlignment: Sendable, Equatable {
    public let name: String
    public static let leading = TextAlignment(name: "leading"), center = TextAlignment(name: "center")
    public static let trailing = TextAlignment(name: "trailing")
}

/// Stands for both `Font` and any `ShapeStyle`/`Color` token upstream code names (`.callout`, `.secondary`,
/// `.orange`, `Color(nsColor:)`, ...) — the Qt renderer maps the `name` to a real `QFont`/`QPalette` role/`QColor`.
/// Also conforms to `View` (as a filled-rectangle leaf) so `Color.clear`/`Color.black` etc. can be used directly as
/// a view — a common real-SwiftUI pattern (`Color.clear.contentShape(Rectangle())`, a background fill, ...). `Font`
/// values technically inherit this too since they share the same underlying type; harmless, since nothing actually
/// builds a `Font` as a standalone view.
public struct StyleToken: Sendable, Equatable, ExpressibleByStringLiteral, PrimitiveView {
    public let name: String
    public init(_ name: String) { self.name = name }
    public init(stringLiteral value: String) { name = value }
    public init(cgColor: CGColor) { name = "cgColor:\(cgColor.red),\(cgColor.green),\(cgColor.blue),\(cgColor.alpha)" }
    public init(nsColor: NSColor) { name = "rgb:\(nsColor.redComponent),\(nsColor.greenComponent),\(nsColor.blueComponent),\(nsColor.alphaComponent)" }
    /// `Color(.sRGB, red:green:blue:opacity:)` — the colorspace argument is unused (Skia paints sRGB throughout).
    public init(_ colorSpace: ColorRenderingSpace = .sRGB, red: Double, green: Double, blue: Double, opacity: Double = 1) {
        name = "rgb:\(red),\(green),\(blue),\(opacity)"
    }
    public init(hue: Double, saturation: Double, brightness: Double, opacity: Double = 1) {
        name = "hsb:\(hue),\(saturation),\(brightness),\(opacity)"
    }
    public init(white: Double, opacity: Double = 1) { name = "white:\(white),\(opacity)" }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "Color")
        node.stringParams["name"] = name
        return node
    }
}
public typealias Font = StyleToken
public typealias Color = StyleToken
public typealias ShapeStyle = StyleToken

extension StyleToken {
    // Animation-shaped members (`.animation(.easeOut(duration:), value:)`). The compat layer's `.animation`
    // modifier is inert (see Modifiers.swift), so these only need to type-check, not carry real curve data.
    public static let linear = StyleToken("linear")
    public static func easeIn(duration: Double = 0.35) -> StyleToken { StyleToken("easeIn") }
    public static func easeOut(duration: Double = 0.35) -> StyleToken { StyleToken("easeOut") }
    public static func easeInOut(duration: Double = 0.35) -> StyleToken { StyleToken("easeInOut") }
    public static func linear(duration: Double) -> StyleToken { StyleToken("linear") }

    // Font.TextStyle-shaped members.
    public static let largeTitle = StyleToken("largeTitle"), title = StyleToken("title"), title2 = StyleToken("title2")
    public static let title3 = StyleToken("title3"), headline = StyleToken("headline"), subheadline = StyleToken("subheadline")
    public static let body = StyleToken("body"), callout = StyleToken("callout"), footnote = StyleToken("footnote")
    public static let caption = StyleToken("caption"), caption2 = StyleToken("caption2")
    // Color-shaped members.
    public static let primary = StyleToken("primary"), secondary = StyleToken("secondary"), clear = StyleToken("clear")
    public static let tertiary = StyleToken("tertiary"), quaternary = StyleToken("quaternary")
    public static let regularMaterial = StyleToken("regularMaterial"), thinMaterial = StyleToken("thinMaterial")
    public static let ultraThinMaterial = StyleToken("ultraThinMaterial"), thickMaterial = StyleToken("thickMaterial")
    public static let red = StyleToken("red"), orange = StyleToken("orange"), yellow = StyleToken("yellow")
    public static let green = StyleToken("green"), blue = StyleToken("blue"), purple = StyleToken("purple")
    public static let cyan = StyleToken("cyan")
    public static let white = StyleToken("white"), black = StyleToken("black"), gray = StyleToken("gray")
    public static let accentColor = StyleToken("accentColor")
    // Font.Weight-shaped members (also reused for `.controlSize`/similar small enums — a bare style tag either way).
    public static let ultraLight = StyleToken("ultraLight"), thin = StyleToken("thin"), light = StyleToken("light")
    public static let regular = StyleToken("regular"), medium = StyleToken("medium"), semibold = StyleToken("semibold")
    public static let bold = StyleToken("bold"), heavy = StyleToken("heavy")
    public static let small = StyleToken("small"), large = StyleToken("large"), mini = StyleToken("mini")
    // TextFieldStyle/ButtonStyle/ToggleStyle-shaped members.
    public static let roundedBorder = StyleToken("roundedBorder"), plain = StyleToken("plain")
    public static let bordered = StyleToken("bordered"), borderedProminent = StyleToken("borderedProminent")
    public static let checkbox = StyleToken("checkbox"), automatic = StyleToken("automatic"), button = StyleToken("button")
    public static let segmented = StyleToken("segmented"), borderless = StyleToken("borderless"), menu = StyleToken("menu")
    public static let radioGroup = StyleToken("radioGroup")
    public static let capsule = StyleToken("capsule")
    public static let borderlessButton = StyleToken("borderlessButton")
    // ScrollIndicatorVisibility-shaped members.
    public static let hidden = StyleToken("hidden"), visible = StyleToken("visible")
    public func weight(_ weight: StyleToken) -> StyleToken { StyleToken("\(name)+weight:\(weight.name)") }
    public func bold() -> StyleToken { StyleToken("\(name)+bold") }
    public func monospacedDigit() -> StyleToken { StyleToken("\(name)+monospacedDigit") }
    /// `Color`-shaped: `Color.black.opacity(0.35)` — a new color token, not the `View.opacity(_:)` modifier.
    public func opacity(_ value: Double) -> StyleToken { StyleToken("\(name)+opacity:\(value)") }
    public static func system(size: Double, weight: StyleToken = "regular") -> StyleToken {
        StyleToken("system:\(size)+weight:\(weight.name)")
    }
    public static func system(_ style: StyleToken, design: StyleToken = "default") -> StyleToken {
        StyleToken("\(style.name)+design:\(design.name)")
    }
    public static let monospaced = StyleToken("monospaced"), rounded = StyleToken("rounded"), serif = StyleToken("serif")
    public static let `default` = StyleToken("default")
}

/// A minimal stand-in for Foundation's `FormatStyle` machinery — just enough for `TextField(_:value:format:)`'s
/// `.number.precision(.fractionLength(_:))` chains to compile; the Qt renderer formats the number itself.
public struct NumberFormat: Sendable {
    public static let number = NumberFormat()
    public func precision(_ precision: NumberFormatPrecision) -> NumberFormat { self }
}
public struct NumberFormatPrecision: Sendable {
    public static func fractionLength(_ length: Int) -> NumberFormatPrecision { NumberFormatPrecision() }
    public static func fractionLength(_ range: ClosedRange<Int>) -> NumberFormatPrecision { NumberFormatPrecision() }
}

/// A single key for `.keyboardShortcut`/`configuredNativeShortcut` (upstream names keys like `.escape`, `.return`,
/// or writes a plain letter as a string literal, e.g. `"p"`).
public struct KeyEquivalent: Sendable, Equatable, ExpressibleByExtendedGraphemeClusterLiteral {
    public let character: Character
    public init(_ character: Character) { self.character = character }
    public init(extendedGraphemeClusterLiteral value: Character) { character = value }
    public static let escape = KeyEquivalent("\u{1b}"), `return` = KeyEquivalent("\r")
    public static let tab = KeyEquivalent("\t"), space = KeyEquivalent(" "), delete = KeyEquivalent("\u{8}")
    public static let upArrow = KeyEquivalent("\u{F700}"), downArrow = KeyEquivalent("\u{F701}")
    public static let leftArrow = KeyEquivalent("\u{F702}"), rightArrow = KeyEquivalent("\u{F703}")
    public static let cancelAction = KeyEquivalent("\u{1b}"), defaultAction = KeyEquivalent("\r")
}

/// A minimal `Shape` for the handful of custom shapes upstream draws (e.g. `HueArrow`); it only needs to exist as
/// a `View` (drawing happens through `Canvas`/`GraphicsContext`, already bridged via Compat/CoreGraphics).
public protocol Shape: PrimitiveView {
    func path(in rect: CGRect) -> Path
}
extension Shape {
    /// A bare shape drawn on its own (outside a `Canvas`) fills its path at whatever size the Qt renderer gives it.
    public func _makeNode(children: [RenderNode]) -> RenderNode { RenderNode(kind: "Shape") }
}

public struct Rectangle: Shape { public init() {}; public func path(in rect: CGRect) -> Path { Path(rect) } }
public struct Circle: Shape { public init() {}; public func path(in rect: CGRect) -> Path { Path(ellipseIn: rect) } }
public struct Angle: Sendable, Equatable {
    public var radians: Double
    public var degrees: Double { get { radians * 180 / .pi } set { radians = newValue * .pi / 180 } }
    public init(radians: Double = 0) { self.radians = radians }
    public static func degrees(_ value: Double) -> Angle { Angle(radians: value * .pi / 180) }
    public static func radians(_ value: Double) -> Angle { Angle(radians: value) }
    public static let zero = Angle(radians: 0)
}

public enum RoundedCornerStyle: Sendable { case circular, continuous }
public enum ColorRenderingSpace: Sendable { case sRGB, sRGBLinear, displayP3 }

public struct RoundedRectangle: Shape {
    public let cornerRadius: Double
    public let style: RoundedCornerStyle
    public init(cornerRadius: Double, style: RoundedCornerStyle = .circular) { self.cornerRadius = cornerRadius; self.style = style }
    public init(cornerSize: CGSize, style: RoundedCornerStyle = .circular) { cornerRadius = cornerSize.width; self.style = style }
    public func path(in rect: CGRect) -> Path { Path(roundedRect: rect, cornerRadius: cornerRadius, style: style) }
}
public struct Capsule: Shape { public init() {}; public func path(in rect: CGRect) -> Path { Path(rect) } }


extension Shape {
    /// Not yet stroke-only in the Qt renderer (it draws every `Shape` node filled) — compiles and shows the shape,
    /// just not as an outline yet.
    public func strokeBorder(_ style: ShapeStyle, lineWidth: Double = 1) -> some View { self }
    public func strokeBorder(_ style: ShapeStyle, style strokeStyle: StrokeStyle) -> some View { self }
    /// A filled shape tagged with its color; the Qt renderer reads `.foregroundStyle` off the node like any other.
    public func fill(_ style: ShapeStyle) -> some View { foregroundStyle(style) }
    /// `AngularGradient`/`LinearGradient` aren't `ShapeStyle` (a concrete token here, not a protocol); tag with
    /// the gradient's first color as a stand-in — the Qt renderer doesn't paint real gradients from this path yet.
    public func fill(_ gradient: AngularGradient) -> some View { foregroundStyle(gradient.gradient.colors.first ?? .clear) }
    /// Not yet drawn stroke-only (see `strokeBorder` above) — compiles and shows the shape filled.
    public func stroke(_ style: ShapeStyle, lineWidth: Double = 1) -> some View { foregroundStyle(style) }
    public func stroke(_ style: ShapeStyle, style strokeStyle: StrokeStyle) -> some View { foregroundStyle(style) }
    /// Not yet modeled geometrically — the Qt renderer draws every `Shape` node at its container's full size.
    public func inset(by amount: Double) -> Self { self }
}

/// Where a gesture's coordinates are reported in — `.local`/`.global`/`.named(_:)`. Only the identity matters here
/// (nothing computes real cross-widget coordinate transforms yet), so this is just a tag.
public struct CoordinateSpace: Sendable {
    let name: String?
    public static let local = CoordinateSpace(name: nil)
    public static let global = CoordinateSpace(name: "global")
    public static func named(_ name: some Hashable) -> CoordinateSpace { CoordinateSpace(name: "\(name)") }
}

/// A press-and-drag gesture. `.onChanged`/`.onEnded` register callbacks the Qt renderer can call once real mouse
/// tracking is wired to a widget (a later phase — see `.gesture(_:)`, currently a no-op `View` modifier); building
/// this value and chaining onto it must still compile and behave like real SwiftUI's fluent builder.
public struct DragGesture {
    public struct Value: Sendable { public var location: CGPoint = .zero; public var translation: CGSize = .zero }
    public let minimumDistance: Double
    public let coordinateSpace: CoordinateSpace
    var onChangedAction: ((Value) -> Void)?
    var onEndedAction: ((Value) -> Void)?
    public init(minimumDistance: Double = 10, coordinateSpace: CoordinateSpace = .local) {
        self.minimumDistance = minimumDistance; self.coordinateSpace = coordinateSpace
    }
    public func onChanged(_ action: @escaping (Value) -> Void) -> DragGesture {
        var copy = self; copy.onChangedAction = action; return copy
    }
    public func onEnded(_ action: @escaping (Value) -> Void) -> DragGesture {
        var copy = self; copy.onEndedAction = action; return copy
    }
}

/// A tap that also reports where it landed — structurally real, inert like `DragGesture` when handed to
/// `.simultaneousGesture` (which is itself inert; see Modifiers.swift).
public struct SpatialTapGesture {
    public struct Value: Sendable { public var location: CGPoint = .zero }
    public let count: Int
    var onEndedAction: ((Value) -> Void)?
    public init(count: Int = 1, coordinateSpace: CoordinateSpace = .local) { self.count = count }
    public func onEnded(_ action: @escaping (Value) -> Void) -> SpatialTapGesture {
        var copy = self; copy.onEndedAction = action; return copy
    }
}

/// Minimal `Gradient`/`AngularGradient` — enough for `AngularGradient(gradient:center:)`-style construction to
/// type-check and resolve as a tree node; the Qt renderer doesn't paint gradients from these yet.
public struct Gradient: Sendable {
    public var colors: [Color]
    public init(colors: [Color]) { self.colors = colors }
}

public struct AngularGradient: View, PrimitiveView {
    let gradient: Gradient
    let center: UnitPoint
    public init(gradient: Gradient, center: UnitPoint) { self.gradient = gradient; self.center = center }
    public init(colors: [Color], center: UnitPoint) { self.gradient = Gradient(colors: colors); self.center = center }
    public func _makeNode(children: [RenderNode]) -> RenderNode {
        var node = RenderNode(kind: "AngularGradient")
        node.stringParams["colors"] = gradient.colors.map(\.name).joined(separator: ",")
        return node
    }
}
