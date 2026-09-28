// CoreGraphicsCompat/Context.swift — CGContext shim over the Skia render-device bridge.
//
// Plan §3, §4, §5: CoreGraphics CGContext compatibility class.
// Draws through a `CanvasBackend` (Skia by default; see CanvasBackend.swift), with pure-Swift fallbacks.
// If the Skia bridge is unavailable or fails, falls back gracefully to pure-Swift rendering.

import Foundation

public final class CGContext: @unchecked Sendable {
    /// Backing pixel layout. Contexts made with Apple's `CGContext(data:...)` initialiser follow the colour space:
    /// a gray space gives a one-byte-per-pixel plane (what upstream's masks and C kernels expect).
    public enum PixelFormat: Sendable { case rgba, gray }

    public let width: Int
    public let height: Int
    public let format: PixelFormat
    public let bytesPerRow: Int
    /// Apple bitmap contexts have their origin at the bottom-left (y up); the compat's own initialisers keep the
    /// top-left origin the Linux core was written against.
    public let isYUp: Bool

    private let pixelData: UnsafeMutablePointer<UInt8>
    private let pixelCount: Int
    /// The rasterizer drawing into `pixelData`; nil when no backend can serve this context (Swift fallbacks then run).
    private var canvas: CanvasBackend?
    private let render: CompRenderFn?
    private var externalData: UnsafeMutableRawPointer?
    private var externalBytesPerRow: Int = 0

    private struct State {
        var alpha: CGFloat = 1.0
        var blendMode: CGBlendMode = .normal
        var interpolationQuality: CGInterpolationQuality = .default
        var shouldAntialias: Bool = true
        var fillColor: CGColor = .black
        var strokeColor: CGColor = .black
        var lineWidth: CGFloat = 1.0
        var lineCap: CGLineCap = .butt
        var lineJoin: CGLineJoin = .miter
        var miterLimit: CGFloat = 10
        var dashPhase: CGFloat = 0
        var shadowOffset = CGSize.zero
        var shadowBlur: CGFloat = 0
        var dashLengths: [CGFloat] = []
        var ctm: CGAffineTransform = .identity
    }

    private var state: State = State()
    private var stateStack: [State] = []
    private var currentPath: CGMutablePath = CGMutablePath()

    public var data: UnsafeMutableRawPointer? {
        syncToExternalData()
        return externalData ?? UnsafeMutableRawPointer(pixelData)
    }

    public var buffer: PixelBuffer {
        if format == .gray {
            // Legacy RGBA view of a gray plane (opaque, r = g = b).
            var expanded = [UInt8](repeating: 255, count: width * height * 4)
            for i in 0..<(width * height) { let v = pixelData[i]; expanded[i * 4] = v; expanded[i * 4 + 1] = v; expanded[i * 4 + 2] = v }
            return PixelBuffer(width: width, height: height, bytes: expanded)
        }
        let array = Array<UInt8>(unsafeUninitializedCapacity: pixelCount) { buf, initializedCount in
            memcpy(buf.baseAddress!, pixelData, pixelCount)
            initializedCount = pixelCount
        }
        return PixelBuffer(width: width, height: height, bytes: array)
    }

    public convenience init(width: Int, height: Int, render: CompRenderFn? = nil) {
        self.init(width: width, height: height, format: .rgba, yUp: false, render: render)
    }

    init(width: Int, height: Int, format: PixelFormat, yUp: Bool, render: CompRenderFn? = nil) {
        precondition(width > 0 && height > 0)
        self.width = width
        self.height = height
        self.format = format
        self.isYUp = yUp
        self.bytesPerRow = width * (format == .gray ? 1 : 4)
        self.pixelCount = bytesPerRow * height
        self.pixelData = UnsafeMutablePointer<UInt8>.allocate(capacity: pixelCount)
        self.pixelData.initialize(repeating: 0, count: pixelCount)
        self.render = render ?? compositor_compat_current_render_fn()
        makeCanvas()
    }

    private func makeCanvas() {
        canvas = CanvasBackends.make(pixels: pixelData, width: width, height: height, bytesPerRow: bytesPerRow, format: format)
        // The flip lives below the visible CTM: user space is y-up, memory rows still run top-down.
        if isYUp {
            canvas?.translate(0, Float(height))
            canvas?.scale(1, -1)
        }
    }

    /// Apple's bitmap-context initialiser. The colour space picks the pixel layout (gray -> one byte per pixel,
    /// otherwise RGBA) and the origin is bottom-left, as in CoreGraphics.
    public convenience init?(data: UnsafeMutableRawPointer?,
                             width: Int,
                             height: Int,
                             bitsPerComponent: Int,
                             bytesPerRow: Int,
                             space: CGColorSpace,
                             bitmapInfo: UInt32) {
        guard width > 0, height > 0, bitsPerComponent == 8 else { return nil }
        let format: PixelFormat = space.model == .monochrome ? .gray : .rgba
        self.init(width: width, height: height, format: format, yUp: true)
        if let srcData = data {
            self.externalData = srcData
            self.externalBytesPerRow = bytesPerRow
            let rowLength = self.bytesPerRow
            for y in 0..<height { memcpy(pixelData.advanced(by: y * rowLength), srcData.advanced(by: y * bytesPerRow), min(rowLength, bytesPerRow)) }
        }
    }

    public init(buffer: PixelBuffer, render: CompRenderFn? = nil) {
        precondition(buffer.width > 0 && buffer.height > 0)
        let w = buffer.width
        let h = buffer.height
        let count = w * h * 4
        self.width = w
        self.height = h
        self.format = .rgba
        self.isYUp = false
        self.bytesPerRow = w * 4
        self.pixelCount = count
        self.render = render ?? compositor_compat_current_render_fn()

        let ptr = UnsafeMutablePointer<UInt8>.allocate(capacity: count)
        _ = buffer.bytes.withUnsafeBufferPointer { srcPtr in
            memcpy(ptr, srcPtr.baseAddress!, min(count, srcPtr.count))
        }
        self.pixelData = ptr

        self.canvas = CanvasBackends.make(pixels: ptr, width: w, height: h, bytesPerRow: w * 4, format: .rgba)
    }

    deinit {
        canvas = nil   // the backend must stop drawing before its pixel memory goes
        syncToExternalData()
        pixelData.deallocate()
    }

    public func flush() {
        syncToExternalData()
    }

    public func syncToExternalData() {
        guard let dstData = externalData else { return }
        let rowLength = self.bytesPerRow
        for y in 0..<height {
            memcpy(dstData.advanced(by: y * externalBytesPerRow), pixelData.advanced(by: y * rowLength), min(rowLength, externalBytesPerRow))
        }
    }

    // MARK: - State Management

    public func saveGState() {
        stateStack.append(state)
        canvas?.save()
    }

    public func restoreGState() {
        guard let prev = stateStack.popLast() else { return }
        state = prev
        canvas?.restore()
    }

    public func setAlpha(_ alpha: CGFloat) {
        state.alpha = alpha
        canvas?.setAlpha(Float(alpha))
    }

    public func setBlendMode(_ mode: CGBlendMode) {
        state.blendMode = mode
        canvas?.setBlendMode(mode.rawValue)
    }

    public var interpolationQuality: CGInterpolationQuality {
        get { state.interpolationQuality }
        set {
            state.interpolationQuality = newValue
            canvas?.setInterpolationQuality(mapQuality(newValue))
        }
    }

    public func setShouldAntialias(_ antialias: Bool) {
        state.shouldAntialias = antialias
        canvas?.setAntialias(antialias)
    }

    // MARK: - Transforms

    public func translateBy(x: CGFloat, y: CGFloat) {
        state.ctm = state.ctm.translatedBy(x: x, y: y)
        canvas?.translate(Float(x), Float(y))
    }

    public func scaleBy(x sx: CGFloat, y sy: CGFloat) {
        state.ctm = state.ctm.scaledBy(x: sx, y: sy)
        canvas?.scale(Float(sx), Float(sy))
    }

    public func rotate(by radians: CGFloat) {
        state.ctm = state.ctm.rotated(by: radians)
        canvas?.rotate(Float(radians))
    }

    /// Apple's Swift name for `concatCTM`.
    public func concatenate(_ transform: CGAffineTransform) { concatCTM(transform) }

    /// The current transformation matrix (user space to device space).
    public var ctm: CGAffineTransform { userSpaceToDeviceSpaceTransform }

    public func concatCTM(_ transform: CGAffineTransform) {
        state.ctm = state.ctm.concatenating(transform)
        canvas?.concat(transform)
    }

    public var userSpaceToDeviceSpaceTransform: CGAffineTransform {
        if isYUp { return state.ctm }   // the y flip is below the visible CTM, as in CoreGraphics
        return canvas?.totalMatrix ?? state.ctm
    }

    // MARK: - Clipping

    public func clip(to rect: CGRect) {
        canvas?.clip(rect: rect, antialias: state.shouldAntialias)
    }

    public func clip(to rect: CGRect, mask: CGImage) {
        if let canvas {
            let pImg = mask.portableImage
            var bytes = pImg.bytes
            if isYUp {
                // The mask's first row belongs at the rect's top edge (max y in y-up space): reverse the rows.
                let row = pImg.bytesPerRow
                var flipped = [UInt8](repeating: 0, count: bytes.count)
                for y in 0..<pImg.height { flipped.replaceSubrange((y * row)..<((y + 1) * row), with: bytes[((pImg.height - 1 - y) * row)..<((pImg.height - y) * row)]) }
                bytes = flipped
            }
            canvas.clip(mask: bytes, width: pImg.width, height: pImg.height, stride: pImg.bytesPerRow, rect: rect,
                        isGray: mask.isGrayPlane)
        }
    }

    public func clip(using rule: CGPathFillRule = .winding) {
        canvas?.clip(path: currentPath.segments, evenOdd: rule == .evenOdd, antialias: state.shouldAntialias)
        currentPath = CGMutablePath()
    }

    public var boundingBoxOfClipPath: CGRect {
        if let bounds = canvas?.clipBounds, !bounds.isNull { return bounds }
        return CGRect(x: 0, y: 0, width: width, height: height)
    }

    // MARK: - Paths

    public func beginPath() { currentPath = CGMutablePath() }
    public func closePath() { currentPath.closeSubpath() }
    public func move(to point: CGPoint) { currentPath.move(to: point) }
    public func addLine(to point: CGPoint) { currentPath.addLine(to: point) }
    public func addLines(between points: [CGPoint]) { currentPath.addLines(between: points) }
    public func addQuadCurve(to end: CGPoint, control: CGPoint) { currentPath.addQuadCurve(to: end, control: control) }
    public func addCurve(to end: CGPoint, control1: CGPoint, control2: CGPoint) {
        currentPath.addCurve(to: end, control1: control1, control2: control2)
    }
    public func addRect(_ rect: CGRect) { currentPath.addRect(rect) }
    public func addEllipse(in rect: CGRect) { currentPath.addEllipse(in: rect) }
    public func addPath(_ path: CGPath) { currentPath.addPath(path) }

    public func setLineCap(_ cap: CGLineCap) { state.lineCap = cap }
    /// Recorded for callers that read it back; dashed strokes are not rasterised yet.
    public func setLineDash(phase: CGFloat, lengths: [CGFloat]) { state.dashPhase = phase; state.dashLengths = lengths }

    /// Shadows are recorded but not rasterised yet.
    public func setShadow(offset: CGSize, blur: CGFloat, color: CGColor? = nil) { state.shadowOffset = offset; state.shadowBlur = blur }
    /// The path under construction (Apple exposes it as `path`).
    public var path: CGPath? { currentPath.isEmpty ? nil : currentPath }
    public func convertToDeviceSpace(_ rect: CGRect) -> CGRect { rect.applying(state.ctm) }
    public func beginTransparencyLayer(in rect: CGRect, auxiliaryInfo: [AnyHashable: Any]? = nil) { beginTransparencyLayer(auxiliaryInfo: auxiliaryInfo) }
    public func convertToDeviceSpace(_ point: CGPoint) -> CGPoint { point.applying(state.ctm) }
    public func convertToUserSpace(_ point: CGPoint) -> CGPoint { point.applying(state.ctm.inverted()) }

    /// The colour space of the backing bitmap (sRGB, premultiplied RGBA).
    public var colorSpace: CGColorSpace? { .srgbSpace }

    public func drawPath(using mode: CGPathDrawingMode) {
        let path = currentPath
        switch mode {
        case .fill, .eoFill, .fillStroke, .eoFillStroke:
            fill(path, rule: (mode == .eoFill || mode == .eoFillStroke) ? .evenOdd : .winding)
        case .stroke: break
        }
        if mode == .stroke || mode == .fillStroke || mode == .eoFillStroke { stroke(path) }
        currentPath = CGMutablePath()
    }
    public func setLineJoin(_ join: CGLineJoin) { state.lineJoin = join }
    public func setMiterLimit(_ limit: CGFloat) { state.miterLimit = limit }

    // MARK: - Painting & Colors

    public func setFillColor(gray: CGFloat, alpha: CGFloat) {
        state.fillColor = CGColor(gray: gray, alpha: alpha)
    }

    public func setFillColor(red: CGFloat, green: CGFloat, blue: CGFloat, alpha: CGFloat) {
        state.fillColor = CGColor(red: red, green: green, blue: blue, alpha: alpha)
    }

    public func setFillColor(_ color: CGColor) {
        state.fillColor = color
    }

    public func setStrokeColor(_ color: CGColor) {
        state.strokeColor = color
    }

    public func setLineWidth(_ width: CGFloat) {
        state.lineWidth = width
    }

    public func fill(_ rect: CGRect) {
        if let canvas, !state.shouldAntialias, fillsThroughClip {
            fillHardEdged(rect, on: canvas)
        } else if let canvas {
            canvas.fill(rect: rect, color: state.fillColor)
        } else {
            fillSwift(rect, color: state.fillColor)
        }
    }

    /// Fills each rect independently, as Apple's `fill(_ rects: [CGRect])` does (not their union).
    public func fill(_ rects: [CGRect]) {
        for rect in rects { fill(rect) }
    }

    /// A hard-edged rect fill under a rotated or skewed transform. Skia rounds an aliased fill's edge differently from an
    /// aliased clip's, and Core Graphics does not: a fill and a clip to the same rect cover the same pixels. Code that
    /// clears a region and then draws into a clip of it (upstream's tiled renderer) depends on that, or a seam pixel is
    /// drawn without having been cleared. Filling through a clip built from the same path as `clip(to:)` restores it.
    private var fillsThroughClip: Bool {
        let t = userSpaceToDeviceSpaceTransform
        return abs(t.b) > 1e-9 || abs(t.c) > 1e-9
    }

    private func fillHardEdged(_ rect: CGRect, on canvas: CanvasBackend) {
        canvas.save()
        canvas.clip(path: CGPath(rect: rect).segments, evenOdd: false, antialias: false)
        // Everything the clip leaves is the rect: fill a rect that covers all of it.
        canvas.fill(rect: boundingBoxOfClipPath.insetBy(dx: -2, dy: -2), color: state.fillColor)
        canvas.restore()
    }

    public func fillPath(using rule: CGPathFillRule = .winding) {
        let path = currentPath
        currentPath = CGMutablePath()
        fill(path, rule: rule)
    }

    /// Fills `path` (user space) with the current fill colour, honouring the clip, alpha and blend mode.
    public func fill(_ path: CGPath, rule: CGPathFillRule = .winding) {
        let c = state.fillColor
        if let canvas {
            canvas.fill(path: path.segments, evenOdd: rule == .evenOdd, color: c)
            return
        }
        fillPathSwift(path, rule: rule, color: c)
    }

    public func stroke(_ rect: CGRect) {
        // Simple stroke fallback: stroke rect edges
        let lw = state.lineWidth
        fill(CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: lw))
        fill(CGRect(x: rect.minX, y: rect.maxY - lw, width: rect.width, height: lw))
        fill(CGRect(x: rect.minX, y: rect.minY, width: lw, height: rect.height))
        fill(CGRect(x: rect.maxX - lw, y: rect.minY, width: lw, height: rect.height))
    }

    public func strokePath() {
        let path = currentPath
        currentPath = CGMutablePath()
        stroke(path)
    }

    /// Strokes `path` (user space) with the current stroke colour, width, cap, join and miter limit.
    public func stroke(_ path: CGPath) {
        let c = state.strokeColor
        if let canvas {
            canvas.stroke(path: path.segments, width: Float(state.lineWidth), cap: state.lineCap.rawValue,
                          join: state.lineJoin.rawValue, miterLimit: Float(state.miterLimit), color: c)
            return
        }
        let outline = path.copy(strokingWithWidth: state.lineWidth, lineCap: state.lineCap,
                                lineJoin: state.lineJoin, miterLimit: state.miterLimit)
        fillPathSwift(outline, rule: .winding, color: c)
    }

    public func fillEllipse(in rect: CGRect) {
        currentPath.addEllipse(in: rect)
        fillPath()
    }

    // MARK: - Gradients

    public func drawLinearGradient(_ gradient: CGGradient, start: CGPoint, end: CGPoint,
                                   options: CGGradientDrawingOptions) {
        canvas?.linearGradient(gradient, from: start, to: end, options: Int32(options.rawValue))
    }

    public func drawRadialGradient(_ gradient: CGGradient, startCenter: CGPoint, startRadius: CGFloat,
                                   endCenter: CGPoint, endRadius: CGFloat, options: CGGradientDrawingOptions) {
        canvas?.radialGradient(gradient, from: startCenter, startRadius: Float(startRadius), to: endCenter,
                               endRadius: Float(endRadius), options: Int32(options.rawValue))
    }

    public func strokeEllipse(in rect: CGRect) {
        stroke(rect)
    }

    public func clear(_ rect: CGRect) {
        if let canvas {
            canvas.clear(rect: rect)
        } else {
            clearSwift(rect)
        }
    }

    // MARK: - Drawing Images

    public func draw(_ image: CGImage, in rect: CGRect, opacity: Double = 1) {
        if let canvas {
            // The canvas already holds setAlpha's graphics state. Pass only the per-draw
            // opacity; multiplying here as well would square the context alpha in Skia.
            // In y-up user space an image's first row sits at the rect's top edge (max y): draw it through a local flip.
            if isYUp {
                saveGState()
                translateBy(x: rect.minX, y: rect.maxY)
                scaleBy(x: 1, y: -1)
                drawRaw(canvas, image, in: CGRect(x: 0, y: 0, width: rect.width, height: rect.height), opacity: opacity)
                restoreGState()
            } else {
                drawRaw(canvas, image, in: rect, opacity: opacity)
            }
            return
        }
        guard format == .rgba else { return }
        let finalOpacity = opacity * Double(state.alpha)

        // Check injected render closure fallback
        if finalOpacity >= 1, rect.origin == .zero,
           rect.width == CGFloat(image.width),
           rect.height == CGFloat(image.height),
           let render = self.render {
            let pImg = image.portableImage
            let rc = pImg.bytes.withUnsafeBufferPointer { srcPtr in
                render(srcPtr.baseAddress!, self.pixelData, pImg.width, pImg.height)
            }
            if rc == 0 { return }
        }

        drawSwift(image.portableImage, in: rect, opacity: finalOpacity)
    }

    /// `opacity` is the caller's own; the context's alpha is applied by the backend from its tracked state.
    private func drawRaw(_ canvas: CanvasBackend, _ image: CGImage, in rect: CGRect, opacity: Double) {
        canvas.draw(image: image.portableImage, isGray: image.isGrayPlane, in: rect, opacity: Float(opacity),
                    blendMode: state.blendMode.rawValue, quality: mapQuality(state.interpolationQuality))
    }

    public func draw(_ image: PortableImage, in rect: CGRect, opacity: Double = 1) {
        draw(CGImage(image), in: rect, opacity: opacity)
    }

    // MARK: - Transparency Layers

    public func beginTransparencyLayer(auxiliaryInfo: [AnyHashable: Any]? = nil) {
        saveGState()
        canvas?.beginLayer(alpha: Float(state.alpha))
    }

    public func endTransparencyLayer() {
        canvas?.endLayer()
        restoreGState()
    }

    // MARK: - Output

    public func makeImage() -> CGImage? {
        syncToExternalData()
        if format == .gray {
            let plane = Array<UInt8>(unsafeUninitializedCapacity: pixelCount) { buf, initializedCount in
                memcpy(buf.baseAddress!, pixelData, pixelCount)
                initializedCount = pixelCount
            }
            return CGImage(PortableImage(width: width, height: height, kind: .mask, bytesPerRow: width, bytes: plane))
        }
        return CGImage(buffer)
    }

    // MARK: - Swift Fallbacks

    private func fillSwift(_ rect: CGRect, color: CGColor) {
        let r = UInt8(clamping: Int(color.red * 255))
        let g = UInt8(clamping: Int(color.green * 255))
        let b = UInt8(clamping: Int(color.blue * 255))
        let a = UInt8(clamping: Int(color.alpha * state.alpha * 255))
        let ix = max(0, Int(rect.minX)), iy = max(0, Int(rect.minY))
        let iw = min(width - ix, Int(rect.width)), ih = min(height - iy, Int(rect.height))
        guard iw > 0 && ih > 0 else { return }

        for y in iy..<(iy + ih) {
            for x in ix..<(ix + iw) {
                let off = (y * width + x) * 4
                pixelData[off] = r
                pixelData[off + 1] = g
                pixelData[off + 2] = b
                pixelData[off + 3] = a
            }
        }
    }

    /// Software path fill: 4x4 supersampled coverage through `CGPath.contains`, source-over with the fill alpha.
    /// Used only when no render device is bound; ignores clipping and blend modes beyond source-over.
    private func fillPathSwift(_ path: CGPath, rule: CGPathFillRule, color: CGColor) {
        guard !path.isEmpty else { return }
        let device = state.ctm == .identity ? path : (path.copy(using: [state.ctm]) ?? path)
        let bounds = device.boundingBoxOfPath
        guard !bounds.isNull else { return }
        let x0 = max(0, Int(floor(bounds.minX))), x1 = min(width, Int(ceil(bounds.maxX)))
        let y0 = max(0, Int(floor(bounds.minY))), y1 = min(height, Int(ceil(bounds.maxY)))
        guard x0 < x1, y0 < y1 else { return }
        let a = Double(color.alpha * state.alpha)
        let premul = [Double(color.red) * a, Double(color.green) * a, Double(color.blue) * a, a]
        for y in y0..<y1 {
            for x in x0..<x1 {
                var hits = 0
                for sy in 0..<4 { for sx in 0..<4 {
                    if device.contains(CGPoint(x: CGFloat(x) + (CGFloat(sx) + 0.5) / 4, y: CGFloat(y) + (CGFloat(sy) + 0.5) / 4), using: rule) { hits += 1 }
                } }
                guard hits > 0 else { continue }
                let cov = Double(hits) / 16
                let off = (y * width + x) * 4
                let inv = 1 - premul[3] * cov
                for i in 0..<4 {
                    let dst = Double(pixelData[off + i])
                    pixelData[off + i] = UInt8(max(0, min(255, (premul[i] * cov * 255 + dst * inv).rounded())))
                }
            }
        }
    }

    private func clearSwift(_ rect: CGRect) {
        let ix = max(0, Int(rect.minX)), iy = max(0, Int(rect.minY))
        let iw = min(width - ix, Int(rect.width)), ih = min(height - iy, Int(rect.height))
        guard iw > 0 && ih > 0 else { return }

        for y in iy..<(iy + ih) {
            let rowOff = (y * width + ix) * 4
            memset(pixelData.advanced(by: rowOff), 0, iw * 4)
        }
    }

    /// Pure-Swift software drawing used when no render device is bound. The compat module has no knowledge of
    /// the document domain, so whoever links a renderer installs it (see `CompositorCore.installSoftwareRasterizer`).
    public typealias SoftwareDraw = (_ image: PortableImage, _ rect: CGRect, _ opacity: Double, _ into: inout PixelBuffer) -> Void
    nonisolated(unsafe) public static var softwareDraw: SoftwareDraw?

    private func drawSwift(_ image: PortableImage, in rect: CGRect, opacity: Double) {
        guard let softwareDraw = CGContext.softwareDraw else { return }
        var tempBuffer = self.buffer
        softwareDraw(image, rect, opacity, &tempBuffer)
        _ = tempBuffer.bytes.withUnsafeBufferPointer { srcPtr in
            memcpy(self.pixelData, srcPtr.baseAddress!, pixelCount)
        }
    }

    private func mapQuality(_ q: CGInterpolationQuality) -> Int32 {
        switch q {
        case .none: return 1
        case .low: return 2
        case .medium: return 3
        case .high: return 4
        case .default: return 0
        }
    }
}

public enum CGPathDrawingMode: Int32, Sendable { case fill, eoFill, stroke, fillStroke, eoFillStroke }

public typealias CGContextCompat = CGContext
