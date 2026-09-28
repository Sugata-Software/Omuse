// ImageIO.swift — CGImageSource / CGImageDestination as Apple spells them, over `ImageCodecRegistry`.
// Property/option keys are CFString constants (String on Linux), so `[kCGImageSourceShouldCache: false] as CFDictionary`
// and `properties[kCGImagePropertyPixelWidth] as? Int` compile and behave unchanged.

@_exported import CoreGraphics
import Foundation
import Accelerate
import UniformTypeIdentifiers

// MARK: - Keys

public let kCGImageSourceShouldCache: CFString = "kCGImageSourceShouldCache"
public let kCGImageSourceShouldCacheImmediately: CFString = "kCGImageSourceShouldCacheImmediately"
public let kCGImageSourceCreateThumbnailFromImageAlways: CFString = "kCGImageSourceCreateThumbnailFromImageAlways"
public let kCGImageSourceCreateThumbnailFromImageIfAbsent: CFString = "kCGImageSourceCreateThumbnailFromImageIfAbsent"
public let kCGImageSourceCreateThumbnailWithTransform: CFString = "kCGImageSourceCreateThumbnailWithTransform"
public let kCGImageSourceThumbnailMaxPixelSize: CFString = "kCGImageSourceThumbnailMaxPixelSize"
public let kCGImagePropertyPixelWidth: CFString = "PixelWidth"
public let kCGImagePropertyPixelHeight: CFString = "PixelHeight"
public let kCGImagePropertyDPIWidth: CFString = "DPIWidth"
public let kCGImagePropertyDPIHeight: CFString = "DPIHeight"
public let kCGImagePropertyDepth: CFString = "Depth"
public let kCGImagePropertyOrientation: CFString = "Orientation"
public let kCGImagePropertyHasAlpha: CFString = "HasAlpha"
public let kCGImageDestinationLossyCompressionQuality: CFString = "kCGImageDestinationLossyCompressionQuality"

// MARK: - Source

public final class CGImageSource: @unchecked Sendable {
    let data: Data
    let info: ImageInfo
    private var cached: CGImage?
    init(data: Data, info: ImageInfo) { self.data = data; self.info = info }

    func image() -> CGImage? {
        if let cached { return cached }
        cached = ImageCodecRegistry.decode(data)
        return cached
    }
}

private func boolOption(_ options: CFDictionary?, _ key: CFString) -> Bool? {
    options?[key] as? Bool
}

public func CGImageSourceCreateWithData(_ data: CFData, _ options: CFDictionary?) -> CGImageSource? {
    guard let info = ImageCodecRegistry.identify(data) else { return nil }
    return CGImageSource(data: data, info: info)
}

public func CGImageSourceCreateWithURL(_ url: CFURL, _ options: CFDictionary?) -> CGImageSource? {
    guard let data = try? Data(contentsOf: url) else { return nil }
    return CGImageSourceCreateWithData(data, options)
}

/// The image's type identifier, or nil when it is not an image.
public func CGImageSourceGetType(_ source: CGImageSource) -> CFString? { source.info.typeIdentifier }
public func CGImageSourceGetCount(_ source: CGImageSource) -> Int { 1 }

public func CGImageSourceCopyPropertiesAtIndex(_ source: CGImageSource, _ index: Int, _ options: CFDictionary?) -> CFDictionary? {
    guard index == 0 else { return nil }
    var info = source.info
    // Header-only recognition (magic bytes) knows the type but not the size; decode to learn it.
    if info.width == 0, let image = source.image() { info.width = image.width; info.height = image.height }
    guard info.width > 0 else { return nil }
    var dict: [AnyHashable: Any] = [
        kCGImagePropertyPixelWidth: info.width, kCGImagePropertyPixelHeight: info.height,
        kCGImagePropertyDepth: info.bitDepth, kCGImagePropertyOrientation: info.orientation,
        kCGImagePropertyHasAlpha: info.hasAlpha,
    ]
    if let dpi = info.dpi { dict[kCGImagePropertyDPIWidth] = dpi; dict[kCGImagePropertyDPIHeight] = dpi }
    return dict
}

public func CGImageSourceCreateImageAtIndex(_ source: CGImageSource, _ index: Int, _ options: CFDictionary?) -> CGImage? {
    guard index == 0 else { return nil }
    return source.image()
}

/// Downscaled image with its longest side at most `kCGImageSourceThumbnailMaxPixelSize`. With
/// `kCGImageSourceCreateThumbnailWithTransform` the EXIF orientation is applied.
public func CGImageSourceCreateThumbnailAtIndex(_ source: CGImageSource, _ index: Int, _ options: CFDictionary?) -> CGImage? {
    guard index == 0, var image = source.image() else { return nil }
    if boolOption(options, kCGImageSourceCreateThumbnailWithTransform) == true, source.info.orientation != 1 {
        image = ImageOrientation.apply(source.info.orientation, to: image)
    }
    guard let maxSide = options?[kCGImageSourceThumbnailMaxPixelSize] as? Int, maxSide > 0,
          max(image.width, image.height) > maxSide else { return image }
    let scale = Double(maxSide) / Double(max(image.width, image.height))
    let w = max(1, Int((Double(image.width) * scale).rounded())), h = max(1, Int((Double(image.height) * scale).rounded()))
    return ImageOrientation.resampled(image, width: w, height: h)
}

// MARK: - Destination

public final class CGImageDestination: @unchecked Sendable {
    let sink: NSMutableData
    let type: String
    var image: CGImage?
    var properties: [AnyHashable: Any] = [:]
    var finalized = false
    var url: URL?
    init(sink: NSMutableData, type: String) { self.sink = sink; self.type = type }
}

public func CGImageDestinationCreateWithData(_ data: CFMutableData, _ type: CFString, _ count: Int, _ options: CFDictionary?) -> CGImageDestination? {
    guard count == 1, UTType(type)?.conforms(to: .image) == true else { return nil }
    return CGImageDestination(sink: data, type: type)
}

public func CGImageDestinationCreateWithURL(_ url: CFURL, _ type: CFString, _ count: Int, _ options: CFDictionary?) -> CGImageDestination? {
    guard count == 1, UTType(type)?.conforms(to: .image) == true else { return nil }
    let destination = CGImageDestination(sink: NSMutableData(), type: type)
    destination.url = url
    return destination
}

public func CGImageDestinationAddImage(_ destination: CGImageDestination, _ image: CGImage, _ properties: CFDictionary?) {
    guard destination.image == nil, !destination.finalized else { return }
    destination.image = image
    destination.properties = properties ?? [:]
}

public func CGImageDestinationFinalize(_ destination: CGImageDestination) -> Bool {
    guard !destination.finalized, let image = destination.image else { return false }
    destination.finalized = true
    let quality = destination.properties[kCGImageDestinationLossyCompressionQuality] as? Double
    let dpi = (destination.properties[kCGImagePropertyDPIWidth] as? Double) ?? (destination.properties[kCGImagePropertyDPIWidth] as? CGFloat).map(Double.init)
    let orientation = (destination.properties[kCGImagePropertyOrientation] as? Int).map(Int32.init) ?? (destination.properties[kCGImagePropertyOrientation] as? Int32) ?? 1
    guard let data = ImageCodecRegistry.encode(image, typeIdentifier: destination.type, quality: quality, dpi: dpi, orientation: orientation) else { return false }
    if let url = destination.url {
        do { try data.write(to: url); return true } catch { return false }
    }
    destination.sink.append(data)
    return true
}

// MARK: - Orientation and resampling helpers

enum ImageOrientation {
    /// Applies an EXIF orientation (1...8) so the result is upright. Works on the canonical pixel layout.
    static func apply(_ orientation: Int32, to image: CGImage) -> CGImage {
        guard orientation != 1, (2...8).contains(orientation) else { return image }
        let w = image.width, h = image.height, ch = image.isGrayPlane ? 1 : 4
        let swap = orientation >= 5
        let ow = swap ? h : w, oh = swap ? w : h
        let src = image.portableImage.bytes, srcRow = image.bytesPerRow
        var out = [UInt8](repeating: 0, count: ow * oh * ch)
        for y in 0..<h {
            for x in 0..<w {
                var nx = x, ny = y
                switch orientation {
                case 2: nx = w - 1 - x
                case 3: nx = w - 1 - x; ny = h - 1 - y
                case 4: ny = h - 1 - y
                case 5: nx = y; ny = x
                case 6: nx = h - 1 - y; ny = x
                case 7: nx = h - 1 - y; ny = w - 1 - x
                default: nx = y; ny = w - 1 - x
                }
                for c in 0..<ch { out[(ny * ow + nx) * ch + c] = src[y * srcRow + x * ch + c] }
            }
        }
        return CGImage(PortableImage(width: ow, height: oh, kind: image.isGrayPlane ? .mask : .rgba, bytesPerRow: ow * ch, bytes: out))
    }

    static func resampled(_ image: CGImage, width: Int, height: Int) -> CGImage {
        let ch = image.isGrayPlane ? 1 : 4
        var out = [UInt8](repeating: 0, count: width * height * ch)
        var src = image.portableImage.bytes
        src.withUnsafeMutableBytes { s in
            out.withUnsafeMutableBytes { o in
                vImageResample8(source: s.baseAddress!, sourceWidth: image.width, sourceHeight: image.height,
                                sourceRowBytes: image.bytesPerRow, destination: o.baseAddress!, destinationWidth: width,
                                destinationHeight: height, destinationRowBytes: width * ch, channels: ch, highQuality: true)
            }
        }
        return CGImage(PortableImage(width: width, height: height, kind: image.isGrayPlane ? .mask : .rgba, bytesPerRow: width * ch, bytes: out))
    }
}
