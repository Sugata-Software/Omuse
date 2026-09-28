// VisionCompatTests — VNGenerateForegroundInstanceMaskRequest through the classical floor segmenter and a fake backend.

import CoreGraphics
import XCTest
import Vision
import CoreVideo

final class VisionCompatTests: XCTestCase {
    /// 80x60 white scene with a blue disc (bigger) and a red square (smaller).
    private func scene() -> CGImage {
        var b = PixelBuffer(width: 80, height: 60)
        for y in 0..<60 { for x in 0..<80 {
            var c: (UInt8, UInt8, UInt8, UInt8) = (240, 240, 240, 255)
            if (x - 22) * (x - 22) + (y - 30) * (y - 30) <= 14 * 14 { c = (20, 40, 200, 255) }
            if x >= 56 && x < 70 && y >= 14 && y < 26 { c = (210, 30, 30, 255) }
            b[x, y] = c
        } }
        return CGImage(b)
    }

    func testFindsTwoInstancesLargestFirstAndLeavesBackgroundZero() throws {
        let handler = VNImageRequestHandler(cgImage: scene(), options: [:])
        let request = VNGenerateForegroundInstanceMaskRequest()
        try handler.perform([request])
        let observation = try XCTUnwrap(request.results?.first)
        XCTAssertEqual(Array(observation.allInstances), [1, 2])
        let labels = observation.instanceMask
        func label(_ x: Int, _ y: Int) -> UInt8 { labels.bytes[y * labels.width + x] }
        XCTAssertEqual(label(2, 2), 0, "corner is background")
        XCTAssertEqual(label(22 * labels.width / 80, 30 * labels.height / 60), 1, "the bigger disc is instance 1")
        XCTAssertEqual(label(62 * labels.width / 80, 20 * labels.height / 60), 2, "the smaller square is instance 2")
        XCTAssertEqual(CVPixelBufferGetWidth(labels), labels.width)
    }

    func testMasksSelectOnlyTheRequestedInstancesAndScaleToTheImage() throws {
        let handler = VNImageRequestHandler(cgImage: scene(), orientation: .up)
        let request = VNGenerateForegroundInstanceMaskRequest()
        try handler.perform([request])
        let observation = try XCTUnwrap(request.results?.first)
        let coarse = try observation.generateMask(forInstances: IndexSet(integer: 2))
        XCTAssertGreaterThan(coarse.floats.reduce(0, +), 0)
        XCTAssertEqual(coarse.floats[coarse.width * 30 * 60 / 60 / 2 * 0 + 2], 0, "background is zero")
        let scaled = try observation.generateScaledMaskForImage(forInstances: IndexSet(integer: 2), from: handler)
        XCTAssertEqual(scaled.width, 80); XCTAssertEqual(scaled.height, 60)
        XCTAssertGreaterThan(scaled.floats[20 * 80 + 62], 0.9, "inside the square")
        XCTAssertLessThan(scaled.floats[30 * 80 + 22], 0.1, "the disc is another instance")
        XCTAssertThrowsError(try observation.generateMask(forInstances: IndexSet(integer: 7)))
    }

    func testAUniformImageHasNoInstances() throws {
        var b = PixelBuffer(width: 40, height: 30)
        for y in 0..<30 { for x in 0..<40 { b[x, y] = (200, 200, 200, 255) } }
        let request = VNGenerateForegroundInstanceMaskRequest()
        try VNImageRequestHandler(cgImage: CGImage(b), options: [:]).perform([request])
        XCTAssertTrue(try XCTUnwrap(request.results?.first).allInstances.isEmpty)
    }

    func testABackendCanReplaceTheSegmenter() throws {
        final class Fake: ForegroundSegmentationBackend {
            var calls = 0
            func segment(rgba: [UInt8], width: Int, height: Int) throws -> SegmentationResult {
                calls += 1
                var labels = [UInt8](repeating: 0, count: width * height)
                for i in 0..<(width * height / 2) { labels[i] = 1 }
                return SegmentationResult(labels: labels, width: width, height: height, instanceCount: 1)
            }
        }
        let fake = Fake()
        ForegroundSegmentationRegistry.backend = fake
        defer { ForegroundSegmentationRegistry.backend = nil }
        let request = VNGenerateForegroundInstanceMaskRequest()
        try VNImageRequestHandler(cgImage: scene(), options: [:]).perform([request])
        XCTAssertEqual(fake.calls, 1)
        XCTAssertEqual(try XCTUnwrap(request.results?.first).allInstances.count, 1)
    }

    func testTheLabelBufferFeedsCoreImageLikeUpstreamsObjectSelection() throws {
        let request = VNGenerateForegroundInstanceMaskRequest()
        try VNImageRequestHandler(cgImage: scene(), options: [:]).perform([request])
        let observation = try XCTUnwrap(request.results?.first)
        let ci = CIImageProbe.grayBytes(observation.instanceMask)
        XCTAssertEqual(ci.max(), 2, "labels survive the CIImage gray round trip as their own values")
    }
}

import CoreImage
private enum CIImageProbe {
    static func grayBytes(_ buffer: CVPixelBuffer) -> [UInt8] {
        let ctx = CIContext(options: [.workingColorSpace: NSNull(), .outputColorSpace: NSNull()])
        let image = CIImage(cvPixelBuffer: buffer)
        return ctx.createCGImage(image, from: image.extent, format: .L8, colorSpace: CGColorSpaceCreateDeviceGray())!.portableImage.bytes
    }
}
