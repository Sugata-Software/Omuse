// CoreGraphicsCompat/Geometry.swift — Geometry extensions for CoreGraphics parity.
//
// Plan §3, §5: Complements CompositorGeometry.swift with convenience members
// expected by CoreGraphics-oriented rendering code.
// Foundation on Linux already ships CGRect.integral, offsetBy, insetBy,
// so those are inherited from Foundation directly.

import Foundation

extension CGAffineTransform {
    /// Returns true if this transform is identity.
    public var isIdentity: Bool {
        self == .identity
    }
}
