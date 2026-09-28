import Foundation

/// Cursors are opaque tokens here: the model asks for a shape, the Qt shell maps it to a `QCursor`.
public final class NSCursor: @unchecked Sendable, Equatable {
    public enum Shape: Equatable, Sendable {
        case arrow, iBeam, crosshair, openHand, closedHand, pointingHand, resizeLeftRight, resizeUpDown
        case frameResize(FrameResizePosition, FrameResizeDirections)
        case custom(hotSpot: CGPoint)
    }
    public struct FrameResizePosition: Equatable, Sendable {
        public let rawValue: Int
        public static let top = FrameResizePosition(rawValue: 0), left = FrameResizePosition(rawValue: 1)
        public static let bottom = FrameResizePosition(rawValue: 2), right = FrameResizePosition(rawValue: 3)
        public static let topLeft = FrameResizePosition(rawValue: 4), topRight = FrameResizePosition(rawValue: 5)
        public static let bottomLeft = FrameResizePosition(rawValue: 6), bottomRight = FrameResizePosition(rawValue: 7)
    }
    public struct FrameResizeDirections: OptionSet, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let inward = FrameResizeDirections(rawValue: 1)
        public static let outward = FrameResizeDirections(rawValue: 2)
        public static let all: FrameResizeDirections = [.inward, .outward]
    }

    public let shape: Shape
    public let image: NSImage
    public let hotSpot: CGPoint

    public init(shape: Shape) { self.shape = shape; image = NSImage(size: CGSize(width: 16, height: 16)); hotSpot = .zero }
    public init(image: NSImage, hotSpot: CGPoint) {
        shape = .custom(hotSpot: hotSpot); self.image = image; self.hotSpot = hotSpot
    }

    nonisolated(unsafe) public static var current = NSCursor.arrow
    nonisolated(unsafe) private static var stack: [NSCursor] = []
    /// Called whenever the model changes the cursor; the host maps it to a real one.
    nonisolated(unsafe) public static var onChange: ((NSCursor) -> Void)?

    public static let arrow = NSCursor(shape: .arrow)
    public static let iBeam = NSCursor(shape: .iBeam)
    public static let crosshair = NSCursor(shape: .crosshair)
    public static let openHand = NSCursor(shape: .openHand)
    public static let closedHand = NSCursor(shape: .closedHand)
    public static let pointingHand = NSCursor(shape: .pointingHand)
    public static let resizeLeftRight = NSCursor(shape: .resizeLeftRight)
    public static let resizeUpDown = NSCursor(shape: .resizeUpDown)
    public static func frameResize(position: FrameResizePosition, directions: FrameResizeDirections) -> NSCursor {
        NSCursor(shape: .frameResize(position, directions))
    }

    public func set() { NSCursor.current = self; NSCursor.onChange?(self) }
    public func push() { NSCursor.stack.append(NSCursor.current); set() }
    public func pop() { NSCursor.pop() }
    public static func pop() { (stack.popLast() ?? arrow).set() }
    public static func hide() {}
    public static func unhide() {}

    public static func == (lhs: NSCursor, rhs: NSCursor) -> Bool { lhs === rhs }
}
