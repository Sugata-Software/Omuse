// ImageCodec.swift — the seam under CGImageSource / CGImageDestination.
//
// Decoding and encoding are pluggable (dependency inversion): the Qt host registers its image plugins (PNG, JPEG,
// TIFF, WebP, ...) through `compositor_imageio_register`; without one, a portable pure-Swift PNG codec still lets
// projects, exports and tests round-trip. A backend that does not know a format returns nil.

import Foundation
import CompatSupport
#if canImport(Glibc)
import Glibc
#endif
import CoreGraphics
import UniformTypeIdentifiers

public struct ImageInfo: Sendable {
    public var typeIdentifier: String
    public var width: Int
    public var height: Int
    public var bitDepth: Int = 8
    public var dpi: Double?
    /// EXIF orientation 1...8.
    public var orientation: Int32 = 1
    public var hasAlpha = true
    public init(typeIdentifier: String, width: Int, height: Int, bitDepth: Int = 8, dpi: Double? = nil,
                orientation: Int32 = 1, hasAlpha: Bool = true) {
        self.typeIdentifier = typeIdentifier; self.width = width; self.height = height; self.bitDepth = bitDepth
        self.dpi = dpi; self.orientation = orientation; self.hasAlpha = hasAlpha
    }
}

public protocol ImageCodecBackend: AnyObject {
    /// Cheap header read; nil when the format is not recognised.
    func identify(_ data: Data) -> ImageInfo?
    /// Full decode to 8-bit premultiplied RGBA (or an 8-bit gray plane as a mask image).
    func decode(_ data: Data) -> CGImage?
    /// `quality` is 0...1 for lossy formats; `dpi` is written when the format can carry it.
    func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?) -> Data?
    /// Like `encode` but also records an EXIF orientation (1...8) in the file when the format can carry one.
    func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?, orientation: Int32) -> Data?
}

public extension ImageCodecBackend {
    func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?, orientation: Int32) -> Data? {
        encode(image, typeIdentifier: typeIdentifier, quality: quality, dpi: dpi)
    }
}

public enum ImageCodecRegistry {
    /// Tried first; falls through to the portable codec when it does not handle a format.
    public static let hostSlot = ServiceSlot<ImageCodecBackend>()
    public static var host: ImageCodecBackend? {
        get { hostSlot.installedValue }
        set { hostSlot.install(newValue) }
    }
    /// Runs `body` with `codec` as the host backend (nil: only the portable codecs), then restores the previous one.
    public static func withHost<Result>(_ codec: ImageCodecBackend?, _ body: () throws -> Result) rethrows -> Result {
        try hostSlot.withOverride(codec, body)
    }
    public static let portable: ImageCodecBackend = PortablePNGCodec()

    static var backends: [ImageCodecBackend] {
        _ = autoloaded
        return host.map { [$0, portable, gif] } ?? [portable, gif]
    }
    static let gif: ImageCodecBackend = PortableGIFEncoder()

    /// Without a host-registered backend, look for the Qt codec library (`COMPOSITOR_IMAGEIO_BACKEND`, then the
    /// system loader path and the Flatpak install) once, the way the Skia bridge is found.
    private static let autoloaded: Bool = {
        guard host == nil else { return false }
        #if canImport(Glibc)
        let env = ProcessInfo.processInfo.environment["COMPOSITOR_IMAGEIO_BACKEND"] ?? ""
        // Environment override, the system loader's path, or the Flatpak install: never the working directory.
        let candidates = [env, "libCompositorQtImageIO.so", "/app/lib/libCompositorQtImageIO.so"]
        typealias Functions = @convention(c) (UnsafeMutablePointer<CompositorImageDecodeFn?>?, UnsafeMutablePointer<CompositorImageEncodeFn?>?) -> Int32
        for path in candidates where !path.isEmpty {
            guard let handle = dlopen(path, RTLD_NOW | RTLD_LOCAL), let sym = dlsym(handle, "compositor_qt_imageio_functions") else { continue }
            var decode: CompositorImageDecodeFn?, encode: CompositorImageEncodeFn?
            if unsafeBitCast(sym, to: Functions.self)(&decode, &encode) == 1, let decode, let encode {
                host = CCallbackCodec(decode: decode, encode: encode)
                return true
            }
        }
        #endif
        return false
    }()

    public static func identify(_ data: Data) -> ImageInfo? {
        for b in backends { if let info = b.identify(data) { return info } }
        return Self.sniff(data)
    }
    public static func decode(_ data: Data) -> CGImage? {
        for b in backends { if let image = b.decode(data) { return image } }
        return nil
    }
    public static func encode(_ image: CGImage, typeIdentifier: String, quality: Double? = nil, dpi: Double? = nil, orientation: Int32 = 1) -> Data? {
        for b in backends { if let data = b.encode(image, typeIdentifier: typeIdentifier, quality: quality, dpi: dpi, orientation: orientation) { return data } }
        return nil
    }

    /// Format recognition by magic bytes, for when no backend can read the header (type known, size not).
    static func sniff(_ d: Data) -> ImageInfo? {
        let b = [UInt8](d.prefix(12))
        func info(_ uti: String) -> ImageInfo { ImageInfo(typeIdentifier: uti, width: 0, height: 0) }
        if b.count >= 8, b[0...7] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] { return info("public.png") }
        if b.count >= 3, b[0] == 0xFF, b[1] == 0xD8, b[2] == 0xFF { return info("public.jpeg") }
        if b.count >= 4, b[0...3] == [0x49, 0x49, 0x2A, 0x00] || b[0...3] == [0x4D, 0x4D, 0x00, 0x2A] { return info("public.tiff") }
        if b.count >= 6, String(bytes: b[0...2], encoding: .ascii) == "GIF" { return info("com.compuserve.gif") }
        if b.count >= 2, b[0] == 0x42, b[1] == 0x4D { return info("com.microsoft.bmp") }
        if b.count >= 12, String(bytes: b[0...3], encoding: .ascii) == "RIFF", String(bytes: b[8...11], encoding: .ascii) == "WEBP" { return info("org.webmproject.webp") }
        if b.count >= 12, String(bytes: b[4...11], encoding: .ascii)?.hasPrefix("ftypheic") == true { return info("public.heic") }
        return nil
    }
}

// MARK: - C registration (the Qt host installs its codecs here)

/// Decode: returns 0 on success and hands back a malloc'd premultiplied-RGBA buffer (freed by the callee's caller
/// with `free`). `channels` is 4 for colour or 1 for a gray plane. `uti` receives a NUL-terminated identifier.
public typealias CompositorImageDecodeFn = @convention(c) (
    UnsafePointer<UInt8>?, Int, UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?, UnsafeMutablePointer<Int32>?,
    UnsafeMutablePointer<Int32>?, UnsafeMutablePointer<Int32>?, UnsafeMutablePointer<Double>?, UnsafeMutablePointer<Int32>?,
    UnsafeMutablePointer<CChar>?, Int) -> Int32
/// Encode: `pixels` premultiplied RGBA (channels 4) or gray (1). Returns 0 and a malloc'd buffer on success.
public typealias CompositorImageEncodeFn = @convention(c) (
    UnsafePointer<UInt8>?, Int32, Int32, Int32, UnsafePointer<CChar>?, Double, Double, Int32,
    UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?, UnsafeMutablePointer<Int>?) -> Int32

final class CCallbackCodec: ImageCodecBackend {
    let decodeFn: CompositorImageDecodeFn, encodeFn: CompositorImageEncodeFn
    init(decode: @escaping CompositorImageDecodeFn, encode: @escaping CompositorImageEncodeFn) { decodeFn = decode; encodeFn = encode }

    private func run(_ data: Data) -> (pixels: [UInt8], w: Int, h: Int, channels: Int, dpi: Double, orientation: Int32, uti: String)? {
        var out: UnsafeMutablePointer<UInt8>?
        var w: Int32 = 0, h: Int32 = 0, channels: Int32 = 0, orientation: Int32 = 1
        var dpi: Double = 0
        var uti = [CChar](repeating: 0, count: 96)
        let rc = data.withUnsafeBytes { raw in
            decodeFn(raw.bindMemory(to: UInt8.self).baseAddress, data.count, &out, &w, &h, &channels, &dpi, &orientation, &uti, uti.count)
        }
        guard rc == 0, let out, w > 0, h > 0, channels == 1 || channels == 4 else { return nil }
        defer { free(out) }
        let count = Int(w) * Int(h) * Int(channels)
        return (Array(UnsafeBufferPointer(start: out, count: count)), Int(w), Int(h), Int(channels), dpi, orientation, String(cString: uti))
    }

    func identify(_ data: Data) -> ImageInfo? {
        guard let r = run(data) else { return nil }
        return ImageInfo(typeIdentifier: r.uti, width: r.w, height: r.h, bitDepth: 8, dpi: r.dpi > 0 ? r.dpi : nil,
                         orientation: r.orientation, hasAlpha: r.channels == 4)
    }
    func decode(_ data: Data) -> CGImage? {
        guard let r = run(data) else { return nil }
        let kind: PortableImage.Kind = r.channels == 4 ? .rgba : .mask
        return CGImage(PortableImage(width: r.w, height: r.h, kind: kind, bytesPerRow: r.w * r.channels, bytes: r.pixels))
    }
    func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?) -> Data? {
        encode(image, typeIdentifier: typeIdentifier, quality: quality, dpi: dpi, orientation: 1)
    }
    func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?, orientation: Int32) -> Data? {
        var out: UnsafeMutablePointer<UInt8>?
        var length = 0
        let channels: Int32 = image.isGrayPlane ? 1 : 4
        let rc = image.portableImage.bytes.withUnsafeBufferPointer { px in
            typeIdentifier.withCString { uti in
                encodeFn(px.baseAddress, Int32(image.width), Int32(image.height), channels, uti, quality ?? -1, dpi ?? 0, orientation, &out, &length)
            }
        }
        guard rc == 0, let out, length > 0 else { return nil }
        defer { free(out) }
        return Data(bytes: out, count: length)
    }
}

/// Called by the Qt host at startup with its QImageReader/QImageWriter-backed callbacks.
@_cdecl("compositor_imageio_register")
public func compositor_imageio_register(_ decode: CompositorImageDecodeFn?, _ encode: CompositorImageEncodeFn?) {
    guard let decode, let encode else { ImageCodecRegistry.host = nil; return }
    ImageCodecRegistry.host = CCallbackCodec(decode: decode, encode: encode)
}
