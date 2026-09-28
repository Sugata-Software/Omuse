// CoreImageCompatTests — the CIImage/CIFilter/CIContext subset upstream uses, checked against values worked out by
// hand (identity round trips, mask blending, blend-mode formulas, blur edge behaviour, orientation).

import CoreGraphics
import XCTest
import CoreImage
import CoreVideo

final class CoreImageCompatTests: XCTestCase {
    private let unmanaged = CIContext(options: [.workingColorSpace: NSNull(), .outputColorSpace: NSNull()])
    private let managed = CIContext(options: [.cacheIntermediates: false])

    private func rgba(_ w: Int, _ h: Int, _ f: (Int, Int) -> (UInt8, UInt8, UInt8, UInt8)) -> CGImage {
        var b = PixelBuffer(width: w, height: h)
        for y in 0..<h { for x in 0..<w { b[x, y] = f(x, y) } }
        return CGImage(b)
    }
    private func gray(_ w: Int, _ h: Int, _ f: (Int, Int) -> UInt8) -> CGImage {
        CGImage(PortableImage(width: w, height: h, kind: .mask, bytesPerRow: w, bytes: (0..<(w * h)).map { f($0 % w, $0 / w) }))
    }
    private func render(_ image: CIImage, _ ctx: CIContext, _ w: Int, _ h: Int, mask: Bool = false) -> CGImage {
        ctx.createCGImage(image, from: CGRect(x: 0, y: 0, width: w, height: h), format: mask ? .L8 : .RGBA8,
                          colorSpace: mask ? CGColorSpaceCreateDeviceGray() : CGColorSpace(name: CGColorSpace.sRGB)!)!
    }
    private func px(_ image: CGImage, _ x: Int, _ y: Int) -> [Int] {
        let o = y * image.bytesPerRow + x * (image.isGrayPlane ? 1 : 4)
        return (0..<(image.isGrayPlane ? 1 : 4)).map { Int(image.portableImage.bytes[o + $0]) }
    }

    func testIdentityRoundTripKeepsPixelsAndTopLeftOrientation() {
        let image = rgba(4, 3) { x, y in (UInt8(x * 60), UInt8(y * 80), 40, 255) }
        let back = render(CIImage(cgImage: image), unmanaged, 4, 3)
        XCTAssertEqual(back.portableImage.bytes, image.portableImage.bytes)
        XCTAssertEqual(px(back, 3, 0), [180, 0, 40, 255], "row 0 is still the top row after the bottom-up CI flip")
        let g = gray(3, 2) { x, y in UInt8(x * 100 + y * 20) }
        XCTAssertEqual(render(CIImage(cgImage: g), unmanaged, 3, 2, mask: true).portableImage.bytes, g.portableImage.bytes)
    }

    func testManagedContextRoundTripsThroughLinearLight() {
        let image = rgba(2, 1) { x, _ in (UInt8(50 + x * 100), 128, 200, 255) }
        let back = render(CIImage(cgImage: image), managed, 2, 1)
        for i in 0..<8 { XCTAssertEqual(Int(back.portableImage.bytes[i]), Int(image.portableImage.bytes[i]), accuracy: 1) }
    }

    func testBlendWithMaskMixesBackgroundAndInput() {
        let fg = rgba(3, 1) { _, _ in (200, 0, 0, 255) }
        let bg = rgba(3, 1) { _, _ in (0, 0, 100, 255) }
        let mask = gray(3, 1) { x, _ in [0, 128, 255][x] }
        let out = render(CIImage(cgImage: fg).applyingFilter("CIBlendWithMask", parameters: [
            kCIInputBackgroundImageKey: CIImage(cgImage: bg), kCIInputMaskImageKey: CIImage(cgImage: mask)]), unmanaged, 3, 1)
        XCTAssertEqual(px(out, 0, 0), [0, 0, 100, 255])
        XCTAssertEqual(px(out, 1, 0)[0], 100, accuracy: 1)
        XCTAssertEqual(px(out, 1, 0)[2], 50, accuracy: 1)
        XCTAssertEqual(px(out, 2, 0), [200, 0, 0, 255])
    }

    func testColorMatrixColorClampAndIdentityCube() {
        let image = rgba(1, 1) { _, _ in (100, 150, 200, 255) }
        let scaled = render(CIImage(cgImage: image).applyingFilter("CIColorMatrix", parameters: [
            "inputRVector": CIVector(x: 0.5, y: 0, z: 0, w: 0), "inputGVector": CIVector(x: 0, y: 1, z: 0, w: 0),
            "inputBVector": CIVector(x: 0, y: 0, z: 1, w: 0), "inputBiasVector": CIVector(x: 0, y: 0, z: -0.1, w: 0)]), unmanaged, 1, 1)
        XCTAssertEqual(px(scaled, 0, 0)[0], 50, accuracy: 1)
        XCTAssertEqual(px(scaled, 0, 0)[2], 174, accuracy: 1)
        let clamped = render(CIImage(cgImage: image).applyingFilter("CIColorClamp", parameters: [
            "inputMinComponents": CIVector(x: 0.5, y: 0.5, z: 0.5, w: 0), "inputMaxComponents": CIVector(x: 0.7, y: 0.7, z: 0.7, w: 1)]), unmanaged, 1, 1)
        XCTAssertEqual(px(clamped, 0, 0)[0], 128, accuracy: 1, "0.39 is raised to the 0.5 floor")
        XCTAssertEqual(px(clamped, 0, 0)[2], 179, accuracy: 1, "0.78 is lowered to the 0.7 ceiling")
        // 3-point identity cube (r fastest): entry(r,g,b) = (r,g,b,1) / 2.
        var cube: [Float] = []
        for b in 0..<3 { for g in 0..<3 { for r in 0..<3 { cube += [Float(r) / 2, Float(g) / 2, Float(b) / 2, 1] } } }
        let through = render(CIImage(cgImage: image).applyingFilter("CIColorCube", parameters: [
            "inputCubeDimension": 3, "inputCubeData": cube.withUnsafeBytes { Data($0) }]), unmanaged, 1, 1)
        XCTAssertEqual(through.portableImage.bytes, image.portableImage.bytes)
    }

    func testInspectGaussianBlurFilter() {
        let img = rgba(40, 20) { x, y in x < 20 ? (255, 255, 255, 255) : (0, 0, 0, 0) }
        let ci = CIImage(cgImage: img)
        let blurred = ci.applyingGaussianBlur(sigma: 3)
        let out = render(blurred.cropped(to: CGRect(x: 0, y: 0, width: 40, height: 20)), unmanaged, 40, 20)
        print("BLURRED DIRECT 40x20:")
        for x in [0, 5, 10, 15, 20, 25, 30, 35, 38] {
            print("DIRECT x=\(x) alpha=\(px(out, x, 10)[3])")
        }
    }

    func testColorDodgeAndBurnFollowTheBlendFormulas() {
        let src = rgba(1, 1) { _, _ in (128, 128, 128, 255) }
        let back = rgba(1, 1) { _, _ in (128, 128, 128, 255) }
        func blend(_ name: String) -> Int {
            let f = CIFilter(name: name)!
            f.setValue(CIImage(cgImage: src), forKey: kCIInputImageKey)
            f.setValue(CIImage(cgImage: back), forKey: kCIInputBackgroundImageKey)
            return px(render(f.outputImage!, unmanaged, 1, 1), 0, 0)[0]
        }
        XCTAssertEqual(blend("CIColorDodgeBlendMode"), 255, accuracy: 1, "0.5 / (1 - 0.5) saturates")
        XCTAssertEqual(blend("CIColorBurnBlendMode"), 0, accuracy: 3, "1 - (1 - 0.502) / 0.502 is about 0.008")
        // Managed context (with linear working space) must also respect perceptual sRGB formulas
        let top08 = rgba(1, 1) { _, _ in (UInt8((0.8 * 255).rounded()), UInt8((0.8 * 255).rounded()), UInt8((0.8 * 255).rounded()), 255) }
        let base04 = rgba(1, 1) { _, _ in (UInt8((0.4 * 255).rounded()), UInt8((0.4 * 255).rounded()), UInt8((0.4 * 255).rounded()), 255) }
        let burnFilter = CIFilter(name: "CIColorBurnBlendMode")!
        burnFilter.setValue(CIImage(cgImage: top08), forKey: kCIInputImageKey)
        burnFilter.setValue(CIImage(cgImage: base04), forKey: kCIInputBackgroundImageKey)
        let burnOut = px(render(burnFilter.outputImage!, managed, 1, 1), 0, 0)[0]
        XCTAssertEqual(burnOut, 64, accuracy: 2, "Color Burn of 0.4 under 0.8 is 0.25 (~64/255)")
        let dodgeFilter = CIFilter(name: "CIColorDodgeBlendMode")!
        dodgeFilter.setValue(CIImage(cgImage: top08), forKey: kCIInputImageKey)
        dodgeFilter.setValue(CIImage(cgImage: base04), forKey: kCIInputBackgroundImageKey)
        let dodgeOut = px(render(dodgeFilter.outputImage!, managed, 1, 1), 0, 0)[0]
        XCTAssertEqual(dodgeOut, 255, accuracy: 1, "Color Dodge of 0.4 under 0.8 is 1.0 (255/255)")
        XCTAssertNil(CIFilter(name: "CINotAFilter"))
    }

    func testGaussianBlurSpreadsAPointAndClampedExtentKeepsTheEdgeBright() {
        let dot = gray(9, 9) { x, y in x == 4 && y == 4 ? 255 : 0 }
        let blurred = CIImage(cgImage: dot).applyingGaussianBlur(sigma: 1.2)
        XCTAssertGreaterThan(blurred.extent.width, 9, "an unclamped blur grows the extent")
        let out = render(blurred.cropped(to: CGRect(x: 0, y: 0, width: 9, height: 9)), unmanaged, 9, 9, mask: true)
        XCTAssertGreaterThan(px(out, 4, 4)[0], px(out, 5, 4)[0]); XCTAssertEqual(px(out, 3, 4)[0], px(out, 5, 4)[0], accuracy: 1)
        XCTAssertGreaterThan(px(out, 4, 3)[0], 0)
        let flat = gray(8, 8) { _, _ in 200 }
        let soft = CIImage(cgImage: flat).clampedToExtent().applyingGaussianBlur(sigma: 2).cropped(to: CGRect(x: 0, y: 0, width: 8, height: 8))
        XCTAssertEqual(px(render(soft, unmanaged, 8, 8, mask: true), 0, 0)[0], 200, accuracy: 1, "clamping stops the border fading")
        let open = CIImage(cgImage: flat).applyingGaussianBlur(sigma: 2).cropped(to: CGRect(x: 0, y: 0, width: 8, height: 8))
        XCTAssertLessThan(px(render(open, unmanaged, 8, 8, mask: true), 0, 0)[0], 150, "without clamping the corner fades to transparent")
    }

    func testMotionBlurSmearsAlongItsAngleOnly() {
        let line = gray(11, 11) { x, _ in x == 5 ? 255 : 0 }   // a vertical line
        let smeared = render(CIImage(cgImage: line).clampedToExtent().applyingFilter("CIMotionBlur", parameters: [
            kCIInputRadiusKey: 2.0, kCIInputAngleKey: 0.0]).cropped(to: CGRect(x: 0, y: 0, width: 11, height: 11)), unmanaged, 11, 11, mask: true)
        XCTAssertGreaterThan(px(smeared, 4, 5)[0], 20, "spreads horizontally")
        XCTAssertGreaterThan(px(smeared, 6, 5)[0], 20)
        XCTAssertEqual(px(smeared, 5, 2)[0], px(smeared, 5, 8)[0], accuracy: 2, "uniform along the line's own direction")
    }

    func testTransformAndExifOrientationMoveTheRightPixels() {
        let img = gray(2, 1) { x, _ in x == 0 ? 10 : 20 }   // [10 20]
        XCTAssertEqual(CIImage(cgImage: img).transformed(by: CGAffineTransform(scaleX: 2, y: 3)).extent, CGRect(x: 0, y: 0, width: 4, height: 3))
        let rotated = CIImage(cgImage: img).oriented(forExifOrientation: 6)
        XCTAssertEqual(rotated.extent, CGRect(x: 0, y: 0, width: 1, height: 2))
        let out = render(rotated, unmanaged, 1, 2, mask: true)
        XCTAssertEqual(out.portableImage.bytes, [10, 20], "orientation 6: the left pixel ends up on top")
        let mirrored = render(CIImage(cgImage: img).oriented(forExifOrientation: 2), unmanaged, 2, 1, mask: true)
        XCTAssertEqual(mirrored.portableImage.bytes, [20, 10])
    }

    func testPerspectiveTransformWithTheSourceCornersIsTheIdentity() {
        let img = rgba(6, 4) { x, y in (UInt8(x * 40), UInt8(y * 60), 90, 255) }
        let e = CGRect(x: 0, y: 0, width: 6, height: 4)
        let warped = CIImage(cgImage: img).applyingFilter("CIPerspectiveTransform", parameters: [
            "inputTopLeft": CIVector(x: 0, y: e.maxY), "inputTopRight": CIVector(x: e.maxX, y: e.maxY),
            "inputBottomRight": CIVector(x: e.maxX, y: 0), "inputBottomLeft": CIVector(x: 0, y: 0)])
        let out = render(warped, unmanaged, 6, 4)
        for i in 0..<(6 * 4 * 4) { XCTAssertEqual(Int(out.portableImage.bytes[i]), Int(img.portableImage.bytes[i]), accuracy: 2, "byte \(i)") }
    }

    func testEdgePreservingUpsampleFollowsTheGuideEdge() {
        // A 2x1 coarse mask (left off, right on) upsampled to 8x2 by a guide that is dark on the left three quarters.
        let guide = rgba(8, 2) { x, _ in x < 6 ? (20, 20, 20, 255) : (230, 230, 230, 255) }
        let coarse = CVPixelBuffer(width: 2, height: 1, gray8: [0, 255])
        let f = CIFilter(name: "CIEdgePreserveUpsampleFilter")!
        f.setValue(CIImage(cgImage: guide), forKey: kCIInputImageKey)
        f.setValue(CIImage(cvPixelBuffer: coarse), forKey: "inputSmallImage")
        f.setValue(5, forKey: "inputSpatialSigma"); f.setValue(0.15, forKey: "inputLumaSigma")
        let out = render(f.outputImage!, unmanaged, 8, 2, mask: true)
        XCTAssertLessThan(px(out, 4, 0)[0], 60, "dark guide region stays with the 'off' side even near the middle")
        XCTAssertGreaterThan(px(out, 7, 0)[0], 200, "bright guide region takes the 'on' value")
    }

    func testCVPixelBufferBecomesAnOpaqueGrayImage() {
        let buffer = CVPixelBuffer(width: 2, height: 2, gray32Float: [0, 0.5, 1, 0.25])
        XCTAssertEqual(CVPixelBufferGetWidth(buffer), 2)
        let out = render(CIImage(cvPixelBuffer: buffer), unmanaged, 2, 2, mask: true)
        XCTAssertEqual(out.portableImage.bytes, [0, 128, 255, 64])
    }
}
