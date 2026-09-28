// Tests for the CoreGraphicsCompat CGContext shim (plan §3, §4). Verify the
// CG-shaped drawing surface routes through an injected render closure and
// falls back to the pure-Swift LayerRenderer when no device is registered.
// Run in-sandbox: swift test.

import CoreGraphics
import XCTest

final class CoreGraphicsCompatTests: XCTestCase {


    private func makeRedImage(_ w: Int, _ h: Int) -> PortableImage {
        var buf = PixelBuffer(width: w, height: h)
        for y in 0..<h { for x in 0..<w { buf[x, y] = (255, 0, 0, 255) } }
        return PortableImage(buf)
    }

    func testDrawWithIdentityClosureCopiesPixels() {
        let img = makeRedImage(2, 2)
        let ctx = CGContextCompat(width: 2, height: 2) { src, dst, w, h in
            let n = w * h * 4
            for i in 0..<n { dst[i] = src[i] }
            return 0
        }
        ctx.draw(img, in: CGRect(x: 0, y: 0, width: 2, height: 2))
        let out = ctx.makeImage()!
        XCTAssertEqual(out.bytes, img.bytes)
    }

    func testDrawFallsBackToSwiftWhenClosureReturnsNonZero() {
        let img = makeRedImage(2, 2)
        // Closure "fails" (device lost, plan §6); Swift fallback must draw red.
        let ctx = CGContextCompat(width: 2, height: 2) { _, _, _, _ in -2 }
        ctx.draw(img, in: CGRect(x: 0, y: 0, width: 2, height: 2))
        let out = ctx.makeImage()!
        for p in 0..<4 {
            let i = p * 4
            XCTAssertEqual(out.bytes[i], 255, "red channel at \(p)")
            XCTAssertEqual(out.bytes[i + 3], 255, "alpha at \(p)")
        }
    }

    func testDrawFallsBackToSwiftWithNoClosure() {
        let img = makeRedImage(1, 1)
        let ctx = CGContextCompat(width: 1, height: 1, render: nil)
        ctx.draw(img, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        let out = ctx.makeImage()!
        XCTAssertEqual(out.bytes[0], 255)
        XCTAssertEqual(out.bytes[3], 255)
    }

    func testNonIdentityRectUsesSwiftFallback() {
        let img = makeRedImage(2, 2)
        // A capturing closure cannot be converted to a C function pointer, so
        // use a non-capturing "always fail" closure: if the Skia path ran it
        // would return 0 and the test would see copied bytes; the Swift
        // fallback draws a scaled red rect into the 4x4 context instead.
        let ctx = CGContextCompat(width: 4, height: 4) { _, _, _, _ in -1 }
        // Scaled rect -> not identity -> Swift path, closure must not run.
        ctx.draw(img, in: CGRect(x: 1, y: 1, width: 2, height: 2))
        let out = ctx.makeImage()!
        // Swift fallback draws red into the placed rect; pixels at (1,1)
        // should be red, pixels at (0,0) should be transparent.
        let i00 = 0 * 4
        XCTAssertEqual(out.bytes[i00 + 3], 0, "corner alpha is transparent")
        // Inside the placed rect the Swift LayerRenderer drew red.
        let i11 = (1 * 4 + 1) * 4
        XCTAssertEqual(out.bytes[i11], 255, "red at (1,1)")
        XCTAssertEqual(out.bytes[i11 + 3], 255, "alpha at (1,1)")
    }

    // MARK: - Step 7 Parity Tests (Analytical & Oracle)

    func testPartialAlphaOverNonEmptyDestination() {
        // Setup a 2x2 backdrop: opaque blue (0, 0, 255, 255)
        var backdropBuf = PixelBuffer(width: 2, height: 2)
        for y in 0..<2 {
            for x in 0..<2 { backdropBuf[x, y] = (0, 0, 255, 255) }
        }
        let ctx = CGContext(buffer: backdropBuf)

        // Draw 50% transparent red: premultiplied RGBA (128, 0, 0, 128)
        var redBuf = PixelBuffer(width: 2, height: 2)
        for y in 0..<2 {
            for x in 0..<2 { redBuf[x, y] = (128, 0, 0, 128) }
        }
        let redImg = CGImage(redBuf)

        ctx.draw(redImg, in: CGRect(x: 0, y: 0, width: 2, height: 2))
        let out = ctx.makeImage()!

        // Analytical source-over:
        // out.r = src.r + dst.r * (255 - src.a) / 255 = 128 + 0 = 128
        // out.g = 0
        // out.b = src.b + dst.b * (255 - src.a) / 255 = 0 + (255 * 127 + 127) / 255 = 127
        // out.a = src.a + dst.a * (255 - src.a) / 255 = 128 + 127 = 255
        for p in 0..<4 {
            let offset = p * 4
            let r = Int(out.bytes[offset])
            let g = Int(out.bytes[offset + 1])
            let b = Int(out.bytes[offset + 2])
            let a = Int(out.bytes[offset + 3])
            XCTAssertEqual(r, 128, accuracy: 1, "red channel at pixel \(p)")
            XCTAssertEqual(g, 0, accuracy: 1, "green channel at pixel \(p)")
            XCTAssertEqual(b, 127, accuracy: 1, "blue channel at pixel \(p)")
            XCTAssertEqual(a, 255, accuracy: 1, "alpha channel at pixel \(p)")
        }
    }

    func testTransformTranslateScaleRotate() {
        let ctx = CGContext(width: 8, height: 8)
        ctx.saveGState()
        ctx.translateBy(x: 2, y: 3)
        ctx.scaleBy(x: 2, y: 2)

        let t = ctx.userSpaceToDeviceSpaceTransform
        XCTAssertEqual(t.a, 2, accuracy: 1e-4)
        XCTAssertEqual(t.d, 2, accuracy: 1e-4)
        XCTAssertEqual(t.tx, 2, accuracy: 1e-4)
        XCTAssertEqual(t.ty, 3, accuracy: 1e-4)

        ctx.restoreGState()
        let tRestored = ctx.userSpaceToDeviceSpaceTransform
        XCTAssertEqual(tRestored.a, 1, accuracy: 1e-4)
        XCTAssertEqual(tRestored.d, 1, accuracy: 1e-4)
        XCTAssertEqual(tRestored.tx, 0, accuracy: 1e-4)
        XCTAssertEqual(tRestored.ty, 0, accuracy: 1e-4)
    }

    func testRectClipping() {
        let ctx = CGContext(width: 8, height: 8)
        ctx.clip(to: CGRect(x: 2, y: 2, width: 4, height: 4))

        let bounds = ctx.boundingBoxOfClipPath
        XCTAssertEqual(bounds.minX, 2, accuracy: 1e-4)
        XCTAssertEqual(bounds.minY, 2, accuracy: 1e-4)
        XCTAssertEqual(bounds.width, 4, accuracy: 1e-4)
        XCTAssertEqual(bounds.height, 4, accuracy: 1e-4)

        ctx.setFillColor(red: 1, green: 1, blue: 1, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: 8, height: 8))

        let out = ctx.makeImage()!
        // (0,0) is outside the clip: must remain 0
        let p00 = (0 * 8 + 0) * 4
        XCTAssertEqual(out.bytes[p00 + 3], 0, "outside clip must be transparent")

        // (2,2) is inside the clip: must be white (255, 255, 255, 255)
        let p22 = (2 * 8 + 2) * 4
        XCTAssertEqual(out.bytes[p22], 255, "inside clip red")
        XCTAssertEqual(out.bytes[p22 + 1], 255, "inside clip green")
        XCTAssertEqual(out.bytes[p22 + 2], 255, "inside clip blue")
        XCTAssertEqual(out.bytes[p22 + 3], 255, "inside clip alpha")
    }

    func testPathClipping() {
        let ctx = CGContext(width: 8, height: 8)
        let path = CGMutablePath()
        path.addRect(CGRect(x: 1, y: 1, width: 5, height: 5))
        ctx.addPath(path)
        ctx.clip()

        ctx.setFillColor(red: 0, green: 1, blue: 0, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: 8, height: 8))

        let out = ctx.makeImage()!
        // (0,0) outside path
        XCTAssertEqual(out.bytes[3], 0)
        // (1,1) inside path: green
        let p11 = (1 * 8 + 1) * 4
        XCTAssertEqual(out.bytes[p11 + 1], 255)
        XCTAssertEqual(out.bytes[p11 + 3], 255)
    }

    func testAlphaMaskClipping() {
        let ctx = CGContext(width: 4, height: 4)
        // Create 4x4 mask: left half (x=0,1) opaque (255), right half (x=2,3) 0
        var mask = MaskBuffer(width: 4, height: 4)
        for y in 0..<4 {
            for x in 0..<4 {
                mask[x, y] = x < 2 ? 255 : 0
            }
        }
        let maskImage = CGImage(mask: mask)
        ctx.clip(to: CGRect(x: 0, y: 0, width: 4, height: 4), mask: maskImage)

        ctx.setFillColor(red: 1, green: 0, blue: 0, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: 4, height: 4))

        let out = ctx.makeImage()!
        // Left half (1,1) should be red
        let pLeft = (1 * 4 + 1) * 4
        XCTAssertEqual(out.bytes[pLeft], 255)
        XCTAssertEqual(out.bytes[pLeft + 3], 255)
        // Right half (3,1) should be transparent
        let pRight = (1 * 4 + 3) * 4
        XCTAssertEqual(out.bytes[pRight + 3], 0)
    }

    func testBlendModesParity() {
        // Multiply: 0.5 * 0.5 ~ 0.25 (64)
        var buf = PixelBuffer(width: 2, height: 2)
        for y in 0..<2 {
            for x in 0..<2 { buf[x, y] = (128, 128, 128, 255) }
        }
        let ctx = CGContext(buffer: buf)
        ctx.setBlendMode(.multiply)

        var top = PixelBuffer(width: 2, height: 2)
        for y in 0..<2 {
            for x in 0..<2 { top[x, y] = (128, 128, 128, 255) }
        }
        ctx.draw(CGImage(top), in: CGRect(x: 0, y: 0, width: 2, height: 2))

        let out = ctx.makeImage()!
        // Expected ~ 64 (128 * 128 / 255)
        XCTAssertEqual(Int(out.bytes[0]), 64, accuracy: 2)
        XCTAssertEqual(Int(out.bytes[3]), 255)

        // Clear: clears pixels to 0
        ctx.clear(CGRect(x: 0, y: 0, width: 2, height: 2))
        let cleared = ctx.makeImage()!
        for b in cleared.bytes {
            XCTAssertEqual(b, 0)
        }
    }

    func testTransparencyLayer() {
        let ctx = CGContext(width: 4, height: 4)
        ctx.setFillColor(red: 0, green: 0, blue: 1, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: 4, height: 4))

        ctx.beginTransparencyLayer()
        ctx.setAlpha(0.5)
        ctx.setFillColor(red: 1, green: 0, blue: 0, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: 4, height: 4))
        ctx.endTransparencyLayer()

        let out = ctx.makeImage()!
        // Red 0.5 over Blue 1.0 -> R ~ 128, G = 0, B ~ 127, A = 255
        XCTAssertEqual(Int(out.bytes[0]), 128, accuracy: 2)
        XCTAssertEqual(Int(out.bytes[2]), 127, accuracy: 2)
        XCTAssertEqual(out.bytes[3], 255)
    }

    func testImageDrawingAppliesContextAlphaOnce() throws {
        // Exercise both the Linux convenience context and Apple's y-up bitmap context.
        let contexts = [CGContext(width: 3, height: 1), try XCTUnwrap(CGContext(
            data: nil, width: 3, height: 1, bitsPerComponent: 8, bytesPerRow: 12,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))]
        let red = makeRedImage(1, 1)
        for ctx in contexts {
            ctx.saveGState()
            ctx.setAlpha(0.5)
            ctx.draw(red, in: CGRect(x: 0, y: 0, width: 1, height: 1))
            ctx.draw(red, in: CGRect(x: 1, y: 0, width: 1, height: 1), opacity: 0.5)
            ctx.restoreGState()
            ctx.draw(red, in: CGRect(x: 2, y: 0, width: 1, height: 1))
            let pixels = try XCTUnwrap(ctx.makeImage()).bytes
            for (x, expected) in [128, 64, 255].enumerated() {
                XCTAssertEqual(Int(pixels[x * 4]), expected, accuracy: 1)
                XCTAssertEqual(Int(pixels[x * 4 + 3]), expected, accuracy: 1)
            }
        }
    }

    func testMaskedImageDrawingMultipliesCoverageAndContextAlphaOnce() throws {
        let ctx = CGContext(width: 1, height: 1)
        var mask = MaskBuffer(width: 1, height: 1)
        mask[0, 0] = 128
        let rect = CGRect(x: 0, y: 0, width: 1, height: 1)
        ctx.clip(to: rect, mask: CGImage(mask: mask))
        ctx.setAlpha(0.5)
        ctx.draw(makeRedImage(1, 1), in: rect)
        let pixels = try XCTUnwrap(ctx.makeImage()).bytes
        XCTAssertEqual(Int(pixels[0]), 64, accuracy: 1)
        XCTAssertEqual(Int(pixels[3]), 64, accuracy: 1)
    }

    func testCGPathOperations() {
        let path = CGMutablePath()
        XCTAssertTrue(path.isEmpty)

        path.addRect(CGRect(x: 10, y: 20, width: 30, height: 40))
        XCTAssertFalse(path.isEmpty)
        XCTAssertEqual(path.boundingBox, CGRect(x: 10, y: 20, width: 30, height: 40))

        let copy = path.copy()
        XCTAssertEqual(copy.boundingBox, CGRect(x: 10, y: 20, width: 30, height: 40))

        let cut = CGMutablePath()
        cut.addRect(CGRect(x: 15, y: 25, width: 10, height: 10))
        let subtracted = path.subtracting(cut, using: .winding)
        XCTAssertFalse(subtracted.isEmpty)
    }

    func testExternalBufferSynchronization() {
        var pixel = [UInt8](repeating: 0, count: 4)
        pixel.withUnsafeMutableBytes { bytes in
            guard let ctx = CGContext(
                data: bytes.baseAddress,
                width: 1,
                height: 1,
                bitsPerComponent: 8,
                bytesPerRow: 4,
                space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else {
                XCTFail("Failed to create context")
                return
            }
            ctx.setFillColor(red: 1, green: 0, blue: 0, alpha: 1)
            ctx.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
            ctx.flush()
        }
        XCTAssertEqual(pixel[0], 255, "red channel")
        XCTAssertEqual(pixel[1], 0, "green channel")
        XCTAssertEqual(pixel[2], 0, "blue channel")
        XCTAssertEqual(pixel[3], 255, "alpha channel")
    }
}

