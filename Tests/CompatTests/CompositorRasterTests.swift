// Tests for the portable canonical raster buffers and PortableImage. Pin the
// canonical contract (stride, channel order, cropping semantics) that the
// file-map raster tiers depend on. Run in-sandbox: swift test.

import CoreGraphics
import XCTest

final class CompositorRasterTests: XCTestCase {

    func testPixelBufferStrideAndChannelOrder() {
        var buf = PixelBuffer(width: 2, height: 1)
        buf[0, 0] = (r: 10, g: 20, b: 30, a: 40)
        XCTAssertEqual(buf.bytesPerRow, 8) // width*4, no padding
        XCTAssertEqual(buf.bytes, [10, 20, 30, 40, 0, 0, 0, 0])
        XCTAssertEqual(buf[0, 0].r, 10)
        XCTAssertEqual(buf[0, 0].a, 40)
    }

    func testPixelBufferFill() {
        let buf = PixelBuffer(width: 1, height: 1, fill: 0xFF00FF80)
        // Pack order: argb? Our fill decodes (fill>>24)=R=0xFF, (>>16)=G=0x00,
        // (>>8)=B=0xFF, fill&0xFF=A=0x80.
        XCTAssertEqual(buf.bytes, [0xFF, 0x00, 0xFF, 0x80])
    }

    func testMaskBufferStride() {
        var m = MaskBuffer(width: 3, height: 2, fill: 0)
        m[1, 0] = 255
        XCTAssertEqual(m.bytesPerRow, 3)
        XCTAssertEqual(m.bytes, [0, 255, 0, 0, 0, 0])
    }

    func testPortableImageFromPixelBuffer() {
        var buf = PixelBuffer(width: 2, height: 1)
        buf[0, 0] = (1, 2, 3, 4); buf[1, 0] = (5, 6, 7, 8)
        let img = PortableImage(buf)
        XCTAssertEqual(img.kind, .rgba)
        XCTAssertEqual(img.bytesPerPixel, 4)
        XCTAssertEqual(img.bytesPerRow, 8)
        XCTAssertEqual(img.bytes.count, 8)
    }

    func testCroppingInteriorRegion() {
        var buf = PixelBuffer(width: 4, height: 4)
        // Paint a distinct 2x2 block at (1,1).
        buf[1, 1] = (10, 20, 30, 40); buf[2, 1] = (50, 60, 70, 80)
        buf[1, 2] = (90, 100, 110, 120); buf[2, 2] = (130, 140, 150, 160)
        let img = PortableImage(buf)
        let crop = img.cropping(to: CGRect(x: 1, y: 1, width: 2, height: 2))
        XCTAssertNotNil(crop)
        XCTAssertEqual(crop?.width, 2)
        XCTAssertEqual(crop?.height, 2)
        XCTAssertEqual(crop?.bytes, [10, 20, 30, 40, 50, 60, 70, 80,
                                    90, 100, 110, 120, 130, 140, 150, 160])
    }

    func testCroppingDisjointReturnsNil() {
        let buf = PixelBuffer(width: 4, height: 4)
        let img = PortableImage(buf)
        XCTAssertNil(img.cropping(to: CGRect(x: 10, y: 10, width: 2, height: 2)))
        XCTAssertNil(img.cropping(to: .null))
        XCTAssertNil(img.cropping(to: CGRect(x: 0, y: 0, width: 0, height: 1)))
    }

    func testCroppingClampsToImageBounds() {
        var buf = PixelBuffer(width: 4, height: 4)
        buf[0, 0] = (1, 1, 1, 1)
        let img = PortableImage(buf)
        // Rect extends past the right edge; crop clamps to width 4.
        let crop = img.cropping(to: CGRect(x: 0, y: 0, width: 10, height: 10))
        XCTAssertEqual(crop?.width, 4)
        XCTAssertEqual(crop?.height, 4)
        XCTAssertEqual(crop?.bytes[0], 1)
    }

    func testMaskImageCropping() {
        var m = MaskBuffer(width: 3, height: 3)
        m[1, 1] = 200
        let img = PortableImage(m)
        XCTAssertEqual(img.kind, .mask)
        XCTAssertEqual(img.bytesPerPixel, 1)
        XCTAssertEqual(img.bytesPerRow, 3)
        let crop = img.cropping(to: CGRect(x: 1, y: 1, width: 1, height: 1))
        XCTAssertEqual(crop?.bytes, [200])
    }
}