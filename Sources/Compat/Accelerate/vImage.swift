// Accelerate (vImage subset) for Linux. Upstream uses vImage in three places: halving images with a Lanczos kernel
// (DownsampleCache), table lookups on 8-bit planes, and 4x4 matrix multiplies on premultiplied ARGB8888 (Invert).
// These are portable Swift implementations of exactly those entry points, with vImage's buffer/flag/error shapes.

import Foundation
import Dispatch

public typealias vImagePixelCount = UInt
public typealias vImage_Error = Int
public typealias vImage_Flags = UInt32
public typealias Pixel_8 = UInt8

public let kvImageNoError: vImage_Error = 0
public let kvImageMemoryAllocationError: vImage_Error = -21771
public let kvImageInvalidParameter: vImage_Error = -21773
public let kvImageBufferSizeMismatch: vImage_Error = -21776
public let kvImageNoFlags: Int = 0
public let kvImageDoNotTile: Int = 16
public let kvImageHighQualityResampling: Int = 32

public struct vImage_Buffer {
    public var data: UnsafeMutableRawPointer!
    public var height: vImagePixelCount
    public var width: vImagePixelCount
    public var rowBytes: Int
    public init() { data = nil; height = 0; width = 0; rowBytes = 0 }
    public init(data: UnsafeMutableRawPointer!, height: vImagePixelCount, width: vImagePixelCount, rowBytes: Int) {
        self.data = data; self.height = height; self.width = width; self.rowBytes = rowBytes
    }
}

// MARK: - Scaling

/// Resamples an interleaved 8-bit image. `highQuality` selects a Lanczos-3 kernel; otherwise a tent (bilinear /
/// area-weighted when shrinking). Both are separable and scale their support with the shrink ratio, so downsampling
/// filters properly instead of aliasing.
public func vImageResample8(source: UnsafeRawPointer, sourceWidth: Int, sourceHeight: Int, sourceRowBytes: Int,
                            destination: UnsafeMutableRawPointer, destinationWidth: Int, destinationHeight: Int,
                            destinationRowBytes: Int, channels: Int, highQuality: Bool) {
    guard sourceWidth > 0, sourceHeight > 0, destinationWidth > 0, destinationHeight > 0 else { return }
    let src = source.assumingMemoryBound(to: UInt8.self)
    let dst = destination.assumingMemoryBound(to: UInt8.self)

    func kernel(_ x: Double) -> Double {
        let ax = abs(x)
        if highQuality {
            if ax >= 3 { return 0 }
            if ax < 1e-9 { return 1 }
            let px = Double.pi * ax
            return 3 * sin(px) * sin(px / 3) / (px * px)
        }
        return ax >= 1 ? 0 : 1 - ax
    }
    let support = highQuality ? 3.0 : 1.0

    /// For each destination index: first source index, and normalised weights.
    func weights(from srcCount: Int, to dstCount: Int) -> [(start: Int, w: [Float])] {
        let scale = Double(srcCount) / Double(dstCount)
        let stretch = max(1, scale)
        return (0..<dstCount).map { i in
            let center = (Double(i) + 0.5) * scale - 0.5
            let lo = Int((center - support * stretch).rounded(.up)), hi = Int((center + support * stretch).rounded(.down))
            var ws: [Double] = []
            var total = 0.0
            for j in lo...max(lo, hi) { let w = kernel((Double(j) - center) / stretch); ws.append(w); total += w }
            if total == 0 { ws = [1]; total = 1 }
            // Clamp taps to the image edge (replicate), merging duplicate edge indices.
            var merged: [Int: Double] = [:]
            for (k, w) in ws.enumerated() { merged[min(max(lo + k, 0), srcCount - 1), default: 0] += w / total }
            let start = merged.keys.min()!, end = merged.keys.max()!
            return (start, (start...end).map { Float(merged[$0] ?? 0) })
        }
    }

    let hw = weights(from: sourceWidth, to: destinationWidth)
    let vw = weights(from: sourceHeight, to: destinationHeight)
    // Horizontal pass into float rows (all source rows), then vertical pass.
    var mid = [Float](repeating: 0, count: sourceHeight * destinationWidth * channels)
    for y in 0..<sourceHeight {
        let row = src + y * sourceRowBytes
        for x in 0..<destinationWidth {
            let (start, ws) = hw[x]
            for c in 0..<channels {
                var acc: Float = 0
                for (k, w) in ws.enumerated() { acc += w * Float(row[(start + k) * channels + c]) }
                mid[(y * destinationWidth + x) * channels + c] = acc
            }
        }
    }
    for y in 0..<destinationHeight {
        let (start, ws) = vw[y]
        let out = dst + y * destinationRowBytes
        for x in 0..<destinationWidth {
            for c in 0..<channels {
                var acc: Float = 0
                for (k, w) in ws.enumerated() { acc += w * mid[((start + k) * destinationWidth + x) * channels + c] }
                out[x * channels + c] = UInt8(max(0, min(255, acc.rounded())))
            }
        }
    }
}

private func scale(_ src: UnsafePointer<vImage_Buffer>, _ dest: UnsafePointer<vImage_Buffer>, channels: Int,
                   flags: vImage_Flags) -> vImage_Error {
    let s = src.pointee, d = dest.pointee
    guard s.data != nil, d.data != nil else { return kvImageInvalidParameter }
    vImageResample8(source: s.data, sourceWidth: Int(s.width), sourceHeight: Int(s.height), sourceRowBytes: s.rowBytes,
                    destination: d.data, destinationWidth: Int(d.width), destinationHeight: Int(d.height),
                    destinationRowBytes: d.rowBytes, channels: channels,
                    highQuality: Int(flags) & kvImageHighQualityResampling != 0)
    return kvImageNoError
}

@discardableResult
public func vImageScale_ARGB8888(_ src: UnsafePointer<vImage_Buffer>, _ dest: UnsafePointer<vImage_Buffer>,
                                 _ tempBuffer: UnsafeMutableRawPointer?, _ flags: vImage_Flags) -> vImage_Error {
    scale(src, dest, channels: 4, flags: flags)
}

@discardableResult
public func vImageScale_Planar8(_ src: UnsafePointer<vImage_Buffer>, _ dest: UnsafePointer<vImage_Buffer>,
                                _ tempBuffer: UnsafeMutableRawPointer?, _ flags: vImage_Flags) -> vImage_Error {
    scale(src, dest, channels: 1, flags: flags)
}

// MARK: - Table lookup and matrix multiply

@discardableResult
public func vImageTableLookUp_Planar8(_ src: UnsafePointer<vImage_Buffer>, _ dest: UnsafePointer<vImage_Buffer>,
                                      _ table: UnsafePointer<Pixel_8>, _ flags: vImage_Flags) -> vImage_Error {
    let s = src.pointee, d = dest.pointee
    guard s.data != nil, d.data != nil else { return kvImageInvalidParameter }
    guard s.width == d.width, s.height == d.height else { return kvImageBufferSizeMismatch }
    for y in 0..<Int(s.height) {
        let from = (s.data + y * s.rowBytes).assumingMemoryBound(to: UInt8.self)
        let to = (d.data + y * d.rowBytes).assumingMemoryBound(to: UInt8.self)
        for x in 0..<Int(s.width) { to[x] = table[Int(from[x])] }
    }
    return kvImageNoError
}

/// `dest[j] = clamp((Σ_i (src[i] + preBias[i]) * matrix[i*4 + j]) / divisor + postBias[j] / divisor)`. The matrix is
/// row-major, applied as pixel × matrix over the four interleaved channels in memory order (vImage's convention).
@discardableResult
public func vImageMatrixMultiply_ARGB8888(_ src: UnsafePointer<vImage_Buffer>, _ dest: UnsafePointer<vImage_Buffer>,
                                          _ matrix: UnsafePointer<Int16>, _ divisor: Int32,
                                          _ preBias: UnsafePointer<Int16>?, _ postBias: UnsafePointer<Int32>?,
                                          _ flags: vImage_Flags) -> vImage_Error {
    let s = src.pointee, d = dest.pointee
    guard s.data != nil, d.data != nil, divisor != 0 else { return kvImageInvalidParameter }
    guard s.width == d.width, s.height == d.height else { return kvImageBufferSizeMismatch }
    let m0 = Int32(matrix[0]), m1 = Int32(matrix[1]), m2 = Int32(matrix[2]), m3 = Int32(matrix[3])
    let m4 = Int32(matrix[4]), m5 = Int32(matrix[5]), m6 = Int32(matrix[6]), m7 = Int32(matrix[7])
    let m8 = Int32(matrix[8]), m9 = Int32(matrix[9]), m10 = Int32(matrix[10]), m11 = Int32(matrix[11])
    let m12 = Int32(matrix[12]), m13 = Int32(matrix[13]), m14 = Int32(matrix[14]), m15 = Int32(matrix[15])

    let pre0 = preBias.map { Int32($0[0]) } ?? 0
    let pre1 = preBias.map { Int32($0[1]) } ?? 0
    let pre2 = preBias.map { Int32($0[2]) } ?? 0
    let pre3 = preBias.map { Int32($0[3]) } ?? 0

    let post0 = postBias.map { $0[0] } ?? 0
    let post1 = postBias.map { $0[1] } ?? 0
    let post2 = postBias.map { $0[2] } ?? 0
    let post3 = postBias.map { $0[3] } ?? 0

    let halfDivisor = divisor / 2
    let width = Int(s.width)
    let height = Int(s.height)

    // Fast-path: premultiplied invert (the exact matrix PixelInvert uses).
    let isInvert = divisor == 256 &&
        m0 == -256 && m1 == 0 && m2 == 0 && m3 == 0 &&
        m4 == 0 && m5 == -256 && m6 == 0 && m7 == 0 &&
        m8 == 0 && m9 == 0 && m10 == -256 && m11 == 0 &&
        m12 == 256 && m13 == 256 && m14 == 256 && m15 == 256 &&
        pre0 == 0 && pre1 == 0 && pre2 == 0 && pre3 == 0 &&
        post0 == 0 && post1 == 0 && post2 == 0 && post3 == 0

    let chunks = min(height, max(1, ProcessInfo.processInfo.activeProcessorCount * 2))
    let chunkSize = (height + chunks - 1) / chunks

    if isInvert {
        DispatchQueue.concurrentPerform(iterations: chunks) { c in
            let startY = c * chunkSize
            guard startY < height else { return }
            let endY = min(height, startY + chunkSize)
            for y in startY..<endY {
                var srcP = (s.data + y * s.rowBytes).assumingMemoryBound(to: UInt8.self)
                var dstP = (d.data + y * d.rowBytes).assumingMemoryBound(to: UInt8.self)
                for _ in 0..<width {
                    let a = srcP[3]
                    dstP[0] = a - srcP[0]
                    dstP[1] = a - srcP[1]
                    dstP[2] = a - srcP[2]
                    dstP[3] = a
                    srcP += 4
                    dstP += 4
                }
            }
        }
        return kvImageNoError
    }

    if divisor == 256 {
        DispatchQueue.concurrentPerform(iterations: chunks) { c in
            let startY = c * chunkSize
            guard startY < height else { return }
            let endY = min(height, startY + chunkSize)
            for y in startY..<endY {
                var srcP = (s.data + y * s.rowBytes).assumingMemoryBound(to: UInt8.self)
                var dstP = (d.data + y * d.rowBytes).assumingMemoryBound(to: UInt8.self)
                for _ in 0..<width {
                    let px0 = Int32(srcP[0]) + pre0
                    let px1 = Int32(srcP[1]) + pre1
                    let px2 = Int32(srcP[2]) + pre2
                    let px3 = Int32(srcP[3]) + pre3

                    let sum0 = px0 * m0 + px1 * m4 + px2 * m8 + px3 * m12 + post0
                    let sum1 = px0 * m1 + px1 * m5 + px2 * m9 + px3 * m13 + post1
                    let sum2 = px0 * m2 + px1 * m6 + px2 * m10 + px3 * m14 + post2
                    let sum3 = px0 * m3 + px1 * m7 + px2 * m11 + px3 * m15 + post3

                    let v0 = sum0 >= 0 ? (sum0 + 128) >> 8 : -((-sum0 + 128) >> 8)
                    let v1 = sum1 >= 0 ? (sum1 + 128) >> 8 : -((-sum1 + 128) >> 8)
                    let v2 = sum2 >= 0 ? (sum2 + 128) >> 8 : -((-sum2 + 128) >> 8)
                    let v3 = sum3 >= 0 ? (sum3 + 128) >> 8 : -((-sum3 + 128) >> 8)

                    dstP[0] = UInt8(max(0, min(255, v0)))
                    dstP[1] = UInt8(max(0, min(255, v1)))
                    dstP[2] = UInt8(max(0, min(255, v2)))
                    dstP[3] = UInt8(max(0, min(255, v3)))

                    srcP += 4
                    dstP += 4
                }
            }
        }
    } else {
        DispatchQueue.concurrentPerform(iterations: chunks) { c in
            let startY = c * chunkSize
            guard startY < height else { return }
            let endY = min(height, startY + chunkSize)
            for y in startY..<endY {
                var srcP = (s.data + y * s.rowBytes).assumingMemoryBound(to: UInt8.self)
                var dstP = (d.data + y * d.rowBytes).assumingMemoryBound(to: UInt8.self)
                for _ in 0..<width {
                    let px0 = Int32(srcP[0]) + pre0
                    let px1 = Int32(srcP[1]) + pre1
                    let px2 = Int32(srcP[2]) + pre2
                    let px3 = Int32(srcP[3]) + pre3

                    let sum0 = px0 * m0 + px1 * m4 + px2 * m8 + px3 * m12 + post0
                    let sum1 = px0 * m1 + px1 * m5 + px2 * m9 + px3 * m13 + post1
                    let sum2 = px0 * m2 + px1 * m6 + px2 * m10 + px3 * m14 + post2
                    let sum3 = px0 * m3 + px1 * m7 + px2 * m11 + px3 * m15 + post3

                    let v0 = sum0 >= 0 ? (sum0 + halfDivisor) / divisor : -((-sum0 + halfDivisor) / divisor)
                    let v1 = sum1 >= 0 ? (sum1 + halfDivisor) / divisor : -((-sum1 + halfDivisor) / divisor)
                    let v2 = sum2 >= 0 ? (sum2 + halfDivisor) / divisor : -((-sum2 + halfDivisor) / divisor)
                    let v3 = sum3 >= 0 ? (sum3 + halfDivisor) / divisor : -((-sum3 + halfDivisor) / divisor)

                    dstP[0] = UInt8(max(0, min(255, v0)))
                    dstP[1] = UInt8(max(0, min(255, v1)))
                    dstP[2] = UInt8(max(0, min(255, v2)))
                    dstP[3] = UInt8(max(0, min(255, v3)))

                    srcP += 4
                    dstP += 4
                }
            }
        }
    }
    return kvImageNoError
}
