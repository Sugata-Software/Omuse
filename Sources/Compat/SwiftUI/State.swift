// SwiftUI's local state property wrappers. Upstream panels use these purely for their own transient UI state
// (typed zoom text, focus, drag state); the underlying model state is `@Bindable var session: EditorSession`
// (real `Observation`, already re-exported by SwiftUI.swift) and needs no wrapper of its own here.

@propertyWrapper @dynamicMemberLookup public struct Binding<Value> {
    private let getter: () -> Value
    private let setter: (Value) -> Void
    public init(get: @escaping () -> Value, set: @escaping (Value) -> Void) { getter = get; setter = set }
    public var wrappedValue: Value {
        get { getter() }
        nonmutating set { setter(newValue) }
    }
    public var projectedValue: Binding<Value> { self }
    public static func constant(_ value: Value) -> Binding<Value> { Binding(get: { value }, set: { _ in }) }

    /// `$settings.channel` — a sub-binding into one of `Value`'s properties, the same dynamic-member projection
    /// real SwiftUI's `Binding` supports.
    public subscript<Subject>(dynamicMember keyPath: WritableKeyPath<Value, Subject>) -> Binding<Subject> {
        Binding<Subject>(get: { wrappedValue[keyPath: keyPath] }, set: { wrappedValue[keyPath: keyPath] = $0 })
    }
}

// Not `@MainActor`: these are plain boxed storage (no AppKit/UIKit thread affinity of their own), and modifier
// closures that write through them (`.onChange`, `.focused`, ...) run wherever the Qt bridge calls back from.
//
// Both are `struct`s wrapping a private class box, matching real SwiftUI's `State`/`FocusState` (also structs) —
// NOT plain classes. This matters beyond style: Swift only treats a property-wrapper's own no-argument `init()` as
// supplying an implicit default for the *enclosing* type's synthesized memberwise initializer when the wrapper is
// a struct. A class-backed `@propertyWrapper` (what an earlier version of this file used) makes Swift require an
// explicit argument for every such property, which then makes the whole synthesized initializer `private` — e.g.
// `NavigationToolHeader(session:)` would stop compiling. Confirmed via a minimal repro against the toolchain.
final class _Box<Value>: @unchecked Sendable { var value: Value; init(_ value: Value) { self.value = value } }

@propertyWrapper public struct State<Value> {
    private let box: _Box<Value>
    public init(wrappedValue: Value) { box = _Box(wrappedValue) }
    /// Deprecated-but-still-real SwiftUI spelling (`State(initialValue:)`), used directly (not via `@State`) when a
    /// custom `init` needs to seed a property wrapper's storage explicitly.
    public init(initialValue: Value) { box = _Box(initialValue) }
    public var wrappedValue: Value {
        get { box.value }
        nonmutating set { box.value = newValue }
    }
    public var projectedValue: Binding<Value> {
        Binding(get: { box.value }, set: { box.value = $0 })
    }
}

@propertyWrapper public struct FocusState<Value: Equatable> {
    private let box: _Box<Value>
    public init(wrappedValue: Value) { box = _Box(wrappedValue) }
    /// Must stay declared inline (not in a conditional `extension`) — Swift's memberwise-init default-detection for
    /// property wrappers only recognises a zero-argument `init()` written directly on the primary type declaration,
    /// even for a `struct` wrapper. Confirmed via a minimal repro: moving this to `extension FocusState where Value
    /// == Bool` compiled, but silently stopped satisfying the default-parameter heuristic, making every enclosing
    /// view's synthesized initializer `private` again (e.g. `NavigationToolHeader(session:)` failing to link).
    public init() where Value == Bool { box = _Box(false) }
    /// For `@FocusState private var field: SomeEnum?` — matches real SwiftUI's `init() where Value:
    /// ExpressibleByNilLiteral`, needed for the same inline-declaration reason as the `Bool` overload above.
    public init() where Value: ExpressibleByNilLiteral { box = _Box(nil) }
    public var wrappedValue: Value {
        get { box.value }
        nonmutating set { box.value = newValue }
    }
    public var projectedValue: FocusState<Value> { self }
}

import Foundation

/// Persists to `UserDefaults.standard`, matching real SwiftUI's `@AppStorage` contract for the primitive types
/// upstream code uses it with (`Double` — `layersPanelWidth` in `ContentView.swift`).
@propertyWrapper public struct AppStorage<Value> {
    private let key: String
    private let defaultValue: Value
    private let get: (String, Value) -> Value
    private let set: (String, Value) -> Void
    public var wrappedValue: Value {
        get { get(key, defaultValue) }
        nonmutating set { set(key, newValue) }
    }
    public var projectedValue: Binding<Value> {
        Binding(get: { wrappedValue }, set: { wrappedValue = $0 })
    }
    public init(wrappedValue: Value, _ key: String) where Value == Double {
        self.key = key; self.defaultValue = wrappedValue
        get = { UserDefaults.standard.object(forKey: $0) as? Value ?? $1 }
        set = { UserDefaults.standard.set($1, forKey: $0) }
    }
    public init(wrappedValue: Value, _ key: String) where Value == Bool {
        self.key = key; self.defaultValue = wrappedValue
        get = { UserDefaults.standard.object(forKey: $0) as? Value ?? $1 }
        set = { UserDefaults.standard.set($1, forKey: $0) }
    }
    public init(wrappedValue: Value, _ key: String) where Value == String {
        self.key = key; self.defaultValue = wrappedValue
        get = { UserDefaults.standard.object(forKey: $0) as? Value ?? $1 }
        set = { UserDefaults.standard.set($1, forKey: $0) }
    }
    public init(wrappedValue: Value, _ key: String) where Value == Int {
        self.key = key; self.defaultValue = wrappedValue
        get = { UserDefaults.standard.object(forKey: $0) as? Value ?? $1 }
        set = { UserDefaults.standard.set($1, forKey: $0) }
    }
}

/// A callable `openWindow(id:)` action, matching real SwiftUI's `OpenWindowAction`. Inert here (this compat layer
/// hosts one Qt-owned top-level window, not a multi-`WindowGroup` scene graph) — same honesty as `.onReceive`.
public struct OpenWindowAction {
    public func callAsFunction(id: String) {}
    public func callAsFunction(id: String, value: some Hashable) {}
}

public struct EnvironmentValues {
    public var openWindow: OpenWindowAction { OpenWindowAction() }
}

/// `@Environment(\.openWindow)` — only the one key path upstream code actually reads is implemented; real SwiftUI's
/// `EnvironmentValues` machinery (dependency injection through the view tree) isn't needed for that.
@propertyWrapper public struct Environment<Value> {
    private let value: Value
    public init(_ keyPath: KeyPath<EnvironmentValues, Value>) { value = EnvironmentValues()[keyPath: keyPath] }
    public var wrappedValue: Value { value }
}

/// Gives per-property `Binding`s into an `@Observable` reference type (`EditorSession`, ...) via dynamic member
/// lookup — `@Bindable var session: EditorSession` then `$session.brushMode` — matching real SwiftUI's contract.
@propertyWrapper @dynamicMemberLookup public struct Bindable<Value: AnyObject> {
    public var wrappedValue: Value
    public init(wrappedValue: Value) { self.wrappedValue = wrappedValue }
    public var projectedValue: Bindable<Value> { self }
    public subscript<Subject>(dynamicMember keyPath: ReferenceWritableKeyPath<Value, Subject>) -> Binding<Subject> {
        Binding(get: { wrappedValue[keyPath: keyPath] }, set: { wrappedValue[keyPath: keyPath] = $0 })
    }
}
