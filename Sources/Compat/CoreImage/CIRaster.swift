// CIRaster.swift — float pixel rasters and the filter maths behind the CoreImage shim.
//
// Core Image's coordinate system has its origin at the bottom-left with y up; a raster's row 0 is y = rect.minY.
// Pixels are premultiplied RGBA floats in 0...1. Colour management: when `linear` evaluation is requested, sources
// are converted from sRGB to linear light on entry and back on exit, as Core Image's default working space does.

import Foundation
import CoreGraphics
import Dispatch

struct Raster {
    var rect: CGRect              // integral, in CI coordinates
    var width: Int { Int(rect.width) }
    var height: Int { Int(rect.height) }
    var data: [Float]

    init(rect: CGRect) {
        self.rect = rect
        data = [Float](repeating: 0, count: max(0, Int(rect.width) * Int(rect.height) * 4))
    }
    init(rect: CGRect, uninitialized: Bool) {
        self.rect = rect
        let count = max(0, Int(rect.width) * Int(rect.height) * 4)
        if uninitialized {
            data = Array<Float>(unsafeUninitializedCapacity: count) { _, initializedCount in
                initializedCount = count
            }
        } else {
            data = [Float](repeating: 0, count: count)
        }
    }
    init(rect: CGRect, data: [Float]) { self.rect = rect; self.data = data }

    @inline(__always) func index(_ x: Int, _ y: Int) -> Int { (y * width + x) * 4 }

    /// Copy of `region` (which may extend beyond this raster; outside is transparent).
    func cropped(to region: CGRect) -> Raster {
        if rect == region { return self }
        var out = Raster(rect: region)
        let inter = region.intersection(rect)
        guard !inter.isNull, inter.width > 0, inter.height > 0 else { return out }
        let x0 = Int(inter.minX - rect.minX), y0 = Int(inter.minY - rect.minY)
        let ox = Int(inter.minX - region.minX), oy = Int(inter.minY - region.minY)
        let copyBytes = Int(inter.width) * 4 * MemoryLayout<Float>.stride
        let h = Int(inter.height)
        let outWidth = out.width
        out.data.withUnsafeMutableBufferPointer { outBuf in
            self.data.withUnsafeBufferPointer { srcBuf in
                guard let dstPtr = outBuf.baseAddress, let srcPtr = srcBuf.baseAddress else { return }
                for y in 0..<h {
                    let s = index(x0, y0 + y)
                    let d = ((oy + y) * outWidth + ox) * 4
                    memcpy(dstPtr + d, srcPtr + s, copyBytes)
                }
            }
        }
        return out
    }

    /// Pixel at integer position, edge-replicated.
    @inline(__always) func clamped(_ x: Int, _ y: Int) -> (Float, Float, Float, Float) {
        let cx = min(max(x, 0), width - 1), cy = min(max(y, 0), height - 1)
        let i = index(cx, cy)
        return (data[i], data[i + 1], data[i + 2], data[i + 3])
    }

    /// Bilinear sample at continuous device coordinates (pixel centres at +0.5); outside is transparent.
    func bilinear(_ fx: Double, _ fy: Double) -> (Float, Float, Float, Float) {
        let x = fx - 0.5, y = fy - 0.5
        let x0 = Int(floor(x)), y0 = Int(floor(y))
        let tx = Float(x - Double(x0)), ty = Float(y - Double(y0))
        func px(_ ix: Int, _ iy: Int) -> (Float, Float, Float, Float) {
            guard ix >= 0, iy >= 0, ix < width, iy < height else { return (0, 0, 0, 0) }
            let i = index(ix, iy); return (data[i], data[i + 1], data[i + 2], data[i + 3])
        }
        let a = px(x0, y0), b = px(x0 + 1, y0), c = px(x0, y0 + 1), d = px(x0 + 1, y0 + 1)
        func mix(_ p: Float, _ q: Float, _ r: Float, _ s: Float) -> Float { (p * (1 - tx) + q * tx) * (1 - ty) + (r * (1 - tx) + s * tx) * ty }
        return (mix(a.0, b.0, c.0, d.0), mix(a.1, b.1, c.1, d.1), mix(a.2, b.2, c.2, d.2), mix(a.3, b.3, c.3, d.3))
    }
}

enum Gamma {
    static func toLinear(_ v: Float) -> Float { v <= 0.04045 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4) }
    static func toSRGB(_ v: Float) -> Float { v <= 0.0031308 ? v * 12.92 : 1.055 * pow(v, 1 / 2.4) - 0.055 }
    /// Applies `f` to the colour channels of every (unpremultiplied) pixel and re-premultiplies.
    static func map(_ r: inout Raster, _ f: (Float) -> Float) {
        var i = 0
        while i < r.data.count {
            let a = r.data[i + 3]
            if a > 0 {
                for c in 0..<3 { r.data[i + c] = f(min(1, max(0, r.data[i + c] / a))) * a }
            }
            i += 4
        }
    }
}

// MARK: - Filters over rasters

enum RasterFilters {
    /// Separable Gaussian. The input raster already includes the margin the kernel needs.
    static func gaussian(_ input: Raster, sigma: Double, output region: CGRect) -> Raster {
        let radius = Int(ceil(sigma * 3))
        guard radius > 0, sigma > 0 else { return input.cropped(to: region) }
        var kernel = (-radius...radius).map { Float(exp(-Double($0 * $0) / (2 * sigma * sigma))) }
        let total = kernel.reduce(0, +); kernel = kernel.map { $0 / total }
        let w = input.width, h = input.height
        // Horizontal (edge pixels of the requested raster are treated as transparent beyond the margin).
        var tmp = [Float](repeating: 0, count: w * h * 4)
        for y in 0..<h {
            for x in 0..<w {
                var acc: (Float, Float, Float, Float) = (0, 0, 0, 0)
                for k in -radius...radius {
                    let sx = x + k
                    guard sx >= 0, sx < w else { continue }
                    let i = input.index(sx, y), wgt = kernel[k + radius]
                    acc.0 += input.data[i] * wgt; acc.1 += input.data[i + 1] * wgt; acc.2 += input.data[i + 2] * wgt; acc.3 += input.data[i + 3] * wgt
                }
                let o = (y * w + x) * 4
                tmp[o] = acc.0; tmp[o + 1] = acc.1; tmp[o + 2] = acc.2; tmp[o + 3] = acc.3
            }
        }
        var out = Raster(rect: input.rect)
        for y in 0..<h {
            for x in 0..<w {
                var acc: (Float, Float, Float, Float) = (0, 0, 0, 0)
                for k in -radius...radius {
                    let sy = y + k
                    guard sy >= 0, sy < h else { continue }
                    let i = (sy * w + x) * 4, wgt = kernel[k + radius]
                    acc.0 += tmp[i] * wgt; acc.1 += tmp[i + 1] * wgt; acc.2 += tmp[i + 2] * wgt; acc.3 += tmp[i + 3] * wgt
                }
                let o = out.index(x, y)
                out.data[o] = acc.0; out.data[o + 1] = acc.1; out.data[o + 2] = acc.2; out.data[o + 3] = acc.3
            }
        }
        return out.cropped(to: region)
    }

    /// Straight-line blur of length 2*radius through each pixel at `angle` radians (CIMotionBlur).
    static func motionBlur(_ input: Raster, radius: Double, angle: Double, output region: CGRect) -> Raster {
        var out = Raster(rect: region)
        let taps = max(1, Int(ceil(radius * 2)))
        let dx = cos(angle), dy = sin(angle)
        for y in 0..<out.height {
            for x in 0..<out.width {
                let cx = Double(out.rect.minX - input.rect.minX) + Double(x) + 0.5
                let cy = Double(out.rect.minY - input.rect.minY) + Double(y) + 0.5
                var acc: (Float, Float, Float, Float) = (0, 0, 0, 0)
                for t in 0...taps {
                    let s = (Double(t) / Double(taps) * 2 - 1) * radius
                    let p = input.bilinear(cx + s * dx, cy + s * dy)
                    acc.0 += p.0; acc.1 += p.1; acc.2 += p.2; acc.3 += p.3
                }
                let n = Float(taps + 1), o = out.index(x, y)
                out.data[o] = acc.0 / n; out.data[o + 1] = acc.1 / n; out.data[o + 2] = acc.2 / n; out.data[o + 3] = acc.3 / n
            }
        }
        return out
    }

    /// CIColorMatrix: on unpremultiplied colour, out = M * rgba + bias, clamped, premultiplied again.
    static func colorMatrix(_ input: Raster, r: [Float], g: [Float], b: [Float], a: [Float], bias: [Float]) -> Raster {
        var out = input
        var i = 0
        while i < out.data.count {
            let alpha = input.data[i + 3]
            let c: [Float] = alpha > 0 ? [input.data[i] / alpha, input.data[i + 1] / alpha, input.data[i + 2] / alpha, alpha] : [0, 0, 0, 0]
            func dot(_ v: [Float], _ bias: Float) -> Float { min(1, max(0, v[0] * c[0] + v[1] * c[1] + v[2] * c[2] + v[3] * c[3] + bias)) }
            let na = dot(a, bias[3])
            out.data[i] = dot(r, bias[0]) * na; out.data[i + 1] = dot(g, bias[1]) * na
            out.data[i + 2] = dot(b, bias[2]) * na; out.data[i + 3] = na
            i += 4
        }
        return out
    }

    /// CIColorClamp: clamps each unpremultiplied component into [min, max].
    static func colorClamp(_ input: Raster, minimum: [Float], maximum: [Float]) -> Raster {
        var out = input
        var i = 0
        while i < out.data.count {
            let alpha = input.data[i + 3]
            var c: [Float] = alpha > 0 ? [input.data[i] / alpha, input.data[i + 1] / alpha, input.data[i + 2] / alpha, alpha] : [0, 0, 0, 0]
            for k in 0..<4 { c[k] = min(maximum[k], max(minimum[k], c[k])) }
            out.data[i] = c[0] * c[3]; out.data[i + 1] = c[1] * c[3]; out.data[i + 2] = c[2] * c[3]; out.data[i + 3] = c[3]
            i += 4
        }
        return out
    }

    /// CIColorCube: trilinear lookup into an N^3 table of premultiplied-in / RGBA float entries (r fastest).
    static func colorCube(_ input: Raster, dimension n: Int, cube: [Float]) -> Raster {
        guard n >= 2, cube.count >= n * n * n * 4 else { return input }
        var out = input
        func entry(_ r: Int, _ g: Int, _ b: Int, _ c: Int) -> Float { cube[((b * n + g) * n + r) * 4 + c] }
        var i = 0
        while i < out.data.count {
            let alpha = input.data[i + 3]
            if alpha > 0 {
                let p = [input.data[i] / alpha, input.data[i + 1] / alpha, input.data[i + 2] / alpha].map { min(1, max(0, $0)) * Float(n - 1) }
                let lo = p.map { min(Int($0), n - 2) }, t = zip(p, lo).map { $0 - Float($1) }
                var result: [Float] = [0, 0, 0]
                for c in 0..<3 {
                    var v: Float = 0
                    for db in 0...1 { for dg in 0...1 { for dr in 0...1 {
                        let w = (dr == 1 ? t[0] : 1 - t[0]) * (dg == 1 ? t[1] : 1 - t[1]) * (db == 1 ? t[2] : 1 - t[2])
                        v += w * entry(lo[0] + dr, lo[1] + dg, lo[2] + db, c)
                    } } }
                    result[c] = v
                }
                out.data[i] = result[0] * alpha; out.data[i + 1] = result[1] * alpha; out.data[i + 2] = result[2] * alpha
            }
            i += 4
        }
        return out
    }

    /// CIBlendWithMask: mix(background, input, mask) where the mask value is its (unpremultiplied) red channel.
    static func blendWithMask(_ input: Raster, background: Raster, mask: Raster) -> Raster {
        var out = Raster(rect: input.rect, uninitialized: true)
        let w = input.width, h = input.height
        out.data.withUnsafeMutableBufferPointer { outBuf in
            input.data.withUnsafeBufferPointer { inBuf in
                background.data.withUnsafeBufferPointer { bgBuf in
                    mask.data.withUnsafeBufferPointer { mskBuf in
                        guard let outPtr = outBuf.baseAddress,
                              let inPtr = inBuf.baseAddress,
                              let bgPtr = bgBuf.baseAddress,
                              let mskPtr = mskBuf.baseAddress else { return }
                        let chunks = min(h, max(1, ProcessInfo.processInfo.activeProcessorCount * 2))
                        let chunkSize = (h + chunks - 1) / chunks
                        DispatchQueue.concurrentPerform(iterations: chunks) { c in
                            let startY = c * chunkSize
                            guard startY < h else { return }
                            let endY = min(h, startY + chunkSize)
                            for y in startY..<endY {
                                let rowStart = y * w * 4
                                var outP = outPtr + rowStart
                                var inP = inPtr + rowStart
                                var bgP = bgPtr + rowStart
                                var mskP = mskPtr + rowStart
                                for _ in 0..<w {
                                    let ma = mskP[3]
                                    if ma <= 0 {
                                        outP[0] = bgP[0]; outP[1] = bgP[1]; outP[2] = bgP[2]; outP[3] = bgP[3]
                                    } else {
                                        let m = ma == 1.0 ? mskP[0] : mskP[0] / ma
                                        if m <= 0.0 {
                                            outP[0] = bgP[0]; outP[1] = bgP[1]; outP[2] = bgP[2]; outP[3] = bgP[3]
                                        } else if m >= 1.0 {
                                            outP[0] = inP[0]; outP[1] = inP[1]; outP[2] = inP[2]; outP[3] = inP[3]
                                        } else {
                                            let invM = 1.0 - m
                                            outP[0] = inP[0] * m + bgP[0] * invM
                                            outP[1] = inP[1] * m + bgP[1] * invM
                                            outP[2] = inP[2] * m + bgP[2] * invM
                                            outP[3] = inP[3] * m + bgP[3] * invM
                                        }
                                    }
                                    outP += 4; inP += 4; bgP += 4; mskP += 4
                                }
                            }
                        }
                    }
                }
            }
        }
        return out
    }

    /// Separable-blend compositing (source over backdrop) in premultiplied form, per the W3C compositing model.
    static func separableBlend(_ source: Raster, backdrop: Raster, _ blend: (Float, Float) -> Float) -> Raster {
        var out = source
        var i = 0
        while i < out.data.count {
            let as_ = source.data[i + 3], ab = backdrop.data[i + 3]
            for c in 0..<3 {
                let cs = as_ > 0 ? source.data[i + c] / as_ : 0
                let cb = ab > 0 ? backdrop.data[i + c] / ab : 0
                let mixed = as_ * ab * blend(cb, cs)
                out.data[i + c] = (1 - ab) * source.data[i + c] + (1 - as_) * backdrop.data[i + c] + mixed
            }
            out.data[i + 3] = as_ + ab - as_ * ab
            i += 4
        }
        return out
    }
    static func colorDodge(_ cb: Float, _ cs: Float) -> Float { cb == 0 ? 0 : (cs >= 1 ? 1 : min(1, cb / (1 - cs))) }
    static func colorBurn(_ cb: Float, _ cs: Float) -> Float { cb >= 1 ? 1 : (cs <= 0 ? 0 : 1 - min(1, (1 - cb) / cs)) }

    /// Inverse-maps `output` through a projective transform given by the 3x3 matrix `inverse` (device -> source).
    static func projective(_ input: Raster, inverse m: [Double], output region: CGRect) -> Raster {
        var out = Raster(rect: region)
        for y in 0..<out.height {
            for x in 0..<out.width {
                let px = Double(out.rect.minX) + Double(x) + 0.5, py = Double(out.rect.minY) + Double(y) + 0.5
                let w = m[6] * px + m[7] * py + m[8]
                guard abs(w) > 1e-12 else { continue }
                let sx = (m[0] * px + m[1] * py + m[2]) / w - Double(input.rect.minX)
                let sy = (m[3] * px + m[4] * py + m[5]) / w - Double(input.rect.minY)
                let p = input.bilinear(sx, sy)
                let o = out.index(x, y)
                out.data[o] = p.0; out.data[o + 1] = p.1; out.data[o + 2] = p.2; out.data[o + 3] = p.3
            }
        }
        return out
    }

    /// Homography taking the unit-square corners (p0..p3 in order) to the target points, returned as a 3x3 row-major.
    static func homography(from src: [CGPoint], to dst: [CGPoint]) -> [Double]? {
        // Solve 8 equations for the 8 unknowns (h33 = 1) with Gaussian elimination.
        var a = [[Double]](repeating: [Double](repeating: 0, count: 9), count: 8)
        for i in 0..<4 {
            let x = Double(src[i].x), y = Double(src[i].y), u = Double(dst[i].x), v = Double(dst[i].y)
            a[2 * i] = [x, y, 1, 0, 0, 0, -u * x, -u * y, u]
            a[2 * i + 1] = [0, 0, 0, x, y, 1, -v * x, -v * y, v]
        }
        for col in 0..<8 {
            var pivot = col
            for r in col..<8 where abs(a[r][col]) > abs(a[pivot][col]) { pivot = r }
            guard abs(a[pivot][col]) > 1e-12 else { return nil }
            a.swapAt(col, pivot)
            for r in 0..<8 where r != col {
                let f = a[r][col] / a[col][col]
                for k in col..<9 { a[r][k] -= f * a[col][k] }
            }
        }
        return (0..<8).map { a[$0][8] / a[$0][$0] } + [1]
    }

    static func invert3(_ m: [Double]) -> [Double]? {
        let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6]) + m[2] * (m[3] * m[7] - m[4] * m[6])
        guard abs(det) > 1e-14 else { return nil }
        return [
            (m[4] * m[8] - m[5] * m[7]) / det, (m[2] * m[7] - m[1] * m[8]) / det, (m[1] * m[5] - m[2] * m[4]) / det,
            (m[5] * m[6] - m[3] * m[8]) / det, (m[0] * m[8] - m[2] * m[6]) / det, (m[2] * m[3] - m[0] * m[5]) / det,
            (m[3] * m[7] - m[4] * m[6]) / det, (m[1] * m[6] - m[0] * m[7]) / det, (m[0] * m[4] - m[1] * m[3]) / det]
    }

    /// CIEdgePreserveUpsampleFilter: upsample `small` to the guide's size by joint-bilateral weighting, so the result's
    /// edges follow the guide image's luminance edges.
    static func edgePreserveUpsample(guide: Raster, small: Raster, spatialSigma: Double, lumaSigma: Double) -> Raster {
        var out = Raster(rect: guide.rect)
        let gw = guide.width, gh = guide.height, sw = small.width, sh = small.height
        guard gw > 0, gh > 0, sw > 0, sh > 0 else { return out }
        func luma(_ r: Raster, _ x: Int, _ y: Int) -> Float {
            let (a, b, c, al) = r.clamped(x, y)
            return al > 0 ? 0.2126 * a / al + 0.7152 * b / al + 0.0722 * c / al : 0
        }
        let radius = max(1, Int(ceil(spatialSigma / max(1, Double(gw) / Double(sw)))) + 1)
        let sx = Double(sw) / Double(gw), sy = Double(sh) / Double(gh)
        let twoS2 = 2 * max(0.25, spatialSigma * spatialSigma / max(1, Double(gw) / Double(sw))), twoL2 = Float(2 * lumaSigma * lumaSigma)
        for y in 0..<gh {
            for x in 0..<gw {
                let cxs = (Double(x) + 0.5) * sx - 0.5, cys = (Double(y) + 0.5) * sy - 0.5
                let gl = luma(guide, x, y)
                var acc: (Float, Float, Float, Float) = (0, 0, 0, 0), total: Float = 0
                for ny in Int(floor(cys)) - radius + 1...Int(floor(cys)) + radius {
                    for nx in Int(floor(cxs)) - radius + 1...Int(floor(cxs)) + radius {
                        let d2 = pow(Double(nx) - cxs, 2) + pow(Double(ny) - cys, 2)
                        let ws = Float(exp(-d2 / twoS2))
                        // Guide luma where that coarse sample sits in guide coordinates.
                        let gx = min(gw - 1, max(0, Int((Double(nx) + 0.5) / sx))), gy = min(gh - 1, max(0, Int((Double(ny) + 0.5) / sy)))
                        let dl = luma(guide, gx, gy) - gl
                        let wr = exp(-dl * dl / twoL2)
                        let p = small.clamped(nx, ny), w = ws * wr
                        acc.0 += p.0 * w; acc.1 += p.1 * w; acc.2 += p.2 * w; acc.3 += p.3 * w; total += w
                    }
                }
                let o = out.index(x, y)
                if total > 0 { out.data[o] = acc.0 / total; out.data[o + 1] = acc.1 / total; out.data[o + 2] = acc.2 / total; out.data[o + 3] = acc.3 / total }
            }
        }
        return out
    }
}
