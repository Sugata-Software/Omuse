import Foundation
import CoreGraphics

/// The current drawing target for AppKit-style drawing (`NSColor.setFill`, text, images). A stack, like Apple's.
public final class NSGraphicsContext: @unchecked Sendable {
    public let cgContext: CGContext
    public let isFlipped: Bool

    nonisolated(unsafe) private static var stack: [NSGraphicsContext?] = []
    nonisolated(unsafe) public static var current: NSGraphicsContext?

    public init(cgContext: CGContext, flipped: Bool) {
        self.cgContext = cgContext
        self.isFlipped = flipped
    }

    public static func saveGraphicsState() {
        stack.append(current)
        current?.cgContext.saveGState()
    }

    public static func restoreGraphicsState() {
        current?.cgContext.restoreGState()
        current = stack.popLast() ?? nil
    }

    public func saveGraphicsState() { Self.saveGraphicsState() }
    public func restoreGraphicsState() { Self.restoreGraphicsState() }
}

extension NSAffineTransform {
    public func concat() {
        let t = CGAffineTransform(
            a: transformStruct.m11, b: transformStruct.m12,
            c: transformStruct.m21, d: transformStruct.m22,
            tx: transformStruct.tX, ty: transformStruct.tY
        )
        NSGraphicsContext.current?.cgContext.concatenate(t)
    }
}
