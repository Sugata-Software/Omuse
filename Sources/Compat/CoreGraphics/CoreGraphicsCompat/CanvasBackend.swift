// CanvasBackend.swift — the seam between `CGContext` and whatever rasterizes into its pixel memory.
//
// `CGContext` keeps the Core Graphics contract (graphics state, y-up bitmaps, gray planes, Apple's initialisers) and
// hands the drawing itself to a `CanvasBackend`. Skia is the backend today (`SkiaCanvasBackend`); a Vulkan-presented
// canvas, an OpenCV/CPU one or a test double plug in the same way, and the registry tries them in order, so a backend
// that cannot start (no device, no library) falls through to the next one and finally to the pure-Swift paths that
// `CGContext` keeps. Depends only on Foundation types: no Skia, Qt or Vulkan leaks through this interface.

import Foundation
import CompatSupport

/// One drawable surface over caller-owned pixel memory. Coordinates are in the backend's own user space; `CGContext`
/// applies Core Graphics's orientation on top with `translate`/`scale` before drawing.
public protocol CanvasBackend: AnyObject {
    var name: String { get }

    // Graphics state
    func save()
    func restore()
    func setAlpha(_ alpha: Float)
    func setBlendMode(_ rawValue: Int32)
    func setInterpolationQuality(_ rawValue: Int32)
    func setAntialias(_ enabled: Bool)

    // Transform. `totalMatrix` is the whole user-to-device matrix, or nil when the backend does not track one.
    func translate(_ tx: Float, _ ty: Float)
    func scale(_ sx: Float, _ sy: Float)
    func rotate(_ radians: Float)
    func concat(_ transform: CGAffineTransform)
    var totalMatrix: CGAffineTransform? { get }

    // Clipping
    func clip(rect: CGRect, antialias: Bool)
    func clip(path: [PathSegment], evenOdd: Bool, antialias: Bool)
    /// `mask` is `width` x `height` (row stride `stride`), stretched over `rect`; `isGray` says it is one byte per pixel.
    func clip(mask: [UInt8], width: Int, height: Int, stride: Int, rect: CGRect, isGray: Bool)
    /// The visible clip in user space.
    var clipBounds: CGRect { get }

    // Painting
    func fill(rect: CGRect, color: CGColor)
    func fill(path: [PathSegment], evenOdd: Bool, color: CGColor)
    func stroke(path: [PathSegment], width: Float, cap: Int32, join: Int32, miterLimit: Float, color: CGColor)
    func clear(rect: CGRect)
    func draw(image: PortableImage, isGray: Bool, in rect: CGRect, opacity: Float, blendMode: Int32, quality: Int32)
    func linearGradient(_ gradient: CGGradient, from start: CGPoint, to end: CGPoint, options: Int32)
    func radialGradient(_ gradient: CGGradient, from start: CGPoint, startRadius: Float, to end: CGPoint, endRadius: Float, options: Int32)

    // Transparency layers
    func beginLayer(alpha: Float)
    func endLayer()
}

/// Creates backends over a context's pixel memory. Return nil when this backend cannot serve the request.
public protocol CanvasBackendFactory {
    var name: String { get }
    /// A cheap check that this backend can start at all (its library is loaded, its device exists).
    var isAvailable: Bool { get }
    func makeCanvas(pixels: UnsafeMutablePointer<UInt8>, width: Int, height: Int, bytesPerRow: Int,
                    format: CGContext.PixelFormat) -> CanvasBackend?
}

/// The ordered list of backends a new `CGContext` tries. The default is Skia; hosts and tests install their own.
public enum CanvasBackends {
    public static let slot = ServiceSlot<[CanvasBackendFactory]>(fallback: { [SkiaCanvasFactory()] })

    /// The backends a new context tries, in order.
    public static var factories: [CanvasBackendFactory] {
        get { slot.current ?? [] }
        set { slot.install(newValue) }
    }

    /// Runs `body` with these backends (e.g. a recording double, or none to force the pure-Swift paths).
    public static func withFactories<Result>(_ factories: [CanvasBackendFactory], _ body: () throws -> Result) rethrows -> Result {
        try slot.withOverride(factories, body)
    }

    /// True when at least one backend can draw; callers that would otherwise fall back to pure Swift ask this.
    public static var isAvailable: Bool { factories.contains { $0.isAvailable } }

    static func make(pixels: UnsafeMutablePointer<UInt8>, width: Int, height: Int, bytesPerRow: Int,
                     format: CGContext.PixelFormat) -> CanvasBackend? {
        for factory in factories {
            if let canvas = factory.makeCanvas(pixels: pixels, width: width, height: height, bytesPerRow: bytesPerRow, format: format) {
                return canvas
            }
        }
        return nil
    }
}
