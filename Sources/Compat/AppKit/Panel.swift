// NSPanel/NSWindowDelegate and the handful of NSWindow members real floating-panel code needs
// (Compositor/UI/FloatingPanel.swift) beyond what View.swift's NSWindow already covers.

import Foundation

@MainActor public protocol NSWindowDelegate: AnyObject {
    func windowDidMove(_ notification: Notification)
    func windowWillClose(_ notification: Notification)
    func windowShouldClose(_ sender: NSWindow) -> Bool
}
extension NSWindowDelegate {
    public func windowShouldClose(_ sender: NSWindow) -> Bool { true }
    public func windowDidMove(_ notification: Notification) {}
    public func windowWillClose(_ notification: Notification) {}
}

@MainActor open class NSPanel: NSWindow {
    public var isFloatingPanel = false
    public var hidesOnDeactivate = false
}

extension NSWindow {
    /// Real AppKit posts these through the notification center on every resize/move; this compat's `frame`
    /// setters don't post anything, so `FloatingPanel.swift`'s docked-panel frame observer never actually fires —
    /// structurally real (the names exist, `addObserver(forName:)` compiles), inert until something posts them.
    public static let didResizeNotification = Notification.Name("NSWindowDidResizeNotification")
    public static let didMoveNotification = Notification.Name("NSWindowDidMoveNotification")
    public func setFrame(_ frameRect: CGRect, display: Bool) { frame = frameRect }
}

extension NSWindow {
    public var isReleasedWhenClosed: Bool {
        get { _isReleasedWhenClosed }
        set { _isReleasedWhenClosed = newValue }
    }
    public var delegate: (any NSWindowDelegate)? {
        get { _delegate }
        set { _delegate = newValue }
    }
    /// Screen-space top-left-origin placement; this compat has no real display, so it just records the frame.
    public func setFrameTopLeftPoint(_ point: NSPoint) {
        frame = CGRect(x: point.x, y: point.y - frame.height, width: frame.width, height: frame.height)
    }
    public func setFrameOrigin(_ point: NSPoint) {
        frame = CGRect(origin: point, size: frame.size)
    }
    public func setContentSize(_ size: CGSize) {
        frame = CGRect(origin: frame.origin, size: size)
    }
    public func center() {}
    public func makeKey() { isVisible = true }
    public func convertToScreen(_ rect: CGRect) -> CGRect { rect.offsetBy(dx: frame.minX, dy: frame.minY) }
}

// Backing storage for the two computed properties above — an associated-object-style side table would be more
// "real AppKit", but this compat's `NSWindow` isn't `@objc`, so a private extension-owned dictionary is simpler.
extension NSWindow {
    private static var releaseFlags: [ObjectIdentifier: Bool] = [:]
    private static var delegates: [ObjectIdentifier: any NSWindowDelegate] = [:]
    fileprivate var _isReleasedWhenClosed: Bool {
        get { Self.releaseFlags[ObjectIdentifier(self)] ?? true }
        set { Self.releaseFlags[ObjectIdentifier(self)] = newValue }
    }
    fileprivate var _delegate: (any NSWindowDelegate)? {
        get { Self.delegates[ObjectIdentifier(self)] }
        set { Self.delegates[ObjectIdentifier(self)] = newValue }
    }
}
