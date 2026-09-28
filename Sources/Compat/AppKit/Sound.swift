import Foundation

public final class NSSound {
    /// Called instead of the system beep; the host may map it to a status-bar flash or the desktop bell.
    nonisolated(unsafe) public static var onBeep: (() -> Void)?
    public static func beep() { onBeep?() }
}
