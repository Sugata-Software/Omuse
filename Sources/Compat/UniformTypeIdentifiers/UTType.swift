// UniformTypeIdentifiers for Linux: the type identifiers, filename extensions and conformance relations upstream
// uses to pick codecs and file filters (images, project package). A small table, not the system registry.

import Foundation

public struct UTType: Hashable, Sendable {
    public let identifier: String

    private struct Info { let ext: [String]; let mime: String?; let parents: [String] }
    private static let table: [String: Info] = [
        "public.item": Info(ext: [], mime: nil, parents: []),
        "public.data": Info(ext: [], mime: nil, parents: ["public.item"]),
        "public.content": Info(ext: [], mime: nil, parents: ["public.item"]),
        "public.image": Info(ext: [], mime: nil, parents: ["public.data", "public.content"]),
        "public.png": Info(ext: ["png"], mime: "image/png", parents: ["public.image"]),
        "public.jpeg": Info(ext: ["jpg", "jpeg"], mime: "image/jpeg", parents: ["public.image"]),
        "public.tiff": Info(ext: ["tiff", "tif"], mime: "image/tiff", parents: ["public.image"]),
        "public.heic": Info(ext: ["heic"], mime: "image/heic", parents: ["public.image"]),
        "public.heif": Info(ext: ["heif"], mime: "image/heif", parents: ["public.image"]),
        "com.compuserve.gif": Info(ext: ["gif"], mime: "image/gif", parents: ["public.image"]),
        "com.microsoft.bmp": Info(ext: ["bmp"], mime: "image/bmp", parents: ["public.image"]),
        "org.webmproject.webp": Info(ext: ["webp"], mime: "image/webp", parents: ["public.image"]),
        "com.adobe.pdf": Info(ext: ["pdf"], mime: "application/pdf", parents: ["public.data", "public.content"]),
        "public.json": Info(ext: ["json"], mime: "application/json", parents: ["public.data"]),
        "public.plain-text": Info(ext: ["txt"], mime: "text/plain", parents: ["public.data", "public.content"]),
        "public.folder": Info(ext: [], mime: nil, parents: ["public.item"]),
        "public.file-url": Info(ext: [], mime: nil, parents: ["public.url"]),
        "public.url": Info(ext: [], mime: nil, parents: ["public.data"]),
        "com.apple.package": Info(ext: [], mime: nil, parents: ["public.item"]),
        "public.camera-raw-image": Info(ext: ["dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "pef", "srw"],
                                        mime: nil, parents: ["public.image"]),
    ]
    /// Types an app declares for itself (exported/imported) are registered here.
    nonisolated(unsafe) private static var declared: [String: Info] = [:]

    private var info: Info? { Self.table[identifier] ?? Self.declared[identifier] }

    public init?(_ identifier: String) {
        guard Self.table[identifier] != nil || Self.declared[identifier] != nil else { return nil }
        self.identifier = identifier
    }
    public init(exportedAs identifier: String, conformingTo parent: UTType = .data) {
        self.identifier = identifier
        if Self.declared[identifier] == nil { Self.declared[identifier] = Info(ext: [], mime: nil, parents: [parent.identifier]) }
    }
    public init(importedAs identifier: String, conformingTo parent: UTType = .data) {
        self.init(exportedAs: identifier, conformingTo: parent)
    }
    public init?(filenameExtension ext: String, conformingTo supertype: UTType = .data) {
        let lower = ext.lowercased()
        guard let match = Self.table.first(where: { $0.value.ext.contains(lower) })
                ?? Self.declared.first(where: { $0.value.ext.contains(lower) }) else { return nil }
        let candidate = UTType(match.key)!
        guard candidate.conforms(to: supertype) else { return nil }
        self = candidate
    }
    public init?(mimeType: String, conformingTo supertype: UTType = .data) {
        guard let match = Self.table.first(where: { $0.value.mime == mimeType }) else { return nil }
        self = UTType(match.key)!
    }

    /// Declares the filename extension of an app-defined type (Apple reads it from the Info.plist).
    public static func register(_ type: UTType, filenameExtension ext: String, mimeType: String? = nil) {
        let parents = declared[type.identifier]?.parents ?? ["public.data"]
        declared[type.identifier] = Info(ext: [ext], mime: mimeType, parents: parents)
    }

    public var preferredFilenameExtension: String? { info?.ext.first }
    public var tags: [UTTagClass: [String]] { [.filenameExtension: info?.ext ?? []] }
    public var preferredMIMEType: String? { info?.mime }

    public func conforms(to other: UTType) -> Bool {
        if identifier == other.identifier { return true }
        return (info?.parents ?? []).contains { UTType($0)?.conforms(to: other) == true }
    }

    public enum UTTagClass: Hashable, Sendable { case filenameExtension, mimeType }

    public static let item = UTType("public.item")!
    public static let data = UTType("public.data")!
    public static let content = UTType("public.content")!
    public static let image = UTType("public.image")!
    public static let png = UTType("public.png")!
    public static let jpeg = UTType("public.jpeg")!
    public static let tiff = UTType("public.tiff")!
    public static let heic = UTType("public.heic")!
    public static let heif = UTType("public.heif")!
    public static let gif = UTType("com.compuserve.gif")!
    public static let bmp = UTType("com.microsoft.bmp")!
    public static let webP = UTType("org.webmproject.webp")!
    public static let pdf = UTType("com.adobe.pdf")!
    public static let json = UTType("public.json")!
    public static let plainText = UTType("public.plain-text")!
    public static let folder = UTType("public.folder")!
    public static let fileURL = UTType("public.file-url")!
    public static let url = UTType("public.url")!
    public static let package = UTType("com.apple.package")!
    public static let rawImage = UTType("public.camera-raw-image")!
}
