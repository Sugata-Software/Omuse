// ImageIOCompatTests — CGImageSource / CGImageDestination over the portable PNG codec and a pluggable backend.

import CoreGraphics
import XCTest
import ImageIO
@testable import ImageIO

final class ImageIOCompatTests: XCTestCase {
    /// 24x20 RGBA PNG made by an independent encoder (Python zlib, level 9) with row filters 0-4 cycling and 300 dpi.
    private static let reference = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAABgAAAAUCAYAAACXtf2DAAAACXBIWXMAAC4jAAAuIwF4pT92AAABhklEQVR42tWRL0/DYBCHjz9DNCSYJoCpmakhIRXMnJlpSGZewUzNTM1MBTNnlt0akpkaTA2mBlODOYNpkNPT0/saP94QxD7ATMWTXB7zy+UhItKABggpQEQ3iClEQvfKFCGlIRzFyOgBOSVa0AhCjJLGqChFTRNtyKGlKYwydDTDlnLd0Rx7KnCgBc7oegA/QEesPatTuXM/QEesPatTugu6C3RwNSDP2rP6v+lU7vJvyb/mWXnoiBM5vteAI4Q8RMQxYn5Awokyj5Ayw/EYGafIeaIFOwhPUXKGimeoOdeG52i5gPECHQu2vNQdl9jzBgeufOTnqO+Rn277HlkSDWSEUBiRjBFLikQmyuKQyhROMmQyQy65FjKHSIFSFqhEUMtSGynRygYmFTp5x1Zq3ckH9tLgIJ8+8tuo75Ffhn2PbBMNzCG0KSLLENsMieXKNkdqBZwtkJkgt6UWVkJsg9IqVPaO2mpt7AOtNTD7RGcttvalOzPs7RsH63zkH9f3yK+P/Y78CxWLLcpmPwuvAAAAAElFTkSuQmCC")!

    private func referencePixel(_ x: Int, _ y: Int) -> (Int, Int, Int, Int) {
        (x * 10 % 256, y * 12 % 256, (x + y) * 5 % 256, (x + y) % 5 != 0 ? 255 : 128)
    }

    func testDecodesAnExternallyCompressedPNGWithEveryRowFilter() throws {
        let source = try XCTUnwrap(CGImageSourceCreateWithData(Self.reference as CFData, nil))
        XCTAssertEqual(CGImageSourceGetType(source) as String?, "public.png")
        let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertEqual(image.width, 24); XCTAssertEqual(image.height, 20)
        let bytes = image.portableImage.bytes
        for (x, y) in [(0, 0), (5, 3), (23, 19), (11, 7), (12, 12)] {
            let (r, g, b, a) = referencePixel(x, y)
            let o = (y * 24 + x) * 4
            XCTAssertEqual(Int(bytes[o + 3]), a)
            XCTAssertEqual(Int(bytes[o]), (r * a + 127) / 255, accuracy: 1, "premultiplied red at \(x),\(y)")
            XCTAssertEqual(Int(bytes[o + 1]), (g * a + 127) / 255, accuracy: 1)
            XCTAssertEqual(Int(bytes[o + 2]), (b * a + 127) / 255, accuracy: 1)
        }
    }

    func testPropertiesUseAppleKeysAndTypes() throws {
        let source = try XCTUnwrap(CGImageSourceCreateWithData(Self.reference as CFData, [kCGImageSourceShouldCache: false] as CFDictionary))
        let properties = try XCTUnwrap(CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])
        XCTAssertEqual(properties[kCGImagePropertyPixelWidth] as? Int, 24)
        XCTAssertEqual(properties[kCGImagePropertyPixelHeight] as? Int, 20)
        XCTAssertEqual(properties[kCGImagePropertyDepth] as? Int, 8)
        XCTAssertEqual(properties[kCGImagePropertyOrientation] as? Int32, 1)
        XCTAssertEqual(try XCTUnwrap(properties[kCGImagePropertyDPIWidth] as? Double), 300, accuracy: 0.01)
        XCTAssertEqual(CGImageSourceGetCount(source), 1)
    }

    func testRGBAExportRoundTripsThroughDestinationAndSource() throws {
        var buffer = PixelBuffer(width: 5, height: 4)
        for y in 0..<4 { for x in 0..<5 {
            let a = UInt8(255 - x * 50)   // includes a transparent-ish column
            buffer[x, y] = (UInt8(Int(a) * (x * 40) / 255), 0, UInt8(Int(a) * (y * 60) / 255), a)
        } }
        let original = CGImage(buffer)
        let data = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(data, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, original, [kCGImagePropertyDPIWidth: 144.0, kCGImagePropertyDPIHeight: 144.0] as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let source = try XCTUnwrap(CGImageSourceCreateWithData(data as Data as CFData, nil))
        let back = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertEqual(back.portableImage.width, 5)
        for i in 0..<(5 * 4 * 4) { XCTAssertEqual(Int(back.portableImage.bytes[i]), Int(original.portableImage.bytes[i]), accuracy: 2, "byte \(i)") }
        let properties = try XCTUnwrap(CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])
        XCTAssertEqual(try XCTUnwrap(properties[kCGImagePropertyDPIWidth] as? Double), 144, accuracy: 0.1)
    }

    func testGrayMaskRoundTripsExactlyAsAMaskImage() throws {
        let plane: [UInt8] = (0..<(7 * 3)).map { UInt8($0 * 12) }
        let mask = CGImage(PortableImage(width: 7, height: 3, kind: .mask, bytesPerRow: 7, bytes: plane))
        let data = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(data, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, mask, nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let source = try XCTUnwrap(CGImageSourceCreateWithData(data as Data as CFData, nil))
        let back = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary))
        XCTAssertTrue(back.isGrayPlane)
        XCTAssertEqual(back.portableImage.bytes, plane)
    }

    func testThumbnailShrinksToTheMaxPixelSizeKeepingAspect() throws {
        let source = try XCTUnwrap(CGImageSourceCreateWithData(Self.reference as CFData, nil))
        let thumb = try XCTUnwrap(CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 12,
            kCGImageSourceCreateThumbnailWithTransform: true] as CFDictionary))
        XCTAssertEqual(thumb.width, 12); XCTAssertEqual(thumb.height, 10)
        let untouched = try XCTUnwrap(CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceThumbnailMaxPixelSize: 100] as CFDictionary))
        XCTAssertEqual(untouched.width, 24, "an image already smaller than the limit is not enlarged")
    }

    func testExifOrientationIsAppliedToUprightTransform() {
        // 2x1 image [A B]; orientation 6 (rotate 90 CW) must give a 1x2 image [A over B].
        let img = CGImage(PortableImage(width: 2, height: 1, kind: .mask, bytesPerRow: 2, bytes: [10, 20]))
        let rotated = ImageOrientation.apply(6, to: img)
        XCTAssertEqual(rotated.width, 1); XCTAssertEqual(rotated.height, 2)
        XCTAssertEqual(rotated.portableImage.bytes, [10, 20])
        XCTAssertEqual(ImageOrientation.apply(2, to: img).portableImage.bytes, [20, 10], "mirrored horizontally")
    }

    func testUnknownDataAndNonImageDestinationsAreRejected() {
        XCTAssertNil(CGImageSourceCreateWithData(Data([1, 2, 3, 4]) as CFData, nil))
        XCTAssertNil(CGImageDestinationCreateWithData(NSMutableData(), "com.adobe.pdf" as CFString, 1, nil))
        XCTAssertNil(CGImageSourceCreateWithURL(URL(fileURLWithPath: "/nonexistent/x.png") as CFURL, nil))
    }

    func testHostBackendIsConsultedFirstAndFallsBackToThePortableCodec() throws {
        final class Fake: ImageCodecBackend {
            var encoded: [String] = []
            func identify(_ data: Data) -> ImageInfo? { data == Data([0xFF, 0xD8, 0xFF, 0xE0]) ? ImageInfo(typeIdentifier: "public.jpeg", width: 3, height: 2) : nil }
            func decode(_ data: Data) -> CGImage? { identify(data).map { _ in CGImage(PortableImage(width: 3, height: 2, kind: .rgba, bytesPerRow: 12, bytes: [UInt8](repeating: 9, count: 24))) } }
            func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?) -> Data? {
                guard typeIdentifier == "public.jpeg" else { return nil }
                encoded.append("q=\(quality ?? -1)"); return Data([0xFF, 0xD8, 0xFF, 0xE0])
            }
        }
        let fake = Fake()
        ImageCodecRegistry.host = fake
        defer { ImageCodecRegistry.host = nil }
        let jpeg = try XCTUnwrap(CGImageSourceCreateWithData(Data([0xFF, 0xD8, 0xFF, 0xE0]) as CFData, nil))
        XCTAssertEqual(CGImageSourceGetType(jpeg) as String?, "public.jpeg")
        let out = NSMutableData()
        let dest = try XCTUnwrap(CGImageDestinationCreateWithData(out, "public.jpeg" as CFString, 1, nil))
        CGImageDestinationAddImage(dest, try XCTUnwrap(CGImageSourceCreateImageAtIndex(jpeg, 0, nil)),
                                   [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(dest))
        XCTAssertEqual(fake.encoded, ["q=0.8"])
        // PNG is not the fake's format, so the portable codec still handles it.
        XCTAssertNotNil(CGImageSourceCreateWithData(Self.reference as CFData, nil))
    }
}
