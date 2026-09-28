// AppKitCompatTests — the AppKit / FoundationCompat / UniformTypeIdentifiers shims that let upstream's model code
// compile unmodified: colours and graphics-context drawing, pasteboard change counts, injectable alerts and panels,
// FileWrapper packages, NSFileCoordinator and UTType conformance.

import CoreGraphics
import XCTest
import AppKit
import FoundationCompat
import UniformTypeIdentifiers

final class AppKitCompatTests: XCTestCase {

    func testNSColorDrawsThroughTheCurrentGraphicsContext() {
        let ctx = CGContext(width: 4, height: 4)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        NSColor(srgbRed: 1, green: 0, blue: 0, alpha: 1).setFill()
        ctx.fill(CGRect(x: 0, y: 0, width: 4, height: 4))
        NSGraphicsContext.restoreGraphicsState()
        XCTAssertNil(NSGraphicsContext.current, "the previous (empty) state is restored")
        XCTAssertEqual(ctx.buffer.bytes[0], 255)
        XCTAssertEqual(NSColor.white.cgColor.red, 1)
        XCTAssertEqual(NSColor(white: 0.25, alpha: 0.5).cgColor.alpha, 0.5)
    }

    func testPasteboardChangeCountAndImageReadiness() {
        let board = NSPasteboard.general
        let before = board.changeCount
        board.clearContents()
        XCTAssertEqual(board.changeCount, before + 1)
        XCTAssertFalse(board.canReadObject(forClasses: [NSImage.self], options: nil))
        board.setData(Data([1, 2, 3]), forType: .png)
        XCTAssertTrue(board.canReadObject(forClasses: [NSImage.self], options: nil))
        XCTAssertEqual(board.data(forType: .png), Data([1, 2, 3]))
    }

    @MainActor func testAlertAndPanelsAreInjectable() {
        let alert = NSAlert()
        alert.addButton(withTitle: "Bake"); alert.addButton(withTitle: "Cancel")
        XCTAssertEqual(alert.runModal(), .alertFirstButtonReturn, "headless default presses the first button")
        NSAlert.handler = { $0.buttonTitles.count == 2 ? .alertSecondButtonReturn : .alertFirstButtonReturn }
        defer { NSAlert.handler = nil }
        XCTAssertEqual(alert.runModal(), .alertSecondButtonReturn)
        XCTAssertEqual(NSOpenPanel().runModal(), .cancel, "no host handler means the user cancelled")
    }

    func testFileWrapperPackageRoundTripsAtomically() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("wrapper-\(UUID()).comp")
        defer { try? FileManager.default.removeItem(at: dir) }
        let package = FileWrapper(directoryWithFileWrappers: [
            "manifest.json": FileWrapper(regularFileWithContents: Data("{}".utf8)),
            "images": FileWrapper(directoryWithFileWrappers: ["a.png": FileWrapper(regularFileWithContents: Data([9, 8]))]),
        ])
        var coordinationError: NSError?
        var wrote = false
        NSFileCoordinator().coordinate(writingItemAt: dir, options: .forReplacing, error: &coordinationError) { destination in
            wrote = (try? package.write(to: destination, options: .atomic, originalContentsURL: nil)) != nil
        }
        XCTAssertTrue(wrote); XCTAssertNil(coordinationError)
        let read = try FileWrapper(url: dir)
        XCTAssertTrue(read.isDirectory)
        XCTAssertEqual(read.fileWrappers?["manifest.json"]?.regularFileContents, Data("{}".utf8))
        XCTAssertEqual(read.fileWrappers?["images"]?.fileWrappers?["a.png"]?.regularFileContents, Data([9, 8]))
        // Replacing an existing package must leave no temporary sibling behind.
        try package.write(to: dir, options: .atomic, originalContentsURL: nil)
        let siblings = try FileManager.default.contentsOfDirectory(atPath: dir.deletingLastPathComponent().path)
        XCTAssertFalse(siblings.contains { $0.hasPrefix(".\(dir.lastPathComponent)") })
    }

    func testUTTypeConformanceAndExtensions() {
        XCTAssertTrue(UTType.png.conforms(to: .image))
        XCTAssertTrue(UTType.jpeg.conforms(to: .data))
        XCTAssertFalse(UTType.pdf.conforms(to: .image))
        XCTAssertEqual(UTType(filenameExtension: "JPG"), .jpeg)
        XCTAssertEqual(UTType.tiff.preferredFilenameExtension, "tiff")
        let project = UTType(exportedAs: "com.example.project", conformingTo: .package)
        UTType.register(project, filenameExtension: "comp")
        XCTAssertEqual(UTType(filenameExtension: "comp", conformingTo: .package), project)
    }

    func testItemProviderResolvesConformingRepresentations() {
        let provider = NSItemProvider(representations: ["public.png": Data([7])])
        XCTAssertTrue(provider.hasItemConformingToTypeIdentifier("public.image"))
        let received = expectation(description: "loaded")
        provider.loadDataRepresentation(forTypeIdentifier: "public.image") { data, _ in
            XCTAssertEqual(data, Data([7])); received.fulfill()
        }
        wait(for: [received], timeout: 1)
    }

    func testCursorTokensAndBeepHook() {
        var beeps = 0
        NSSound.onBeep = { beeps += 1 }
        defer { NSSound.onBeep = nil }
        NSSound.beep()
        XCTAssertEqual(beeps, 1)
        NSCursor.iBeam.push()
        XCTAssertEqual(NSCursor.current, .iBeam)
        NSCursor.pop()
        XCTAssertEqual(NSCursor.current, .arrow)
    }
}
