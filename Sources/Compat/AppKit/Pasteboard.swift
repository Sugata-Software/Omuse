import Foundation
import CompatSupport
import UniformTypeIdentifiers

/// General pasteboard. In-process store with Apple's change-count semantics; the host can mirror it to the desktop
/// clipboard through `backend` (see `IClipboardService`).
public final class NSPasteboard: @unchecked Sendable {
    public struct Name: Hashable, RawRepresentable, Sendable {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let general = Name(rawValue: "general")
        public static let drag = Name(rawValue: "drag")
    }

    public struct PasteboardType: Hashable, RawRepresentable, Sendable {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public init(_ rawValue: String) { self.rawValue = rawValue }
        public static let png = PasteboardType(rawValue: "public.png")
        public static let tiff = PasteboardType(rawValue: "public.tiff")
        public static let string = PasteboardType(rawValue: "public.utf8-plain-text")
        public static let fileURL = PasteboardType(rawValue: "public.file-url")
    }
    public struct ReadingOptionKey: Hashable, RawRepresentable, Sendable {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
    }

    public protocol Backend: AnyObject {
        func write(_ items: [PasteboardType: Data])
        func read() -> [PasteboardType: Data]
    }

    public static let backendSlot = ServiceSlot<Backend>()
    public static var backend: Backend? {
        get { backendSlot.installedValue }
        set { backendSlot.install(newValue) }
    }
    /// Runs `body` with `backend` as the clipboard the general pasteboard mirrors, then restores the previous one.
    public static func withBackend<Result>(_ backend: Backend?, _ body: () throws -> Result) rethrows -> Result {
        try backendSlot.withOverride(backend, body)
    }
    public static let general = NSPasteboard()
    public static let drag = NSPasteboard()

    public init(name: Name = .general) {}
    public static func withName(_ name: Name) -> NSPasteboard {
        name == .drag ? drag : general
    }

    private var items: [PasteboardType: Data] = [:]
    public private(set) var changeCount = 0

    @discardableResult public func clearContents() -> Int { items = [:]; changeCount += 1; return changeCount }
    @discardableResult public func setData(_ data: Data?, forType type: PasteboardType) -> Bool {
        guard let data else { return false }
        items[type] = data
        Self.backend?.write(items)
        return true
    }
    public func data(forType type: PasteboardType) -> Data? {
        if let backend = Self.backend { let outside = backend.read(); if !outside.isEmpty { return outside[type] } }
        return items[type]
    }
    public var types: [PasteboardType]? { Array(items.keys) }
    public func canReadObject(forClasses classes: [AnyClass], options: [ReadingOptionKey: Any]?) -> Bool {
        let held = Self.backend?.read() ?? items
        if classes.contains(where: { $0 is NSImage.Type }) { return held[.png] != nil || held[.tiff] != nil }
        return !held.isEmpty
    }
    public func string(forType type: PasteboardType) -> String? {
        guard let d = data(forType: type) else { return nil }
        return String(data: d, encoding: .utf8)
    }
    public func availableType(from types: [PasteboardType]) -> PasteboardType? {
        for t in types {
            if data(forType: t) != nil { return t }
        }
        return nil
    }
}
