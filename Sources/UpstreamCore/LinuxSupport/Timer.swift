// Part of the `Compositor` module but not upstream code (Sources/UpstreamCore/LinuxSupport is ours).
//
// Apple's `Timer(timeInterval:repeats:block:)` lets a timer created by a main-actor object touch that object's state
// from the block. swift-corelibs-foundation declares the block `@Sendable`, which upstream's marching-ants timer
// (Rendering/EditorCanvas.swift) cannot satisfy under Xcode's default main-actor isolation. Declaring `Timer` in the
// module that uses it shadows Foundation's type for upstream code; this wrapper keeps Apple's block semantics and
// forwards to a real Foundation timer. Only the members upstream uses exist (`init(timeInterval:repeats:block:)`,
// `invalidate()`, and `RunLoop.add`).

import Foundation
import Combine

typealias Timer = MainActorTimer

final class MainActorTimer {
    let inner: Foundation.Timer
    init(timeInterval interval: TimeInterval, repeats: Bool, block: @escaping (MainActorTimer) -> Void) {
        // The wrapper is created before the timer, so the block gets the wrapper back like Apple's passes the timer.
        var wrapper: MainActorTimer?
        nonisolated(unsafe) let unchecked = block
        let sendable: @Sendable (Foundation.Timer) -> Void = { _ in if let w = wrapper { unchecked(w) } }
        inner = Foundation.Timer(fire: Date().addingTimeInterval(interval), interval: repeats ? interval : 0,
                                 repeats: repeats, block: sendable)
        wrapper = self
    }
    func invalidate() { inner.invalidate() }
    var isValid: Bool { inner.isValid }
}

extension RunLoop {
    func add(_ timer: MainActorTimer, forMode mode: RunLoop.Mode) { add(timer.inner, forMode: mode) }
}

// `Timer` (unqualified, in this module) resolves to `MainActorTimer` per the shadowing `typealias` above, so
// `Compositor/UI/ProjectTabs.swift`'s `Timer.publish(every:on:in:).autoconnect()` needs its own `.publish`, distinct
// from `Combine.swift`'s extension on the real `Foundation.Timer` (which nothing in this module ever sees).
extension MainActorTimer {
    struct TimerPublisher: CombinePublisher {
        typealias Output = Date
        func autoconnect() -> TimerPublisher { self }
    }
    static func publish(every interval: Double, on runLoop: RunLoop, in mode: RunLoop.Mode) -> TimerPublisher {
        TimerPublisher()
    }
}
