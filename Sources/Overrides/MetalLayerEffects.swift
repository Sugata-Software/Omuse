// OVERRIDE for Compositor/Rendering/MetalLayerEffects.swift (excluded from the Linux build).
//
// Same surface as upstream's Metal type (`MetalLayerEffects.shared`, `render(_:effects:)`), so `LayerEffectsRenderer`
// compiles unmodified and asks "is there an accelerator?" exactly as on macOS. The accelerator is a chain of backends
// running the nine passes of the Metal kernels (alpha, spread rows/columns, ring, shift, blur rows/columns, inside,
// compose): Vulkan compute first, the portable C++ tier last, each failing over to the next. See
// `LayerEffectsBackends` and backends/effects.

import Foundation
import CoreGraphics
import CompatSupport
#if canImport(Glibc)
import Glibc
#endif
import CompositorEffectsBackend

final class MetalLayerEffects {
    /// Always present: the C tier needs no device. (Upstream's own CoreImage path remains the fallback if every
    /// backend in the chain fails for an image.)
    nonisolated(unsafe) static var shared: MetalLayerEffects? = MetalLayerEffects()

    private init() {}

    /// Same signature as upstream: the layer's pixels with room around them, stroke and drop shadow composited in.
    func render(_ pixels: CGImage, effects: LayerEffects) throws -> CGImage {
        var lastError: Error = ExportError.render
        for backend in LayerEffectsBackends.chain {
            do { return try backend.render(pixels, effects: effects) } catch { lastError = error }
        }
        throw lastError
    }
}

/// One tier of the effects chain. Implementations take the padded layer pixels and return the composited image.
protocol LayerEffectsBackend: AnyObject {
    var name: String { get }
    func render(_ pixels: CGImage, effects: LayerEffects) throws -> CGImage
}

enum LayerEffectsBackends {
    /// The tiers to try, in order. `COMPOSITOR_EFFECTS=auto|vulkan|skia|opencv|cpu` chooses; `auto`, the default, is a hardware
    /// Vulkan device when there is one, then OpenCV when this build has it, then the C++ tier. `skia` puts the Skia image-filter tier first (it is approximate
    /// and, on Skia's raster backend, slower than the C++ tier, so it is opt-in until Skia is built with a GPU backend).
    static let slot = ServiceSlot<[LayerEffectsBackend]>(fallback: { defaultChain() })

    static var chain: [LayerEffectsBackend] { slot.current ?? [] }

    static func withChain<Result>(_ chain: [LayerEffectsBackend], _ body: () throws -> Result) rethrows -> Result {
        try slot.withOverride(chain, body)
    }

    private static func defaultChain() -> [LayerEffectsBackend] {
        let choice = ProcessInfo.processInfo.environment["COMPOSITOR_EFFECTS"]?.lowercased() ?? "auto"
        var tiers: [LayerEffectsBackend] = []
        // `auto` skips a software Vulkan device (the C++ tier is faster there); `vulkan` takes whatever device exists.
        if choice == "auto" || choice == "vulkan", let vulkan = CEffectsBackend.vulkan(),
           choice == "vulkan" || !vulkan.isSoftwareDevice { tiers.append(vulkan) }
        if choice == "skia", let skia = CEffectsBackend.skia() { tiers.append(skia) }
        // OpenCV (SIMD, threaded) beats the plain C++ tier several times over, so it comes next in `auto`; the C++ tier is
        // the reference and the last resort, present in every chain.
        if ["auto", "opencv"].contains(choice), let opencv = CEffectsBackend.opencv() { tiers.append(opencv) }
        tiers.append(CEffectsBackend.cpu())
        return tiers
    }
}

/// The C++ tiers behind `CompositorEffectsBackend.h`: the CPU reference, or a Vulkan device.
final class CEffectsBackend: LayerEffectsBackend {
    private typealias SkiaEffectsFn = @convention(c) (UnsafePointer<CompositorEffectsParams>?, UnsafePointer<UInt8>?, UnsafeMutablePointer<UInt8>?) -> Int32
    private enum Kind { case cpu; case opencv; case vulkan(OpaquePointer); case skia(SkiaEffectsFn) }
    private let kind: Kind
    let name: String
    /// A Vulkan device that is really a CPU (llvmpipe): correct, but the direct C++ tier is faster than emulating a GPU.
    let isSoftwareDevice: Bool

    private init(kind: Kind, name: String, isSoftwareDevice: Bool = false) {
        self.kind = kind; self.name = name; self.isSoftwareDevice = isSoftwareDevice
    }
    deinit { if case .vulkan(let context) = kind { compositor_vulkan_effects_destroy(context) } }

    static func cpu() -> CEffectsBackend { CEffectsBackend(kind: .cpu, name: "cpu") }

    /// The OpenCV tier, when this build has OpenCV (its C entry reports -2 otherwise, which a probe detects).
    static func opencv() -> CEffectsBackend? {
        var probe = CompositorEffectsParams(); probe.width = 1; probe.height = 1
        var pixel = [UInt8](repeating: 0, count: 4), out = [UInt8](repeating: 0, count: 4)
        return compositor_effects_opencv(&probe, &pixel, &out) == 0 ? CEffectsBackend(kind: .opencv, name: "opencv") : nil
    }

    /// The Skia image-filter tier, when the Skia bridge library is loaded (it is once any Skia canvas exists).
    static func skia() -> CEffectsBackend? {
        _ = CanvasBackends.isAvailable   // loads the bridge library if it has not been yet
        #if canImport(Glibc)
        guard let symbol = dlsym(nil, "compositor_skia_effects_render") else { return nil }
        return CEffectsBackend(kind: .skia(unsafeBitCast(symbol, to: SkiaEffectsFn.self)), name: "skia")
        #else
        return nil
        #endif
    }

    static func vulkan() -> CEffectsBackend? {
        guard let context = compositor_vulkan_effects_create() else { return nil }
        let device = String(cString: compositor_vulkan_effects_device(context))
        return CEffectsBackend(kind: .vulkan(context), name: "vulkan (\(device))",
                               isSoftwareDevice: compositor_vulkan_effects_device_type(context) == 4)
    }

    func render(_ pixels: CGImage, effects: LayerEffects) throws -> CGImage {
        // The native ABI predates Inner Glow. Compose that pass using the
        // shared coverage implementation instead of silently omitting it.
        if let glow = effects.innerGlow, glow.isEnabled, glow.size > 0, glow.opacity > 0 {
            return try renderWithInnerGlow(pixels, effects: effects, glow: glow)
        }
        let width = pixels.width, height = pixels.height
        let count = width * height
        guard count > 0, count <= 80_000_000 else { throw ExportError.tooLarge }
        // The pixels as premultiplied RGBA bytes, the way the Metal path hands them to the GPU.
        let source = try BrushRaster.context(width: width, height: height, mask: false)
        BrushRaster.draw(pixels, in: CGRect(x: 0, y: 0, width: width, height: height), mask: false, context: source)
        guard let bytes = source.data else { throw ExportError.render }
        var params = Self.parameters(width: width, height: height, effects: effects)
        var output = [UInt8](repeating: 0, count: count * 4)
        let status: Int32 = output.withUnsafeMutableBufferPointer { out in
            switch kind {
            case .cpu:
                return compositor_effects_cpu(&params, bytes.assumingMemoryBound(to: UInt8.self), out.baseAddress)
            case .vulkan(let context):
                return compositor_vulkan_effects_render(context, &params,
                                                        bytes.assumingMemoryBound(to: UInt8.self), out.baseAddress)
            case .opencv:
                return compositor_effects_opencv(&params, bytes.assumingMemoryBound(to: UInt8.self), out.baseAddress)
            case .skia(let render):
                return render(&params, bytes.assumingMemoryBound(to: UInt8.self), out.baseAddress)
            }
        }
        guard status == 0 else { throw ExportError.render }
        // Copied out of the working buffer, which is freed as soon as this returns.
        guard let provider = CGDataProvider(data: Data(output) as CFData),
              let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                                  bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue),
                                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
        else { throw ExportError.render }
        return image
    }

    /// Keep the shared renderer's ordering: source/overlay, inner glow,
    /// inner shadow, then inside stroke. Coverage always comes from the original
    /// pixels, not from the already decorated image.
    private func renderWithInnerGlow(_ pixels: CGImage, effects: LayerEffects, glow: InnerGlowEffect) throws -> CGImage {
        var base = effects
        base.innerGlow = nil
        base.innerShadow = nil
        if base.stroke?.inside == true { base.stroke = nil }
        let rendered = try render(pixels, effects: base)
        let bounds = CGRect(x: 0, y: 0, width: pixels.width, height: pixels.height)
        let context = try BrushRaster.context(width: pixels.width, height: pixels.height, mask: false)
        BrushRaster.draw(rendered, in: bounds, mask: false, context: context)
        func fill(_ color: PaletteColor, opacity: Double, coverage: CGImage) {
            BrushRaster.fill(CGColor(srgbRed: color.red, green: color.green, blue: color.blue, alpha: 1),
                             coverage: coverage, in: bounds, alpha: CGFloat(opacity), context: context)
        }
        fill(glow.color, opacity: glow.opacity,
             coverage: try LayerEffectsRenderer.innerGlowCoverage(pixels, placed: bounds, size: bounds.size, glow: glow))
        if let inner = effects.innerShadow, inner.isEnabled, inner.opacity > 0 {
            fill(inner.color, opacity: inner.opacity,
                 coverage: try LayerEffectsRenderer.innerCoverage(pixels, placed: bounds, size: bounds.size, shadow: inner))
        }
        if let stroke = effects.stroke, stroke.isEnabled, stroke.inside, stroke.size > 0, stroke.opacity > 0 {
            fill(stroke.color, opacity: stroke.opacity,
                 coverage: try LayerEffectsRenderer.ringCoverage(pixels, in: bounds.size, stroke: stroke))
        }
        guard let result = context.makeImage() else { throw ExportError.render }
        return result
    }

    /// The effects as the passes' parameters, with the same enable rules as the Metal renderer.
    static func parameters(width: Int, height: Int, effects: LayerEffects) -> CompositorEffectsParams {
        func color(_ c: PaletteColor, _ opacity: CGFloat) -> CompositorEffectColor {
            CompositorEffectColor(r: Float(c.red), g: Float(c.green), b: Float(c.blue), opacity: Float(opacity))
        }
        var p = CompositorEffectsParams()
        p.width = UInt32(width); p.height = UInt32(height)
        if let stroke = effects.stroke, stroke.isEnabled, stroke.size > 0, stroke.opacity > 0 {
            p.has_stroke = 1
            p.stroke_inside = stroke.inside ? 1 : 0
            p.stroke_reach = UInt32(max(1, Int(stroke.size.rounded())))
            p.stroke = color(stroke.color, stroke.opacity)
        }
        if let shadow = effects.shadow, shadow.isEnabled, shadow.opacity > 0 {
            p.has_shadow = 1
            p.shadow_dx = Float(shadow.offset.width); p.shadow_dy = Float(shadow.offset.height)
            p.shadow_sigma = Float(shadow.blur / 2)
            p.shadow = color(shadow.color, shadow.opacity)
        }
        if let overlay = effects.colorOverlay, overlay.isEnabled, overlay.opacity > 0 {
            p.has_overlay = 1
            p.overlay = color(overlay.color, overlay.opacity)
        }
        if let inner = effects.innerShadow, inner.isEnabled, inner.opacity > 0 {
            p.has_inner = 1
            p.inner_dx = Float(inner.offset.width); p.inner_dy = Float(inner.offset.height)
            p.inner_sigma = Float(inner.blur / 2)
            p.inner = color(inner.color, inner.opacity)
        }
        if let glow = effects.outerGlow, glow.isEnabled, glow.opacity > 0 {
            p.has_outer = 1
            p.outer_sigma = Float(glow.size / 2)
            p.outer = color(glow.color, glow.opacity)
        }
        return p
    }
}
