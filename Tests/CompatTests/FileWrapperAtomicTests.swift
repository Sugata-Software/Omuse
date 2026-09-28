import Foundation
import Glibc
import XCTest
@testable import FoundationCompat

final class FileWrapperAtomicTests: XCTestCase {
    private func inDirectory(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        try body(root)
    }

    private func package(_ text: String) -> FileWrapper {
        FileWrapper(directoryWithFileWrappers: [
            "manifest.json": FileWrapper(regularFileWithContents: Data(text.utf8)),
            "images": FileWrapper(directoryWithFileWrappers: [
                "pixels.bin": FileWrapper(regularFileWithContents: Data([1, 2, 3, 4]))
            ])
        ])
    }

    func testNewAndReplacementPackagesPublishAllChildrenAndRemoveOldOnes() throws {
        try inDirectory { root in
            let destination = root.appendingPathComponent("drawing.comp")
            try package("old").write(to: destination, options: .atomic, originalContentsURL: nil)
            try Data([9]).write(to: destination.appendingPathComponent("obsolete"))
            try package("new").write(to: destination, options: .atomic, originalContentsURL: destination)
            let loaded = try FileWrapper(url: destination)
            XCTAssertEqual(loaded.fileWrappers?["manifest.json"]?.regularFileContents, Data("new".utf8))
            XCTAssertEqual(loaded.fileWrappers?["images"]?.fileWrappers?["pixels.bin"]?.regularFileContents,
                           Data([1, 2, 3, 4]))
            XCTAssertNil(loaded.fileWrappers?["obsolete"])
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["drawing.comp"])
        }
    }

    func testPublicationFailurePreservesOldSaveAndCleansStaging() throws {
        for error in [ENOSPC, EACCES, EOPNOTSUPP] {
            try inDirectory { root in
                let destination = root.appendingPathComponent("drawing.comp")
                try package("old").write(to: destination, options: .atomic, originalContentsURL: nil)
                XCTAssertThrowsError(try package("new").writeAtomically(to: destination) { staged, target in
                    XCTAssertEqual(try FileWrapper(url: staged).fileWrappers?["manifest.json"]?.regularFileContents,
                                   Data("new".utf8))
                    XCTAssertEqual(try FileWrapper(url: target).fileWrappers?["manifest.json"]?.regularFileContents,
                                   Data("old".utf8), "The previous save must exist at the commit point")
                    throw NSError(domain: NSPOSIXErrorDomain, code: Int(error))
                }) { failure in
                    XCTAssertEqual((failure as NSError).code, Int(error))
                }
                XCTAssertEqual(try FileWrapper(url: destination).fileWrappers?["manifest.json"]?.regularFileContents,
                               Data("old".utf8))
                XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["drawing.comp"])
            }
        }
    }

    func testStagingFailurePreservesOldSaveAndRemovesPartialPackage() throws {
        try inDirectory { root in
            let destination = root.appendingPathComponent("drawing.comp")
            try package("old").write(to: destination, options: .atomic, originalContentsURL: nil)
            let invalid = FileWrapper(directoryWithFileWrappers: [
                String(repeating: "x", count: 256): FileWrapper(regularFileWithContents: Data([1]))
            ])
            XCTAssertThrowsError(try invalid.write(to: destination, options: .atomic, originalContentsURL: nil))
            XCTAssertEqual(try FileWrapper(url: destination).fileWrappers?["manifest.json"]?.regularFileContents,
                           Data("old".utf8))
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["drawing.comp"])
        }
    }

    func testRegularFileReplacement() throws {
        try inDirectory { root in
            let destination = root.appendingPathComponent("drawing.txt")
            try Data("old".utf8).write(to: destination)
            try FileWrapper(regularFileWithContents: Data("new".utf8))
                .write(to: destination, options: .atomic, originalContentsURL: nil)
            XCTAssertEqual(try Data(contentsOf: destination), Data("new".utf8))
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["drawing.txt"])
        }
    }

    func testDestinationSymlinkIsReplacedWithoutTouchingItsTarget() throws {
        try inDirectory { root in
            let other = root.appendingPathComponent("other.comp")
            let destination = root.appendingPathComponent("drawing.comp")
            try package("unrelated").write(to: other, options: .atomic, originalContentsURL: nil)
            try FileManager.default.createSymbolicLink(at: destination, withDestinationURL: other)
            try package("new").write(to: destination, options: .atomic, originalContentsURL: nil)
            XCTAssertEqual(try FileWrapper(url: other).fileWrappers?["manifest.json"]?.regularFileContents,
                           Data("unrelated".utf8))
            XCTAssertEqual(try FileWrapper(url: destination).fileWrappers?["manifest.json"]?.regularFileContents,
                           Data("new".utf8))
            XCTAssertThrowsError(try FileManager.default.destinationOfSymbolicLink(atPath: destination.path))
        }
    }

    func testInvalidChildNamesFailBeforeWriting() throws {
        for name in ["", ".", "..", "../escaped", "/absolute", "nested/file", "null\0byte"] {
            try inDirectory { root in
                let wrapper = FileWrapper(directoryWithFileWrappers: [
                    name: FileWrapper(regularFileWithContents: Data([1]))
                ])
                for options: FileWrapper.WritingOptions in [[], .atomic] {
                    XCTAssertThrowsError(try wrapper.write(to: root.appendingPathComponent("drawing.comp"),
                                                          options: options, originalContentsURL: nil))
                }
                XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
            }
        }
    }
}
