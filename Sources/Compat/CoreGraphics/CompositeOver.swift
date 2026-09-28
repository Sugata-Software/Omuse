// CompositorCore — portable Swift core exposed to the Qt/C++ host via a narrow
// C ABI (see docs/linux-port-plan.md, ENG-1). This target has NO Apple imports;
// it builds on Linux via SwiftPM and is the LayerCompositing (SOLID) entry point
// for the vertical slice.

import Foundation

/// Premultiplied source-over compositing of a source tile onto a destination
/// tile, modulated by an optional coverage mask and opacity.
///
/// Canonical buffer contract: 8-bit premultiplied RGBA, explicit `stride`, and a
/// separate 8-bit grayscale coverage row stride equal to `width`. See
/// `include/CompositorCore.h` for the versioned C ABI contract.
///
/// Exposed to C/C++ as `compositor_composite_over` via `@_cdecl`.
/// `@_cdecl` is unofficial and Swift-version-sensitive; the toolchain is pinned
/// in `Package.swift` (`swiftLanguageMode: .v5`) and the Freedesktop Swift
/// runtime extension. Changing the toolchain is an ABI-affecting decision.
@_cdecl("compositor_composite_over")
public func compositorCompositeOver(
    dstRGBA: UnsafeMutablePointer<UInt8>?,
    srcRGBA: UnsafePointer<UInt8>?,
    coverage: UnsafePointer<UInt8>?,
    width: Int,
    height: Int,
    stride: Int,
    opacity: Float
) -> Int32 {
    // ENG-15: validate geometry at the ABI entry; reject malformed input before
    // any pixel work rather than passing bad geometry to the kernels. The dst/src
    // pointers are imported as nullable so a NULL from the C side fails safely here.
    guard width > 0, height > 0, stride >= width * 4 else { return -1 }
    guard let dst = dstRGBA, let src = srcRGBA else { return -1 }

    let opacity = min(max(opacity, 0.0), 1.0)

    for y in 0..<height {
        let rowOffset = y * stride
        let coverageRowOffset = y * width
        for x in 0..<width {
            let d = rowOffset + x * 4
            let sr = Float(src[d]);     let sg = Float(src[d + 1])
            let sb = Float(src[d + 2]); let sa = Float(src[d + 3])
            let dr = Float(dst[d]);     let dg = Float(dst[d + 1])
            let db = Float(dst[d + 2]); let da = Float(dst[d + 3])

            let coverageValue: Float = coverage.map { Float($0[coverageRowOffset + x]) / 255.0 } ?? 1.0

            // Effective source alpha (0..1): src alpha scaled by coverage and opacity.
            let effectiveAlpha = (sa / 255.0) * coverageValue * opacity
            let inverse = 1.0 - effectiveAlpha

            // Premultiplied source-over. Source color is premultiplied, so scaling
            // the stored premultiplied channels by (coverage*opacity) yields the
            // source contribution at the effective alpha.
            dst[d]     = clampU8(sr * coverageValue * opacity + dr * inverse)
            dst[d + 1] = clampU8(sg * coverageValue * opacity + dg * inverse)
            dst[d + 2] = clampU8(sb * coverageValue * opacity + db * inverse)
            dst[d + 3] = clampU8(sa * coverageValue * opacity + da * inverse)
        }
    }
    return 0
}

@inlinable
public func clampU8(_ value: Float) -> UInt8 {
    if value.isNaN { return 0 }
    if value <= 0 { return 0 }
    if value >= 255 { return 255 }
    // Round to nearest.
    return UInt8(value + 0.5)
}