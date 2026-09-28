// OVERRIDE for Compositor/Rendering/MetalBrushCoverage.swift (excluded from the Linux build).
//
// Same public surface as upstream's Metal type — `MetalBrushCoverage.shared`, `Tile`, `tile(width:height:)`,
// `render(_:settled:tail:mapping:settings:canvas:)` — so upstream's BrushStroke compiles unmodified, implemented by the
// portable backend chain: Vulkan compute when a device is available, otherwise the C CPU kernel (see
// `AdaptiveBrushCoverage`). Tiles keep their density and preview as plain arrays instead of MTLBuffers.
// Contract test: Tests/CompositorCoreTests/OverrideSignatureTests.swift.

import Foundation
import CoreGraphics

final class MetalBrushCoverage {
    static let shared: MetalBrushCoverage? = MetalBrushCoverage()

    private let backend: BrushCoverageComputing

    private init() { backend = BrushCoverageBackends.automatic() }

    /// Stroke-local tile storage: accumulated density and the 8-bit preview that is copied into the coverage context.
    final class Tile {
        let width: Int, height: Int
        var permanent: [Float]
        var preview: [UInt8]
        init(width: Int, height: Int) {
            self.width = width; self.height = height
            permanent = [Float](repeating: 0, count: width * height)
            preview = [UInt8](repeating: 0, count: width * height)
        }
    }

    func tile(width: Int, height: Int) throws -> Tile {
        guard (1...256).contains(width), (1...256).contains(height) else { throw ExportError.render }
        return Tile(width: width, height: height)
    }

    func render(_ tiles: [(Tile, CGRect, CGContext)], settled: [SIMD4<Float>], tail: [SIMD4<Float>],
                mapping: CGAffineTransform, settings: BrushSettings, canvas: CGSize) throws {
        guard !tiles.isEmpty else { return }
        let scale = Float(max(0.001, min(hypot(mapping.a, mapping.b), hypot(mapping.c, mapping.d))))
        let spacing = Float(max(0.25, settings.diameter * BrushStroke.spacingFraction(settings.hardness)))
        // Compute every tile first; only publish once all succeeded, so a failure leaves nothing half-written.
        var results: [(Tile, BrushCoverageResult, CGContext, Int)] = []
        for (tile, rect, context) in tiles {
            let origin = rect.origin.applying(mapping)
            let request = BrushCoverageRequest(
                width: Int(rect.width), height: Int(rect.height), origin: origin, mapping: mapping, canvas: canvas,
                radius: Float(settings.diameter / 2), hardness: Float(settings.hardness), antialiasWidth: scale,
                spacing: spacing, settled: settled, tail: tail, permanent: tile.permanent)
            results.append((tile, try backend.render(request), context, Int(rect.width * rect.height)))
        }
        for (tile, result, context, count) in results {
            tile.permanent = result.permanent
            tile.preview = result.preview
            // CGContext owns its memory so makeImage's copy-on-write snapshots stay immutable.
            guard let destination = context.data else { throw ExportError.render }
            result.preview.withUnsafeBufferPointer { _ = memcpy(destination, $0.baseAddress!, min(count, $0.count)) }
        }
    }
}
