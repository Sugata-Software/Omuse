// UpstreamEditor.swift — the Linux shell's command adapter over upstream's UNMODIFIED `EditorSession`.
//
// Replaces the forked `Sources/CompositorCore/EditorBridge.swift`, which drove a hand-edited copy of the session. This
// file lives inside the `Compositor` module (via Sources/UpstreamCore/LinuxBridge) only to reach upstream's internal
// types without editing them; it holds no editing logic, every command calls an upstream method.
//
// Threading: the Qt shell calls from the process's main thread while the C++ event loop runs. Synchronous commands run
// with `MainActor.assumeIsolated`; upstream's async operations (fill, invert, commit, render) are awaited by pumping the
// run loop from that same plain callback (`awaitOnMain`), which lets the main-actor task progress. Verified by
// Tests/LinuxOverrideTests (differential tests against the fork's bridge, the behavioural spec being replaced).

import Foundation
import CoreGraphics

/// Runs `operation` (main-actor, async) to completion from a synchronous call on the main thread.
func awaitOnMain<Value: Sendable>(_ operation: @escaping @MainActor @Sendable () async -> Value) -> Value {
    nonisolated(unsafe) var result: Value?
    Task { @MainActor in result = await operation() }
    while result == nil { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.001)) }
    return result!
}

private struct Command: Decodable {
    let version: Int
    let action: String
    var width: Int?
    var height: Int?
    var layerID: UUID?
    var name: String?
    var kind: String?
    var value: Double?
    var enabled: Bool?
    var forward: Bool?
    var horizontally: Bool?
    var x: Double?
    var y: Double?
    var parameters: [String: Double]?
    var points: [[Double]]?
    var adjustment: LayerAdjustment?
    var layerIDs: [UUID]?
    var parentID: UUID?
    var targetID: UUID?
    var shortcuts: [String: ShortcutChord]?
}

private struct State: Encodable {
    struct Layer: Encodable {
        let id: UUID
        let name: String
        let visible: Bool
        let opacity: Double
        let blendMode: String
        let parentID: UUID?
        let isGroup: Bool
        let hasMask: Bool
        let maskEnabled: Bool?
        let maskLinked: Bool?
        let maskPlacement: LayerTransform?
        let transform: LayerTransform
    }
    let version = 1
    let width: Int
    let height: Int
    let resolution: Double
    let activeLayerID: UUID?
    let modified: Bool
    let canUndo: Bool
    let canRedo: Bool
    let undoName: String
    let redoName: String
    let busy: Bool
    let layers: [Layer]
    let error: String?
    let hasSelection: Bool
    let canTransformSelection: Bool
    let isMaskSelected: Bool
    let activeLayerIsVisible: Bool
    let activeLayerIsGroup: Bool
    let activeLayerHasMask: Bool
    let activeLayerHasParent: Bool
    let activeLayerIsClipped: Bool
    let canToggleClippingMask: Bool
    let canMergeLayers: Bool
    let mergeTitle: String
    let canMoveActiveLayerUp: Bool
    let canMoveActiveLayerDown: Bool
    let tool: String
    let brush: [String: Double]
    let selectionControls: [String: String]
    let selectedLayerIDs: [UUID]
    let palette: [String: Double]
    let colorPicker: [String: Double]?
    let shortcuts: [ShortcutRecord]
    let toolSettings: [String: String]
    let cropRect: [Double]?
    let textContent: String?
    struct ShortcutRecord: Encodable {
        let id: String
        let title: String
        let group: String
        let key: String
        let modifiers: Int
        let originalKey: String
        let originalModifiers: Int
    }
}

/// Result codes shared with the C ABI: 0 ok, -1 invalid argument, -2 no document, -3 busy, -4 unsupported version,
/// -5 operation failed, -6 unknown handle, -7 command not supported by this bridge yet.
final class UpstreamEditor {
    let session = EditorSession()
    private(set) var error: String?

    /// Commands this adapter can run today; the rest report -7 so callers (and the parity tests) see the gap explicitly.
    static let supportedActions: Set<String> = [
        "markSaved", "syncToolSettings", "setCropRect", "saveShortcuts", "placeLayers", "validateLayerDrop", "selectLayers", "setPaletteColor", "swapPalette", "resetPalette",
        "openColorPicker", "finishColorPicker", "toolPointerBegin", "toolPointerMove", "toolPointerEnd", "toolCancel", "toolApply", "cycleToolMode", "setTextContent",
        "new", "addLayer", "addGroup", "groupSelectedLayers", "selectLayer", "deleteLayer", "renameLayer", "setVisible",
        "setOpacity", "setBlendMode", "setSelectedOpacity", "cycleBlendMode", "flipLayer", "flipCanvas", "undo", "redo",
        "addRevealMask", "addHideMask", "deleteMask", "setMaskEnabled", "setMaskLinked", "moveLayer",
        "selectAll", "deselect", "invertSelection", "loadLayerSelection", "loadMaskSelection", "selectSubject", "featherSelection",
        "selectRectangle", "selectEllipse", "selectLasso", "expandSelection", "contractSelection",
        "fillForeground", "fillBackground", "clearSelection", "invert", "copy", "copyMerged", "cut", "paste",
        "duplicateLayer", "layerViaCopy", "toggleClippingMask", "moveActiveLayer", "moveActiveLayerOutOfGroup", "mergeLayers",
        "stepBrushSize", "stepBrushHardness", "setBrushSettings", "brushBegin", "brushMove", "brushEnd", "brushCancel", "cloneSetSource", "magicWand",
        "filterBegin", "filterPreview", "filterCommit", "filterCancel", "filterSetPreview",
        "setMaskSelected", "invertMask", "transform", "transformBegin", "transformPreview", "transformCommit", "transformCancel",
        "distortBegin", "distortCommit", "addShape", "warpBegin", "warpMove", "warpEnd", "warpCancel",
        "resizeCanvas", "cropCanvas", "resizeImage", "addAdjustment", "adjustmentBegin", "adjustmentPreview",
        "adjustmentCommit", "adjustmentCancel", "contentFill", "removeBackground", "smartMatte", "selectTool",
    ]

    /// The adjustment as it was when editing began, for cancel.
    private var adjustmentOriginal: (id: UUID, value: LayerAdjustment)?
    /// The manifest of a project being loaded; its layers' images and masks arrive through `installLayerAsset`.
    private var loadingManifest: ProjectManifest?

    /// Synchronous entry for the C ABI (called from the Qt main thread): runs `commandAsync`, pumping the run loop while
    /// upstream's async operations finish. Must not be called from inside a main-actor job (tests use `commandAsync`).
    func command(_ json: Data) -> Int32 {
        awaitOnMain { [self] in await commandAsync(json) }
    }

    func commandAsync(_ json: Data) async -> Int32 {
        guard let command = try? JSONDecoder().decode(Command.self, from: json) else { error = "Invalid command JSON"; return -1 }
        guard command.version == 1 else { error = "Unsupported command version"; return -4 }
        let s = session
        switch command.action {
        case "markSaved": s.history.markSaved()
        case "syncToolSettings":
            s.refreshGradient()
            if s.tool == .crop { s.changeCropRatio() }
        case "setCropRect":
            guard let point = point(command), let width = command.width, let height = command.height else { return fail(-1, "crop rectangle required") }
            let rect = CGRect(x:point.x,y:point.y,width:CGFloat(width),height:CGFloat(height))
            guard CropGeometry.valid(rect) else { return fail(-1, "invalid crop rectangle") }
            s.cropRect = rect
            s.changeCropRatio()
        case "saveShortcuts":
            guard let values = command.shortcuts else { return fail(-1, "shortcuts required") }
            guard Set(values.keys).isSubset(of: Set(ShortcutDefinition.all.map(\.id))) else { return fail(-1, "unknown shortcut") }
            if let problem = ShortcutSettings.problem(in: values) { return fail(-1, problem) }
            guard ShortcutSettings.shared.save(values) else { return fail(-5, "Shortcuts could not be saved") }
        case "selectLayers":
            guard let ids = command.layerIDs, !ids.isEmpty,
                  Set(ids).isSubset(of: Set(s.document?.layers.map(\.id) ?? [])) else { return fail(-1, "invalid layers") }
            s.selectLayers(Set(ids), primary: command.layerID)
        case "placeLayers", "validateLayerDrop":
            guard let requested = command.layerIDs, !requested.isEmpty, let document = s.document else { return fail(-1, "layers required") }
            let selected = Set(requested)
            guard selected.count == requested.count, selected.isSubset(of: Set(document.layers.map(\.id))) else { return fail(-1, "invalid layers") }
            // Selected folders carry descendants. A drop is validated as a whole before opening an undo transaction.
            let children = selected.reduce(into: Set<UUID>()) { $0.formUnion(s.descendantIDs(of: $1)) }
            let roots = selected.subtracting(children)
            let ids = LayerHierarchy.entries(document.layers.map(\.hierarchyRecord), topFirst: true).map { $0.layer.id }.filter(roots.contains)
            guard ids.allSatisfy({ s.canPlaceLayer($0, in: command.parentID) }),
                  command.targetID.map({ target in !selected.contains(target) && !children.contains(target)
                    && document.layers.contains { $0.id == target && $0.parentID == command.parentID } }) ?? true
            else { return fail(-1, "Cannot place a folder inside itself or its descendants") }
            // Validate depth for the whole move too; prevents partial changes at the hierarchy limit.
            var proposed = document.layers
            for index in proposed.indices where roots.contains(proposed[index].id) { proposed[index].parentID = command.parentID }
            guard (try? LayerHierarchy.validate(proposed.map(\.hierarchyRecord))) != nil else { return fail(-1, "Invalid layer hierarchy") }
            if command.enabled == true, document.layers.count + roots.count + children.count > 10_000 {
                return fail(-1, "Too many layers to duplicate")
            }
            if command.action == "placeLayers" {
                let copying = command.enabled == true
                s.beginEdit(copying ? "Duplicate Layers" : "Move Layers")
                let order = command.targetID == nil && command.forward != true ? Array(ids.reversed()) : ids
                for id in order {
                    if copying { _ = s.duplicateLayer(id, in: command.parentID, above: command.targetID, atBottom: command.forward ?? false) }
                    else { _ = s.placeLayer(id, in: command.parentID, above: command.targetID, atBottom: command.forward ?? false) }
                }
                if !copying { s.selectLayers(roots, primary: ids.first) }
                s.endEdit()
            }
        case "setPaletteColor":
            let p = command.parameters ?? [:]
            guard let r = p["red"], let g = p["green"], let b = p["blue"], [r,g,b].allSatisfy({ $0.isFinite && (0...1).contains($0) }) else { return fail(-1, "invalid color") }
            s.setPaletteColor(PaletteColor(red:r, green:g, blue:b), background:command.enabled ?? false)
        case "swapPalette": s.swapPaletteColors()
        case "resetPalette": s.resetPaletteColors()
        case "openColorPicker": s.openColorPicker(background: command.enabled ?? false)
        case "finishColorPicker":
            guard let picker = s.colorPicker else { return fail(-1, "no color picker") }
            if command.enabled == true {
                let p = command.parameters ?? [:]
                guard let r = p["red"], let g = p["green"], let b = p["blue"], [r,g,b].allSatisfy({ $0.isFinite && (0...1).contains($0) }) else { return fail(-1, "invalid color") }
                picker.hsb.setRGB(PaletteColor(red:r, green:g, blue:b))
            }
            s.closeColorPicker(commit: command.enabled ?? false)
        case "cycleToolMode": s.cycleToolMode()
        case "toolPointerBegin":
            guard let point = point(command) else { return fail(-1, "invalid point") }
            switch s.tool {
            case .shape: s.beginShape(at: point)
            case .gradient: s.beginGradient(at: point)
            case .blur: s.beginBrush(at: point)
            case .type: s.beginText(at: point)
            default: return fail(-1, "unsupported pointer tool")
            }
        case "toolPointerMove":
            guard let point = point(command) else { return fail(-1, "invalid point") }
            switch s.tool {
            case .shape: s.dragShape(to: point, square: command.enabled ?? false, fromCenter: command.forward ?? false)
            case .gradient: s.moveGradient(end: point)
            case .blur: s.continueBrush(at: point)
            default: return fail(-1, "unsupported pointer tool")
            }
        case "toolPointerEnd":
            switch s.tool {
            case .shape: s.finishShape()
            case .gradient: s.endGradientDrag()
            case .blur: await s.finishBrush()
            default: break
            }
        case "toolCancel":
            s.cancelShape(); s.cancelGradient(); s.cancelBrush(); s.cancelText(); s.cancelCrop()
        case "toolApply":
            await s.commitGradient()
            if s.tool == .crop { await s.commitCrop() }
            guard s.finishText() else { return fail(-5, "Text could not be rendered") }
        case "setTextContent":
            guard let content = command.name, content.utf16.count <= 100_000, s.textDraft != nil else { return fail(-1, "invalid text") }
            s.changeTextStyle { $0.content = content }
        case "new":
            guard let width = command.width, let height = command.height, (1...30_000).contains(width), (1...30_000).contains(height)
            else { return fail(-1, "invalid size") }
            s.createDocument(width: width, height: height, emptyLayer: true)
        case "addLayer":
            guard s.document != nil else { return fail(-2, "no document") }
            s.addBlankLayer()
        case "addGroup": s.addGroup()
        case "groupSelectedLayers": s.groupSelectedLayers()
        case "selectLayer":
            guard let id = command.layerID else { return fail(-1, "layerID required") }
            guard s.document?.layers.contains(where: { $0.id == id }) == true else { return fail(-5, "no such layer") }
            s.selectLayer(id)
        case "deleteLayer":
            guard s.activeLayerID != nil else { return fail(-5, "no layer") }
            s.deleteActiveLayer()
        case "renameLayer":
            guard let name = command.name, let id = s.activeLayerID else { return fail(-1, "name required") }
            s.renameLayer(id, to: name)
        case "setVisible":
            guard let enabled = command.enabled, let layer = s.activeLayer else { return fail(-1, "enabled required") }
            if layer.isVisible != enabled { s.toggleLayerVisibility(layer.id) }
        case "setOpacity":
            guard let value = command.value, value.isFinite, s.activeLayer != nil else { return fail(-1, "value required") }
            s.setLayerOpacity(min(1, max(0, value)))
        case "setBlendMode":
            guard let name = command.kind, let mode = LayerBlendMode(rawValue: name) else { return fail(-1, "unknown blend mode") }
            s.setLayerBlendMode(mode)
        case "setSelectedOpacity":
            guard let value = command.value else { return fail(-1, "value required") }
            s.setSelectedLayersOpacity(value)
        case "cycleBlendMode": s.cycleBlendMode(forward: command.forward ?? true)
        case "flipLayer": s.flipLayers(horizontally: command.horizontally ?? true)
        case "flipCanvas": s.flipCanvas(horizontally: command.horizontally ?? true)
        case "undo": s.undo()
        case "redo": s.redo()
        case "addRevealMask", "addHideMask":
            guard s.activeLayer != nil else { return fail(-5, "no layer") }
            s.addLayerMask(revealing: command.action == "addRevealMask")
        case "deleteMask": s.deleteLayerMask()
        case "setMaskEnabled":
            guard let enabled = command.enabled, let mask = s.activeLayer?.mask else { return fail(-1, "no mask") }
            if mask.isEnabled != enabled { s.toggleLayerMask() }
        case "setMaskLinked":
            guard let linked = command.enabled, let layer = s.activeLayer, let mask = layer.mask else { return fail(-1, "no mask") }
            if mask.isLinked != linked { s.toggleMaskLink(layer.id) }
        case "moveLayer":
            guard let dx = command.x, let dy = command.y, dx.isFinite, dy.isFinite, s.activeLayer != nil else { return fail(-1, "delta required") }
            s.nudgeLayer(dx: CGFloat(dx), dy: CGFloat(dy))
        case "selectRectangle", "selectEllipse":
            guard s.document != nil else { return fail(-2, "no document") }
            guard let x = command.x, let y = command.y, let width = command.width, let height = command.height,
                  [x, y].allSatisfy({ $0.isFinite && abs($0) <= 1_000_000 }), (0...30_000).contains(width), (0...30_000).contains(height)
            else { return fail(-1, "invalid rectangle") }
            let rect = CGRect(x: x, y: y, width: CGFloat(width), height: CGFloat(height))
            select(command.action == "selectEllipse" ? CGPath(ellipseIn: rect, transform: nil) : CGPath(rect: rect, transform: nil),
                   mode: SelectionMode(rawValue: command.kind ?? "New") ?? .replace)
        case "selectLasso":
            guard s.document != nil else { return fail(-2, "no document") }
            guard let list = command.points, list.count >= 3 else { return fail(-1, "lasso needs 3+ points") }
            let points = list.compactMap { $0.count == 2 && $0[0].isFinite && $0[1].isFinite ? CGPoint(x: $0[0], y: $0[1]) : nil }
            guard points.count == list.count else { return fail(-1, "invalid point") }
            let path = CGMutablePath()
            path.addLines(between: points); path.closeSubpath()
            select(path, mode: SelectionMode(rawValue: command.kind ?? "New") ?? .replace)
        case "selectAll": s.selectAll()
        case "deselect": s.deselect()
        case "invertSelection": s.invertSelection()
        case "loadLayerSelection":
            guard let id = s.activeLayerID else { return fail(-5, "no layer") }
            s.loadLayerSelection(layerID: id)
        case "loadMaskSelection":
            guard let id = s.activeLayerID else { return fail(-5, "no layer") }
            s.loadMaskSelection(layerID: id)
        case "selectSubject":
            await s.selectSubject()
        case "featherSelection":
            s.featherSelection(by: Int(command.parameters?["amount"] ?? 1))
        case "expandSelection": s.expandSelection(by: Int(command.parameters?["amount"] ?? 1))
        case "contractSelection": s.contractSelection(by: Int(command.parameters?["amount"] ?? 1))
        case "fillForeground", "fillBackground":
            let background = command.action == "fillBackground"
            let p = command.parameters ?? [:]
            if p["red"] != nil || p["green"] != nil || p["blue"] != nil {
                s.setPaletteColor(PaletteColor(red: p["red"] ?? 0, green: p["green"] ?? 0, blue: p["blue"] ?? 0), background: background)
            }
            await s.fillSelection(with: background ? .background : .foreground)
        case "clearSelection": await s.clearSelectedPixels()
        case "invert": await s.invertPixels()
        case "copy": s.copySelection()
        case "copyMerged": s.copyMergedSelection()
        case "cut": await s.cutSelection()
        case "paste": s.paste()
        case "duplicateLayer": s.duplicateActiveLayer()
        case "layerViaCopy": s.layerViaCopy()
        case "toggleClippingMask":
            guard let id = s.activeLayerID else { return fail(-5, "no layer") }
            s.toggleClippingMask(id)
        case "moveActiveLayer":
            s.moveActiveLayer(by: Int(command.value ?? 1))
        case "moveActiveLayerOutOfGroup":
            s.moveActiveLayerOutOfGroup()
        case "mergeLayers":
            s.mergeLayers()
        case "cloneSetSource":
            guard let point = point(command) else { return fail(-1, "invalid point") }
            s.setCloneSource(point)
        case "stepBrushSize", "stepBrushHardness":
            guard s.tool.isBrushTool, let increase = command.enabled else { return fail(-1, "brush tool required") }
            if command.action == "stepBrushSize" { s.changeBrushSize(increase: increase) }
            else { s.changeBrushHardness(increase: increase) }
        case "setBrushSettings":
            let p = command.parameters ?? [:]
            guard p.values.allSatisfy({ $0.isFinite }) else { return fail(-1, "invalid brush settings") }
            if let value = p["erasing"] { s.brushMode = value != 0 ? .erase : .paint }
            if let value = p["diameter"] { s.brushSettings.diameter = min(2000, max(1, value)) }
            if let value = p["hardness"] { s.brushSettings.hardness = min(1, max(0, value)) }
            if let value = p["opacity"] { s.brushSettings.opacity = min(1, max(0.01, value)) }
            if let value = p["red"] { s.brushSettings.red = min(1, max(0, value)) }
            if let value = p["green"] { s.brushSettings.green = min(1, max(0, value)) }
            if let value = p["blue"] { s.brushSettings.blue = min(1, max(0, value)) }
        case "brushBegin":
            let p = command.parameters ?? [:]
            guard s.document != nil, let point = point(command) else { return fail(-1, "invalid brush start") }
            let healing = (p["healing"] ?? 0) != 0
            let cloning = command.kind == "Clone"
            if cloning, s.cloneSource == nil { return fail(-5, "no clone source set (send cloneSetSource first)") }
            s.selectTool(healing ? .spotHealing : cloning ? .cloneStamp : .brush)
            if healing, let modeIndex = p["healingMode"], (0..<SpotHealingMode.allCases.count).contains(Int(modeIndex)) {
                s.spotHealingMode = SpotHealingMode.allCases[Int(modeIndex)]
            }
            if cloning { s.cloneSettings.aligned = (p["aligned"] ?? 1) != 0; s.cloneSettings.sampleAllLayers = (p["sampleAllLayers"] ?? 0) != 0 }
            var settings = s.brushSettings
            settings.diameter = p["diameter"] ?? settings.diameter; settings.hardness = p["hardness"] ?? settings.hardness; settings.opacity = p["opacity"] ?? settings.opacity
            settings.red = p["red"] ?? settings.red; settings.green = p["green"] ?? settings.green; settings.blue = p["blue"] ?? settings.blue
            s.brushSettings = settings
            s.brushMode = (p["erasing"] ?? 0) != 0 ? .erase : .paint
            if let mask = p["mask"] { s.isMaskSelected = mask != 0 }
            // A mask stroke paints white (reveal) or black (hide): upstream keeps that choice as the mask palette.
            if s.isMaskSelected { s.maskPaintWhite = (0.2126 * settings.red + 0.7152 * settings.green + 0.0722 * settings.blue) > 0.5 }
            s.beginBrush(at: point)
            guard s.brushStroke != nil else { return fail(-5, cloning ? "clone stamp could not start (out of range?)" : "brush could not start") }
        case "brushMove":
            guard let point = point(command) else { return fail(-1, "invalid point") }
            s.continueBrush(at: point)
        case "brushEnd": await s.finishBrush()
        case "brushCancel": s.cancelBrush()
        case "magicWand":
            guard s.document != nil, let point = point(command) else { return fail(-1, "invalid point") }
            let p = command.parameters ?? [:]
            var settings = s.wandSettings
            settings.tolerance = Int(p["tolerance"] ?? Double(settings.tolerance))
            settings.contiguous = (p["contiguous"] ?? (settings.contiguous ? 1 : 0)) != 0
            settings.sampleAllLayers = (p["sampleAllLayers"] ?? (settings.sampleAllLayers ? 1 : 0)) != 0
            s.wandSettings = settings
            let mode = SelectionMode(rawValue: command.kind ?? "New") ?? .replace
            if s.wandMode == .object { await s.selectObject(at: point, mode: mode) }
            else { await s.magicWand(at: point, mode: mode) }
        case "filterBegin":
            guard let name = command.kind, let kind = FilterKind(rawValue: name) else { return fail(-1, "unknown filter") }
            s.beginFilter(kind)
            guard s.filterEdit != nil else { return fail(-5, "filter could not start") }
            s.updateFilter(filterSettings(command, s.filterSettings), preview: true)
        case "filterPreview":
            guard s.filterEdit != nil else { return fail(-1, "no filter in progress") }
            s.updateFilter(filterSettings(command, s.filterEdit?.settings ?? s.filterSettings), preview: s.filterEdit?.preview ?? true)
        case "filterCommit":
            guard s.filterEdit != nil else { return fail(-1, "no filter in progress") }
            await s.commitFilter()
        case "filterCancel": s.cancelFilter()
        case "filterSetPreview":
            guard let enabled = command.enabled, let edit = s.filterEdit else { return fail(-1, "no filter in progress") }
            edit.preview = enabled
        case "setMaskSelected":
            s.isMaskSelected = command.enabled ?? false
            if s.isMaskSelected, s.activeLayer?.mask?.isLinked == false { s.beginTransform() }
        case "invertMask":
            guard let layer = s.activeLayer, layer.mask != nil else { return fail(-5, "no mask") }
            let was = s.isMaskSelected
            s.isMaskSelected = true
            await s.invertPixels()
            s.isMaskSelected = was
        case "transformBegin": s.beginTransform()
        case "transform":
            guard var transform = s.activeLayer?.transform else { return fail(-5, "no layer") }
            apply(command.parameters ?? [:], to: &transform)
            s.beginTransform()
            s.previewTransform(transform)
            s.commitTransform()
        case "transformPreview":
            guard var draft = s.transformEdit?.draft ?? s.activeLayer?.transform else { return fail(-5, "no layer") }
            apply(command.parameters ?? [:], to: &draft)
            s.previewTransform(draft)
        case "transformCommit": s.commitTransform()
        case "transformCancel": s.cancelTransform()
        case "distortBegin":
            s.beginTransform()
            s.beginDistort()
        case "distortCommit":
            guard let edit = s.transformEdit, let list = command.points, list.count == 4 else { return fail(-1, "four corners required") }
            let corners = list.compactMap { $0.count == 2 && $0[0].isFinite && $0[1].isFinite ? CGPoint(x: $0[0], y: $0[1]) : nil }
            guard corners.count == 4 else { return fail(-1, "invalid corner") }
            s.commitDistort(edit, corners: corners)
        case "addShape":
            guard s.document != nil, let name = command.kind, let kind = ShapeKind(rawValue: name),
                  let width = command.width, let height = command.height, let x = command.x, let y = command.y else { return fail(-1, "invalid shape") }
            let p = command.parameters ?? [:]
            s.setPaletteColor(PaletteColor(red: p["red"] ?? 0, green: p["green"] ?? 0, blue: p["blue"] ?? 0), background: false)
            s.selectTool(.shape)
            s.shapeKind = kind
            s.shapeCornerRadius = p["cornerRadius"] ?? 0
            s.beginShape(at: CGPoint(x: x, y: y))
            s.dragShape(to: CGPoint(x: x + Double(width), y: y + Double(height)), square: false, fromCenter: false)
            s.finishShape()
        case "warpBegin":
            guard let name = command.kind, let mode = BlurToolMode(rawValue: name), let point = point(command) else { return fail(-1, "invalid warp") }
            let p = command.parameters ?? [:]
            var settings = s.brushSettings
            settings.diameter = p["diameter"] ?? settings.diameter; settings.hardness = p["hardness"] ?? settings.hardness; settings.opacity = p["opacity"] ?? settings.opacity
            s.selectTool(.blur)
            s.blurMode = mode
            s.brushSettings = settings
            s.beginWarp(at: point)
            guard s.warpStroke != nil else { return fail(-5, "warp could not start") }
        case "warpMove":
            guard let point = point(command) else { return fail(-1, "invalid point") }
            s.continueBrush(at: point)
        case "warpEnd": s.finishWarp()
        case "warpCancel": s.cancelBrush()
        case "resizeCanvas":
            guard let width = command.width, let height = command.height, let snapshot = s.projectSnapshot() else { return fail(-1, "invalid size") }
            let p = command.parameters ?? [:]
            let anchor = p["anchor"] ?? 4
            guard (0...8).contains(anchor), anchor.rounded() == anchor else { return fail(-1, "invalid anchor") }
            var options = CanvasSizeOptions(width: width, height: height, anchor: Int(anchor))
            if command.enabled == true { options.fill = CanvasExtensionColor(red: p["red"] ?? 0, green: p["green"] ?? 0, blue: p["blue"] ?? 0) }
            guard let resized = try? await CanvasResizer.shared.resize(snapshot, to: options) else { return fail(-5, "canvas resize failed") }
            s.applyDocumentSize(resized, actionName: "Canvas Size")
        case "cropCanvas":
            guard let width = command.width, let height = command.height, let snapshot = s.projectSnapshot() else { return fail(-1, "invalid crop") }
            let options = CanvasSizeOptions(width: width, height: height, contentOffset: CGPoint(x: -(command.x ?? 0), y: -(command.y ?? 0)))
            guard let cropped = try? await CanvasResizer.shared.resize(snapshot, to: options) else { return fail(-5, "crop failed") }
            s.applyDocumentSize(cropped, actionName: "Crop")
        case "resizeImage":
            guard let width = command.width, let height = command.height, let snapshot = s.projectSnapshot() else { return fail(-1, "invalid size") }
            var sampling = LayerSampling.high
            if let kind = command.kind { guard let chosen = LayerSampling(rawValue: kind) else { return fail(-1, "unknown sampling") }; sampling = chosen }
            let options = ImageSizeOptions(width: width, height: height, resolution: command.value ?? s.document?.resolution ?? 72, sampling: sampling)
            guard let resized = try? await ImageResizer.shared.resize(snapshot, to: options) else { return fail(-5, "image resize failed") }
            s.applyImageSize(resized)
        case "addAdjustment":
            guard let name = command.kind, let kind = AdjustmentKind(rawValue: name) else { return fail(-1, "unknown adjustment") }
            s.addAdjustment(kind)
        case "adjustmentBegin":
            guard let id = command.layerID, let value = s.document?.layers.first(where: { $0.id == id })?.adjustment else { return fail(-1, "not an adjustment layer") }
            s.selectLayer(id)
            s.adjustmentEditingID = id
            adjustmentOriginal = (id, value)
        case "adjustmentPreview":
            guard let value = command.adjustment, let id = s.adjustmentEditingID else { return fail(-1, "no adjustment being edited") }
            s.updateAdjustment(id, value: value)
        case "adjustmentCommit":
            guard let value = command.adjustment, let id = s.adjustmentEditingID ?? adjustmentOriginal?.id else { return fail(-1, "no adjustment being edited") }
            s.beginEdit("Edit \(value.kind.rawValue) Adjustment")
            s.updateAdjustment(id, value: value)
            s.endEdit()
            s.adjustmentEditingID = nil; adjustmentOriginal = nil
        case "adjustmentCancel":
            if let original = adjustmentOriginal { s.updateAdjustment(original.id, value: original.value) }
            s.adjustmentEditingID = nil; adjustmentOriginal = nil
        case "contentFill":
            s.beginFilter(.contentAwareFill)
            guard s.filterEdit != nil else { return fail(-5, "content-aware fill needs a selection") }
            await s.commitFilter()
        case "removeBackground", "smartMatte":
            s.beginFilter(.removeBackground)
            guard s.filterEdit != nil else { return fail(-5, "remove background could not start") }
            var settings = s.filterEdit?.settings ?? s.filterSettings
            let p = command.parameters ?? [:]
            settings.refineEdges = p["refineEdges"] ?? settings.refineEdges
            settings.matteContrast = p["matteContrast"] ?? settings.matteContrast
            settings.shiftEdge = p["shiftEdge"] ?? settings.shiftEdge
            s.updateFilter(settings, preview: true)
            await s.commitFilter()
        case "selectTool":
            guard let toolName = command.kind ?? command.name,
                  let tool = NavigationTool(rawValue: toolName)
            else { return fail(-1, "unknown tool") }
            s.selectTool(tool)
            if let mode = command.name {
                if let k = LassoKind(rawValue: mode) {
                    if tool == .marquee { s.marqueeKind = k }
                    if tool == .lasso { s.lassoKind = k }
                }
                if let blur = BlurToolMode(rawValue: mode) { s.blurMode = blur }
                if let w = WandMode(rawValue: mode) { s.wandMode = w }
                if let b = BrushToolMode(rawValue: mode) { s.brushMode = b }
                if let k = ShapeKind(rawValue: mode) { s.shapeKind = k }
            }
        default:
            return fail(-7, "Unsupported by the upstream bridge yet: \(command.action)")
        }
        error = nil
        return 0
    }

    private func fail(_ code: Int32, _ message: String) -> Int32 { error = message; return code }

    private func apply(_ p: [String: Double], to transform: inout LayerTransform) {
        transform.origin.x = p["x"] ?? transform.origin.x
        transform.origin.y = p["y"] ?? transform.origin.y
        transform.size.width = p["width"] ?? transform.size.width
        transform.size.height = p["height"] ?? transform.size.height
        transform.rotation = p["rotation"] ?? transform.rotation
    }

    // MARK: Project persistence (the shell writes the files; the session hands over and takes back layer pixels)

    func exportManifest() throws -> Data {
        guard let snapshot = session.projectSnapshot() else { throw ExportError.render }
        return try JSONEncoder().encode(snapshot.manifest)
    }

    /// Installs the project's structure; layer images and masks then arrive through `installLayerAsset`.
    func importManifest(_ data: Data) -> Int32 {
        guard let manifest = try? JSONDecoder().decode(ProjectManifest.self, from: data), (1...30_000).contains(manifest.width), (1...30_000).contains(manifest.height),
              manifest.width * manifest.height <= 100_000_000, manifest.layers.count <= 10_000,
              (try? LiveMaskGraph.validate(manifest.layers)) != nil else { return fail(-1, "invalid project manifest") }
        session.installProject(ProjectSnapshot(manifest: manifest, images: [:]), from: URL(fileURLWithPath: "/dev/null"))
        session.projectURL = nil
        loadingManifest = manifest
        session.history.reset()
        error = nil
        return 0
    }

    func layerPixels(id: UUID, mask: Bool) -> PortableImage? {
        guard let layer = session.document?.layers.first(where: { $0.id == id }) else { return nil }
        return (mask ? layer.mask?.asset.image : layer.asset?.image)?.portableImage
    }

    func installLayerAsset(_ pixels: PortableImage, id: UUID, mask: Bool) -> Int32 {
        let s = session
        guard var document = s.document, let index = document.layers.firstIndex(where: { $0.id == id }) else { return fail(-5, "no such layer") }
        let image = CGImage(pixels)
        if mask {
            guard let record = loadingManifest?.layers.first(where: { $0.id == id }) ?? nil, record.maskFile != nil,
                  let asset = try? LayerMask.asset(from: image) else { return fail(-1, "invalid mask") }
            document.layers[index].mask = LayerMask(asset: asset, isEnabled: record.maskEnabled ?? true, placement: record.maskPlacement, isLinked: record.maskLinked ?? true)
        } else {
            guard pixels.kind == .rgba, pixels.width > 0, pixels.height > 0, let thumbnail = try? PixelAdjust.thumbnail(of: image) else { return fail(-1, "invalid image") }
            document.layers[index].asset = ImportedImage(image: image, thumbnail: thumbnail, name: document.layers[index].name)
            let record = loadingManifest?.layers.first(where: { $0.id == id }) ?? nil
            document.layers[index].shape = LayerShape.loaded(record?.shape, image: image)
            document.layers[index].text = LayerText.loaded(record?.text, image: image)
        }
        s.document = document
        s.history.reset()
        error = nil
        return 0
    }

    private func point(_ command: Command) -> CGPoint? {
        guard let x = command.x, let y = command.y, x.isFinite, y.isFinite, abs(x) <= 1_000_000, abs(y) <= 1_000_000 else { return nil }
        return CGPoint(x: x, y: y)
    }

    /// The command's parameters laid over the settings the edit already has, as the fork's bridge did.
    private func filterSettings(_ command: Command, _ current: FilterSettings) -> FilterSettings {
        let p = command.parameters ?? [:]
        var settings = current
        settings.radius = p["radius"] ?? settings.radius
        settings.distance = p["distance"] ?? settings.distance
        settings.angle = p["angle"] ?? settings.angle
        settings.amount = p["amount"] ?? settings.amount
        settings.distortion = p["distortion"] ?? settings.distortion
        settings.gaussian = (p["gaussian"] ?? (settings.gaussian ? 1 : 0)) != 0
        settings.monochromatic = (p["monochromatic"] ?? (settings.monochromatic ? 1 : 0)) != 0
        return settings
    }

    private func select(_ path: CGPath, mode: SelectionMode) {
        if mode == .replace { session.setSelection(DocumentSelection(path: path), name: "Select") }
        else { session.applySelection(path, mode: mode, name: "Select") }
    }

    func stateJSON() throws -> Data {
        let s = session
        let active = s.activeLayer
        let state = State(width: s.document?.width ?? 0, height: s.document?.height ?? 0, resolution: s.document?.resolution ?? 72,
            activeLayerID: s.activeLayerID, modified: s.history.isModified, canUndo: s.history.canUndo, canRedo: s.history.canRedo,
            undoName: s.history.undoName, redoName: s.history.redoName,
            busy: s.brushStroke != nil || s.filterEdit != nil || s.transformEdit != nil,
            layers: (s.document?.layers ?? []).map { State.Layer(id: $0.id, name: $0.name, visible: $0.isVisible, opacity: $0.opacity,
                blendMode: $0.blendMode.rawValue, parentID: $0.parentID, isGroup: $0.isGroup, hasMask: $0.mask != nil,
                maskEnabled: $0.mask?.isEnabled, maskLinked: $0.mask?.isLinked, maskPlacement: $0.mask?.placement,
                transform: s.displayedTransform(for: $0)) }, error: error,
            hasSelection: s.selection != nil,
            canTransformSelection: s.canTransformSelection,
            isMaskSelected: s.isMaskSelected,
            activeLayerIsVisible: active?.isVisible ?? true,
            activeLayerIsGroup: active?.isGroup ?? false,
            activeLayerHasMask: active?.mask != nil,
            activeLayerHasParent: active?.parentID != nil,
            activeLayerIsClipped: active?.maskSourceID != nil,
            canToggleClippingMask: s.activeLayerID.map { s.canToggleClippingMask($0) } ?? false,
            canMergeLayers: s.canMergeLayers,
            mergeTitle: s.mergeTitle,
            canMoveActiveLayerUp: s.canMoveActiveLayer(by: 1),
            canMoveActiveLayerDown: s.canMoveActiveLayer(by: -1),
            tool: s.tool.rawValue,
            brush: ["diameter": Double(s.brushSettings.diameter), "hardness": Double(s.brushSettings.hardness),
                    "opacity": Double(s.brushSettings.opacity), "smoothing": Double(s.brushSettings.smoothing),
                    "red": s.brushSettings.red, "green": s.brushSettings.green, "blue": s.brushSettings.blue,
                    "erasing": s.brushMode == .erase ? 1 : 0,
                    "aligned": s.cloneSettings.aligned ? 1 : 0, "sampleAllLayers": s.cloneSettings.sampleAllLayers ? 1 : 0,
                    "healingMode": Double(SpotHealingMode.allCases.firstIndex(of: s.spotHealingMode) ?? 0)],
            selectionControls: ["mode": s.selectionModeChoice.rawValue, "marquee": s.marqueeKind.rawValue,
                                "lasso": s.lassoKind.rawValue],
            selectedLayerIDs: Array(s.selectedLayerIDs),
            palette: ["red": s.paletteColor(background: false).red, "green": s.paletteColor(background: false).green, "blue": s.paletteColor(background: false).blue,
                      "backgroundRed": s.paletteColor(background: true).red, "backgroundGreen": s.paletteColor(background: true).green, "backgroundBlue": s.paletteColor(background: true).blue],
            colorPicker: s.colorPicker.map { ["red": $0.color.red, "green": $0.color.green, "blue": $0.color.blue] },
            shortcuts: ShortcutDefinition.all.map { d in
                let chord = ShortcutSettings.shared.chord(d)
                return State.ShortcutRecord(id:d.id, title:d.title, group:d.group, key:chord.key, modifiers:chord.modifiers,
                                            originalKey:d.original.key, originalModifiers:d.original.modifiers)
            },
            toolSettings: ["shape": s.shapeKind.rawValue, "blur": s.blurMode.rawValue, "wand": s.wandMode.rawValue,
                           "tolerance": String(s.wandSettings.tolerance), "contiguous": s.wandSettings.contiguous ? "1" : "0",
                           "sampleAllLayers": s.wandSettings.sampleAllLayers ? "1" : "0"],
            cropRect: s.cropRect.map { [$0.minX,$0.minY,$0.width,$0.height] },
            textContent: s.textDraft?.style.content)
        return try JSONEncoder().encode(state)
    }

    /// Adds premultiplied RGBA8 pixels as a layer (`replacing`: as a new one-layer document of that size, history cleared).
    func importRGBA(_ pixels: [UInt8], width: Int, height: Int, name: String, replacing: Bool) -> Int32 {
        guard (1...30_000).contains(width), (1...30_000).contains(height), width * height <= 100_000_000,
              pixels.count == width * height * 4, !name.isEmpty else { return fail(-1, "invalid image") }
        // Validate the canonical contract before any C pixel kernel can see it.
        for i in stride(from: 0, to: pixels.count, by: 4) where pixels[i] > pixels[i + 3] || pixels[i + 1] > pixels[i + 3] || pixels[i + 2] > pixels[i + 3] {
            return fail(-1, "pixels are not premultiplied")
        }
        let image = CGImage(PortableImage(width: width, height: height, kind: .rgba, bytesPerRow: width * 4, bytes: pixels))
        guard let thumbnail = try? PixelAdjust.thumbnail(of: image) else { return fail(-5, "thumbnail failed") }
        let asset = ImportedImage(image: image, thumbnail: thumbnail, name: name)
        let s = session
        if replacing || s.document == nil {
            let layer = ImageLayer(asset: asset, origin: .zero)
            s.document = CanvasDocument(width: width, height: height, layers: [layer])
            s.activeLayerID = layer.id
            s.history.reset()
        } else {
            s.insert(asset)
        }
        error = nil
        return 0
    }

    /// The project as the canvas shows it right now: pending transforms applied, live filter/levels/hue-saturation
    /// previews standing in for the layer they preview, and an in-progress brush stroke's touched tiles composited
    /// in (all the way upstream's own canvas draws them: `EditorCanvas.swift` calls this same `BrushStroke.paintSnapshot()`
    /// for its live paint, we just call it from here instead of an NSView draw pass).
    func displayedSnapshot() -> ProjectSnapshot? {
        let s = session
        guard var snapshot = s.projectSnapshot(), let document = s.document else { return nil }
        var images = snapshot.images
        var masks = snapshot.masks
        var manifest = snapshot.manifest
        let stroke = s.brushStroke ?? s.gradientEdit?.raster
        for (index, layer) in document.layers.enumerated() {
            var shown = s.displayedTransform(for: layer)
            let preview = s.filterEdit?.previewImage(for: layer.id) ?? s.levels?.previewImage(for: layer.id)
                ?? s.hueSaturation?.previewImage(for: layer.id)
            if let preview, let asset = layer.asset {
                images[layer.id] = ImportedImage(image: preview, thumbnail: asset.thumbnail, name: asset.name)
            } else if let stroke, stroke.layer.id == layer.id, !stroke.isMask, let painted = try? stroke.paintSnapshot() {
                // A stroke can grow the layer's painted bounds beyond its committed transform (a brush dab
                // outside the current edges); paintSnapshot() reports the new transform for exactly that,
                // the same value BrushCommit.Input.sourceRect/transform would carry at commit time.
                images[layer.id] = painted.asset
                shown = painted.transform
            } else if let stroke, stroke.layer.id == layer.id, stroke.isMask, let painted = try? stroke.paintSnapshot() {
                // Mask strokes are placed in the mask's own pixel grid (maskPlacement), not the layer transform.
                masks[layer.id] = painted.asset
            }
            if shown != layer.transform {
                let record = manifest.layers[index]
                manifest.layers[index] = ProjectLayerRecord(id: record.id, name: record.name, isVisible: record.isVisible, transform: shown,
                    imageFile: record.imageFile, parentID: record.parentID, isGroup: record.isGroup, opacity: record.opacity,
                    blendMode: record.blendMode, maskFile: record.maskFile, maskEnabled: record.maskEnabled, maskSourceID: record.maskSourceID,
                    adjustment: record.adjustment, maskPlacement: record.maskPlacement, maskLinked: record.maskLinked,
                    shape: record.shape, effects: record.effects, text: record.text)
            }
        }
        snapshot = ProjectSnapshot(manifest: manifest, images: images, masks: masks)
        return snapshot
    }

    /// Lets upstream's background work (a filter preview being prepared on a task) finish before a frame is read, by
    /// pumping the main run loop from the shell's plain callback. Bounded, so a stuck preview cannot hang the shell.
    func settle(timeout: TimeInterval = 3) {
        func waiting() -> Bool {
            if let edit = session.filterEdit, edit.preview, edit.preparedPreview == nil, edit.previewError == nil { return true }
            if let edit = session.levels, edit.preview, edit.preparedPreview == nil { return true }
            return false
        }
        let deadline = Date(timeIntervalSinceNow: timeout)
        while waiting(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.005)) }
    }

    /// The composite as premultiplied RGBA8 (document size), from upstream's own exporter.
    func renderRGBA() throws -> (bytes: [UInt8], width: Int, height: Int) {
        settle()
        guard let snapshot = displayedSnapshot() else { throw ExportError.render }
        // The exporter is an actor that never needs the main thread, so this blocks the caller on a plain semaphore:
        // safe from a Qt callback and from a main-actor test alike (pumping the main run loop would deadlock in the latter).
        nonisolated(unsafe) var outcome: Result<ExportRaster, Error>?
        let done = DispatchSemaphore(value: 0)
        nonisolated(unsafe) let captured = snapshot
        Task.detached {
            do { outcome = .success(try await ImageExporter.shared.render(captured)) } catch { outcome = .failure(error) }
            done.signal()
        }
        done.wait()
        guard let raster = outcome else { throw ExportError.render }
        let image = try raster.get().image
        let context = try BrushRaster.context(width: image.width, height: image.height, mask: false)
        BrushRaster.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height), mask: false, context: context)
        guard let data = context.data else { throw ExportError.render }
        return (Array(UnsafeBufferPointer(start: data.assumingMemoryBound(to: UInt8.self), count: image.width * image.height * 4)),
                image.width, image.height)
    }
}
