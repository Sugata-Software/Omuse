// CoreVideo: the pixel-buffer type Vision hands back (a segmentation mask). Only the size accessors and the raw
// plane are needed; formats are the one-component 8-bit and 32-bit float masks.

import Foundation

public struct OSType: RawRepresentable, Hashable, Sendable, ExpressibleByIntegerLiteral {
    public let rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }
    public init(integerLiteral value: UInt32) { rawValue = value }
}
public let kCVPixelFormatType_OneComponent8: OSType = 0x4C303038   // 'L008'
public let kCVPixelFormatType_OneComponent32Float: OSType = 0x4C303066  // 'L00f'

public final class CVPixelBuffer: @unchecked Sendable {
    public let width: Int
    public let height: Int
    public let pixelFormat: OSType
    /// Row-major top-down. 8-bit planes use `bytes`; float planes use `floats`.
    public internal(set) var bytes: [UInt8]
    public internal(set) var floats: [Float]

    public init(width: Int, height: Int, gray8 bytes: [UInt8]) {
        precondition(bytes.count == width * height)
        self.width = width; self.height = height; pixelFormat = kCVPixelFormatType_OneComponent8
        self.bytes = bytes; floats = []
    }
    public init(width: Int, height: Int, gray32Float floats: [Float]) {
        precondition(floats.count == width * height)
        self.width = width; self.height = height; pixelFormat = kCVPixelFormatType_OneComponent32Float
        bytes = []; self.floats = floats
    }
    /// One value 0...1 per pixel regardless of storage.
    public var normalized: [Float] {
        pixelFormat == kCVPixelFormatType_OneComponent8 ? bytes.map { Float($0) / 255 } : floats
    }
}

public func CVPixelBufferGetWidth(_ buffer: CVPixelBuffer) -> Int { buffer.width }
public func CVPixelBufferGetHeight(_ buffer: CVPixelBuffer) -> Int { buffer.height }
public func CVPixelBufferGetPixelFormatType(_ buffer: CVPixelBuffer) -> OSType { buffer.pixelFormat }
