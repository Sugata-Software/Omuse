import Foundation
import CoreGraphics

/// Core Animation transactions: layers do not exist here, so these only balance begin/commit.
public final class CATransaction {
    nonisolated(unsafe) private static var depth = 0
    nonisolated(unsafe) public static var disablesActions = false
    public static func begin() { depth += 1 }
    public static func commit() { depth = max(0, depth - 1) }
    public static func setDisableActions(_ flag: Bool) { disablesActions = flag }
    public static func flush() {}
}

/// A drawable outline that renders through the current `NSGraphicsContext`.
public final class NSBezierPath {
    public enum LineCapStyle: UInt { case butt, round, square }
    public enum LineJoinStyle: UInt { case miter, round, bevel }
    public enum WindingRule: UInt { case nonZero, evenOdd }
    public let path = CGMutablePath()
    public var lineWidth: CGFloat = 1
    public var lineCapStyle: LineCapStyle = .butt
    public var lineJoinStyle: LineJoinStyle = .miter
    public var windingRule: WindingRule = .nonZero
    private var dash: [CGFloat] = [], phase: CGFloat = 0

    public init() {}
    public convenience init(rect: CGRect) { self.init(); appendRect(rect) }
    public convenience init(ovalIn rect: CGRect) { self.init(); appendOval(in: rect) }
    public convenience init(roundedRect rect: CGRect, xRadius: CGFloat, yRadius: CGFloat) {
        self.init(); path.addRoundedRect(in: rect, cornerWidth: xRadius, cornerHeight: yRadius)
    }
    public var cgPath: CGPath { path }
    public var isEmpty: Bool { path.isEmpty }
    public var bounds: CGRect { path.boundingBoxOfPath.isNull ? .zero : path.boundingBoxOfPath }
    public var currentPoint: CGPoint { path.currentPoint }

    public func move(to p: CGPoint) { path.move(to: p) }
    public func line(to p: CGPoint) { path.addLine(to: p) }
    public func curve(to end: CGPoint, controlPoint1 c1: CGPoint, controlPoint2 c2: CGPoint) { path.addCurve(to: end, control1: c1, control2: c2) }
    public func close() { path.closeSubpath() }
    public func appendRect(_ rect: CGRect) { path.addRect(rect) }
    public func appendOval(in rect: CGRect) { path.addEllipse(in: rect) }
    public func append(_ other: NSBezierPath) { path.addPath(other.path) }
    public func copy() -> Any { let c = NSBezierPath(); c.path.segments = path.segments; c.lineWidth = lineWidth; c.lineCapStyle = lineCapStyle; c.lineJoinStyle = lineJoinStyle; c.windingRule = windingRule; return c }
    public func removeAllPoints() { path.segments.removeAll() }
    public func setLineDash(_ pattern: [CGFloat]?, count: Int, phase: CGFloat) { dash = pattern.map { Array($0.prefix(count)) } ?? []; self.phase = phase }
    public func transform(using t: AffineTransform) {
        var ct = CGAffineTransform(a: t.m11, b: t.m12, c: t.m21, d: t.m22, tx: t.tX, ty: t.tY)
        if let moved = path.copy(using: &ct) { path.segments = moved.segments }
    }
    public func contains(_ point: CGPoint) -> Bool { path.contains(point, using: windingRule == .evenOdd ? .evenOdd : .winding) }

    public func fill() {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.fill(path, rule: windingRule == .evenOdd ? .evenOdd : .winding)
    }
    public func stroke() {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.saveGState()
        ctx.setLineWidth(lineWidth)
        ctx.setLineCap(CGLineCap(rawValue: Int32(lineCapStyle.rawValue)) ?? .butt)
        ctx.setLineJoin(CGLineJoin(rawValue: Int32(lineJoinStyle.rawValue)) ?? .miter)
        if !dash.isEmpty { ctx.setLineDash(phase: phase, lengths: dash) }
        ctx.stroke(path)
        ctx.restoreGState()
    }
    public func addClip() { NSGraphicsContext.current?.cgContext.addPath(path); NSGraphicsContext.current?.cgContext.clip(using: windingRule == .evenOdd ? .evenOdd : .winding) }
}

public final class NSShadow {
    public var shadowOffset = CGSize.zero
    public var shadowBlurRadius: CGFloat = 0
    public var shadowColor: NSColor?
    public init() {}
    public func set() {
        NSGraphicsContext.current?.cgContext.setShadow(offset: shadowOffset, blur: shadowBlurRadius, color: shadowColor?.cgColor)
    }
}

extension CGRect {
    /// `NSRect.fill()` — fills with the current graphics context's fill colour.
    public func fill() { NSGraphicsContext.current?.cgContext.fill(self) }
    public func frame(withWidth width: CGFloat) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.setLineWidth(width); ctx.stroke(self)
    }
}
public func NSRectFill(_ rect: CGRect) { rect.fill() }

extension NSImage {
    public func draw(in rect: CGRect) {
        guard let cg = cgImage(forProposedRect: nil, context: nil, hints: nil) else { return }
        NSGraphicsContext.current?.cgContext.draw(cg, in: rect)
    }
    public func draw(in rect: CGRect, from source: CGRect, operation: NSCompositingOperation, fraction: CGFloat) {
        guard let cg = cgImage(forProposedRect: nil, context: nil, hints: nil) else { return }
        NSGraphicsContext.current?.cgContext.draw(cg, in: rect, opacity: Double(fraction))
    }
    public func withSymbolConfiguration(_ configuration: SymbolConfiguration) -> NSImage? { self }
}
public enum NSCompositingOperation: UInt { case clear, copy, sourceOver = 2 }

extension NSBitmapImageRep {
    /// The colour at a pixel (top-left origin), converted to straight sRGB.
    public func colorAt(x: Int, y: Int) -> NSColor? { pixelColor(x: x, y: y) }
}
