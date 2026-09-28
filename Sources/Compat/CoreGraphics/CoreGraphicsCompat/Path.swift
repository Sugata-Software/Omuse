// CoreGraphicsCompat/Path.swift — Apple-shaped CGPath / CGMutablePath.
//
// The Swift segment list is the source of truth, so geometry queries (bounds, hit-testing, transforms,
// enumeration) never depend on a GPU/Skia library. Skia (via the CompPath C ABI) is used only for what needs
// a real geometry engine: boolean operations (SkPathOps) and stroke outlines, and for painting/clipping.

import Foundation

public enum CGPathFillRule: Int, Sendable {
    case winding
    case evenOdd
}

public enum CGLineCap: Int32, Sendable { case butt, round, square }
public enum CGLineJoin: Int32, Sendable { case miter, round, bevel }

public enum CGPathElementType: Int32, Sendable {
    case moveToPoint, addLineToPoint, addQuadCurveToPoint, addCurveToPoint, closeSubpath
}

/// Same shape as Apple's `CGPathElement`: a type plus a pointer to its points (valid inside the callback).
public struct CGPathElement {
    public var type: CGPathElementType
    public var points: UnsafeMutablePointer<CGPoint>
    public init(type: CGPathElementType, points: UnsafeMutablePointer<CGPoint>) {
        self.type = type; self.points = points
    }
}

public enum PathSegment: Equatable {
    case move(CGPoint)
    case line(CGPoint)
    case quad(CGPoint, CGPoint)
    case cubic(CGPoint, CGPoint, CGPoint)
    case close
}

// MARK: - Skia path ABI (optional; resolved at runtime like CompCanvasBridge)

enum SkiaContextABI {
    typealias CreateEx = @convention(c) (UnsafeMutablePointer<UInt8>?, Int, Int, Int, Int32) -> OpaquePointer?
    typealias DrawImageEx = @convention(c) (OpaquePointer?, UnsafePointer<UInt8>?, Int, Int, Int, Int32, Float, Float, Float, Float, Float, Int32, Int32) -> Void
    static let createEx: CreateEx? = compatLookup("compositor_canvas_create_ex")
    static let drawImageEx: DrawImageEx? = compatLookup("compositor_canvas_draw_image_rect_ex")
}

enum SkiaPathABI {
    typealias Create = @convention(c) () -> OpaquePointer?
    typealias Destroy = @convention(c) (OpaquePointer?) -> Void
    typealias Pt = @convention(c) (OpaquePointer?, Float, Float) -> Void
    typealias Quad = @convention(c) (OpaquePointer?, Float, Float, Float, Float) -> Void
    typealias Cubic = @convention(c) (OpaquePointer?, Float, Float, Float, Float, Float, Float) -> Void
    typealias Close = @convention(c) (OpaquePointer?) -> Void
    typealias Op = @convention(c) (OpaquePointer?, OpaquePointer?, Int32, Int32, Int32) -> OpaquePointer?
    typealias Stroke = @convention(c) (OpaquePointer?, Float, Int32, Int32, Float) -> OpaquePointer?
    typealias Elements = @convention(c) (OpaquePointer?, UnsafeMutablePointer<UInt8>?, Int, UnsafeMutablePointer<Float>?, Int, UnsafeMutablePointer<Int>?) -> Int

    static let bridge = CompCanvasBridge.shared
    static let create: Create? = compatLookup("compositor_path_create")
    static let destroy: Destroy? = compatLookup("compositor_path_destroy")
    static let moveTo: Pt? = compatLookup("compositor_path_move_to")
    static let lineTo: Pt? = compatLookup("compositor_path_line_to")
    static let quadTo: Quad? = compatLookup("compositor_path_quad_to")
    static let cubicTo: Cubic? = compatLookup("compositor_path_cubic_to")
    static let close: Close? = compatLookup("compositor_path_close")
    static let op: Op? = compatLookup("compositor_path_op")
    static let stroke: Stroke? = compatLookup("compositor_path_stroke")
    static let elements: Elements? = compatLookup("compositor_path_elements")
    typealias FillPath = @convention(c) (OpaquePointer?, OpaquePointer?, Int32, Float, Float, Float, Float) -> Void
    typealias StrokePath = @convention(c) (OpaquePointer?, OpaquePointer?, Float, Int32, Int32, Float, Float, Float, Float, Float) -> Void
    typealias LinearGradient = @convention(c) (OpaquePointer?, Float, Float, Float, Float, UnsafePointer<Float>?, UnsafePointer<Float>?, Int, Int32) -> Void
    typealias RadialGradient = @convention(c) (OpaquePointer?, Float, Float, Float, Float, Float, Float, UnsafePointer<Float>?, UnsafePointer<Float>?, Int, Int32) -> Void
    static let fillPath: FillPath? = compatLookup("compositor_canvas_fill_path")
    static let strokePath: StrokePath? = compatLookup("compositor_canvas_stroke_path")
    static let linearGradient: LinearGradient? = compatLookup("compositor_canvas_draw_linear_gradient")
    static let radialGradient: RadialGradient? = compatLookup("compositor_canvas_draw_radial_gradient")
    static var available: Bool { create != nil && op != nil }

    /// Builds a Skia path from segments; the caller owns (and must destroy) the result.
    static func make(_ segments: [PathSegment]) -> OpaquePointer? {
        guard let create, let path = create() else { return nil }
        for segment in segments {
            switch segment {
            case .move(let p): moveTo?(path, Float(p.x), Float(p.y))
            case .line(let p): lineTo?(path, Float(p.x), Float(p.y))
            case .quad(let c, let p): quadTo?(path, Float(c.x), Float(c.y), Float(p.x), Float(p.y))
            case .cubic(let c1, let c2, let p):
                cubicTo?(path, Float(c1.x), Float(c1.y), Float(c2.x), Float(c2.y), Float(p.x), Float(p.y))
            case .close: close?(path)
            }
        }
        return path
    }

    /// Reads a Skia path back into segments (conics arrive as quads).
    static func read(_ path: OpaquePointer?) -> [PathSegment] {
        guard let path, let elements else { return [] }
        var floatCount = 0
        let count = elements(path, nil, 0, nil, 0, &floatCount)
        guard count > 0 else { return [] }
        var types = [UInt8](repeating: 0, count: count)
        var floats = [Float](repeating: 0, count: max(floatCount, 2))
        var written = 0
        _ = types.withUnsafeMutableBufferPointer { t in
            floats.withUnsafeMutableBufferPointer { f in
                elements(path, t.baseAddress, count, f.baseAddress, f.count, &written)
            }
        }
        var out: [PathSegment] = []
        var i = 0
        func pt() -> CGPoint { defer { i += 2 }; return CGPoint(x: CGFloat(floats[i]), y: CGFloat(floats[i + 1])) }
        for t in types {
            switch t {
            case 0: out.append(.move(pt()))
            case 1: out.append(.line(pt()))
            case 2: let c = pt(), p = pt(); out.append(.quad(c, p))
            case 3: let c1 = pt(), c2 = pt(), p = pt(); out.append(.cubic(c1, c2, p))
            default: out.append(.close)
            }
        }
        return out
    }
}

@inline(__always) func compatLookup<T>(_ name: String) -> T? {
    #if canImport(Glibc)
    _ = CompCanvasBridge.shared   // makes sure the bridge library has been dlopen'ed first
    guard let sym = dlsym(nil, name) else { return nil }
    return unsafeBitCast(sym, to: T.self)
    #else
    return nil
    #endif
}

// MARK: - CGPath

public class CGPath: @unchecked Sendable, Hashable {
    public var segments: [PathSegment]

    public init(segments: [PathSegment]) { self.segments = segments }

    public init() { self.segments = [] }

    public init(rect: CGRect, transform: UnsafePointer<CGAffineTransform>? = nil) {
        segments = []
        Self.appendRect(rect, to: &segments)
        if let t = transform?.pointee, t != .identity { segments = Self.transformed(segments, t) }
    }

    public init(ellipseIn rect: CGRect, transform: UnsafePointer<CGAffineTransform>? = nil) {
        segments = []
        Self.appendEllipse(rect, to: &segments)
        if let t = transform?.pointee, t != .identity { segments = Self.transformed(segments, t) }
    }

    public init(roundedRect rect: CGRect, cornerWidth: CGFloat, cornerHeight: CGFloat,
                transform: UnsafePointer<CGAffineTransform>? = nil) {
        segments = []
        Self.appendRoundedRect(rect, cornerWidth: cornerWidth, cornerHeight: cornerHeight, to: &segments)
        if let t = transform?.pointee, t != .identity { segments = Self.transformed(segments, t) }
    }

    // Value equality of the outline, as Apple's `CFEqual` on paths.
    public static func == (lhs: CGPath, rhs: CGPath) -> Bool { lhs.segments == rhs.segments }
    public func hash(into hasher: inout Hasher) {
        for s in segments {
            switch s {
            case .move(let p): hasher.combine(0); hasher.combine(p.x); hasher.combine(p.y)
            case .line(let p): hasher.combine(1); hasher.combine(p.x); hasher.combine(p.y)
            case .quad(let c, let p): hasher.combine(2); hasher.combine(c.x); hasher.combine(c.y); hasher.combine(p.x); hasher.combine(p.y)
            case .cubic(let a, let b, let p): hasher.combine(3); hasher.combine(a.x); hasher.combine(a.y); hasher.combine(b.x); hasher.combine(b.y); hasher.combine(p.x); hasher.combine(p.y)
            case .close: hasher.combine(4)
            }
        }
    }

    // MARK: Queries

    public var isEmpty: Bool { segments.isEmpty }

    /// The current point: the last point added, or the subpath start after a close. Nil for an empty path.
    public var currentPoint: CGPoint {
        var start = CGPoint.zero, current = CGPoint.zero
        for s in segments {
            switch s {
            case .move(let p): start = p; current = p
            case .line(let p): current = p
            case .quad(_, let p): current = p
            case .cubic(_, _, let p): current = p
            case .close: current = start
            }
        }
        return current
    }

    /// Bounds of all points, control points included (Apple's `boundingBox`). `.null` when empty.
    public var boundingBox: CGRect {
        var pts: [CGPoint] = []
        for s in segments {
            switch s {
            case .move(let p), .line(let p): pts.append(p)
            case .quad(let c, let p): pts += [c, p]
            case .cubic(let c1, let c2, let p): pts += [c1, c2, p]
            case .close: break
            }
        }
        return Self.bounds(of: pts)
    }

    /// Tight bounds of the drawn outline (curve extrema, not control points). `.null` when empty.
    public var boundingBoxOfPath: CGRect {
        Self.bounds(of: flattenedSubpaths().flatMap { $0.points })
    }

    /// Fill hit-test with the given rule.
    public func contains(_ point: CGPoint, using rule: CGPathFillRule = .winding,
                         transform: CGAffineTransform = .identity) -> Bool {
        let p = transform == .identity ? point : point.applying(transform.inverted())
        var winding = 0
        for sub in flattenedSubpaths() {
            let pts = sub.points
            guard pts.count >= 2 else { continue }
            var previous = pts[pts.count - 1]
            for current in pts {
                if (current.y > p.y) != (previous.y > p.y) {
                    let x = (previous.x - current.x) * (p.y - current.y) / (previous.y - current.y) + current.x
                    if p.x < x { winding += current.y > previous.y ? 1 : -1 }
                }
                previous = current
            }
        }
        return rule == .evenOdd ? (winding & 1) != 0 : winding != 0
    }

    /// Polylines (curves flattened) with each subpath implicitly closed for filling.
    func flattenedSubpaths(curveSteps: Int = 24) -> [(points: [CGPoint], closed: Bool)] {
        var result: [(points: [CGPoint], closed: Bool)] = []
        var current: [CGPoint] = []
        var start = CGPoint.zero
        func flush(closed: Bool) { if !current.isEmpty { result.append((current, closed)) }; current = [] }
        for s in segments {
            switch s {
            case .move(let p): flush(closed: false); current = [p]; start = p
            case .line(let p): if current.isEmpty { current = [start] }; current.append(p)
            case .quad(let c, let p):
                if current.isEmpty { current = [start] }
                let a = current.last!
                for i in 1...curveSteps {
                    let t = CGFloat(i) / CGFloat(curveSteps), u = 1 - t
                    current.append(CGPoint(x: u * u * a.x + 2 * u * t * c.x + t * t * p.x,
                                           y: u * u * a.y + 2 * u * t * c.y + t * t * p.y))
                }
            case .cubic(let c1, let c2, let p):
                if current.isEmpty { current = [start] }
                let a = current.last!
                for i in 1...curveSteps {
                    let t = CGFloat(i) / CGFloat(curveSteps), u = 1 - t
                    current.append(CGPoint(
                        x: u * u * u * a.x + 3 * u * u * t * c1.x + 3 * u * t * t * c2.x + t * t * t * p.x,
                        y: u * u * u * a.y + 3 * u * u * t * c1.y + 3 * u * t * t * c2.y + t * t * t * p.y))
                }
            case .close: flush(closed: true); current = []
            }
        }
        flush(closed: false)
        return result
    }

    // MARK: Copies and transforms

    public func copy() -> CGPath { CGPath(segments: segments) }
    public func mutableCopy() -> CGMutablePath { CGMutablePath(segments: segments) }

    public func copy(using transform: UnsafePointer<CGAffineTransform>?) -> CGPath? {
        guard let t = transform?.pointee else { return copy() }
        return CGPath(segments: Self.transformed(segments, t))
    }

    public func mutableCopy(using transform: UnsafePointer<CGAffineTransform>?) -> CGMutablePath? {
        guard let t = transform?.pointee else { return mutableCopy() }
        return CGMutablePath(segments: Self.transformed(segments, t))
    }

    /// Enumerates the path in Apple's element form; the `points` pointer is only valid inside the block.
    public func applyWithBlock(_ block: (UnsafePointer<CGPathElement>) -> Void) {
        var buffer = [CGPoint](repeating: .zero, count: 3)
        buffer.withUnsafeMutableBufferPointer { buf in
            let base = buf.baseAddress!
            for s in segments {
                let type: CGPathElementType
                switch s {
                case .move(let p): base[0] = p; type = .moveToPoint
                case .line(let p): base[0] = p; type = .addLineToPoint
                case .quad(let c, let p): base[0] = c; base[1] = p; type = .addQuadCurveToPoint
                case .cubic(let c1, let c2, let p): base[0] = c1; base[1] = c2; base[2] = p; type = .addCurveToPoint
                case .close: type = .closeSubpath
                }
                var element = CGPathElement(type: type, points: base)
                withUnsafePointer(to: &element) { block($0) }
            }
        }
    }

    // MARK: Boolean operations and strokes (Skia PathOps / SkStroke)

    public func union(_ other: CGPath, using rule: CGPathFillRule = .winding) -> CGPath { boolean(other, op: 2, rule) }
    public func intersection(_ other: CGPath, using rule: CGPathFillRule = .winding) -> CGPath { boolean(other, op: 1, rule) }
    public func subtracting(_ other: CGPath, using rule: CGPathFillRule = .winding) -> CGPath { boolean(other, op: 0, rule) }
    /// (A - B) + (B - A): built from two exact operations because Skia's XOR result relies on even-odd nesting,
    /// which this fill-rule-less path model cannot carry.
    public func symmetricDifference(_ other: CGPath, using rule: CGPathFillRule = .winding) -> CGPath {
        subtracting(other, using: rule).union(other.subtracting(self, using: rule), using: rule)
    }

    private func boolean(_ other: CGPath, op: Int32, _ rule: CGPathFillRule) -> CGPath {
        guard SkiaPathABI.available, let opFn = SkiaPathABI.op,
              let a = SkiaPathABI.make(segments), let b = SkiaPathABI.make(other.segments) else {
            // Without the geometry engine only conservative answers exist: a union keeps every contour (exact
            // under the winding rule), a difference keeps the minuend, an intersection is empty.
            switch op {
            case 2: return CGPath(segments: segments + other.segments)
            case 0: return copy()
            default: return CGPath()
            }
        }
        defer { SkiaPathABI.destroy?(a); SkiaPathABI.destroy?(b) }
        let even: Int32 = rule == .evenOdd ? 1 : 0
        guard let out = opFn(a, b, op, even, even) else { return CGPath() }
        defer { SkiaPathABI.destroy?(out) }
        return CGPath(segments: SkiaPathABI.read(out))
    }

    /// The outline of this path stroked with the given width, as a fillable path.
    public func copy(strokingWithWidth width: CGFloat, lineCap: CGLineCap, lineJoin: CGLineJoin,
                     miterLimit: CGFloat, transform: CGAffineTransform = .identity) -> CGPath {
        guard let strokeFn = SkiaPathABI.stroke, let raw = SkiaPathABI.make(segments) else { return CGPath() }
        defer { SkiaPathABI.destroy?(raw) }
        guard let out = strokeFn(raw, Float(width), lineCap.rawValue, lineJoin.rawValue, Float(miterLimit)) else { return CGPath() }
        defer { SkiaPathABI.destroy?(out) }
        let stroked = CGPath(segments: SkiaPathABI.read(out))
        return transform == .identity ? stroked : CGPath(segments: Self.transformed(stroked.segments, transform))
    }

    /// Union of the path with itself under the winding rule: resolves overlaps into non-overlapping contours.
    public func normalized(using rule: CGPathFillRule = .winding) -> CGPath { union(CGPath(), using: rule) }

    // MARK: Helpers

    static func bounds(of points: [CGPoint]) -> CGRect {
        guard let first = points.first else { return .null }
        var minX = first.x, minY = first.y, maxX = first.x, maxY = first.y
        for p in points.dropFirst() {
            minX = min(minX, p.x); minY = min(minY, p.y); maxX = max(maxX, p.x); maxY = max(maxY, p.y)
        }
        return CGRect(x: minX, y: minY, width: maxX - minX, height: maxY - minY)
    }

    static func transformed(_ segments: [PathSegment], _ t: CGAffineTransform) -> [PathSegment] {
        segments.map { s in
            switch s {
            case .move(let p): return .move(p.applying(t))
            case .line(let p): return .line(p.applying(t))
            case .quad(let c, let p): return .quad(c.applying(t), p.applying(t))
            case .cubic(let c1, let c2, let p): return .cubic(c1.applying(t), c2.applying(t), p.applying(t))
            case .close: return .close
            }
        }
    }

    static func appendRect(_ r: CGRect, to s: inout [PathSegment]) {
        s.append(.move(CGPoint(x: r.minX, y: r.minY)))
        s.append(.line(CGPoint(x: r.maxX, y: r.minY)))
        s.append(.line(CGPoint(x: r.maxX, y: r.maxY)))
        s.append(.line(CGPoint(x: r.minX, y: r.maxY)))
        s.append(.close)
    }

    /// Four cubic Béziers (kappa = 0.5522847498), starting at the rightmost point.
    static func appendEllipse(_ r: CGRect, to s: inout [PathSegment]) {
        let k: CGFloat = 0.5522847498307936
        let cx = r.midX, cy = r.midY, rx = r.width / 2, ry = r.height / 2
        s.append(.move(CGPoint(x: cx + rx, y: cy)))
        s.append(.cubic(CGPoint(x: cx + rx, y: cy + k * ry), CGPoint(x: cx + k * rx, y: cy + ry), CGPoint(x: cx, y: cy + ry)))
        s.append(.cubic(CGPoint(x: cx - k * rx, y: cy + ry), CGPoint(x: cx - rx, y: cy + k * ry), CGPoint(x: cx - rx, y: cy)))
        s.append(.cubic(CGPoint(x: cx - rx, y: cy - k * ry), CGPoint(x: cx - k * rx, y: cy - ry), CGPoint(x: cx, y: cy - ry)))
        s.append(.cubic(CGPoint(x: cx + k * rx, y: cy - ry), CGPoint(x: cx + rx, y: cy - k * ry), CGPoint(x: cx + rx, y: cy)))
        s.append(.close)
    }

    static func appendRoundedRect(_ r: CGRect, cornerWidth: CGFloat, cornerHeight: CGFloat, to s: inout [PathSegment]) {
        let cw = min(max(0, cornerWidth), r.width / 2), ch = min(max(0, cornerHeight), r.height / 2)
        guard cw > 0, ch > 0 else { appendRect(r, to: &s); return }
        let k: CGFloat = 0.5522847498307936
        let l = r.minX, rt = r.maxX, t = r.minY, b = r.maxY
        s.append(.move(CGPoint(x: l + cw, y: t)))
        s.append(.line(CGPoint(x: rt - cw, y: t)))
        s.append(.cubic(CGPoint(x: rt - cw + k * cw, y: t), CGPoint(x: rt, y: t + ch - k * ch), CGPoint(x: rt, y: t + ch)))
        s.append(.line(CGPoint(x: rt, y: b - ch)))
        s.append(.cubic(CGPoint(x: rt, y: b - ch + k * ch), CGPoint(x: rt - cw + k * cw, y: b), CGPoint(x: rt - cw, y: b)))
        s.append(.line(CGPoint(x: l + cw, y: b)))
        s.append(.cubic(CGPoint(x: l + cw - k * cw, y: b), CGPoint(x: l, y: b - ch + k * ch), CGPoint(x: l, y: b - ch)))
        s.append(.line(CGPoint(x: l, y: t + ch)))
        s.append(.cubic(CGPoint(x: l, y: t + ch - k * ch), CGPoint(x: l + cw - k * cw, y: t), CGPoint(x: l + cw, y: t)))
        s.append(.close)
    }
}

// MARK: - CGMutablePath

public final class CGMutablePath: CGPath, @unchecked Sendable {
    public override init() { super.init(segments: []) }
    public override init(segments: [PathSegment]) { super.init(segments: segments) }

    public func move(to point: CGPoint, transform: CGAffineTransform = .identity) {
        segments.append(.move(point.applying(transform)))
    }

    public func addLine(to point: CGPoint, transform: CGAffineTransform = .identity) {
        segments.append(.line(point.applying(transform)))
    }

    public func addLines(between points: [CGPoint], transform: CGAffineTransform = .identity) {
        guard let first = points.first else { return }
        move(to: first, transform: transform)
        for p in points.dropFirst() { addLine(to: p, transform: transform) }
    }

    public func addQuadCurve(to end: CGPoint, control: CGPoint, transform: CGAffineTransform = .identity) {
        segments.append(.quad(control.applying(transform), end.applying(transform)))
    }

    public func addCurve(to end: CGPoint, control1: CGPoint, control2: CGPoint, transform: CGAffineTransform = .identity) {
        segments.append(.cubic(control1.applying(transform), control2.applying(transform), end.applying(transform)))
    }

    public func addRect(_ rect: CGRect, transform: CGAffineTransform = .identity) {
        var s: [PathSegment] = []
        CGPath.appendRect(rect, to: &s)
        segments += transform == .identity ? s : CGPath.transformed(s, transform)
    }

    public func addRects(_ rects: [CGRect], transform: CGAffineTransform = .identity) {
        for r in rects { addRect(r, transform: transform) }
    }

    public func addEllipse(in rect: CGRect, transform: CGAffineTransform = .identity) {
        var s: [PathSegment] = []
        CGPath.appendEllipse(rect, to: &s)
        segments += transform == .identity ? s : CGPath.transformed(s, transform)
    }

    public func addRoundedRect(in rect: CGRect, cornerWidth: CGFloat, cornerHeight: CGFloat,
                               transform: CGAffineTransform = .identity) {
        var s: [PathSegment] = []
        CGPath.appendRoundedRect(rect, cornerWidth: cornerWidth, cornerHeight: cornerHeight, to: &s)
        segments += transform == .identity ? s : CGPath.transformed(s, transform)
    }

    public func addPath(_ path: CGPath, transform: CGAffineTransform = .identity) {
        segments += transform == .identity ? path.segments : CGPath.transformed(path.segments, transform)
    }

    public func closeSubpath() { segments.append(.close) }
}
