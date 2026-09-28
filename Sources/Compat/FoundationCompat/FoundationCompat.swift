// FoundationCompat — the few Apple-Foundation APIs swift-corelibs-foundation lacks and upstream code uses:
// `FileWrapper`, `NSFileCoordinator`, security-scoped URL access and `autoreleasepool`. Files that import only
// Foundation see these through the implicit `-import-module FoundationCompat` build setting, so upstream sources
// stay unmodified.

import Foundation
import UniformTypeIdentifiers
import LinuxFileOperations

/// Objective-C's out-error parameter; `error: &coordinationError` binds to it exactly as on Apple platforms.
public typealias NSErrorPointer = UnsafeMutablePointer<NSError?>?

public func autoreleasepool<Result>(invoking body: () throws -> Result) rethrows -> Result { try body() }

extension URL {
    /// Sandbox scope is a macOS concept; on Linux (and in Flatpak, through portals) the URL is already usable.
    public func startAccessingSecurityScopedResource() -> Bool { false }
    public func stopAccessingSecurityScopedResource() {}
}

/// Reads and writes a directory of files as one document package (Apple's `FileWrapper`, the subset the project
/// store uses: regular files, directories, atomic package writes and reading a package back).
open class FileWrapper {
    public struct WritingOptions: OptionSet, Sendable {
        public let rawValue: UInt
        public init(rawValue: UInt) { self.rawValue = rawValue }
        public static let atomic = WritingOptions(rawValue: 1 << 0)
        public static let withNameUpdating = WritingOptions(rawValue: 1 << 1)
    }
    public struct ReadingOptions: OptionSet, Sendable {
        public let rawValue: UInt
        public init(rawValue: UInt) { self.rawValue = rawValue }
        public static let immediate = ReadingOptions(rawValue: 1 << 0)
        public static let withoutMapping = ReadingOptions(rawValue: 1 << 1)
    }

    public var preferredFilename: String?
    public var filename: String?
    public private(set) var regularFileContents: Data?
    public private(set) var fileWrappers: [String: FileWrapper]?
    public var isRegularFile: Bool { regularFileContents != nil }
    public var isDirectory: Bool { fileWrappers != nil }

    public init(regularFileWithContents contents: Data) { regularFileContents = contents }

    public init(directoryWithFileWrappers wrappers: [String: FileWrapper]) {
        fileWrappers = wrappers
        for (name, wrapper) in wrappers { wrapper.preferredFilename = name; wrapper.filename = name }
    }

    public init(url: URL, options: ReadingOptions = []) throws {
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) else {
            throw CocoaError(.fileReadNoSuchFile)
        }
        if isDir.boolValue {
            var children: [String: FileWrapper] = [:]
            for name in try FileManager.default.contentsOfDirectory(atPath: url.path) {
                children[name] = try FileWrapper(url: url.appendingPathComponent(name), options: options)
            }
            fileWrappers = children
        } else {
            regularFileContents = try Data(contentsOf: url)
        }
        preferredFilename = url.lastPathComponent
        filename = url.lastPathComponent
    }

    @discardableResult
    public func addRegularFile(withContents data: Data, preferredFilename: String) -> String {
        let wrapper = FileWrapper(regularFileWithContents: data)
        wrapper.preferredFilename = preferredFilename; wrapper.filename = preferredFilename
        fileWrappers?[preferredFilename] = wrapper
        return preferredFilename
    }

    /// Atomic writes stage a complete sibling package before publishing it.
    /// This guarantees atomic namespace replacement, not power-loss durability.
    public func write(to url: URL, options: WritingOptions = [], originalContentsURL: URL?) throws {
        if options.contains(.atomic) {
            try writeAtomically(to: url)
        } else {
            try validateFilenames()
            try writeContents(to: url)
        }
    }

    // The publication seam lets tests inject disk-full/unsupported-filesystem
    // failures after staging without changing the filesystem or process state.
    func writeAtomically(to url: URL, publish: (URL, URL) throws -> Void = FileWrapper.publish) throws {
        try validateFilenames()
        let fm = FileManager.default
        let staging = url.deletingLastPathComponent()
            .appendingPathComponent(".compositor-save-\(UUID().uuidString)")
        try fm.createDirectory(at: staging, withIntermediateDirectories: false,
                               attributes: [.posixPermissions: 0o700])
        // Before publication this removes partial output. After exchange it
        // removes the previous version. Cleanup failure must not turn a
        // successful save into an error; a hidden sibling may remain in that case.
        defer { try? fm.removeItem(at: staging) }
        let contents = staging.appendingPathComponent("contents")
        try writeContents(to: contents)
        try publish(contents, url)
    }

    private static func publish(_ staged: URL, _ destination: URL) throws {
        let error = staged.withUnsafeFileSystemRepresentation { source in
            destination.withUnsafeFileSystemRepresentation { target in
                compositor_publish_file(source, target)
            }
        }
        guard error == 0 else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(error),
                          userInfo: [NSFilePathErrorKey: destination.path])
        }
    }

    private func validateFilenames() throws {
        for (name, wrapper) in fileWrappers ?? [:] {
            guard !name.isEmpty, name != ".", name != "..",
                  !name.contains("/"), !name.utf8.contains(0) else {
                throw CocoaError(.fileWriteInvalidFileName)
            }
            try wrapper.validateFilenames()
        }
    }

    private func writeContents(to url: URL) throws {
        let fm = FileManager.default
        if let data = regularFileContents {
            try data.write(to: url)
        } else if let wrappers = fileWrappers {
            try fm.createDirectory(at: url, withIntermediateDirectories: true)
            for (name, wrapper) in wrappers { try wrapper.writeContents(to: url.appendingPathComponent(name)) }
        }
    }
}

/// Coordinates file access with other processes on Apple platforms. Linux has no file-coordination service, so the
/// accessor simply runs on the given URL; atomic replacement is the caller's `write(... .atomic)`.
open class NSFileCoordinator {
    public struct ReadingOptions: OptionSet, Sendable {
        public let rawValue: UInt
        public init(rawValue: UInt) { self.rawValue = rawValue }
        public static let withoutChanges = ReadingOptions(rawValue: 1 << 0)
        public static let resolvesSymbolicLink = ReadingOptions(rawValue: 1 << 1)
    }
    public struct WritingOptions: OptionSet, Sendable {
        public let rawValue: UInt
        public init(rawValue: UInt) { self.rawValue = rawValue }
        public static let forDeleting = WritingOptions(rawValue: 1 << 0)
        public static let forMoving = WritingOptions(rawValue: 1 << 1)
        public static let forMerging = WritingOptions(rawValue: 1 << 4)
        public static let forReplacing = WritingOptions(rawValue: 1 << 3)
    }
    public init() {}
    public init(filePresenter: AnyObject?) {}

    public func coordinate(readingItemAt url: URL, options: ReadingOptions = [],
                           error outError: NSErrorPointer, byAccessor reader: (URL) -> Void) {
        reader(url)
    }

    public func coordinate(writingItemAt url: URL, options: WritingOptions = [],
                           error outError: NSErrorPointer, byAccessor writer: (URL) -> Void) {
        writer(url)
    }

    public func coordinate(readingItemAt readURL: URL, options readOptions: ReadingOptions = [],
                           writingItemAt writeURL: URL, options writeOptions: WritingOptions = [],
                           error outError: NSErrorPointer, byAccessor accessor: (URL, URL) -> Void) {
        accessor(readURL, writeURL)
    }
}

public func CFAbsoluteTimeGetCurrent() -> Double { Date().timeIntervalSinceReferenceDate }

/// Undo/redo with Apple's registration model (grouping, action names, redo registered while undoing).
open class UndoManager {
    private struct Group { var actions: [() -> Void] = []; var name = "" }
    private var undoStack: [Group] = [], redoStack: [Group] = []
    private var open: Group?
    private var undoing = false, redoing = false
    public private(set) var groupingLevel = 0
    public var groupsByEvent = false
    public init() {}

    open var canUndo: Bool { !undoStack.isEmpty }
    open var canRedo: Bool { !redoStack.isEmpty }
    open var isUndoing: Bool { undoing }
    open var isRedoing: Bool { redoing }
    open var undoActionName: String { undoStack.last?.name ?? "" }
    open var redoActionName: String { redoStack.last?.name ?? "" }

    open func beginUndoGrouping() { groupingLevel += 1; if open == nil { open = Group() } }
    open func endUndoGrouping() {
        groupingLevel = max(0, groupingLevel - 1)
        if groupingLevel == 0, let group = open { open = nil; if !group.actions.isEmpty { push(group) } }
    }
    private func push(_ group: Group) { if undoing { redoStack.append(group) } else { undoStack.append(group); if !redoing { redoStack = [] } } }

    open func registerUndo<Target: AnyObject>(withTarget target: Target, handler: @escaping (Target) -> Void) {
        let action = { [weak target] in if let target { handler(target) } }
        if open != nil { open!.actions.append(action) } else { push(Group(actions: [action])) }
    }
    open func setActionName(_ name: String) { if open != nil { open!.name = name } else if undoing { if !redoStack.isEmpty { redoStack[redoStack.count - 1].name = name } } else if !undoStack.isEmpty { undoStack[undoStack.count - 1].name = name } }

    open func undo() {
        guard let group = undoStack.popLast() else { return }
        undoing = true; beginUndoGrouping()
        for a in group.actions.reversed() { a() }
        open?.name = group.name
        endUndoGrouping(); undoing = false
    }
    open func redo() {
        guard let group = redoStack.popLast() else { return }
        redoing = true; beginUndoGrouping()
        for a in group.actions.reversed() { a() }
        open?.name = group.name
        endUndoGrouping(); redoing = false
    }
    open func removeAllActions() { undoStack = []; redoStack = []; open = nil; groupingLevel = 0 }
}

extension RunLoop.Mode {
    public static let eventTracking = RunLoop.Mode("NSEventTrackingRunLoopMode")
    public static let modalPanel = RunLoop.Mode("NSModalPanelRunLoopMode")
}

