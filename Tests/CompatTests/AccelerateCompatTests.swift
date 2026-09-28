// AccelerateCompatTests — the vImage entry points upstream calls, with the same buffer/flag/matrix conventions.

import CoreGraphics
import XCTest
import Accelerate

final class AccelerateCompatTests: XCTestCase {
    private func withBuffers(width: Int, height: Int, channels: Int, source: [UInt8],
                             dstWidth: Int? = nil, dstHeight: Int? = nil,
                             _ body: (inout vImage_Buffer, inout vImage_Buffer) -> vImage_Error) -> (vImage_Error, [UInt8]) {
        var input = source
        let dw = dstWidth ?? width, dh = dstHeight ?? height
        var output = [UInt8](repeating: 0, count: dw * dh * channels)
        var error: vImage_Error = -1
        input.withUnsafeMutableBytes { i in
            output.withUnsafeMutableBytes { o in
                var s = vImage_Buffer(data: i.baseAddress, height: vImagePixelCount(height), width: vImagePixelCount(width), rowBytes: width * channels)
                var d = vImage_Buffer(data: o.baseAddress, height: vImagePixelCount(dh), width: vImagePixelCount(dw), rowBytes: dw * channels)
                error = body(&s, &d)
            }
        }
        return (error, output)
    }

    func testInvertMatrixKeepsAlphaAndComplementsPremultipliedColour() {
        // Half-transparent red (premultiplied: 100,0,0,200) and opaque blue.
        let source: [UInt8] = [100, 0, 0, 200, 0, 0, 255, 255]
        // The exact matrix upstream's Invert uses: pixel x matrix, fixed point x256, divisor 256.
        let matrix: [Int16] = [-256, 0, 0, 0,  0, -256, 0, 0,  0, 0, -256, 0,  256, 256, 256, 256]
        let (error, out) = withBuffers(width: 2, height: 1, channels: 4, source: source) {
            vImageMatrixMultiply_ARGB8888(&$0, &$1, matrix, 256, nil, nil, vImage_Flags(kvImageNoFlags))
        }
        XCTAssertEqual(error, kvImageNoError)
        XCTAssertEqual(out, [100, 200, 200, 200, 255, 255, 0, 255], "colour becomes alpha - colour, alpha is kept")
    }

    func testTableLookUpInvertsAPlane() {
        let table = (0...255).map { Pixel_8(255 - $0) }
        let (error, out) = withBuffers(width: 4, height: 1, channels: 1, source: [0, 1, 128, 255]) {
            vImageTableLookUp_Planar8(&$0, &$1, table, vImage_Flags(kvImageNoFlags))
        }
        XCTAssertEqual(error, kvImageNoError)
        XCTAssertEqual(out, [255, 254, 127, 0])
    }

    func testHighQualityHalvingOfAConstantImageIsConstant() {
        let source = [UInt8](repeating: 77, count: 8 * 8 * 4)
        let (error, out) = withBuffers(width: 8, height: 8, channels: 4, source: source, dstWidth: 4, dstHeight: 4) {
            vImageScale_ARGB8888(&$0, &$1, nil, vImage_Flags(kvImageHighQualityResampling))
        }
        XCTAssertEqual(error, kvImageNoError)
        XCTAssertTrue(out.allSatisfy { $0 == 77 }, "kernel weights are normalised, so flat regions stay flat")
    }

    func testHalvingACheckerboardAveragesToGrey() {
        var source = [UInt8](repeating: 0, count: 8 * 8)
        for y in 0..<8 { for x in 0..<8 { source[y * 8 + x] = (x + y) % 2 == 0 ? 255 : 0 } }
        for high in [false, true] {
            let (error, out) = withBuffers(width: 8, height: 8, channels: 1, source: source, dstWidth: 4, dstHeight: 4) {
                vImageScale_Planar8(&$0, &$1, nil, vImage_Flags(high ? kvImageHighQualityResampling : kvImageNoFlags))
            }
            XCTAssertEqual(error, kvImageNoError)
            // Away from the borders a proper downsampling filter lands near the mean, not on 0 or 255.
            XCTAssertEqual(Int(out[1 * 4 + 1]), 128, accuracy: high ? 40 : 20, "high=\(high)")
        }
    }

    func testScaleUpInterpolatesBetweenPixels() {
        let (_, out) = withBuffers(width: 2, height: 1, channels: 1, source: [0, 200], dstWidth: 4, dstHeight: 1) {
            vImageScale_Planar8(&$0, &$1, nil, vImage_Flags(kvImageNoFlags))
        }
        XCTAssertLessThan(out[0], out[1]); XCTAssertLessThan(out[1], out[2]); XCTAssertLessThan(out[2], out[3])
    }

    func testMismatchedBuffersAreRejected() {
        let table = (0...255).map { Pixel_8($0) }
        var a = [UInt8](repeating: 0, count: 4), b = [UInt8](repeating: 0, count: 8)
        var error = kvImageNoError
        a.withUnsafeMutableBytes { pa in b.withUnsafeMutableBytes { pb in
            var s = vImage_Buffer(data: pa.baseAddress, height: 1, width: 4, rowBytes: 4)
            var d = vImage_Buffer(data: pb.baseAddress, height: 1, width: 8, rowBytes: 8)
            error = vImageTableLookUp_Planar8(&s, &d, table, 0)
        } }
        XCTAssertEqual(error, kvImageBufferSizeMismatch)
    }
}
