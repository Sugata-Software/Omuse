// CompositorHostBootstrap — the Swift-side composition root for the Linux port.
//
// Why a Swift `@main`: on the Freedesktop Swift 6.3 SDK a non-Swift `main` cannot
// bootstrap the Swift runtime + Foundation (no `swift_initSwiftRuntime`; static
// embedding SEGVs at the first `Dictionary` allocation; shared embedding traps
// in Foundation `JSONDecoder`/`Data.withUnsafeBytes` even on pure-Swift `Data`).
// SwiftPM bootstraps both when the entry point is Swift, so the composition root
// is inverted: this Swift `@main` initializes the runtime and Foundation, then
// drives the host through a C ABI. In the Flatpak build, `main` here calls the
// Qt host's `compositor_host_run(argc, argv)` C entry (host/main.cpp) instead of
// the self-test below; the Qt host in turn calls back into `compositor_session_*`.
//
// This target's `main` runs the same create->new->paint->render->undo->redo->
// close journey through the real `compositor_session_*` C ABI that
// SessionJourneyTests verifies under `swift test`. It is the architectural proof
// that the Swift-`@main` root unblocks the host journey (Foundation works here,
// where it traps from a C++ main). Run: `swift run CompositorHostBootstrap`.

import Compositor
import CoreGraphics
import ImageIO
import Foundation
import HostRun

@main
struct HostBootstrap {
    static func main() {
        compositorConfigureBrushAcceleration(ProcessInfo.processInfo.environment["COMPOSITOR_BRUSH_BACKEND"] != "cpu")
        if CommandLine.arguments.contains("--ui-smoke") {
            let result = compositor_host_ui_smoke(CommandLine.argc, CommandLine.unsafeArgv)
            guard result == 0 else { fail("Qt UI smoke returned \(result)") }
            return
        }
        if CommandLine.arguments.contains("--dialog-smoke") {
            let result = compositor_host_dialog_smoke(CommandLine.argc, CommandLine.unsafeArgv)
            guard result == 0 else { fail("Qt dialog smoke returned \(result)") }
            return
        }
        if CommandLine.arguments.contains("--io-smoke") {
            let result = compositor_host_io_smoke(CommandLine.argc, CommandLine.unsafeArgv)
            guard result == 0 else { fail("Qt IO smoke returned \(result)") }
            // The same Qt plugins through Apple's CGImageSource/CGImageDestination API.
            guard ImageCodecRegistry.host != nil else { fail("Qt image backend was not registered") }
            var pixels = PixelBuffer(width: 8, height: 8)
            for y in 0..<8 { for x in 0..<8 { pixels[x, y] = (UInt8(x * 30), UInt8(y * 30), 120, 255) } }
            let jpegData = NSMutableData()
            guard let destination = CGImageDestinationCreateWithData(jpegData, "public.jpeg" as CFString, 1, nil) else { fail("no JPEG destination") }
            CGImageDestinationAddImage(destination, CGImage(pixels), [kCGImageDestinationLossyCompressionQuality: 0.9] as CFDictionary)
            guard CGImageDestinationFinalize(destination), let source = CGImageSourceCreateWithData(jpegData as Data as CFData, nil),
                  CGImageSourceGetType(source) as String? == "public.jpeg",
                  let decoded = CGImageSourceCreateImageAtIndex(source, 0, nil), decoded.width == 8, decoded.height == 8 else {
                fail("JPEG did not round-trip through CGImageSource/CGImageDestination")
            }
            print("CompositorHostBootstrap: Qt IO smoke OK (ImageIO JPEG via Qt plugins)")
            return
        }
        if CommandLine.arguments.contains("--layers-smoke") {
            let result = compositor_host_layers_smoke(CommandLine.argc, CommandLine.unsafeArgv)
            guard result == 0 else { fail("Qt layers smoke returned \(result)") }
            print("CompositorHostBootstrap: Qt layers smoke OK")
            return
        }
        if CommandLine.arguments.contains("--brush-smoke") {
            let result = compositor_host_brush_smoke(CommandLine.argc, CommandLine.unsafeArgv)
            guard result == 0 else { fail("Qt brush smoke returned \(result)") }
            print("CompositorHostBootstrap: Qt brush smoke OK")
            return
        }
        if CommandLine.arguments.contains("--session-smoke") {
            runSessionSmoke()
            print("CompositorHostBootstrap: session journey OK (create/new/paint/render/undo/redo/close)")
            return
        }

        // Run the Qt host window/event loop
        let qt = compositor_host_run(CommandLine.argc, CommandLine.unsafeArgv)
        guard qt == 0 else { fail("compositor_host_run returned \(qt)") }
    }

    private static func runSessionSmoke() {
        let h = compositorSessionCreate()
        guard h != 0 else { fail("session create returned 0") }

        func cmd(_ json: String) -> Int32 {
            let bytes = Array(json.utf8)
            return bytes.withUnsafeBufferPointer { buf in
                compositorSessionCommand(h, buf.baseAddress, buf.count)
            }
        }

        guard cmd(#"{"version":1,"action":"new","width":4,"height":4}"#) == 0 else { fail("new canvas") }
        guard cmd(#"{"version":1,"action":"addLayer"}"#) == 0 else { fail("add layer") }
        guard cmd(#"{"version":1,"action":"brushBegin","x":1,"y":1,"parameters":{"diameter":3,"hardness":1,"opacity":1,"red":1,"green":0,"blue":0,"erasing":0,"mask":0}}"#) == 0 else { fail("brush begin") }
        guard cmd(#"{"version":1,"action":"brushMove","x":2,"y":2}"#) == 0 else { fail("brush move") }
        guard cmd(#"{"version":1,"action":"brushEnd"}"#) == 0 else { fail("brush end") }

        let stateN = compositorSessionState(h, nil, 0)
        guard stateN > 0 else { fail("state byte count") }
        var stateBytes = [UInt8](repeating: 0, count: Int(stateN))
        _ = stateBytes.withUnsafeMutableBufferPointer { buf in
            compositorSessionState(h, buf.baseAddress, Int(stateN))
        }
        let stateJSON = String(bytes: stateBytes, encoding: .utf8) ?? ""
        guard stateJSON.contains(#""width":4"#) else { fail("state width, got: \(stateJSON)") }
        guard stateJSON.contains(#""canUndo":true"#) else { fail("state canUndo, got: \(stateJSON)") }

        var pixels = [UInt8](repeating: 0, count: 64)
        let n = pixels.withUnsafeMutableBufferPointer { buf in
            compositorSessionRender(h, buf.baseAddress, 64)
        }
        guard n == 64 else { fail("render byte count \(n)") }
        var hasRed = false
        for i in stride(from: 0, to: 64, by: 4) {
            if pixels[i] > 0 && pixels[i + 1] == 0 && pixels[i + 2] == 0 && pixels[i + 3] > 0 { hasRed = true; break }
        }
        guard hasRed else { fail("render shows red paint after stroke") }

        guard cmd(#"{"version":1,"action":"undo"}"#) == 0 else { fail("undo") }
        _ = pixels.withUnsafeMutableBufferPointer { buf in compositorSessionRender(h, buf.baseAddress, 64) }
        var blank = true
        for i in stride(from: 0, to: 64, by: 4) {
            if pixels[i] != 0 || pixels[i + 1] != 0 || pixels[i + 2] != 0 || pixels[i + 3] != 0 { blank = false; break }
        }
        guard blank else { fail("render blank after undo") }
        guard cmd(#"{"version":1,"action":"redo"}"#) == 0 else { fail("redo") }
        _ = pixels.withUnsafeMutableBufferPointer { buf in compositorSessionRender(h, buf.baseAddress, 64) }
        var red2 = false
        for i in stride(from: 0, to: 64, by: 4) {
            if pixels[i] > 0 && pixels[i + 1] == 0 && pixels[i + 2] == 0 && pixels[i + 3] > 0 { red2 = true; break }
        }
        guard red2 else { fail("render red after redo") }

        compositorSessionClose(h)
        guard cmd(#"{"version":1,"action":"undo"}"#) == -6 else { fail("closed handle rejected") }
    }

    private static func fail(_ msg: String) -> Never {
        FileHandle.standardError.write("CompositorHostBootstrap FAIL: \(msg)\n".data(using: .utf8)!)
        exit(1)
    }
}
