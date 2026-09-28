// SkiaCanvasBackend.swift — `CanvasBackend` over the Skia canvas C ABI (`CompCanvas`), resolved at runtime so the
// Swift build needs no Skia headers or link flags (see RenderDeviceBinding.swift and SkiaBridge.h).

import Foundation

struct SkiaCanvasFactory: CanvasBackendFactory {
    var name: String { "skia" }
    var isAvailable: Bool { CompCanvasBridge.shared.isAvailable }

    func makeCanvas(pixels: UnsafeMutablePointer<UInt8>, width: Int, height: Int, bytesPerRow: Int,
                    format: CGContext.PixelFormat) -> CanvasBackend? {
        let raw: OpaquePointer?
        if let create = SkiaContextABI.createEx {
            raw = create(pixels, width, height, bytesPerRow, format == .gray ? 1 : 0)
        } else if format == .rgba, let create = CompCanvasBridge.shared.createCanvas {
            raw = create(pixels, width, height, bytesPerRow)
        } else {
            raw = nil
        }
        return raw.map { SkiaCanvasBackend(raw: $0) }
    }
}

final class SkiaCanvasBackend: CanvasBackend, @unchecked Sendable {
    private let raw: OpaquePointer
    private let bridge = CompCanvasBridge.shared
    var name: String { "skia" }

    init(raw: OpaquePointer) { self.raw = raw }
    deinit { bridge.destroyCanvas?(raw) }

    func save() { bridge.save?(raw) }
    func restore() { bridge.restore?(raw) }
    func setAlpha(_ alpha: Float) { bridge.setAlpha?(raw, alpha) }
    func setBlendMode(_ rawValue: Int32) { bridge.setBlendMode?(raw, rawValue) }
    func setInterpolationQuality(_ rawValue: Int32) { bridge.setInterpolationQuality?(raw, rawValue) }
    func setAntialias(_ enabled: Bool) { bridge.setAntialias?(raw, enabled ? 1 : 0) }

    func translate(_ tx: Float, _ ty: Float) { bridge.translate?(raw, tx, ty) }
    func scale(_ sx: Float, _ sy: Float) { bridge.scale?(raw, sx, sy) }
    func rotate(_ radians: Float) { bridge.rotate?(raw, radians) }
    func concat(_ t: CGAffineTransform) {
        bridge.concat?(raw, Float(t.a), Float(t.b), Float(t.c), Float(t.d), Float(t.tx), Float(t.ty))
    }
    var totalMatrix: CGAffineTransform? {
        guard let getCTM = bridge.getCTM else { return nil }
        var a: Float = 1, b: Float = 0, c: Float = 0, d: Float = 1, tx: Float = 0, ty: Float = 0
        getCTM(raw, &a, &b, &c, &d, &tx, &ty)
        return CGAffineTransform(a: CGFloat(a), b: CGFloat(b), c: CGFloat(c), d: CGFloat(d), tx: CGFloat(tx), ty: CGFloat(ty))
    }

    func clip(rect: CGRect, antialias: Bool) {
        bridge.clipRect?(raw, Float(rect.minX), Float(rect.minY), Float(rect.width), Float(rect.height), antialias ? 1 : 0)
    }
    func clip(path: [PathSegment], evenOdd: Bool, antialias: Bool) {
        guard let clipPath = bridge.clipPath, let sk = SkiaPathABI.make(path) else { return }
        clipPath(raw, sk, evenOdd ? 1 : 0, antialias ? 1 : 0)
        SkiaPathABI.destroy?(sk)
    }
    func clip(mask: [UInt8], width: Int, height: Int, stride: Int, rect: CGRect, isGray: Bool) {
        mask.withUnsafeBufferPointer { ptr in
            bridge.clipMask?(raw, ptr.baseAddress!, width, height, stride, Float(rect.minX), Float(rect.minY),
                             Float(rect.width), Float(rect.height), isGray ? 1 : 0)
        }
    }
    var clipBounds: CGRect {
        guard let getClip = bridge.getClipBounds else { return .null }
        var x: Float = 0, y: Float = 0, w: Float = 0, h: Float = 0
        getClip(raw, &x, &y, &w, &h)
        return CGRect(x: CGFloat(x), y: CGFloat(y), width: CGFloat(w), height: CGFloat(h))
    }

    func fill(rect: CGRect, color c: CGColor) {
        bridge.fillRect?(raw, Float(rect.minX), Float(rect.minY), Float(rect.width), Float(rect.height),
                         Float(c.red), Float(c.green), Float(c.blue), Float(c.alpha))
    }
    func fill(path: [PathSegment], evenOdd: Bool, color c: CGColor) {
        guard let fillFn = SkiaPathABI.fillPath, let sk = SkiaPathABI.make(path) else { return }
        fillFn(raw, sk, evenOdd ? 1 : 0, Float(c.red), Float(c.green), Float(c.blue), Float(c.alpha))
        SkiaPathABI.destroy?(sk)
    }
    func stroke(path: [PathSegment], width: Float, cap: Int32, join: Int32, miterLimit: Float, color c: CGColor) {
        guard let strokeFn = SkiaPathABI.strokePath, let sk = SkiaPathABI.make(path) else { return }
        strokeFn(raw, sk, width, cap, join, miterLimit, Float(c.red), Float(c.green), Float(c.blue), Float(c.alpha))
        SkiaPathABI.destroy?(sk)
    }
    func clear(rect: CGRect) {
        bridge.clear?(raw, Float(rect.minX), Float(rect.minY), Float(rect.width), Float(rect.height))
    }
    func draw(image: PortableImage, isGray: Bool, in rect: CGRect, opacity: Float, blendMode: Int32, quality: Int32) {
        image.bytes.withUnsafeBufferPointer { ptr in
            if let drawEx = SkiaContextABI.drawImageEx {
                drawEx(raw, ptr.baseAddress!, image.width, image.height, image.bytesPerRow, isGray ? 1 : 0,
                       Float(rect.minX), Float(rect.minY), Float(rect.width), Float(rect.height), opacity, blendMode, quality)
            } else if !isGray {
                bridge.drawImageRect?(raw, ptr.baseAddress!, image.width, image.height, image.bytesPerRow,
                                      Float(rect.minX), Float(rect.minY), Float(rect.width), Float(rect.height),
                                      opacity, blendMode, quality)
            }
        }
    }
    func linearGradient(_ gradient: CGGradient, from start: CGPoint, to end: CGPoint, options: Int32) {
        guard let draw = SkiaPathABI.linearGradient else { return }
        let colors = gradient.packedColors, locations = gradient.packedLocations
        draw(raw, Float(start.x), Float(start.y), Float(end.x), Float(end.y), colors, locations, locations.count, options)
    }
    func radialGradient(_ gradient: CGGradient, from start: CGPoint, startRadius: Float, to end: CGPoint, endRadius: Float, options: Int32) {
        guard let draw = SkiaPathABI.radialGradient else { return }
        let colors = gradient.packedColors, locations = gradient.packedLocations
        draw(raw, Float(start.x), Float(start.y), startRadius, Float(end.x), Float(end.y), endRadius,
             colors, locations, locations.count, options)
    }

    func beginLayer(alpha: Float) { bridge.beginTransparencyLayer?(raw, alpha) }
    func endLayer() { bridge.endTransparencyLayer?(raw) }
}
