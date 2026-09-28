// A minimal `Combine`-shaped surface — just enough for `Timer.publish(...).autoconnect()` + `.onReceive(_:)`, the
// one pattern `Compositor/UI/ProjectTabs.swift` uses. Not a general Combine reimplementation: real Combine is a
// large framework (Subjects, operators, back-pressure); this compat layer only needs a periodic-tick publisher a
// View can subscribe to. Firing the timer for real needs a live run loop tied to Qt's event loop, which this phase
// doesn't wire up — `.onReceive` compiles and is ready, but is inert until that's connected, the same "structure
// real, one interaction not yet live" honesty as `ForEach.onMove`/`ShortcutRecorder` elsewhere in this layer.

import Foundation

public protocol CombinePublisher {
    associatedtype Output
}

extension Timer {
    public struct TimerPublisher: CombinePublisher {
        public typealias Output = Date
        let interval: Double
        public func autoconnect() -> TimerPublisher { self }
    }
    public static func publish(every interval: Double, on runLoop: RunLoop, in mode: RunLoop.Mode) -> TimerPublisher {
        TimerPublisher(interval: interval)
    }
}
