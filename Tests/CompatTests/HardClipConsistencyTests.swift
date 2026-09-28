import XCTest
import CoreGraphics
@testable import CoreGraphics

/// Core Graphics gives one answer per hard-edged clip path. Under a rotation, Skia's answer used to depend on what was
/// already clipped, so clearing a rect inside a clip of it and then drawing into that clip drew edge pixels twice
/// (upstream's tiled renderer does exactly this).
final class HardClipConsistencyTests: XCTestCase {
    private let n = 240

    private func makeContext() -> CGContext {
        CGContext(data: nil, width: n, height: n, bitsPerComponent: 8, bytesPerRow: n * 4,
                  space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    }
    private func alphas(_ c: CGContext) -> [UInt8] {
        let p = c.data!.assumingMemoryBound(to: UInt8.self)
        return (0..<(n * n)).map { p[$0 * 4 + 3] }
    }
    private func rotated(_ c: CGContext) {
        c.translateBy(x: 120, y: 120); c.rotate(by: 25 * .pi / 180); c.translateBy(x: -120, y: -120)
        c.setShouldAntialias(false)
    }
    private func flat(_ alpha: UInt8) -> CGImage {
        CGImage(PortableImage(width: 8, height: 8, kind: .rgba, bytesPerRow: 32, bytes: [UInt8](repeating: alpha, count: 256)))
    }

    /// A hard fill of a rect, a hard clip to it and an image drawn through that clip all touch the same pixels.
    func testHardFillClipAndImageDrawCoverTheSamePixels() {
        let src = flat(128)
        var fillMismatches = 0, drawMismatches = 0
        for k in 0..<60 {
            let rect = CGRect(x: 30.0 + Double(k) * 0.37, y: 40.0 + Double(k) * 0.53, width: 71.3 + Double(k % 7), height: 53.9)
            let black = CGColor(red: 0, green: 0, blue: 0, alpha: 1)
            let clipped = makeContext(); rotated(clipped)
            clipped.addPath(CGPath(rect: rect)); clipped.clip()
            clipped.setFillColor(black); clipped.fill(CGRect(x: 0, y: 0, width: n, height: n))
            let filled = makeContext(); rotated(filled)
            filled.setFillColor(black); filled.fill(rect)
            let drawn = makeContext(); rotated(drawn)
            drawn.addPath(CGPath(rect: rect)); drawn.clip(); drawn.setShouldAntialias(true)
            drawn.draw(src, in: rect.insetBy(dx: -20, dy: -20))
            let a = alphas(clipped), b = alphas(filled), c = alphas(drawn)
            for i in 0..<(n * n) {
                if (a[i] > 0) != (b[i] > 0) { fillMismatches += 1 }
                if (a[i] > 0) != (c[i] > 0) { drawMismatches += 1 }
            }
        }
        XCTAssertEqual(fillMismatches, 0, "a hard fill of a rect differs from a hard clip to it")
        XCTAssertEqual(drawMismatches, 0, "an image drawn through a hard clip differs from the clip")
    }

    /// The tiled renderer's pattern: draw the unchanged layer, then per tile clear it inside a clip and draw its
    /// replacement inside the same clip. Nothing may end up drawn twice.
    func testClearThenDrawInsideAClipNeverDrawsAnEdgePixelTwice() {
        let half = flat(64)
        for layer in [true, false] {
            for clearWithTheRect in [true, false] {
                var twice = 0, worst = 0
                for k in 0..<40 {
                    let c = makeContext(); rotated(c)
                    let sx = 40.0 + Double(k) * 0.29
                    let tiles = [CGRect(x: sx, y: 40, width: 61.7, height: 120), CGRect(x: sx + 61.7, y: 40, width: 55.1, height: 120)]
                    if layer { c.beginTransparencyLayer(auxiliaryInfo: nil) }
                    c.saveGState(); c.setBlendMode(.normal)
                    c.draw(half, in: CGRect(x: 0, y: 0, width: n, height: n))
                    for tile in tiles {
                        c.saveGState()
                        c.setShouldAntialias(false); c.addPath(CGPath(rect: tile)); c.clip()
                        c.setBlendMode(.clear)
                        c.fill(clearWithTheRect ? tile : CGRect(x: 0, y: 0, width: n, height: n))
                        c.restoreGState()
                        c.saveGState()
                        c.setShouldAntialias(false); c.addPath(CGPath(rect: tile)); c.clip()
                        c.setShouldAntialias(true)
                        c.draw(half, in: tile.insetBy(dx: -30, dy: -30))
                        c.restoreGState()
                    }
                    c.restoreGState()
                    if layer { c.endTransparencyLayer() }
                    for v in alphas(c) where v > 64 { twice += 1; worst = max(worst, Int(v)) }
                }
                XCTAssertEqual(twice, 0, "layer \(layer), clear with rect \(clearWithTheRect): \(twice) pixels drawn twice (worst alpha \(worst))")
            }
        }
    }
}
