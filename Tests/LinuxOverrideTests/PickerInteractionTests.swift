import Foundation
import Testing
import SwiftUI
@testable import Compositor

@MainActor
struct PickerInteractionTests {
    private func flatten(_ node: RenderNode) -> [RenderNode] { [node] + node.children.flatMap(flatten) }

    @Test func brushModePickerWritesTypedEnumAndIgnoresInvalidIndices() throws {
        let session = EditorSession()
        session.selectTool(.brush)
        let node = try #require(flatten(ViewResolver.resolve(BrushControls(session: session))).first { $0.kind == "Picker" })
        let change = try #require(node.handlers["selection"])
        change(1)
        #expect(session.brushMode == .erase)
        change(-1); change(2); change("not an index")
        #expect(session.brushMode == .erase)
        change(0)
        #expect(session.brushMode == .paint)
    }

    @Test func cloneSamplePickerPreservesBooleanTags() throws {
        let session = EditorSession()
        session.selectTool(.cloneStamp)
        let node = try #require(flatten(ViewResolver.resolve(BrushControls(session: session))).first { $0.kind == "Picker" })
        let change = try #require(node.handlers["selection"])
        change(1)
        #expect(session.cloneSettings.sampleAllLayers)
        change(0)
        #expect(!session.cloneSettings.sampleAllLayers)
    }

    @Test func startingStrokePreservesSharedSmoothingAndSettings() async throws {
        let editor = UpstreamEditor()
        #expect(await editor.commandAsync(Data(#"{"version":1,"action":"new","width":32,"height":32}"#.utf8)) == 0)
        editor.session.brushSettings.smoothing = 30
        editor.session.brushSettings.diameter = 12
        #expect(await editor.commandAsync(Data(#"{"version":1,"action":"brushBegin","x":16,"y":16}"#.utf8)) == 0)
        #expect(editor.session.brushSettings.diameter == 12)
        #expect(editor.session.brushSettings.smoothing == 30)
        editor.session.cancelBrush()
    }
}
