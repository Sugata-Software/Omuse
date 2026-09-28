// `Path` and `GraphicsContext`: thin SwiftUI-shaped wrappers over the existing CGContext/CGMutablePath compat
// bridge (Sources/Compat/CoreGraphics/CoreGraphicsCompat) so upstream's `Canvas { context, size in ... }` drawing
// code (histograms, curve graphs, rulers) runs against the real Skia-backed context unmodified.

import CompatSupport

public struct Path: Shape {
    public var cgPath: CGMutablePath
    public init() { cgPath = CGMutablePath() }
    public init(_ cgPath: CGPath) { self.cgPath = CGMutablePath(); self.cgPath.addPath(cgPath) }
    public init(ellipseIn rect: CGRect) { cgPath = CGMutablePath(); cgPath.addEllipse(in: rect) }
    public init(_ rect: CGRect) { cgPath = CGMutablePath(); cgPath.addRect(rect) }
    public init(roundedRect rect: CGRect, cornerRadius: Double, style: RoundedCornerStyle = .circular) {
        cgPath = CGMutablePath(); cgPath.addRect(rect) // exact rounding not modeled; the Qt renderer approximates it.
    }
    public init(roundedRect rect: CGRect, cornerSize: CGSize, style: RoundedCornerStyle = .circular) {
        cgPath = CGMutablePath(); cgPath.addRect(rect)
    }
    public init(_ callback: (inout Path) -> Void) {
        cgPath = CGMutablePath()
        var path = Path()
        callback(&path)
        cgPath = path.cgPath
    }
    public func path(in rect: CGRect) -> Path { self }
    public mutating func move(to point: CGPoint) { cgPath.move(to: point) }
    public mutating func addLine(to point: CGPoint) { cgPath.addLine(to: point) }
    public mutating func addLines(_ points: [CGPoint]) {
        guard let first = points.first else { return }
        cgPath.move(to: first)
        for point in points.dropFirst() { cgPath.addLine(to: point) }
    }
    public mutating func addRect(_ rect: CGRect) { cgPath.addRect(rect) }
    public mutating func addRoundedRect(in rect: CGRect, cornerSize: CGSize, style: RoundedCornerStyle = .circular) {
        cgPath.addRect(rect) // exact rounding not modeled; the Qt renderer approximates it.
    }
    public mutating func addEllipse(in rect: CGRect) { cgPath.addEllipse(in: rect) }
    public mutating func addCurve(to end: CGPoint, control1: CGPoint, control2: CGPoint) {
        cgPath.addCurve(to: end, control1: control1, control2: control2)
    }
    public mutating func addQuadCurve(to end: CGPoint, control: CGPoint) { cgPath.addQuadCurve(to: end, control: control) }
    public mutating func closeSubpath() { cgPath.closeSubpath() }
}

/// A resolved fill/stroke color or gradient token for `GraphicsContext` drawing calls; the Skia-backed `CGContext`
/// only needs a solid `CGColor` today, matching every current `Compositor/UI` use (gradients render through the
/// existing pixel pipeline, not `Canvas`).
public struct GraphicsContextShading: Sendable {
    let color: CGColor
    public static func color(_ color: Color) -> GraphicsContextShading {
        GraphicsContextShading(color: CGColor.named(color.name))
    }
    /// The current foreground style — not tracked separately from an explicit color yet, so this just resolves to
    /// `.primary`, matching the common case (light text/strokes on a dark canvas background).
    public static let foreground = GraphicsContextShading(color: CGColor.named("primary"))
}

public struct GraphicsContext {
    public let cgContext: CGContext
    public init(cgContext: CGContext) { self.cgContext = cgContext }

    public func fill(_ path: Path, with shading: GraphicsContextShading) {
        cgContext.setFillColor(shading.color)
        cgContext.addPath(path.cgPath)
        cgContext.drawPath(using: .fill)
    }
    public func stroke(_ path: Path, with shading: GraphicsContextShading, lineWidth: CGFloat = 1) {
        cgContext.setStrokeColor(shading.color)
        cgContext.setLineWidth(lineWidth)
        cgContext.addPath(path.cgPath)
        cgContext.drawPath(using: .stroke)
    }
    public func stroke(_ path: Path, with shading: GraphicsContextShading, style: StrokeStyle) {
        cgContext.setStrokeColor(shading.color)
        cgContext.setLineWidth(style.lineWidth)
        cgContext.addPath(path.cgPath)
        cgContext.drawPath(using: .stroke)
    }
    public func clip(to path: Path, style: FillStyle = FillStyle()) {
        cgContext.beginPath()
        cgContext.addPath(path.cgPath)
        cgContext.clip(using: style.isEOFilled ? .evenOdd : .winding)
    }
    public func fill(_ path: Path, with shading: GraphicsContextShading, style: FillStyle) {
        cgContext.setFillColor(shading.color)
        cgContext.addPath(path.cgPath)
        cgContext.drawPath(using: style.isEOFilled ? .eoFill : .fill)
    }
}

public struct StrokeStyle: Sendable {
    public var lineWidth: CGFloat
    public var lineCap: CGLineCap, lineJoin: CGLineJoin
    public var miterLimit: CGFloat
    public var dash: [CGFloat]
    public var dashPhase: CGFloat
    public init(lineWidth: CGFloat = 1, lineCap: CGLineCap = .butt, lineJoin: CGLineJoin = .miter,
                miterLimit: CGFloat = 10, dash: [CGFloat] = [], dashPhase: CGFloat = 0) {
        self.lineWidth = lineWidth; self.lineCap = lineCap; self.lineJoin = lineJoin
        self.miterLimit = miterLimit; self.dash = dash; self.dashPhase = dashPhase
    }
}

public struct FillStyle: Sendable {
    public var isEOFilled: Bool
    public init(eoFill: Bool = false, antialiased: Bool = true) { isEOFilled = eoFill }
}

/// Resolves a `StyleToken` name (`.secondary`, `.orange`, ...) to a concrete color; the Qt host installs the real
/// palette lookup via `install(_:)`, matching every other compat-module seam (see ServiceSlot.swift). Falls back to
/// mid-gray so headless drawing/tests never crash on an unresolved token.
public let colorTokenResolver = ServiceSlot<(String) -> CGColor>()

extension CGColor {
    static func named(_ name: String) -> CGColor {
        colorTokenResolver.current?(name) ?? CGColor(gray: 0.5, alpha: 1)
    }
}

extension NSColor {
    /// `NSColor(someSwiftUIColor)` — a real, common conversion upstream code does directly.
    public convenience init(_ color: Color) {
        let cg = CGColor.named(color.name)
        self.init(srgbRed: cg.red, green: cg.green, blue: cg.blue, alpha: cg.alpha)
    }
}
