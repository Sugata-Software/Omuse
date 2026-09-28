// CompositorRaster — portable canonical pixel buffers for the Linux core.
//
// The macOS app stores pixels in CGImage / CGContext. Foundation on Linux has no
// raster image types, so this file provides the storage substrate the file-map
// "Keep logic; replace Apple operations" and "Apple replacement/adaptation" tiers
// build on: an immutable image value (`PortableImage`) backed by the canonical
// tile contract, and the mutable buffers it is built from.
//
// Canonical contract (see docs/linux-port-plan.md and include/CompositorCore.h):
//   - 8-bit premultiplied RGBA, channel byte order R,G,B,A
//   - stride == width * 4 (no padding)
//   - separate 8-bit grayscale coverage/mask buffer, stride == width
//   - immutable committed tiles; mutation produces a new buffer
//
// Scope: storage and rectangular cropping only. The full 2D drawing API
// (fill/stroke/paths/gradients/blend modes/transparency layers that CGContext
// provides) is a later, Skia-backed milestone — it is not reimplemented here. The
// pixel-math and document-model layers that use an image as storage (width,
// height, bytesPerRow, bytes, cropping) are unblocked by this substrate.
//
// SOLID: buffers are value types with a single responsibility (owning pixels); the
// immutable `PortableImage` surface (ISP) lets higher layers depend on the
// contract, not the backing (DIP). Foundation-only; builds on Linux via SwiftPM.

import Foundation

/// Mutable 8-bit premultiplied RGBA buffer, canonical contract (stride == width*4,
/// channel order R,G,B,A). Used to build tiles; committed tiles become an immutable
/// `PortableImage`.
public struct PixelBuffer {
    public let width: Int
    public let height: Int
    /// Owned pixel storage; mutable until the buffer is committed as an
    /// immutable `PortableImage` (value semantics make every assignment a copy).
    public var bytes: [UInt8]

    public var bytesPerRow: Int { width * 4 }

    public init(width: Int, height: Int, fill: UInt32 = 0) {
        precondition(width > 0 && height > 0, "PixelBuffer must be positive-sized")
        self.width = width
        self.height = height
        self.bytes = [UInt8](repeating: 0, count: width * height * 4)
        if fill != 0 {
            let r = UInt8((fill >> 24) & 0xFF), g = UInt8((fill >> 16) & 0xFF)
            let b = UInt8((fill >> 8) & 0xFF), a = UInt8(fill & 0xFF)
            for i in stride(from: 0, to: bytes.count, by: 4) {
                bytes[i] = r; bytes[i + 1] = g; bytes[i + 2] = b; bytes[i + 3] = a
            }
        }
    }

    /// Wrap an existing packed RGBA byte array (no copy semantics issues: owned).
    public init(width: Int, height: Int, bytes: [UInt8]) {
        precondition(width > 0 && height > 0)
        precondition(bytes.count == width * height * 4, "PixelBuffer byte count must equal width*height*4")
        self.width = width
        self.height = height
        self.bytes = bytes
    }

    public subscript(x: Int, y: Int) -> (r: UInt8, g: UInt8, b: UInt8, a: UInt8) {
        get {
            let i = (y * width + x) * 4
            return (bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3])
        }
        set {
            let i = (y * width + x) * 4
            bytes[i] = newValue.r; bytes[i + 1] = newValue.g
            bytes[i + 2] = newValue.b; bytes[i + 3] = newValue.a
        }
    }

    public func withUnsafeBytes<R>(_ body: (UnsafePointer<UInt8>) throws -> R) rethrows -> R {
        try bytes.withUnsafeBufferPointer { ptr in try body(ptr.baseAddress!) }
    }
    public mutating func withUnsafeMutableBytes<R>(_ body: (UnsafeMutablePointer<UInt8>) throws -> R) rethrows -> R {
        try bytes.withUnsafeMutableBufferPointer { ptr in try body(ptr.baseAddress!) }
    }
}

/// Mutable 8-bit grayscale coverage/mask buffer (stride == width). The canonical
/// separate-coverage half of the tile contract.
public struct MaskBuffer {
    public let width: Int
    public let height: Int
    /// Owned pixel storage; mutable until committed into a `PortableImage`.
    public var bytes: [UInt8]

    public var bytesPerRow: Int { width }

    public init(width: Int, height: Int, fill: UInt8 = 0) {
        precondition(width > 0 && height > 0)
        self.width = width
        self.height = height
        self.bytes = [UInt8](repeating: fill, count: width * height)
    }
    public init(width: Int, height: Int, bytes: [UInt8]) {
        precondition(width > 0 && height > 0)
        precondition(bytes.count == width * height)
        self.width = width; self.height = height; self.bytes = bytes
    }

    public subscript(x: Int, y: Int) -> UInt8 {
        get { bytes[y * width + x] }
        set { bytes[y * width + x] = newValue }
    }

    public func withUnsafeBytes<R>(_ body: (UnsafePointer<UInt8>) throws -> R) rethrows -> R {
        try bytes.withUnsafeBufferPointer { ptr in try body(ptr.baseAddress!) }
    }
}

/// Immutable image value backed by a canonical pixel buffer. Replaces `CGImage` as
/// *storage* (width, height, bytesPerRow, bytes, cropping) for the Linux core. The
/// macOS `CGImage` drawing/decode surface is not reimplemented; compositing and
/// drawing land on Skia in a later milestone. This is the storage surface the
/// document model and pixel-math layers hold.
public struct PortableImage: Equatable {
    public enum Kind: Equatable { case rgba, mask }
    public let width: Int
    public let height: Int
    public let kind: Kind
    public let bytesPerRow: Int
    public let bytes: [UInt8]

    public var bytesPerPixel: Int { kind == .rgba ? 4 : 1 }

    public init(_ buffer: PixelBuffer) {
        self.width = buffer.width
        self.height = buffer.height
        self.kind = .rgba
        self.bytesPerRow = buffer.bytesPerRow
        self.bytes = buffer.bytes
    }
    public init(_ buffer: MaskBuffer) {
        self.width = buffer.width
        self.height = buffer.height
        self.kind = .mask
        self.bytesPerRow = buffer.bytesPerRow
        self.bytes = buffer.bytes
    }

    /// Cropping returns the sub-image for the intersection of `rect` and the image
    /// bounds; nil if disjoint. Matches the `CGImage.cropping(to:)` contract the
    /// macOS raster layer relies on (nil on no overlap).
    public func cropping(to rect: CGRect) -> PortableImage? {
        if rect.isNull || rect.isEmpty { return nil }
        let ix = Int(floor(rect.minX)), iy = Int(floor(rect.minY))
        let iw = Int(ceil(rect.maxX)) - ix, ih = Int(ceil(rect.maxY)) - iy
        // Intersect with image bounds in integer pixels.
        let bx = max(0, ix), by = max(0, iy)
        let ex = min(width, ix + iw), ey = min(height, iy + ih)
        let cw = ex - bx, ch = ey - by
        guard cw > 0, ch > 0 else { return nil }
        var out = [UInt8](repeating: 0, count: cw * ch * bytesPerPixel)
        for y in 0..<ch {
            let srcRow = (by + y) * bytesPerRow + bx * bytesPerPixel
            let dstRow = y * cw * bytesPerPixel
            for b in 0..<(cw * bytesPerPixel) { out[dstRow + b] = bytes[srcRow + b] }
        }
        return PortableImage(width: cw, height: ch, kind: kind,
                             bytesPerRow: cw * bytesPerPixel, bytes: out)
    }

    public init(width: Int, height: Int, kind: Kind, bytesPerRow: Int, bytes: [UInt8]) {
        self.width = width; self.height = height; self.kind = kind
        self.bytesPerRow = bytesPerRow; self.bytes = bytes
    }
}