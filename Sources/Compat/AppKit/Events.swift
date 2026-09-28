import Foundation
import CoreGraphics

public typealias CGKeyCode = UInt16

/// Core Graphics event objects, reduced to what synthesising and reading keyboard/mouse events needs.
public final class CGEvent: @unchecked Sendable {
    public enum EventType { case keyDown, keyUp, leftMouseDown, leftMouseUp, mouseMoved, flagsChanged }
    public let type: EventType
    public var location: CGPoint
    public let virtualKey: CGKeyCode
    public var flags = CGEventFlags()
    public init?(keyboardEventSource source: AnyObject?, virtualKey: CGKeyCode, keyDown: Bool) {
        type = keyDown ? .keyDown : .keyUp; self.virtualKey = virtualKey; location = .zero
    }
    public init?(mouseEventSource source: AnyObject?, mouseType: EventType, mouseCursorPosition: CGPoint, mouseButton: Int) {
        type = mouseType; self.virtualKey = 0; location = mouseCursorPosition
    }
}
public struct CGEventFlags: OptionSet, Sendable {
    public let rawValue: UInt64
    public init(rawValue: UInt64) { self.rawValue = rawValue }
    public static let maskShift = CGEventFlags(rawValue: 1 << 17), maskControl = CGEventFlags(rawValue: 1 << 18)
    public static let maskAlternate = CGEventFlags(rawValue: 1 << 19), maskCommand = CGEventFlags(rawValue: 1 << 20)
}

/// An input event. Qt translates its own events into these and hands them to the (unmodified) AppKit-style view code.
public final class NSEvent: @unchecked Sendable {
    public enum EventType: UInt, Sendable {
        case leftMouseDown = 1, leftMouseUp, rightMouseDown, rightMouseUp, mouseMoved, leftMouseDragged, rightMouseDragged
        case mouseEntered, mouseExited, keyDown, keyUp, flagsChanged, cursorUpdate = 17, scrollWheel = 22
        case tabletPoint = 23, tabletProximity, otherMouseDown = 25, otherMouseUp, otherMouseDragged, magnify = 30
    }
    public struct EventTypeMask: OptionSet, Sendable {
        public let rawValue: UInt64
        public init(rawValue: UInt64) { self.rawValue = rawValue }
        public init(type: EventType) { rawValue = 1 << UInt64(type.rawValue) }
        public static let keyDown = EventTypeMask(type: .keyDown), keyUp = EventTypeMask(type: .keyUp)
        public static let flagsChanged = EventTypeMask(type: .flagsChanged), leftMouseDown = EventTypeMask(type: .leftMouseDown)
        public static let leftMouseUp = EventTypeMask(type: .leftMouseUp), mouseMoved = EventTypeMask(type: .mouseMoved)
        public static let scrollWheel = EventTypeMask(type: .scrollWheel), any = EventTypeMask(rawValue: ~0)
    }
    public struct ModifierFlags: OptionSet, Hashable, Sendable {
        public let rawValue: UInt
        public init(rawValue: UInt) { self.rawValue = rawValue }
        public static let capsLock = ModifierFlags(rawValue: 1 << 16), shift = ModifierFlags(rawValue: 1 << 17)
        public static let control = ModifierFlags(rawValue: 1 << 18), option = ModifierFlags(rawValue: 1 << 19)
        public static let command = ModifierFlags(rawValue: 1 << 20), numericPad = ModifierFlags(rawValue: 1 << 21)
        public static let help = ModifierFlags(rawValue: 1 << 22), function = ModifierFlags(rawValue: 1 << 23)
        public static let deviceIndependentFlagsMask = ModifierFlags(rawValue: 0xFFFF0000)
    }
    public struct SubType: RawRepresentable, Hashable, Sendable {
        public let rawValue: Int16
        public init(rawValue: Int16) { self.rawValue = rawValue }
        public static let mouseEvent = SubType(rawValue: 0), tabletPoint = SubType(rawValue: 1), tabletProximity = SubType(rawValue: 2)
    }
    public enum PointingDeviceType: UInt, Sendable { case unknown, pen, cursor, eraser }

    public let type: EventType
    public var locationInWindow: CGPoint
    public var modifierFlags: ModifierFlags
    public var timestamp: TimeInterval
    public var windowNumber: Int
    public weak var window: NSWindow? { NSWindow.window(numbered: windowNumber) }
    public var characters: String?
    public var charactersIgnoringModifiers: String?
    public var isARepeat = false
    public var keyCode: UInt16 = 0
    public var clickCount = 0
    public var pressure: Float = 0
    public var buttonNumber = 0
    public var subtype: SubType = .mouseEvent
    public var pointingDeviceType: PointingDeviceType = .unknown
    public var tilt = CGPoint.zero
    public var rotation: Float = 0
    public var deltaX: CGFloat = 0, deltaY: CGFloat = 0
    public var scrollingDeltaX: CGFloat = 0, scrollingDeltaY: CGFloat = 0
    public var hasPreciseScrollingDeltas = false
    public var magnification: CGFloat = 0
    public var cgEvent: CGEvent?

    init(type: EventType, location: CGPoint, modifierFlags: ModifierFlags, timestamp: TimeInterval, windowNumber: Int) {
        self.type = type; locationInWindow = location; self.modifierFlags = modifierFlags
        self.timestamp = timestamp; self.windowNumber = windowNumber
    }

    public convenience init?(cgEvent: CGEvent) {
        let type: EventType
        switch cgEvent.type {
        case .keyDown: type = .keyDown; case .keyUp: type = .keyUp; case .leftMouseDown: type = .leftMouseDown
        case .leftMouseUp: type = .leftMouseUp; case .mouseMoved: type = .mouseMoved; case .flagsChanged: type = .flagsChanged
        }
        // CGEventFlags and NSEvent.ModifierFlags share their bit positions.
        self.init(type: type, location: cgEvent.location, modifierFlags: ModifierFlags(rawValue: UInt(cgEvent.flags.rawValue)),
                  timestamp: 0, windowNumber: 0)
        keyCode = cgEvent.virtualKey
        self.cgEvent = cgEvent
        if type == .keyDown || type == .keyUp, let key = Self.ansiKeys[cgEvent.virtualKey] {
            // Like AppKit, "ignoring modifiers" still honours Shift (Shift-[ types "{" either way).
            characters = modifierFlags.contains(.shift) ? key.shifted : key.plain
            charactersIgnoringModifiers = characters
        }
    }

    /// US ANSI layout: what Apple's virtual key codes type, without and with Shift.
    private static let ansiKeys: [CGKeyCode: (plain: String, shifted: String)] = {
        var keys: [CGKeyCode: (String, String)] = [:]
        let letters: [(CGKeyCode, String)] = [(0, "a"), (11, "b"), (8, "c"), (2, "d"), (14, "e"), (3, "f"), (5, "g"), (4, "h"), (34, "i"),
            (38, "j"), (40, "k"), (37, "l"), (46, "m"), (45, "n"), (31, "o"), (35, "p"), (12, "q"), (15, "r"), (1, "s"), (17, "t"),
            (32, "u"), (9, "v"), (13, "w"), (7, "x"), (16, "y"), (6, "z")]
        for (code, letter) in letters { keys[code] = (letter, letter.uppercased()) }
        let digits: [(CGKeyCode, String, String)] = [(29, "0", ")"), (18, "1", "!"), (19, "2", "@"), (20, "3", "#"), (21, "4", "$"),
            (23, "5", "%"), (22, "6", "^"), (26, "7", "&"), (28, "8", "*"), (25, "9", "(")]
        for (code, plain, shifted) in digits { keys[code] = (plain, shifted) }
        let punctuation: [(CGKeyCode, String, String)] = [(33, "[", "{"), (30, "]", "}"), (27, "-", "_"), (24, "=", "+"), (41, ";", ":"),
            (39, "'", "\""), (43, ",", "<"), (47, ".", ">"), (44, "/", "?"), (42, "\\", "|"), (50, "`", "~"), (49, " ", " ")]
        for (code, plain, shifted) in punctuation { keys[code] = (plain, shifted) }
        return keys
    }()

    public static func mouseEvent(with type: EventType, location: CGPoint, modifierFlags: ModifierFlags, timestamp: TimeInterval,
                                  windowNumber: Int, context: AnyObject?, eventNumber: Int, clickCount: Int, pressure: Float) -> NSEvent? {
        let e = NSEvent(type: type, location: location, modifierFlags: modifierFlags, timestamp: timestamp, windowNumber: windowNumber)
        e.clickCount = clickCount; e.pressure = pressure
        return e
    }

    public static func keyEvent(with type: EventType, location: CGPoint, modifierFlags: ModifierFlags, timestamp: TimeInterval,
                                windowNumber: Int, context: AnyObject?, characters: String, charactersIgnoringModifiers: String,
                                isARepeat: Bool, keyCode: UInt16) -> NSEvent? {
        let e = NSEvent(type: type, location: location, modifierFlags: modifierFlags, timestamp: timestamp, windowNumber: windowNumber)
        e.characters = characters; e.charactersIgnoringModifiers = charactersIgnoringModifiers
        e.isARepeat = isARepeat; e.keyCode = keyCode
        return e
    }

    // MARK: Global state and monitors (the host feeds these)

    nonisolated(unsafe) public static var modifierFlags: ModifierFlags = []
    nonisolated(unsafe) public static var pressedMouseButtons = 0
    nonisolated(unsafe) public static var mouseLocation = CGPoint.zero
    nonisolated(unsafe) static var monitors: [(id: Int, mask: EventTypeMask, handler: (NSEvent) -> NSEvent?)] = []
    nonisolated(unsafe) private static var nextMonitor = 1

    public static func addLocalMonitorForEvents(matching mask: EventTypeMask, handler: @escaping (NSEvent) -> NSEvent?) -> Any? {
        defer { nextMonitor += 1 }
        monitors.append((nextMonitor, mask, handler))
        return nextMonitor
    }
    public static func removeMonitor(_ monitor: Any) {
        guard let id = monitor as? Int else { return }
        monitors.removeAll { $0.id == id }
    }
    /// Runs an event through the local monitors like AppKit does before it reaches a view; nil means it was consumed.
    public static func filterThroughLocalMonitors(_ event: NSEvent) -> NSEvent? {
        var current: NSEvent? = event
        for m in monitors {
            guard let e = current else { return nil }
            if m.mask.contains(EventTypeMask(type: e.type)) { current = m.handler(e) }
        }
        return current
    }
}
