// CoreGraphicsPathTests — the Apple-shaped CGPath / CGContext / CGGradient surface of the CoreGraphics compat
// module. Geometry queries are pure Swift and always run; booleans, strokes and gradients need the Skia bridge
// (set COMPOSITOR_SKIA_BRIDGE to a built libCompositorSkiaBridge.so) and skip when it is absent.

import CoreGraphics
import XCTest
@testable import CoreGraphics

final class CoreGraphicsPathTests: XCTestCase {

    private func requireSkia() throws {
        try XCTSkipUnless(SkiaPathABI.available, "Skia bridge with PathOps not loaded")
    }

    func testRectAndEllipseHitTesting() {
        let rect = CGPath(rect: CGRect(x: 10, y: 10, width: 20, height: 20))
        XCTAssertTrue(rect.contains(CGPoint(x: 15, y: 15)))
        XCTAssertFalse(rect.contains(CGPoint(x: 5, y: 15)))
        let ellipse = CGPath(ellipseIn: CGRect(x: 0, y: 0, width: 40, height: 20))
        XCTAssertTrue(ellipse.contains(CGPoint(x: 20, y: 10)))
        XCTAssertFalse(ellipse.contains(CGPoint(x: 1, y: 1)), "the corner of the bounds is outside an ellipse")
    }

    func testWindingVersusEvenOddForNestedRects() {
        let path = CGMutablePath()
        path.addRect(CGRect(x: 0, y: 0, width: 40, height: 40))
        path.addRect(CGRect(x: 10, y: 10, width: 20, height: 20))
        let centre = CGPoint(x: 20, y: 20)
        XCTAssertTrue(path.contains(centre, using: .winding), "same orientation nests to winding 2")
        XCTAssertFalse(path.contains(centre, using: .evenOdd))
    }

    func testBoundsCurvesAndCopyUsing() {
        let ellipse = CGPath(ellipseIn: CGRect(x: 5, y: 5, width: 30, height: 10))
        let tight = ellipse.boundingBoxOfPath
        XCTAssertEqual(tight.minX, 5, accuracy: 0.2); XCTAssertEqual(tight.width, 30, accuracy: 0.2)
        XCTAssertEqual(tight.height, 10, accuracy: 0.2)
        var shift = CGAffineTransform(translationX: 100, y: 50)
        let moved = try! XCTUnwrap(ellipse.copy(using: &shift))
        XCTAssertEqual(moved.boundingBoxOfPath.minX, 105, accuracy: 0.2)
        XCTAssertTrue(CGPath().boundingBox.isNull)
        XCTAssertEqual(ellipse.currentPoint.x, 35, accuracy: 0.001)
    }

    func testApplyWithBlockEnumeratesApplesElementTypes() {
        let path = CGMutablePath()
        path.move(to: CGPoint(x: 0, y: 0))
        path.addLine(to: CGPoint(x: 10, y: 0))
        path.addQuadCurve(to: CGPoint(x: 10, y: 10), control: CGPoint(x: 15, y: 5))
        path.addCurve(to: CGPoint(x: 0, y: 10), control1: CGPoint(x: 8, y: 14), control2: CGPoint(x: 2, y: 14))
        path.closeSubpath()
        var types: [CGPathElementType] = []
        var lastPoint = CGPoint.zero
        path.applyWithBlock { element in
            types.append(element.pointee.type)
            if element.pointee.type == .addCurveToPoint { lastPoint = element.pointee.points[2] }
        }
        XCTAssertEqual(types, [.moveToPoint, .addLineToPoint, .addQuadCurveToPoint, .addCurveToPoint, .closeSubpath])
        XCTAssertEqual(lastPoint, CGPoint(x: 0, y: 10))
    }

    func testAddLinesBetweenAndRoundedRect() {
        let poly = CGMutablePath()
        poly.addLines(between: [CGPoint(x: 0, y: 0), CGPoint(x: 20, y: 0), CGPoint(x: 10, y: 20)])
        poly.closeSubpath()
        XCTAssertTrue(poly.contains(CGPoint(x: 10, y: 5)))
        XCTAssertFalse(poly.contains(CGPoint(x: 1, y: 15)))
        let rounded = CGPath(roundedRect: CGRect(x: 0, y: 0, width: 40, height: 40), cornerWidth: 12, cornerHeight: 12)
        XCTAssertTrue(rounded.contains(CGPoint(x: 20, y: 20)))
        XCTAssertFalse(rounded.contains(CGPoint(x: 1, y: 1)), "the rounded corner is cut away")
    }

    func testBooleanOperationsMatchSetSemantics() throws {
        try requireSkia()
        let a = CGPath(rect: CGRect(x: 0, y: 0, width: 20, height: 20))
        let b = CGPath(rect: CGRect(x: 10, y: 10, width: 20, height: 20))
        let union = a.union(b), inter = a.intersection(b), diff = a.subtracting(b), xor = a.symmetricDifference(b)
        let onlyA = CGPoint(x: 5, y: 5), both = CGPoint(x: 15, y: 15), onlyB = CGPoint(x: 25, y: 25), none = CGPoint(x: 25, y: 5)
        XCTAssertEqual([onlyA, both, onlyB, none].map { union.contains($0) }, [true, true, true, false])
        XCTAssertEqual([onlyA, both, onlyB, none].map { inter.contains($0) }, [false, true, false, false])
        XCTAssertEqual([onlyA, both, onlyB, none].map { diff.contains($0) }, [true, false, false, false])
        XCTAssertEqual([onlyA, both, onlyB, none].map { xor.contains($0) }, [true, false, true, false])
        XCTAssertEqual(inter.boundingBoxOfPath, CGRect(x: 10, y: 10, width: 10, height: 10))
    }

    func testStrokeOutline() throws {
        try requireSkia()
        let line = CGMutablePath()
        line.move(to: CGPoint(x: 0, y: 10)); line.addLine(to: CGPoint(x: 40, y: 10))
        let outline = line.copy(strokingWithWidth: 6, lineCap: .butt, lineJoin: .miter, miterLimit: 10)
        XCTAssertTrue(outline.contains(CGPoint(x: 20, y: 12)))
        XCTAssertFalse(outline.contains(CGPoint(x: 20, y: 14)))
        XCTAssertFalse(outline.contains(CGPoint(x: -2, y: 10)), "butt caps do not extend past the end")
    }

    func testFillPathHonoursTheShapeNotItsBoundingBox() {
        let ctx = CGContext(width: 20, height: 20)
        ctx.setFillColor(red: 1, green: 0, blue: 0, alpha: 1)
        ctx.addEllipse(in: CGRect(x: 0, y: 0, width: 20, height: 20))
        ctx.fillPath()
        let px = ctx.buffer
        func alpha(_ x: Int, _ y: Int) -> UInt8 { px.bytes[(y * 20 + x) * 4 + 3] }
        XCTAssertGreaterThan(alpha(10, 10), 250)
        XCTAssertLessThan(alpha(0, 0), 10, "the corner outside the circle stays empty")
    }

    func testLinearGradientAcrossTheCanvas() throws {
        try requireSkia()
        let ctx = CGContext(width: 100, height: 4)
        let gradient = try XCTUnwrap(CGGradient(colorSpace: CGColorSpaceCreateDeviceGray(),
                                                colorComponents: [0, 1, 1, 1], locations: [0, 1], count: 2))
        ctx.drawLinearGradient(gradient, start: CGPoint(x: 0, y: 0), end: CGPoint(x: 100, y: 0),
                               options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
        let px = ctx.buffer
        func red(_ x: Int) -> Int { Int(px.bytes[(1 * 100 + x) * 4]) }
        XCTAssertLessThan(red(2), 12)
        XCTAssertEqual(red(50), 128, accuracy: 8)
        XCTAssertGreaterThan(red(97), 243)
    }

    func testGradientRejectsTooFewStops() {
        XCTAssertNil(CGGradient(colorSpace: .deviceGraySpace, colorComponents: [0, 1], locations: [0], count: 1))
        let colors: CFArray = [CGColor.black, CGColor.white]
        XCTAssertNotNil(CGGradient(colorsSpace: .srgbSpace, colors: colors, locations: [0, 1]))
    }
}
