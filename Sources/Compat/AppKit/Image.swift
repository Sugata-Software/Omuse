import Foundation
import CoreGraphics
import ImageIO

/// A bitmap wrapper. Rendering symbols/vector art is a macOS feature; on Linux an `NSImage` holds pixels.
open class NSImage: @unchecked Sendable {
    public var size: CGSize
    private var bitmap: CGImage?

    public final class SymbolConfiguration {
        public var pointSize: CGFloat = 0
        public var paletteColors: [NSColor] = []
        public init(pointSize: CGFloat, weight: NSFont.Weight) { self.pointSize = pointSize }
        public init(pointSize: CGFloat) { self.pointSize = pointSize }
        public init(paletteColors: [NSColor]) { self.paletteColors = paletteColors }
        public func applying(_ other: SymbolConfiguration) -> SymbolConfiguration {
            let merged = SymbolConfiguration(pointSize: other.pointSize > 0 ? other.pointSize : pointSize)
            merged.paletteColors = other.paletteColors.isEmpty ? paletteColors : other.paletteColors
            return merged
        }
    }

    public init(size: CGSize) { self.size = size }
    public init(cgImage: CGImage, size: CGSize) { self.bitmap = cgImage; self.size = size }
    public convenience init?(data: Data) {
        guard let decoded = ImageCodecRegistry.decode(data) else { return nil }
        self.init(cgImage: decoded, size: CGSize(width: decoded.width, height: decoded.height))
    }
    public convenience init(size: CGSize, flipped: Bool, drawingHandler: (CGRect) -> Bool) {
        self.init(size: size)
        let w = max(1, Int(size.width)), h = max(1, Int(size.height))
        let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!,
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        if flipped { ctx.translateBy(x: 0, y: CGFloat(h)); ctx.scaleBy(x: 1, y: -1) }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: flipped)
        _ = drawingHandler(CGRect(origin: .zero, size: size))
        NSGraphicsContext.restoreGraphicsState()
        bitmap = ctx.makeImage()
    }
    /// SF Symbols do not exist on Linux. Callers force-unwrap this initialiser, so it hands back an empty 16-pt image
    /// (the Qt shell draws its own icons and cursors).
    public convenience init?(systemSymbolName: String, accessibilityDescription: String?) {
        self.init(size: CGSize(width: 16, height: 16))
    }
    /// Reads an image the pasteboard holds as PNG/TIFF data (needs the ImageIO codec backend).
    public convenience init?(pasteboard: NSPasteboard) {
        for type in [NSPasteboard.PasteboardType.png, .tiff] {
            if let data = pasteboard.data(forType: type), let image = ImageCodecRegistry.decode(data) {
                self.init(cgImage: image, size: CGSize(width: image.width, height: image.height)); return
            }
        }
        return nil
    }

    public func cgImage(forProposedRect proposedDestRect: UnsafeMutablePointer<CGRect>?, context: NSGraphicsContext?,
                        hints: [NSImageRep.HintKey: Any]?) -> CGImage? { bitmap }
    public func draw(in rect: CGRect, from source: CGRect, operation: NSCompositingOperation, fraction: CGFloat,
                     respectFlipped: Bool, hints: [NSImageRep.HintKey: Any]?) {
        draw(in: rect, from: source, operation: operation, fraction: fraction)
    }
    public func lockFocus() {}
    public func unlockFocus() {}
}

open class NSImageRep {
    public struct HintKey: Hashable, RawRepresentable { public let rawValue: String; public init(rawValue: String) { self.rawValue = rawValue } }
}

public final class NSBitmapImageRep: NSImageRep, @unchecked Sendable {
    public enum FileType { case png, jpeg, tiff, bmp, gif }
    private let fixed: CGImage?
    /// Present for reps that cache a view's display: drawing goes into this context and `image` reads it back.
    let context: CGContext?
    private var image: CGImage { context?.makeImage() ?? fixed! }
    public init(cgImage: CGImage) { fixed = cgImage; context = nil }
    init(context: CGContext) { fixed = nil; self.context = context }
    /// Decodes PNG/JPEG/TIFF/... data through the ImageIO backend.
    public convenience init?(data: Data) {
        guard let decoded = ImageCodecRegistry.decode(data) else { return nil }
        self.init(cgImage: decoded)
    }
    public var size: CGSize { CGSize(width: image.width, height: image.height) }
    public var cgImage: CGImage? { image }
    public var pixelsWide: Int { image.width }
    public var pixelsHigh: Int { image.height }
    func pixelColor(x: Int, y: Int) -> NSColor? {
        guard x >= 0, y >= 0, x < image.width, y < image.height else { return nil }
        let bytes = image.portableImage.bytes, i = y * image.bytesPerRow
        if image.isGrayPlane { let v = CGFloat(bytes[i + x]) / 255; return NSColor(white: v, alpha: 1) }
        let o = i + x * 4, a = CGFloat(bytes[o + 3]) / 255
        guard a > 0 else { return NSColor(srgbRed: 0, green: 0, blue: 0, alpha: 0) }
        return NSColor(srgbRed: CGFloat(bytes[o]) / 255 / a, green: CGFloat(bytes[o + 1]) / 255 / a, blue: CGFloat(bytes[o + 2]) / 255 / a, alpha: a)
    }

    public func representation(using type: FileType, properties: [PropertyKey: Any]) -> Data? {
        let uti: String
        switch type { case .png: uti = "public.png"; case .jpeg: uti = "public.jpeg"; case .tiff: uti = "public.tiff"
                      case .bmp: uti = "com.microsoft.bmp"; case .gif: uti = "com.compuserve.gif" }
        return ImageCodecRegistry.encode(image, typeIdentifier: uti)
    }
    public struct PropertyKey: Hashable, RawRepresentable { public let rawValue: String; public init(rawValue: String) { self.rawValue = rawValue } }
}

public struct NSColorSpaceName: RawRepresentable, Hashable, Sendable {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let deviceRGB = NSColorSpaceName(rawValue: "NSDeviceRGBColorSpace")
    public static let calibratedRGB = NSColorSpaceName(rawValue: "NSCalibratedRGBColorSpace")
    public static let deviceWhite = NSColorSpaceName(rawValue: "NSDeviceWhiteColorSpace")
}

extension NSBitmapImageRep {
    /// A blank bitmap to draw into (`NSGraphicsContext(bitmapImageRep:)`). Only the 8-bit RGBA layout is supported.
    public convenience init?(bitmapDataPlanes planes: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?, pixelsWide: Int,
                             pixelsHigh: Int, bitsPerSample: Int, samplesPerPixel: Int, hasAlpha: Bool, isPlanar: Bool,
                             colorSpaceName: NSColorSpaceName, bytesPerRow: Int, bitsPerPixel: Int) {
        guard pixelsWide > 0, pixelsHigh > 0, bitsPerSample == 8, samplesPerPixel == 4, !isPlanar else { return nil }
        guard let ctx = CGContext(data: nil, width: pixelsWide, height: pixelsHigh, bitsPerComponent: 8,
                                  bytesPerRow: pixelsWide * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        self.init(context: ctx)
    }

    public var bytesPerRow: Int { (cgImage?.width ?? 0) * 4 }
    public var bitsPerPixel: Int { 32 }
    /// The live pixel memory of a drawable bitmap (premultiplied RGBA, top row first).
    public var bitmapData: UnsafeMutablePointer<UInt8>? { context?.data?.assumingMemoryBound(to: UInt8.self) }
}

extension NSGraphicsContext {
    public convenience init?(bitmapImageRep rep: NSBitmapImageRep) {
        guard let ctx = rep.context else { return nil }
        self.init(cgContext: ctx, flipped: false)
    }
}
