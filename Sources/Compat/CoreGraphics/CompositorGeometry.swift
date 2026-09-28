// CompositorGeometry — the portable affine-transform and geometry glue the Linux
// core needs. Foundation on Linux already ships CGPoint/CGRect/CGSize/CGFloat and
// most CGRect geometry members (minX/maxX/midX/midY, integral, standardized,
// intersection, union, contains, offsetBy, insetBy, isNull, isEmpty) — those are
// reused as-is. What it does NOT ship is CGAffineTransform, CGInterpolationQuality,
// the `applying(_:)` member on CGPoint/CGRect, or `CGSize.isEmpty`. This file fills
// exactly those gaps, mirroring CoreGraphics' matrix layout and concatenation
// order so ported macOS files substitute `import CompositorCore` for
// `import CoreGraphics` with no call-site changes.
//
// Coordinate system: origin top-left, y increases downward (same as CoreGraphics on
// macOS). Foundation's CGFloat is reused (no redefinition).
//
// This is the foundational geometry layer the file-map "Keep logic; replace Apple
// operations" tier builds on (50+ files reach for CGRect, 56 for CGFloat, 17 for
// CGAffineTransform). Foundation-only; builds on Linux via SwiftPM.

import Foundation

// MARK: - CGSize

extension CGSize {
    /// CG reports isEmpty when either dimension is <= 0. Foundation's CGSize on
    /// Linux lacks this member; add it to match CoreGraphics.
    public var isEmpty: Bool { width <= 0 || height <= 0 }
}

// MARK: - CGAffineTransform

/// Portable `CGAffineTransform`. 2D affine transform in CoreGraphics' matrix layout:
///
///     [ a  b  0 ]
///     [ c  d  0 ]
///     [ tx ty 1 ]
///
/// A point (x, y) maps to (a*x + c*y + tx, b*x + d*y + ty). `concatenating(other)`
/// yields a transform equivalent to applying `other` first, then `self`
/// (`self(other(point))`), matching `CGAffineTransform.concatenating`.
///
/// Foundation on Linux does not provide CGAffineTransform at all, so this is the
/// canonical definition for the Linux core, not a redefinition.
public struct CGAffineTransform: Equatable, Sendable, Codable {
    public var a: CGFloat
    public var b: CGFloat
    public var c: CGFloat
    public var d: CGFloat
    public var tx: CGFloat
    public var ty: CGFloat

    public init(a: CGFloat, b: CGFloat, c: CGFloat, d: CGFloat, tx: CGFloat = 0, ty: CGFloat = 0) {
        self.a = a; self.b = b; self.c = c; self.d = d; self.tx = tx; self.ty = ty
    }
    public init(translationX tx: CGFloat, y ty: CGFloat) {
        self.a = 1; self.b = 0; self.c = 0; self.d = 1; self.tx = tx; self.ty = ty
    }
    public init(scaleX sx: CGFloat, y sy: CGFloat) {
        self.a = sx; self.b = 0; self.c = 0; self.d = sy; self.tx = 0; self.ty = 0
    }
    public init(rotationAngle angle: CGFloat) {
        self.a = cos(angle); self.b = sin(angle)
        self.c = -sin(angle); self.d = cos(angle); self.tx = 0; self.ty = 0
    }

    public static let identity = CGAffineTransform(a: 1, b: 0, c: 0, d: 1)

    /// `self(other(point))` — apply `other` first, then `self`. CoreGraphics uses
    /// row-vector convention (`point' = point × M`), so the combined matrix is
    /// `M_other × M_self` (other left-multiplied) to keep other applied first.
    public func concatenating(_ other: CGAffineTransform) -> CGAffineTransform {
        CGAffineTransform(
            a: other.a * a + other.b * c,
            b: other.a * b + other.b * d,
            c: other.c * a + other.d * c,
            d: other.c * b + other.d * d,
            tx: other.tx * a + other.ty * c + tx,
            ty: other.tx * b + other.ty * d + ty)
    }

    /// The inverse transform; returns identity if singular (determinant ~0). CG
    /// returns the original if non-invertible; we return identity to stay finite.
    public func inverted() -> CGAffineTransform {
        let det = a * d - b * c
        guard abs(det) > 0 else { return .identity }
        let inv = 1 / det
        return CGAffineTransform(
            a: d * inv,
            b: -b * inv,
            c: -c * inv,
            d: a * inv,
            tx: (c * ty - d * tx) * inv,
            ty: (b * tx - a * ty) * inv)
    }

    /// Scale, applied in the transformed coordinate space: `self.concatenating(scale)`.
    /// Non-mutating and value-returning, matching CG's `scaledBy(x:y:)`.
    public func scaledBy(x sx: CGFloat, y sy: CGFloat) -> CGAffineTransform {
        concatenating(CGAffineTransform(scaleX: sx, y: sy))
    }
    /// Translate, applied in the transformed coordinate space: `self.concatenating(translation)`.
    public func translatedBy(x tx: CGFloat, y ty: CGFloat) -> CGAffineTransform {
        concatenating(CGAffineTransform(translationX: tx, y: ty))
    }
    /// Rotate, applied in the transformed coordinate space: `self.concatenating(rotation)`.
    public func rotated(by angle: CGFloat) -> CGAffineTransform {
        concatenating(CGAffineTransform(rotationAngle: angle))
    }
}

// MARK: - CGPoint / CGRect applying

extension CGPoint {
    /// Apply an affine transform to this point, matching CoreGraphics' `applying(_:)`.
    /// Foundation's CGPoint on Linux lacks this member.
    public func applying(_ t: CGAffineTransform) -> CGPoint {
        CGPoint(x: t.a * x + t.c * y + t.tx,
                y: t.b * x + t.d * y + t.ty)
    }
}

extension CGRect {
    /// The smallest rect containing this rect after the affine transform is applied
    /// to its four corners, matching `CGRectApplyAffineTransform` returning a rect.
    /// Foundation's CGRect on Linux lacks this member.
    public func applying(_ t: CGAffineTransform) -> CGRect {
        if isNull { return .null }
        let p1 = CGPoint(x: minX, y: minY).applying(t)
        let p2 = CGPoint(x: maxX, y: minY).applying(t)
        let p3 = CGPoint(x: minX, y: maxY).applying(t)
        let p4 = CGPoint(x: maxX, y: maxY).applying(t)
        let xs = [p1.x, p2.x, p3.x, p4.x], ys = [p1.y, p2.y, p3.y, p4.y]
        guard let minX = xs.min(), let maxX = xs.max(),
              let minY = ys.min(), let maxY = ys.max() else { return .null }
        return CGRect(x: minX, y: minY, width: maxX - minX, height: maxY - minY)
    }
}

// MARK: - CGInterpolationQuality

/// Portable `CGInterpolationQuality`. Used by the raster/brush layer to choose
/// resampling quality; Foundation on Linux does not provide it. The raster backend
/// maps it to a concrete resampler in a later milestone.
public enum CGInterpolationQuality: Sendable {
    case `default`
    case none
    case low
    case medium
    case high
}