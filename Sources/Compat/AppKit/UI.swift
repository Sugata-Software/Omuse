import Foundation
import UniformTypeIdentifiers
import CoreGraphics
import UniformTypeIdentifiers

// UI types the model layer names. They are inert containers; anything interactive is an injectable hook
// so the Qt host (or a test) supplies the behaviour.

public enum NSApplication {
    public enum ModalResponse: Int { case OK = 1, cancel = 0, stop = -1000, abort = -1001, continue_ = -1002
        case alertFirstButtonReturn = 1000, alertSecondButtonReturn = 1001, alertThirdButtonReturn = 1002 }
}
public typealias NSModalResponse = NSApplication.ModalResponse

/// Model code asks questions through alerts; the host installs `handler` (Qt message box). Headless default:
/// choose the first button, like pressing Return.
@MainActor open class NSAlert {
    public enum Style: Int, Sendable { case warning, informational, critical }
    public var alertStyle: Style = .warning
    public var messageText = ""
    public var informativeText = ""
    public private(set) var buttonTitles: [String] = []
    nonisolated(unsafe) public static var handler: ((NSAlert) -> NSApplication.ModalResponse)?
    public init() {}
    @discardableResult public func addButton(withTitle title: String) -> AnyObject? { buttonTitles.append(title); return nil }
    public func runModal() -> NSApplication.ModalResponse { Self.handler?(self) ?? .alertFirstButtonReturn }
    public func beginSheetModal(for window: NSWindow) async -> NSApplication.ModalResponse { runModal() }
}

@MainActor open class NSSavePanel {
    public var allowedContentTypes: [UTType] = []
    public var nameFieldStringValue = ""
    public var title = ""
    public var canCreateDirectories = false
    public var isExtensionHidden = true
    public var url: URL?
    nonisolated(unsafe) public static var handler: ((NSSavePanel) -> NSApplication.ModalResponse)?
    public init() {}
    public func runModal() -> NSApplication.ModalResponse { Self.handler?(self) ?? .cancel }
    public func begin() async -> NSApplication.ModalResponse { runModal() }
    public func beginSheetModal(for window: NSWindow) async -> NSApplication.ModalResponse { runModal() }
}

open class NSOpenPanel: NSSavePanel {
    public var allowsMultipleSelection = false
    public var canChooseDirectories = false
    public var canChooseFiles = true
    public var treatsFilePackagesAsDirectories = false
    public var urls: [URL] = []
    nonisolated(unsafe) public static var openHandler: ((NSOpenPanel) -> NSApplication.ModalResponse)?
    public override func runModal() -> NSApplication.ModalResponse { Self.openHandler?(self) ?? .cancel }
}

/// Recent-documents bookkeeping; the Qt shell may mirror it into its own recent-files menu.
@MainActor public final class NSDocumentController {
    public static let shared = NSDocumentController()
    public private(set) var recentDocumentURLs: [URL] = []
    nonisolated(unsafe) public static var onNoteRecent: ((URL) -> Void)?
    public func noteNewRecentDocumentURL(_ url: URL) {
        recentDocumentURLs.removeAll { $0 == url }
        recentDocumentURLs.insert(url, at: 0)
        Self.onNoteRecent?(url)
    }
    public func clearRecentDocuments(_ sender: Any?) { recentDocumentURLs = [] }
}
