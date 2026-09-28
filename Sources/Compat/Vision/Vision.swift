// Vision compat: foreground instance masks (the only Vision API upstream uses — Object Selection and Remove
// Background). Shape follows Apple's: an image request handler, a request whose results are instance-mask
// observations, an 8-bit label map with 0 = background, and float masks for chosen instances.
//
// The segmenter is pluggable (dependency inversion). `ClassicalForegroundSegmenter` is the dependency-free floor
// (border-colour model + connected components); a stronger backend (OpenCV GrabCut, a learned model) is installed
// through `ForegroundSegmentationRegistry` or the C hook `compositor_vision_register`.

@_exported import CoreGraphics
@_exported import CoreVideo
import Foundation
import CompatSupport

public enum CGImagePropertyOrientation: UInt32, Sendable {
    case up = 1, upMirrored, down, downMirrored, leftMirrored, right, rightMirrored, left
}

// MARK: - Segmentation seam

public struct SegmentationResult: Sendable {
    /// Row-major top-down labels, 0 background, 1...instanceCount instances.
    public var labels: [UInt8]
    public var width: Int
    public var height: Int
    public var instanceCount: Int
    public init(labels: [UInt8], width: Int, height: Int, instanceCount: Int) {
        self.labels = labels; self.width = width; self.height = height; self.instanceCount = instanceCount
    }
}

public protocol ForegroundSegmentationBackend: AnyObject {
    /// `rgba` is 8-bit premultiplied, `width`x`height`. Must not mutate anything; throws when it cannot segment.
    func segment(rgba: [UInt8], width: Int, height: Int) throws -> SegmentationResult
}

public enum ForegroundSegmentationRegistry {
    public static let classical: ForegroundSegmentationBackend = ClassicalForegroundSegmenter()
    /// The installed segmenter, falling back to the classical one when the host installs none.
    public static let slot = ServiceSlot<ForegroundSegmentationBackend>(fallback: { classical })
    public static var backend: ForegroundSegmentationBackend? {
        get { slot.installedValue }
        set { slot.install(newValue) }
    }
    static var active: ForegroundSegmentationBackend { slot.current ?? classical }
    public static func withBackend<Result>(_ backend: ForegroundSegmentationBackend?, _ body: () throws -> Result) rethrows -> Result {
        try slot.withOverride(backend, body)
    }
}

/// C hook: `labels` (width*height bytes) is filled by the callee; `instanceCount` receives the number of instances.
public typealias CompositorSegmentFn = @convention(c) (UnsafePointer<UInt8>?, Int32, Int32, UnsafeMutablePointer<UInt8>?,
                                                        UnsafeMutablePointer<Int32>?) -> Int32

private final class CCallbackSegmenter: ForegroundSegmentationBackend {
    let fn: CompositorSegmentFn
    init(_ fn: @escaping CompositorSegmentFn) { self.fn = fn }
    func segment(rgba: [UInt8], width: Int, height: Int) throws -> SegmentationResult {
        var labels = [UInt8](repeating: 0, count: width * height)
        var count: Int32 = 0
        let rc = rgba.withUnsafeBufferPointer { px in labels.withUnsafeMutableBufferPointer { out in
            fn(px.baseAddress, Int32(width), Int32(height), out.baseAddress, &count) } }
        guard rc == 0 else { throw VNError.segmentationFailed }
        return SegmentationResult(labels: labels, width: width, height: height, instanceCount: Int(count))
    }
}

@_cdecl("compositor_vision_register")
public func compositor_vision_register(_ fn: CompositorSegmentFn?) {
    ForegroundSegmentationRegistry.backend = fn.map { CCallbackSegmenter($0) }
}

public enum VNError: Error { case segmentationFailed, invalidRequest, unsupported }

// MARK: - Requests and observations

open class VNObservation {}

public final class VNInstanceMaskObservation: VNObservation, @unchecked Sendable {
    /// Low-resolution label buffer: 0 background, 1...N instances (8-bit, one component).
    public let instanceMask: CVPixelBuffer
    public let allInstances: IndexSet
    private let source: SegmentationResult
    private let imageWidth: Int, imageHeight: Int

    init(result: SegmentationResult, imageWidth: Int, imageHeight: Int) {
        source = result
        self.imageWidth = imageWidth; self.imageHeight = imageHeight
        instanceMask = CVPixelBuffer(width: result.width, height: result.height, gray8: result.labels)
        allInstances = result.instanceCount > 0 ? IndexSet(1...result.instanceCount) : IndexSet()
    }

    /// Float mask at the label buffer's resolution: 1 where the pixel belongs to one of `instances`, else 0.
    public func generateMask(forInstances instances: IndexSet) throws -> CVPixelBuffer {
        guard instances.isSubset(of: allInstances) else { throw VNError.invalidRequest }
        let values = source.labels.map { instances.contains(Int($0)) ? Float(1) : 0 }
        return CVPixelBuffer(width: source.width, height: source.height, gray32Float: values)
    }

    /// Float mask scaled to the analysed image's size (bilinear, so the edge is soft).
    public func generateScaledMaskForImage(forInstances instances: IndexSet, from handler: VNImageRequestHandler) throws -> CVPixelBuffer {
        let coarse = try generateMask(forInstances: instances)
        let w = imageWidth, h = imageHeight
        var out = [Float](repeating: 0, count: w * h)
        let cw = coarse.width, ch = coarse.height, src = coarse.floats
        for y in 0..<h {
            let fy = (Double(y) + 0.5) * Double(ch) / Double(h) - 0.5
            let y0 = Int(floor(fy)), ty = Float(fy - Double(y0))
            for x in 0..<w {
                let fx = (Double(x) + 0.5) * Double(cw) / Double(w) - 0.5
                let x0 = Int(floor(fx)), tx = Float(fx - Double(x0))
                func s(_ ix: Int, _ iy: Int) -> Float { src[min(max(iy, 0), ch - 1) * cw + min(max(ix, 0), cw - 1)] }
                out[y * w + x] = (s(x0, y0) * (1 - tx) + s(x0 + 1, y0) * tx) * (1 - ty) + (s(x0, y0 + 1) * (1 - tx) + s(x0 + 1, y0 + 1) * tx) * ty
            }
        }
        return CVPixelBuffer(width: w, height: h, gray32Float: out)
    }
}

open class VNRequest {
    public init() {}
    func perform(on handler: VNImageRequestHandler) throws {}
}

open class VNImageBasedRequest: VNRequest {}

public final class VNGenerateForegroundInstanceMaskRequest: VNImageBasedRequest {
    public override init() { super.init() }
    /// Typed to the request, as Apple's Swift overlay presents it.
    public private(set) var results: [VNInstanceMaskObservation]?
    override func perform(on handler: VNImageRequestHandler) throws {
        let image = handler.image
        // Analyse at a bounded size, as Vision does (the label buffer is low resolution).
        let longest = max(image.width, image.height)
        let scale = longest > 512 ? 512.0 / Double(longest) : 1
        let w = max(1, Int((Double(image.width) * scale).rounded())), h = max(1, Int((Double(image.height) * scale).rounded()))
        let small = scale == 1 ? image : ImageResampling.resample(image, width: w, height: h)
        let oriented = ImageResampling.applyOrientation(handler.orientation, to: small)
        let result = try ForegroundSegmentationRegistry.active.segment(rgba: oriented.rgba, width: oriented.width, height: oriented.height)
        results = [VNInstanceMaskObservation(result: result, imageWidth: image.width, imageHeight: image.height)]
    }
}

public final class VNImageRequestHandler {
    let image: CGImage
    let orientation: CGImagePropertyOrientation
    public init(cgImage: CGImage, options: [AnyHashable: Any] = [:]) { image = cgImage; orientation = .up }
    public init(cgImage: CGImage, orientation: CGImagePropertyOrientation, options: [AnyHashable: Any] = [:]) {
        image = cgImage; self.orientation = orientation
    }
    public func perform(_ requests: [VNRequest]) throws { for r in requests { try r.perform(on: self) } }
}

// MARK: - Image plumbing

enum ImageResampling {
    struct Pixels { var rgba: [UInt8]; var width: Int; var height: Int }

    static func resample(_ image: CGImage, width: Int, height: Int) -> CGImage {
        // Area-average downscale (cheap and alias-free enough for segmentation input).
        let ch = image.isGrayPlane ? 1 : 4
        let src = image.portableImage.bytes, sw = image.width, sh = image.height, stride = image.bytesPerRow
        var out = [UInt8](repeating: 0, count: width * height * ch)
        for y in 0..<height {
            let y0 = y * sh / height, y1 = max(y0 + 1, (y + 1) * sh / height)
            for x in 0..<width {
                let x0 = x * sw / width, x1 = max(x0 + 1, (x + 1) * sw / width)
                for c in 0..<ch {
                    var sum = 0, n = 0
                    for yy in y0..<y1 { for xx in x0..<x1 { sum += Int(src[yy * stride + xx * ch + c]); n += 1 } }
                    out[(y * width + x) * ch + c] = UInt8(sum / max(1, n))
                }
            }
        }
        return CGImage(PortableImage(width: width, height: height, kind: image.isGrayPlane ? .mask : .rgba, bytesPerRow: width * ch, bytes: out))
    }

    /// RGBA bytes with the EXIF-style orientation applied (mask images become opaque gray RGBA).
    static func applyOrientation(_ o: CGImagePropertyOrientation, to image: CGImage) -> Pixels {
        let w = image.width, h = image.height, ch = image.isGrayPlane ? 1 : 4
        var rgba = [UInt8](repeating: 255, count: w * h * 4)
        for y in 0..<h { for x in 0..<w {
            let s = y * image.bytesPerRow + x * ch, d = (y * w + x) * 4
            if ch == 1 { let v = image.portableImage.bytes[s]; rgba[d] = v; rgba[d + 1] = v; rgba[d + 2] = v }
            else { for c in 0..<4 { rgba[d + c] = image.portableImage.bytes[s + c] } }
        } }
        guard o != .up else { return Pixels(rgba: rgba, width: w, height: h) }
        let swap = [.leftMirrored, .right, .rightMirrored, .left].contains(o)
        let ow = swap ? h : w, oh = swap ? w : h
        var out = [UInt8](repeating: 0, count: ow * oh * 4)
        for y in 0..<h { for x in 0..<w {
            var nx = x, ny = y
            switch o {
            case .upMirrored: nx = w - 1 - x
            case .down: nx = w - 1 - x; ny = h - 1 - y
            case .downMirrored: ny = h - 1 - y
            case .leftMirrored: nx = y; ny = x
            case .right: nx = h - 1 - y; ny = x
            case .rightMirrored: nx = h - 1 - y; ny = w - 1 - x
            case .left: nx = y; ny = w - 1 - x
            case .up: break
            }
            let s = (y * w + x) * 4, d = (ny * ow + nx) * 4
            for c in 0..<4 { out[d + c] = rgba[s + c] }
        } }
        return Pixels(rgba: out, width: ow, height: oh)
    }
}

// MARK: - Classical floor segmenter

/// Separates objects from a fairly uniform surround: models the background from the image border, marks pixels that
/// differ from it (Otsu threshold on the colour distance), cleans the mask with a small opening/closing, and labels
/// connected components by size. It is a floor, not a match for a learned model; OpenCV GrabCut or another backend can
/// replace it through `ForegroundSegmentationRegistry`.
final class ClassicalForegroundSegmenter: ForegroundSegmentationBackend {
    func segment(rgba: [UInt8], width w: Int, height h: Int) throws -> SegmentationResult {
        guard w > 2, h > 2, rgba.count >= w * h * 4 else { throw VNError.segmentationFailed }
        func colour(_ x: Int, _ y: Int) -> (Double, Double, Double) {
            let i = (y * w + x) * 4, a = Double(rgba[i + 3]) / 255
            // Composite over mid-grey so transparent regions read as background-like.
            return (Double(rgba[i]) + (1 - a) * 128, Double(rgba[i + 1]) + (1 - a) * 128, Double(rgba[i + 2]) + (1 - a) * 128)
        }
        // Background model: per-channel median of the border pixels.
        var border: [[Double]] = [[], [], []]
        let band = max(1, min(w, h) / 32)
        for y in 0..<h { for x in 0..<w where x < band || y < band || x >= w - band || y >= h - band {
            let c = colour(x, y); border[0].append(c.0); border[1].append(c.1); border[2].append(c.2) } }
        let bg = border.map { channel -> Double in channel.sorted()[channel.count / 2] }
        var distance = [Double](repeating: 0, count: w * h)
        var maxD = 1.0
        for y in 0..<h { for x in 0..<w {
            let c = colour(x, y), d = ((c.0 - bg[0]) * (c.0 - bg[0]) + (c.1 - bg[1]) * (c.1 - bg[1]) + (c.2 - bg[2]) * (c.2 - bg[2])).squareRoot()
            distance[y * w + x] = d; maxD = max(maxD, d) } }
        guard maxD > 24 else { return SegmentationResult(labels: [UInt8](repeating: 0, count: w * h), width: w, height: h, instanceCount: 0) }
        // Otsu threshold over a 64-bin histogram of the distances.
        var hist = [Double](repeating: 0, count: 64)
        for d in distance { hist[min(63, Int(d / maxD * 63))] += 1 }
        let total = Double(w * h), sumAll = hist.enumerated().reduce(0.0) { $0 + Double($1.offset) * $1.element }
        var wB = 0.0, sumB = 0.0, best = 0.0, threshold = 0
        for t in 0..<64 {
            wB += hist[t]; if wB == 0 { continue }
            let wF = total - wB; if wF == 0 { break }
            sumB += Double(t) * hist[t]
            let mB = sumB / wB, mF = (sumAll - sumB) / wF, between = wB * wF * (mB - mF) * (mB - mF)
            if between > best { best = between; threshold = t }
        }
        let cut = max(24.0, (Double(threshold) + 1) / 63 * maxD)
        var mask = distance.map { $0 > cut }
        mask = Self.morph(mask, w, h, dilate: false); mask = Self.morph(mask, w, h, dilate: true)   // open
        mask = Self.morph(mask, w, h, dilate: true); mask = Self.morph(mask, w, h, dilate: false)   // close
        // Connected components (4-neighbour), largest first; drop specks under 0.5% of the image.
        var comp = [Int](repeating: -1, count: w * h)
        var sizes: [Int] = []
        for start in 0..<(w * h) where mask[start] && comp[start] < 0 {
            var stack = [start], size = 0
            comp[start] = sizes.count
            while let p = stack.popLast() {
                size += 1
                let x = p % w, y = p / w
                for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] where nx >= 0 && ny >= 0 && nx < w && ny < h {
                    let q = ny * w + nx
                    if mask[q] && comp[q] < 0 { comp[q] = sizes.count; stack.append(q) }
                }
            }
            sizes.append(size)
        }
        let keep = sizes.enumerated().filter { Double($0.element) >= Double(w * h) * 0.005 }.sorted { $0.element > $1.element }.prefix(8)
        var label = [Int: UInt8]()
        for (rank, entry) in keep.enumerated() { label[entry.offset] = UInt8(rank + 1) }
        var labels = [UInt8](repeating: 0, count: w * h)
        for i in 0..<(w * h) where comp[i] >= 0 { labels[i] = label[comp[i]] ?? 0 }
        return SegmentationResult(labels: labels, width: w, height: h, instanceCount: keep.count)
    }

    private static func morph(_ mask: [Bool], _ w: Int, _ h: Int, dilate: Bool) -> [Bool] {
        var out = mask
        for y in 0..<h { for x in 0..<w {
            var any = false, all = true
            for ny in max(0, y - 1)...min(h - 1, y + 1) { for nx in max(0, x - 1)...min(w - 1, x + 1) {
                if mask[ny * w + nx] { any = true } else { all = false } } }
            out[y * w + x] = dilate ? any : all
        } }
        return out
    }
}
