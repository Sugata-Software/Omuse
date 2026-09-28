// CompositorCoreTests — Swift-level tests for the @_cdecl seam.
//
// These run under the Freedesktop Swift runtime extension (`swift test`), not
// on this build host (no Swift toolchain). They mirror the C++ unit tests in
// tests/test_composite_over.cpp so the Swift export and the C reference impl
// are both exercised against the same expected values (ENG-10 parity pattern).

import CoreGraphics
import XCTest

final class CompositeOverTests: XCTestCase {

    private func composite(dst: [UInt8], src: [UInt8], coverage: [UInt8]? = nil,
                           width: Int = 1, height: Int = 1, stride: Int = 4,
                           opacity: Float = 1.0) -> [UInt8] {
        var d = dst
        let rc = d.withUnsafeMutableBufferPointer { dstPtr -> Int32 in
            src.withUnsafeBufferPointer { srcPtr in
                if let cov = coverage {
                    return cov.withUnsafeBufferPointer { covPtr in
                        compositorCompositeOver(
                            dstRGBA: dstPtr.baseAddress!,
                            srcRGBA: srcPtr.baseAddress!,
                            coverage: covPtr.baseAddress,
                            width: width, height: height, stride: stride,
                            opacity: opacity)
                    }
                } else {
                    return compositorCompositeOver(
                        dstRGBA: dstPtr.baseAddress!,
                        srcRGBA: srcPtr.baseAddress!,
                        coverage: nil,
                        width: width, height: height, stride: stride,
                        opacity: opacity)
                }
            }
        }
        XCTAssertEqual(rc, 0)
        return d
    }

    func testOpaqueOverTransparent() {
        let out = composite(dst: [0, 0, 0, 0], src: [255, 0, 0, 255])
        XCTAssertEqual(out, [255, 0, 0, 255])
    }

    func testSemitransparentOverOpaque() {
        // src=(128,0,0,128) over dst=(0,0,255,255) -> (128,0,127,255)
        let out = composite(dst: [0, 0, 255, 255], src: [128, 0, 0, 128])
        XCTAssertEqual(out, [128, 0, 127, 255])
    }

    func testCoverageMask() {
        // coverage=128 halves the source: src=(128,0,0,128), dst=(0,0,255,255) -> (64,0,191,255)
        let out = composite(dst: [0, 0, 255, 255], src: [128, 0, 0, 128], coverage: [128])
        XCTAssertEqual(out, [64, 0, 191, 255])
    }

    func testOpacity() {
        // op=0.5: src=(255,0,0,255) over dst=(0,0,255,255) -> (128,0,128,255)
        let out = composite(dst: [0, 0, 255, 255], src: [255, 0, 0, 255], opacity: 0.5)
        XCTAssertEqual(out, [128, 0, 128, 255])
    }

    func testInvalidGeometry() {
        var buf = [UInt8](repeating: 0, count: 4)
        let src = [UInt8](repeating: 0, count: 4)
        let rc = buf.withUnsafeMutableBufferPointer { d in
            src.withUnsafeBufferPointer { s in
                compositorCompositeOver(dstRGBA: d.baseAddress!, srcRGBA: s.baseAddress!,
                                        coverage: nil, width: 2, height: 1, stride: 4, opacity: 1.0)
            }
        }
        XCTAssertEqual(rc, -1)  // stride < width*4
    }
}