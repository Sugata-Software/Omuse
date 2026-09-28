// SessionABI.swift — the C ABI the Qt shell calls (`compositor_session_*`), over upstream's unmodified EditorSession.
//
// Same functions, signatures and return codes as the fork's EditorBridge, which this replaces. Handles are allocated IDs,
// never foreign pointers. Every entry point must be called on the process main thread (the Qt shell does; the Swift
// `@main` bootstrap owns it): sessions are main-actor objects, entered with `MainActor.assumeIsolated`, and async
// upstream operations are awaited by pumping the run loop (see UpstreamEditor.command).

import Foundation
import CoreGraphics

// Not `private`: SwiftUIBridge.swift's `compositor_session_render_tree` reuses this same handle/session registry
// (a Qt-visible tree of the same session's panels, not a separate one) rather than duplicating it.
final class Entry {
    let editor = UpstreamEditor()
    var rendered: (bytes: [UInt8], width: Int, height: Int)?
    /// The last resolved SwiftUI tree's action handlers, per panel: panel name -> node id -> handler key -> closure.
    /// Populated by `SwiftUIBridge.swift`'s `resolvePanel`, read by `compositor_session_dispatch_swiftui_action`.
    var actionHandlers: [String: [String: [String: (Any) -> Void]]] = [:]
}

enum Sessions {
    nonisolated(unsafe) static var next: UInt64 = 1
    nonisolated(unsafe) static var entries: [UInt64: Entry] = [:]
}

/// Runs `body` on the main actor from a C entry point (which the shell calls on the main thread).
func onMain<Result>(_ body: @MainActor () -> Result) -> Result {
    if Thread.isMainThread { return MainActor.assumeIsolated(body) }
    return DispatchQueue.main.sync { MainActor.assumeIsolated(body) }
}

func withEntry(_ handle: UInt64, _ body: @MainActor (Entry) -> Int64) -> Int64 {
    onMain {
        guard let entry = Sessions.entries[handle] else { return -6 }
        return body(entry)
    }
}

/// Kept for source compatibility with the fork's composition root: brush acceleration is chosen by the Metal override
/// (`BrushCoverageBackends.automatic()`), which honours `COMPOSITOR_BRUSH_BACKEND`.
nonisolated public func compositorConfigureBrushAcceleration(_ enabled: Bool) {
    if !enabled { setenv("COMPOSITOR_BRUSH_BACKEND", "cpu", 1) }
}

@_cdecl("compositor_session_create")
nonisolated public func compositorSessionCreate() -> UInt64 {
    onMain {
        guard Sessions.next < UInt64.max else { return 0 }
        let handle = Sessions.next
        Sessions.next += 1
        Sessions.entries[handle] = Entry()
        return handle
    }
}

@_cdecl("compositor_session_close")
nonisolated public func compositorSessionClose(_ handle: UInt64) {
    onMain { _ = Sessions.entries.removeValue(forKey: handle) }
}

@_cdecl("compositor_session_command")
nonisolated public func compositorSessionCommand(_ handle: UInt64, _ json: UnsafePointer<UInt8>?, _ count: Int) -> Int32 {
    guard let json, count > 0, count <= 1_048_576 else { return -1 }
    let data = Data(bytes: json, count: count)
    return Int32(withEntry(handle) { entry in
        let code = entry.editor.command(data)
        if code == 0 { entry.rendered = nil }
        return Int64(code)
    })
}

@_cdecl("compositor_session_import_rgba")
nonisolated public func compositorSessionImportRGBA(_ handle: UInt64, _ pixels: UnsafePointer<UInt8>?, _ count: Int,
                                                    _ width: Int, _ height: Int, _ name: UnsafePointer<UInt8>?, _ nameCount: Int,
                                                    _ replacing: Int32) -> Int32 {
    guard (1...30_000).contains(width), (1...30_000).contains(height), width * height <= 100_000_000,
          count == width * height * 4, let pixels, let name, (1...16_384).contains(nameCount),
          let title = String(data: Data(bytes: name, count: nameCount), encoding: .utf8) else { return -1 }
    let bytes = Array(UnsafeBufferPointer(start: pixels, count: count))
    return Int32(withEntry(handle) { entry in
        let code = entry.editor.importRGBA(bytes, width: width, height: height, name: title, replacing: replacing != 0)
        if code == 0 { entry.rendered = nil }
        return Int64(code)
    })
}

@_cdecl("compositor_session_state")
nonisolated public func compositorSessionState(_ handle: UInt64, _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int64 {
    guard capacity >= 0 else { return -1 }
    return withEntry(handle) { entry in
        guard let data = try? entry.editor.stateJSON() else { return -5 }
        if let output, capacity >= data.count { data.copyBytes(to: output, count: data.count) }
        return Int64(data.count)
    }
}

@_cdecl("compositor_session_render")
nonisolated public func compositorSessionRender(_ handle: UInt64, _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int64 {
    guard capacity >= 0 else { return -1 }
    return withEntry(handle) { entry in
        let image: (bytes: [UInt8], width: Int, height: Int)
        if let cached = entry.rendered { image = cached }
        else {
            guard entry.editor.session.document != nil else { return -2 }
            guard let made = try? entry.editor.renderRGBA() else { return -5 }
            image = made; entry.rendered = made
        }
        if let output, capacity >= image.bytes.count { image.bytes.withUnsafeBufferPointer { output.update(from: $0.baseAddress!, count: $0.count) } }
        return Int64(image.bytes.count)
    }
}

@_cdecl("compositor_session_export_manifest")
nonisolated public func compositorSessionExportManifest(_ handle: UInt64, _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int64 {
    guard capacity >= 0 else { return -1 }
    return withEntry(handle) { entry in
        guard let data = try? entry.editor.exportManifest() else { return -2 }
        if let output, capacity >= data.count { data.copyBytes(to: output, count: data.count) }
        return Int64(data.count)
    }
}

@_cdecl("compositor_session_import_manifest")
nonisolated public func compositorSessionImportManifest(_ handle: UInt64, _ json: UnsafePointer<UInt8>?, _ count: Int) -> Int32 {
    guard let json, count > 0, count <= 4 * 1_048_576 else { return -1 }
    let data = Data(bytes: json, count: count)
    return Int32(withEntry(handle) { entry in
        let code = entry.editor.importManifest(data)
        if code == 0 { entry.rendered = nil }
        return Int64(code)
    })
}

@_cdecl("compositor_session_export_layer")
nonisolated public func compositorSessionExportLayer(_ handle: UInt64, _ layerID: UnsafePointer<UInt8>?, _ layerIDCount: Int,
                                                     _ mask: Int32, _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int,
                                                     _ width: UnsafeMutablePointer<Int>?, _ height: UnsafeMutablePointer<Int>?) -> Int64 {
    guard let layerID, (1...128).contains(layerIDCount), capacity >= 0,
          let idString = String(data: Data(bytes: layerID, count: layerIDCount), encoding: .utf8),
          let id = UUID(uuidString: idString) else { return -1 }
    return withEntry(handle) { entry in
        guard let image = entry.editor.layerPixels(id: id, mask: mask != 0) else { return -5 }
        width?.pointee = image.width
        height?.pointee = image.height
        guard image.bytes.count <= capacity || output == nil else { return Int64(image.bytes.count) }
        if let output { image.bytes.withUnsafeBufferPointer { output.update(from: $0.baseAddress!, count: $0.count) } }
        return Int64(image.bytes.count)
    }
}

@_cdecl("compositor_session_import_layer")
nonisolated public func compositorSessionImportLayer(_ handle: UInt64, _ layerID: UnsafePointer<UInt8>?, _ layerIDCount: Int,
                                                     _ mask: Int32, _ pixels: UnsafePointer<UInt8>?, _ count: Int,
                                                     _ width: Int, _ height: Int) -> Int32 {
    guard let layerID, (1...128).contains(layerIDCount), let pixels, (1...30_000).contains(width), (1...30_000).contains(height),
          width * height <= 100_000_000,
          let idString = String(data: Data(bytes: layerID, count: layerIDCount), encoding: .utf8),
          let id = UUID(uuidString: idString) else { return -1 }
    let expected = width * height * (mask == 0 ? 4 : 1)
    guard count == expected else { return -1 }
    if mask == 0 {
        for i in stride(from: 0, to: count, by: 4) {
            guard pixels[i] <= pixels[i + 3], pixels[i + 1] <= pixels[i + 3], pixels[i + 2] <= pixels[i + 3] else { return -1 }
        }
    }
    let image = PortableImage(width: width, height: height, kind: mask == 0 ? .rgba : .mask, bytesPerRow: width * (mask == 0 ? 4 : 1),
                              bytes: Array(UnsafeBufferPointer(start: pixels, count: count)))
    return Int32(withEntry(handle) { entry in
        let code = entry.editor.installLayerAsset(image, id: id, mask: mask != 0)
        if code == 0 { entry.rendered = nil }
        return Int64(code)
    })
}
