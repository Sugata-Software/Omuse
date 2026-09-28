import Foundation
import Testing
import SwiftUI
@testable import Compositor

@MainActor struct UIIntegrationTests {
    func command(_ editor: UpstreamEditor, _ action: String, _ fields: [String: Any] = [:]) async throws -> Int32 {
        var json=fields; json["version"]=1; json["action"]=action
        return await editor.commandAsync(try JSONSerialization.data(withJSONObject:json))
    }
    @Test func multiLayerDropIsAtomicUndoableAndRejectsCycles() async throws {
        let e=UpstreamEditor()
        #expect(try await command(e,"new",["width":32,"height":32])==0)
        let first=try #require(e.session.activeLayerID)
        _ = try await command(e,"addLayer")
        let second=try #require(e.session.activeLayerID)
        _ = try await command(e,"addGroup")
        let folder=try #require(e.session.activeLayerID)
        let before=e.session.document!.layers.map { [$0.id.uuidString, $0.parentID?.uuidString ?? ""] }
        #expect(try await command(e,"placeLayers",["layerIDs":[first.uuidString,second.uuidString],"parentID":folder.uuidString])==0)
        #expect(e.session.document!.layers.filter { $0.parentID==folder }.count==2)
        #expect(e.session.selectedLayerIDs==Set([first,second]))
        #expect(e.session.history.undoName=="Move Layers")
        _ = try await command(e,"undo")
        #expect(e.session.document!.layers.map { [$0.id.uuidString, $0.parentID?.uuidString ?? ""] }==before)
        #expect(try await command(e,"placeLayers",["layerIDs":[folder.uuidString],"parentID":folder.uuidString]) == -1)
        #expect(e.session.document!.layers.map { [$0.id.uuidString, $0.parentID?.uuidString ?? ""] }==before)
        #expect(try await command(e,"placeLayers",["layerIDs":[first.uuidString,second.uuidString],"targetID":UUID().uuidString]) == -1)
        #expect(e.session.document!.layers.map { [$0.id.uuidString, $0.parentID?.uuidString ?? ""] }==before)
    }
    @Test func selectedFolderCarriesDescendantsAndCopyDoesNotMoveOriginal() async throws {
        let e=UpstreamEditor(); _ = try await command(e,"new",["width":16,"height":16])
        _ = try await command(e,"addGroup"); let folder=try #require(e.session.activeLayerID)
        _ = try await command(e,"addLayer"); let child=try #require(e.session.activeLayerID)
        #expect(e.session.activeLayer?.parentID==folder)
        let count=e.session.document!.layers.count
        #expect(try await command(e,"placeLayers",["layerIDs":[folder.uuidString,child.uuidString],"enabled":true])==0)
        #expect(e.session.document!.layers.count==count+2)
        #expect(e.session.document!.layers.first { $0.id==child }?.parentID==folder)
        _ = try await command(e,"undo"); #expect(e.session.document!.layers.count==count)
    }
    @Test func sharedColorPickerCancelAndCommitRespectTarget() async throws {
        let e=UpstreamEditor(); _ = try await command(e,"new",["width":16,"height":16])
        let original=e.session.foregroundColor
        e.session.openColorPicker(background:false)
        #expect(try await command(e,"finishColorPicker",["enabled":false])==0)
        #expect(e.session.foregroundColor==original && e.session.colorPicker==nil)
        e.session.openColorPicker(background:true)
        #expect(try await command(e,"finishColorPicker",["enabled":true,"parameters":["red":1,"green":0,"blue":0]])==0)
        #expect(e.session.foregroundColor==original)
        #expect(e.session.backgroundColor==PaletteColor(red:1,green:0,blue:0))
        _ = try await command(e,"swapPalette")
        #expect(e.session.foregroundColor.red==1)
    }
    @Test func shapePointerUsesSharedKindAndLineWidth() async throws {
        let e=UpstreamEditor(); _ = try await command(e,"new",["width":32,"height":32])
        e.session.selectTool(.shape); e.session.shapeKind = .line; e.session.shapeLineWidth=7
        _ = try await command(e,"toolPointerBegin",["x":4,"y":4])
        _ = try await command(e,"toolPointerMove",["x":20,"y":20])
        _ = try await command(e,"toolPointerEnd")
        #expect(e.session.activeLayer?.shape?.style.kind == .line)
        #expect(e.session.activeLayer?.shape?.style.lineWidth==7)
        #expect(e.session.history.undoName=="Line")
    }
    @Test func implicitStringPickerWritesCropRatio() throws {
        let s=EditorSession()
        func flatten(_ node:RenderNode)->[RenderNode] { [node]+node.children.flatMap(flatten) }
        let picker=try #require(flatten(ViewResolver.resolve(CropControls(session:s))).first { $0.kind=="Picker" })
        picker.handlers["selection"]?(2)
        #expect(s.cropRatioChoice=="1:1")
    }
    @Test func shortcutConflictsAndInvalidChordsRejected() {
        #expect(ShortcutSettings.problem(in:["Canvas & Layers:Brush tool":ShortcutChord("v")]) != nil)
        #expect(ShortcutSettings.problem(in:["Canvas & Layers:Brush tool":ShortcutChord("k")]) == nil)
        #expect(ShortcutSettings.problem(in:["Canvas & Layers:Brush tool":ShortcutChord("")]) != nil)
        #expect(ShortcutChord("k",3).label=="Alt+Ctrl+K")
    }
}
