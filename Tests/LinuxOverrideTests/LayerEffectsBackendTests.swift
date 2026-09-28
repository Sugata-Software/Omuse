import Foundation
import CoreGraphics
import Testing
@testable import Compositor

/// The Linux effects chain (`MetalLayerEffects` override -> C++ tier) against upstream's own CoreImage/CoreGraphics
/// renderer, which is the oracle: the two must agree closely on every effect, as upstream's GPU and CPU paths do.
@MainActor
struct LayerEffectsBackendTests {
    private func shape() throws -> CGImage {
        let context = try BrushRaster.context(width: 60, height: 44, mask: false)
        context.setFillColor(CGColor(srgbRed: 0.9, green: 0.3, blue: 0.2, alpha: 1))
        context.fillEllipse(in: CGRect(x: 8, y: 6, width: 40, height: 30))
        context.setFillColor(CGColor(srgbRed: 0.2, green: 0.4, blue: 0.9, alpha: 0.5))
        context.fill(CGRect(x: 30, y: 20, width: 24, height: 18))
        return try #require(context.makeImage())
    }

    private func bytes(_ image: CGImage) throws -> [UInt8] {
        let context = try BrushRaster.context(width: image.width, height: image.height, mask: false)
        BrushRaster.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height), mask: false, context: context)
        let data = try #require(context.data).assumingMemoryBound(to: UInt8.self)
        return Array(UnsafeBufferPointer(start: data, count: image.width * image.height * 4))
    }

    private struct Difference { var largest = 0; var mean = 0.0 }
    private func compare(_ a: [UInt8], _ b: [UInt8]) -> Difference {
        var d = Difference(), total = 0
        for i in a.indices { let v = abs(Int(a[i]) - Int(b[i])); d.largest = max(d.largest, v); total += v }
        d.mean = Double(total) / Double(a.count)
        return d
    }

    private let cases: [(String, LayerEffects)] = [
        ("outside stroke", LayerEffects(stroke: StrokeEffect(size: 3, red: 1, green: 1, blue: 0, opacity: 1))),
        ("inside stroke", LayerEffects(stroke: StrokeEffect(size: 2, red: 0, green: 1, blue: 0, opacity: 0.8, inside: true))),
        ("drop shadow", LayerEffects(shadow: ShadowEffect(angle: 45, distance: 6, blur: 8, opacity: 0.6))),
        ("colour overlay", LayerEffects(colorOverlay: ColorOverlayEffect(red: 0, green: 0, blue: 1, opacity: 0.4))),
        ("inner shadow", LayerEffects(innerShadow: InnerShadowEffect(angle: 120, distance: 4, blur: 6, opacity: 0.7))),
        ("outer glow", LayerEffects(outerGlow: OuterGlowEffect(size: 10, red: 1, green: 0.8, blue: 0.2, opacity: 0.8))),
        ("inner glow", LayerEffects(innerGlow: InnerGlowEffect(size: 6, red: 0, green: 1, blue: 1, opacity: 0.8))),
        ("inner glow ordering", LayerEffects(stroke: StrokeEffect(size: 2, red: 1, green: 0, blue: 0, opacity: 0.7, inside: true),
            shadow: ShadowEffect(angle: 60, distance: 5, blur: 6),
            colorOverlay: ColorOverlayEffect(red: 0, green: 0, blue: 1, opacity: 0.3),
            innerShadow: InnerShadowEffect(angle: 90, distance: 3, blur: 4),
            innerGlow: InnerGlowEffect(size: 6, red: 0, green: 1, blue: 1, opacity: 0.8))),
        ("everything", LayerEffects(stroke: StrokeEffect(size: 2, opacity: 1), shadow: ShadowEffect(angle: 60, distance: 5, blur: 6),
                                    colorOverlay: ColorOverlayEffect(red: 1, green: 0, blue: 0, opacity: 0.3),
                                    innerShadow: InnerShadowEffect(angle: 90, distance: 3, blur: 4))),
    ]

    @Test func cTierMatchesUpstreamsOwnRenderer() throws {
        let image = try shape()
        for (name, effects) in cases {
            let saved = MetalLayerEffects.shared
            MetalLayerEffects.shared = nil
            let oracle = try LayerEffectsRenderer.render(image, mask: nil, effects: effects)
            MetalLayerEffects.shared = saved
            let accelerated = try LayerEffectsRenderer.render(image, mask: nil, effects: effects)
            #expect(oracle.inset == accelerated.inset, "\(name): the margin is the same")
            #expect(oracle.image.width == accelerated.image.width && oracle.image.height == accelerated.image.height)
            let d = compare(try bytes(oracle.image), try bytes(accelerated.image))
            print("EFFECTS", name, "largest", d.largest, "mean", d.mean)
            #expect(d.mean < 1.5 && d.largest <= 24, "\(name): largest \(d.largest), mean \(d.mean)")
        }
    }

    @Test func aFailingBackendFallsThroughToTheNext() throws {
        final class Broken: LayerEffectsBackend { let name = "broken"; func render(_ pixels: CGImage, effects: LayerEffects) throws -> CGImage { throw ExportError.render } }
        let image = try shape()
        let effects = LayerEffects(shadow: ShadowEffect())
        let result = try LayerEffectsBackends.withChain([Broken(), CEffectsBackend.cpu()]) {
            try #require(MetalLayerEffects.shared).render(image, effects: effects)
        }
        #expect(result.width == image.width)
        #expect(throws: (any Error).self) {
            try LayerEffectsBackends.withChain([Broken()]) { try #require(MetalLayerEffects.shared).render(image, effects: effects) }
        }
    }
}

/// The Vulkan tier against the C++ tier (which is checked against upstream above). Skipped when no Vulkan device exists.
@MainActor
struct VulkanEffectsTests {
    @Test func vulkanMatchesTheCTierOnEveryEffect() throws {
        guard let vulkan = CEffectsBackend.vulkan() else { return }   // no device: the chain simply starts at the C tier
        let cpu = CEffectsBackend.cpu()
        let context = try BrushRaster.context(width: 70, height: 50, mask: false)
        context.setFillColor(CGColor(srgbRed: 0.9, green: 0.3, blue: 0.2, alpha: 1))
        context.fillEllipse(in: CGRect(x: 14, y: 10, width: 40, height: 28))
        let padded = try #require(context.makeImage())
        let cases: [LayerEffects] = [
            LayerEffects(stroke: StrokeEffect(size: 3, red: 1, green: 1, blue: 0, opacity: 1)),
            LayerEffects(stroke: StrokeEffect(size: 2, red: 0, green: 1, blue: 0, opacity: 0.8, inside: true)),
            LayerEffects(shadow: ShadowEffect(angle: 45, distance: 6, blur: 8, opacity: 0.6)),
            LayerEffects(colorOverlay: ColorOverlayEffect(red: 0, green: 0, blue: 1, opacity: 0.4)),
            LayerEffects(innerShadow: InnerShadowEffect(angle: 120, distance: 4, blur: 6, opacity: 0.7)),
            LayerEffects(outerGlow: OuterGlowEffect(size: 10, red: 1, green: 0.8, blue: 0.2, opacity: 0.8)),
            LayerEffects(stroke: StrokeEffect(size: 2, opacity: 1), shadow: ShadowEffect(angle: 60, distance: 5, blur: 6),
                         colorOverlay: ColorOverlayEffect(red: 1, green: 0, blue: 0, opacity: 0.3), innerShadow: InnerShadowEffect(angle: 90, distance: 3, blur: 4)),
        ]
        for effects in cases {
            let a = try cpu.render(padded, effects: effects), b = try vulkan.render(padded, effects: effects)
            let pa = a.portableImage.bytes, pb = b.portableImage.bytes
            var largest = 0
            for i in pa.indices { largest = max(largest, abs(Int(pa[i]) - Int(pb[i]))) }
            print("VULKAN", vulkan.name, "largest", largest)
            #expect(largest <= 2, "\(vulkan.name): largest difference \(largest)")
        }
    }
}


/// The Skia image-filter tier against the C++ reference (float surfaces, same kernel radii: matches to within a level).
/// Skipped when the Skia bridge is not loaded.
@MainActor
struct SkiaEffectsTests {
    @Test func skiaTierAgreesWithTheReferenceWithinAFewLevels() throws {
        guard let skia = CEffectsBackend.skia() else { return }
        let cpu = CEffectsBackend.cpu()
        let context = try BrushRaster.context(width: 70, height: 50, mask: false)
        context.setFillColor(CGColor(srgbRed: 0.9, green: 0.3, blue: 0.2, alpha: 1))
        context.fillEllipse(in: CGRect(x: 14, y: 10, width: 40, height: 28))
        context.setFillColor(CGColor(srgbRed: 0.2, green: 0.4, blue: 0.9, alpha: 0.5))
        context.fill(CGRect(x: 34, y: 24, width: 22, height: 16))
        let padded = try #require(context.makeImage())
        let cases: [(String, LayerEffects)] = [
            ("outside stroke", LayerEffects(stroke: StrokeEffect(size: 3, red: 1, green: 1, blue: 0, opacity: 1))),
            ("inside stroke", LayerEffects(stroke: StrokeEffect(size: 2, red: 0, green: 1, blue: 0, opacity: 0.8, inside: true))),
            ("drop shadow", LayerEffects(shadow: ShadowEffect(angle: 45, distance: 6, blur: 8, opacity: 0.6))),
            ("colour overlay", LayerEffects(colorOverlay: ColorOverlayEffect(red: 0, green: 0, blue: 1, opacity: 0.4))),
            ("inner shadow", LayerEffects(innerShadow: InnerShadowEffect(angle: 120, distance: 4, blur: 6, opacity: 0.7))),
        ("outer glow", LayerEffects(outerGlow: OuterGlowEffect(size: 10, red: 1, green: 0.8, blue: 0.2, opacity: 0.8))),
            ("inner glow", LayerEffects(innerGlow: InnerGlowEffect(size: 6, red: 0, green: 1, blue: 1, opacity: 0.8))),
        ("inner glow ordering", LayerEffects(stroke: StrokeEffect(size: 2, red: 1, green: 0, blue: 0, opacity: 0.7, inside: true),
            shadow: ShadowEffect(angle: 60, distance: 5, blur: 6),
            colorOverlay: ColorOverlayEffect(red: 0, green: 0, blue: 1, opacity: 0.3),
            innerShadow: InnerShadowEffect(angle: 90, distance: 3, blur: 4),
            innerGlow: InnerGlowEffect(size: 6, red: 0, green: 1, blue: 1, opacity: 0.8))),
        ("everything", LayerEffects(stroke: StrokeEffect(size: 2, opacity: 1), shadow: ShadowEffect(angle: 60, distance: 5, blur: 6),
                                        colorOverlay: ColorOverlayEffect(red: 1, green: 0, blue: 0, opacity: 0.3), innerShadow: InnerShadowEffect(angle: 90, distance: 3, blur: 4))),
        ]
        for (name, effects) in cases {
            let pa = try cpu.render(padded, effects: effects).portableImage.bytes
            let pb = try skia.render(padded, effects: effects).portableImage.bytes
            var largest = 0, total = 0
            for i in pa.indices { let d = abs(Int(pa[i]) - Int(pb[i])); largest = max(largest, d); total += d }
            print("SKIA", name, "largest", largest, "mean", Double(total) / Double(pa.count))
            #expect(largest <= 1, "\(name): largest difference \(largest)")
        }
    }
}


/// The OpenCV tier (pinned OpenCV 4.14.0) against the C++ reference. Skipped in a build without OpenCV.
@MainActor
struct OpenCVEffectsTests {
    @Test func openCVTierAgreesWithTheReference() throws {
        guard let opencv = CEffectsBackend.opencv() else { return }
        let cpu = CEffectsBackend.cpu()
        let context = try BrushRaster.context(width: 70, height: 50, mask: false)
        context.setFillColor(CGColor(srgbRed: 0.9, green: 0.3, blue: 0.2, alpha: 1))
        context.fillEllipse(in: CGRect(x: 14, y: 10, width: 40, height: 28))
        context.setFillColor(CGColor(srgbRed: 0.2, green: 0.4, blue: 0.9, alpha: 0.5))
        context.fill(CGRect(x: 34, y: 24, width: 22, height: 16))
        let padded = try #require(context.makeImage())
        let cases: [(String, LayerEffects)] = [
            ("outside stroke", LayerEffects(stroke: StrokeEffect(size: 3, red: 1, green: 1, blue: 0, opacity: 1))),
            ("inside stroke", LayerEffects(stroke: StrokeEffect(size: 2, red: 0, green: 1, blue: 0, opacity: 0.8, inside: true))),
            ("drop shadow", LayerEffects(shadow: ShadowEffect(angle: 45, distance: 6, blur: 8, opacity: 0.6))),
            ("colour overlay", LayerEffects(colorOverlay: ColorOverlayEffect(red: 0, green: 0, blue: 1, opacity: 0.4))),
            ("inner shadow", LayerEffects(innerShadow: InnerShadowEffect(angle: 120, distance: 4, blur: 6, opacity: 0.7))),
        ("outer glow", LayerEffects(outerGlow: OuterGlowEffect(size: 10, red: 1, green: 0.8, blue: 0.2, opacity: 0.8))),
            ("inner glow", LayerEffects(innerGlow: InnerGlowEffect(size: 6, red: 0, green: 1, blue: 1, opacity: 0.8))),
        ("inner glow ordering", LayerEffects(stroke: StrokeEffect(size: 2, red: 1, green: 0, blue: 0, opacity: 0.7, inside: true),
            shadow: ShadowEffect(angle: 60, distance: 5, blur: 6),
            colorOverlay: ColorOverlayEffect(red: 0, green: 0, blue: 1, opacity: 0.3),
            innerShadow: InnerShadowEffect(angle: 90, distance: 3, blur: 4),
            innerGlow: InnerGlowEffect(size: 6, red: 0, green: 1, blue: 1, opacity: 0.8))),
        ("everything", LayerEffects(stroke: StrokeEffect(size: 2, opacity: 1), shadow: ShadowEffect(angle: 60, distance: 5, blur: 6),
                                        colorOverlay: ColorOverlayEffect(red: 1, green: 0, blue: 0, opacity: 0.3), innerShadow: InnerShadowEffect(angle: 90, distance: 3, blur: 4))),
        ]
        for (name, effects) in cases {
            let pa = try cpu.render(padded, effects: effects).portableImage.bytes
            let pb = try opencv.render(padded, effects: effects).portableImage.bytes
            var largest = 0, total = 0
            for i in pa.indices { let d = abs(Int(pa[i]) - Int(pb[i])); largest = max(largest, d); total += d }
            print("OPENCV", name, "largest", largest, "mean", Double(total) / Double(pa.count))
            #expect(largest <= 1, "\(name): largest difference \(largest)")
        }
    }
}

/// Timing only (printed, never asserted): which tier is fastest on a large layer.
@MainActor
struct EffectsTimingTests {
    @Test func timeTheTiersOnALargeLayer() throws {
        let context = try BrushRaster.context(width: 2400, height: 1600, mask: false)
        context.setFillColor(CGColor(srgbRed: 0.9, green: 0.3, blue: 0.2, alpha: 1))
        context.fillEllipse(in: CGRect(x: 300, y: 250, width: 1800, height: 1100))
        let image = try #require(context.makeImage())
        let effects = LayerEffects(stroke: StrokeEffect(size: 12, opacity: 1), shadow: ShadowEffect(angle: 60, distance: 30, blur: 40),
                                   innerShadow: InnerShadowEffect(angle: 90, distance: 10, blur: 16))
        var tiers: [(String, CEffectsBackend)] = [("cpu", CEffectsBackend.cpu())]
        if let o = CEffectsBackend.opencv() { tiers.append(("opencv", o)) }
        if let s = CEffectsBackend.skia() { tiers.append(("skia", s)) }
        if let v = CEffectsBackend.vulkan() { tiers.append((v.name, v)) }
        for (name, tier) in tiers {
            let start = ContinuousClock.now
            _ = try tier.render(image, effects: effects)
            print("TIMING", name, ContinuousClock.now - start)
        }
    }
}
