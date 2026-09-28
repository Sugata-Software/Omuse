// Geometry unit tests for the portable CoreGraphics replacements. These pin the
// CG semantics the ported document/Rendering logic depends on (y-down edges,
// null/empty, integral, intersection, affine concat/invert). Run in-sandbox:
//   flatpak run --command=bash org.kde.Sdk//6.10 -c \
//     'export PATH=/usr/lib/sdk/swift6/bin:$PATH; swift test'

import CoreGraphics
import XCTest

final class CompositorGeometryTests: XCTestCase {

    func testRectEdgesAndStandardization() {
        let r = CGRect(x: 5, y: 6, width: 3, height: 4)
        XCTAssertEqual(r.minX, 5); XCTAssertEqual(r.minY, 6)
        XCTAssertEqual(r.maxX, 8); XCTAssertEqual(r.maxY, 10)
        XCTAssertEqual(r.midX, 6.5); XCTAssertEqual(r.midY, 8)
        // A negative-size rect standardizes to its top-left corner with positive size.
        let neg = CGRect(x: 8, y: 10, width: -3, height: -4)
        XCTAssertEqual(neg.standardized, CGRect(x: 5, y: 6, width: 3, height: 4))
    }

    func testIntegralFloorsOriginAndCeilsMax() {
        let r = CGRect(x: 1.2, y: 2.7, width: 3.1, height: 4.6)
        XCTAssertEqual(r.integral, CGRect(x: 1, y: 2, width: 4, height: 6))
    }

    func testNullAndIntersection() {
        let a = CGRect(x: 0, y: 0, width: 10, height: 10)
        let b = CGRect(x: 20, y: 20, width: 5, height: 5)
        XCTAssertTrue(a.intersection(b).isNull)
        XCTAssertTrue(a.intersection(b).isEmpty)
        XCTAssertEqual(a.intersection(CGRect(x: 5, y: 5, width: 10, height: 10)),
                       CGRect(x: 5, y: 5, width: 5, height: 5))
        XCTAssertFalse(a.intersects(b))
        XCTAssertTrue(a.intersects(CGRect(x: 5, y: 5, width: 10, height: 10)))
    }

    func testUnionWithNullIsSelf() {
        let r = CGRect(x: 1, y: 2, width: 3, height: 4)
        XCTAssertEqual(r.union(.null), r)
        XCTAssertEqual(.null.union(r), r)
        XCTAssertEqual(CGRect(x: 0, y: 0, width: 5, height: 5).union(CGRect(x: 3, y: 3, width: 5, height: 5)),
                       CGRect(x: 0, y: 0, width: 8, height: 8))
    }

    func testContainsPointExclusiveAtMax() {
        let r = CGRect(x: 0, y: 0, width: 10, height: 10)
        XCTAssertTrue(r.contains(CGPoint(x: 0, y: 0)))
        XCTAssertTrue(r.contains(CGPoint(x: 9.9, y: 9.9)))
        XCTAssertFalse(r.contains(CGPoint(x: 10, y: 10)))  // max edge exclusive (CG semantics)
    }

    func testOffsetAndInset() {
        let r = CGRect(x: 1, y: 2, width: 10, height: 10)
        XCTAssertEqual(r.offsetBy(dx: 3, dy: -1), CGRect(x: 4, y: 1, width: 10, height: 10))
        XCTAssertEqual(r.insetBy(dx: 1, dy: 2), CGRect(x: 2, y: 4, width: 8, height: 6))
    }

    func testAffineTransformPointMapping() {
        // Identity leaves the point unchanged.
        let p = CGPoint(x: 3, y: 4)
        XCTAssertEqual(p.applying(.identity), p)
        // Translation adds.
        let t = CGAffineTransform(translationX: 10, y: 20)
        XCTAssertEqual(CGPoint(x: 3, y: 4).applying(t), CGPoint(x: 13, y: 24))
        // Scale multiplies.
        let s = CGAffineTransform(scaleX: 2, y: 3)
        XCTAssertEqual(CGPoint(x: 3, y: 4).applying(s), CGPoint(x: 6, y: 12))
    }

    func testAffineTransformConcatenationOrder() {
        // translate then scale: scale applies first (closer to the point), then translate.
        let scale = CGAffineTransform(scaleX: 2, y: 2)
        let translate = CGAffineTransform(translationX: 10, y: 0)
        // t.concatenating(s) means t(s(point)): scale first, then translate.
        let combined = translate.concatenating(scale)
        XCTAssertEqual(CGPoint(x: 3, y: 4).applying(combined), CGPoint(x: 16, y: 8))
    }

    func testAffineTransformInvertedRoundTrip() {
        let t = CGAffineTransform(rotationAngle: .pi / 4).concatenating(CGAffineTransform(translationX: 5, y: 6))
        let p = CGPoint(x: 7, y: 8)
        let mapped = p.applying(t)
        let back = mapped.applying(t.inverted())
        XCTAssertEqual(back.x, p.x, accuracy: 1e-9)
        XCTAssertEqual(back.y, p.y, accuracy: 1e-9)
    }

    func testAffineScaledByTranslatedByRotated() {
        // pixelToDocument-style chain: translate . rotate . scale . translate (all
        // non-mutating, value-returning, as the ported BrushRaster code expects).
        let chain = CGAffineTransform(translationX: 50, y: 50)
            .rotated(by: .pi / 2)
            .scaledBy(x: 2, y: 1)
            .translatedBy(x: -1, y: -1)
        // Must be finite and non-identity.
        XCTAssertTrue(chain.a.isFinite && chain.d.isFinite)
        XCTAssertNotEqual(chain, .identity)
    }

    func testRectApplyingReturnsBoundingBox() {
        let r = CGRect(x: 0, y: 0, width: 2, height: 2)
        let scale = CGAffineTransform(scaleX: 3, y: 4)
        XCTAssertEqual(r.applying(scale), CGRect(x: 0, y: 0, width: 6, height: 8))
    }

    func testInterpolationQualityCases() {
        // Compile-time presence of the cases the raster layer selects between.
        let cases: [CGInterpolationQuality] = [.default, .none, .low, .medium, .high]
        XCTAssertEqual(cases.count, 5)
    }
}