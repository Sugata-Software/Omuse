import XCTest
import CoreGraphics
@testable import CoreGraphics
import CompatSupport
import ImageIO
import AppKit

/// A backend that only records what `CGContext` asks of it: proof the context depends on the protocol, not on Skia.
private final class RecordingCanvas: CanvasBackend, @unchecked Sendable {
    var name: String { "recording" }
    var calls: [String] = []
    func save() { calls.append("save") }
    func restore() { calls.append("restore") }
    func setAlpha(_ alpha: Float) { calls.append("alpha \(alpha)") }
    func setBlendMode(_ rawValue: Int32) { calls.append("blend \(rawValue)") }
    func setInterpolationQuality(_ rawValue: Int32) {}
    func setAntialias(_ enabled: Bool) { calls.append("aa \(enabled)") }
    func translate(_ tx: Float, _ ty: Float) { calls.append("translate \(tx) \(ty)") }
    func scale(_ sx: Float, _ sy: Float) { calls.append("scale \(sx) \(sy)") }
    func rotate(_ radians: Float) {}
    func concat(_ transform: CGAffineTransform) {}
    var totalMatrix: CGAffineTransform? { nil }
    func clip(rect: CGRect, antialias: Bool) { calls.append("clipRect \(Int(rect.width))x\(Int(rect.height))") }
    func clip(path: [PathSegment], evenOdd: Bool, antialias: Bool) { calls.append("clipPath \(path.count)") }
    func clip(mask: [UInt8], width: Int, height: Int, stride: Int, rect: CGRect, isGray: Bool) { calls.append("clipMask \(isGray)") }
    var clipBounds: CGRect { .null }
    func fill(rect: CGRect, color: CGColor) { calls.append("fillRect \(Int(rect.minX)),\(Int(rect.minY)),\(Int(rect.width)),\(Int(rect.height))") }
    func fill(path: [PathSegment], evenOdd: Bool, color: CGColor) { calls.append("fillPath") }
    func stroke(path: [PathSegment], width: Float, cap: Int32, join: Int32, miterLimit: Float, color: CGColor) { calls.append("stroke") }
    func clear(rect: CGRect) { calls.append("clear") }
    func draw(image: PortableImage, isGray: Bool, in rect: CGRect, opacity: Float, blendMode: Int32, quality: Int32) { calls.append("draw \(isGray)") }
    func linearGradient(_ gradient: CGGradient, from start: CGPoint, to end: CGPoint, options: Int32) {}
    func radialGradient(_ gradient: CGGradient, from start: CGPoint, startRadius: Float, to end: CGPoint, endRadius: Float, options: Int32) {}
    func beginLayer(alpha: Float) { calls.append("beginLayer") }
    func endLayer() { calls.append("endLayer") }
}

private struct FixedFactory: CanvasBackendFactory {
    let canvas: CanvasBackend?
    var name: String { "fixed" }
    var isAvailable: Bool { canvas != nil }
    func makeCanvas(pixels: UnsafeMutablePointer<UInt8>, width: Int, height: Int, bytesPerRow: Int, format: CGContext.PixelFormat) -> CanvasBackend? { canvas }
}

final class CanvasBackendTests: XCTestCase {

    func testContextDrawsThroughWhicheverBackendIsInstalled() {
        let recorder = RecordingCanvas()
        CanvasBackends.withFactories([FixedFactory(canvas: recorder)]) {
            let context = CGContext(width: 10, height: 10)
            context.saveGState()
            context.setAlpha(0.5)
            context.fill(CGRect(x: 1, y: 2, width: 3, height: 4))
            context.clip(to: CGRect(x: 0, y: 0, width: 5, height: 5))
            context.restoreGState()
        }
        XCTAssertEqual(recorder.calls, ["save", "alpha 0.5", "fillRect 1,2,3,4", "clipRect 5x5", "restore"])
    }

    func testAppleStyleContextFlipsOnTheBackendBelowTheVisibleTransform() {
        let recorder = RecordingCanvas()
        CanvasBackends.withFactories([FixedFactory(canvas: recorder)]) {
            let context = CGContext(data: nil, width: 4, height: 6, bitsPerComponent: 8, bytesPerRow: 16,
                                    space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            XCTAssertEqual(recorder.calls, ["translate 0.0 6.0", "scale 1.0 -1.0"])
            XCTAssertEqual(context.ctm, .identity, "the y flip is below the visible CTM")
        }
    }

    func testBackendsAreTriedInOrderAndAnEmptyListFallsBackToSwift() {
        let recorder = RecordingCanvas()
        CanvasBackends.withFactories([FixedFactory(canvas: nil), FixedFactory(canvas: recorder)]) {
            CGContext(width: 4, height: 4).saveGState()
            XCTAssertEqual(recorder.calls, ["save"], "the first factory declined, the second served")
        }
        CanvasBackends.withFactories([]) {
            let bare = CGContext(width: 4, height: 4)
            bare.setFillColor(red: 1, green: 0, blue: 0, alpha: 1)
            bare.fill(CGRect(x: 0, y: 0, width: 2, height: 2))
            XCTAssertEqual(bare.buffer[0, 0].0, 255, "with no backend the pure-Swift path still paints")
            XCTAssertFalse(CanvasBackends.isAvailable)
        }
    }
}

final class ServiceSlotTests: XCTestCase {
    private protocol Greeter: AnyObject { var word: String { get } }
    private final class Fixed: Greeter { let word: String; init(_ word: String) { self.word = word } }

    func testFallbackInstallAndScopedOverride() {
        let slot = ServiceSlot<Greeter>(fallback: { Fixed("built-in") })
        XCTAssertEqual(slot.current?.word, "built-in")
        XCTAssertNil(slot.installedValue)
        slot.install(Fixed("host"))
        XCTAssertEqual(slot.current?.word, "host")
        slot.withOverride(Fixed("test")) { XCTAssertEqual(slot.current?.word, "test") }
        XCTAssertEqual(slot.current?.word, "host", "the override restores what was installed")
        slot.install(nil)
        XCTAssertEqual(slot.current?.word, "built-in")
    }

    func testOverrideRestoresEvenWhenTheBodyThrows() {
        struct Boom: Error {}
        let slot = ServiceSlot<Greeter>()
        slot.install(Fixed("host"))
        XCTAssertThrowsError(try slot.withOverride(Fixed("test")) { throw Boom() })
        XCTAssertEqual(slot.current?.word, "host")
    }
}

final class CompatSeamOverrideTests: XCTestCase {
    private final class NullCodec: ImageCodecBackend {
        func identify(_ data: Data) -> ImageInfo? { nil }
        func decode(_ data: Data) -> CGImage? { nil }
        func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?) -> Data? { nil }
    }
    private final class MemoryClipboard: NSPasteboard.Backend {
        var stored: [NSPasteboard.PasteboardType: Data] = [:]
        func write(_ items: [NSPasteboard.PasteboardType: Data]) { stored = items }
        func read() -> [NSPasteboard.PasteboardType: Data] { stored }
    }

    func testCodecAndClipboardSeamsAreOverridableAndRestored() {
        let before = ImageCodecRegistry.host
        let codec = NullCodec()
        ImageCodecRegistry.withHost(codec) { XCTAssertTrue(ImageCodecRegistry.host === codec) }
        XCTAssertTrue(ImageCodecRegistry.host === before)

        let clipboard = MemoryClipboard()
        NSPasteboard.withBackend(clipboard) {
            let board = NSPasteboard.general
            board.clearContents()
            board.setData(Data([1, 2, 3]), forType: .png)
            XCTAssertEqual(clipboard.stored[.png], Data([1, 2, 3]))
        }
        XCTAssertNil(NSPasteboard.backend)
    }
}
