// SwiftUIBridge.swift — the C ABI (`compositor_session_render_tree`) the Qt host calls to get a resolved
// `Compositor/UI` panel as JSON, so it can build real Qt widgets generically instead of hand-mirroring each panel.
// Same conventions as SessionABI.swift: opaque session handles, JSON-blob-with-size-query for structured data.

import Foundation
import SwiftUI

struct CompositorToolHeaders: View {
    @Bindable var session: EditorSession

    var body: some View {
        Group {
            if session.tool == .move {
                TransformInspector(session: session).id(session.activeLayerID)
                Divider()
            }
            if session.tool.isBrushTool {
                BrushControls(session: session)
                Divider()
            }
            if session.tool.isSelectionTool {
                LassoControls(session: session)
                Divider()
            }
            if session.tool == .gradient {
                GradientControls(session: session)
                Divider()
            }
            if session.tool == .type {
                TypeControls(session: session)
                Divider()
            }
            if session.tool == .shape {
                ShapeControls(session: session)
                Divider()
            }
            if session.tool == .eyedropper {
                HStack(spacing: 16) {
                    Text("Eyedropper").font(ToolHeaderStyle.titleFont)
                    Toggle("Sample Ring", isOn: $session.showsSampleRing).toggleStyle(.checkbox)
                    Spacer()
                }.padding(.horizontal, 18).toolHeaderBar()
                Divider()
            }
            if session.tool == .hand || session.tool == .zoom {
                NavigationToolHeader(session: session)
                Divider()
            }
            if session.tool == .crop {
                CropControls(session: session)
                Divider()
            }
            if session.tool == .idle {
                HStack(spacing: 16) {
                    Text("Select a tool").font(ToolHeaderStyle.titleFont)
                    Spacer()
                }.padding(.horizontal, 18).toolHeaderBar()
                Divider()
            }
        }
    }
}

struct CompositorToolRail: View {
    @Bindable var session: EditorSession

    var body: some View {
        ScrollView(.vertical) {
            VStack(spacing: 10) {
                ForEach(NavigationTool.allCases.filter { $0 != .idle }, id: \.self) { tool in
                    Button { session.selectTool(tool) } label: {
                        Group {
                            if tool == .gradient { GradientToolIcon().frame(width: 18, height: 18) }
                            else if tool == .cloneStamp { CloneStampToolIcon().frame(width: 18, height: 18) }
                            else if tool == .lasso, session.lassoKind == .polygonal { PolygonalLassoToolIcon().frame(width: 18, height: 18) }
                            else if tool == .wand, session.wandMode == .object { ObjectSelectionToolIcon().frame(width: 18, height: 18) }
                            else { Image(systemName: tool == .marquee && session.marqueeKind == .ellipse ? "circle.dashed" : session.symbol(for: tool)).font(.system(size: 17)) }
                        }
                        .frame(width: 36, height: 36)
                        .background(session.tool == tool ? Color.white.opacity(0.12) : .clear,
                                    in: RoundedRectangle(cornerRadius: 7))
                        .overlay {
                            RoundedRectangle(cornerRadius: 7)
                                .strokeBorder(session.tool == tool ? Color.white.opacity(0.14) : .clear)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain).help(tool.label).accessibilityLabel(tool.label)
                    .foregroundStyle(.primary)
                    .accessibilityAddTraits(session.tool == tool ? .isSelected : [])
                }
                // Qt owns the rail's palette overlay geometry; its buttons call the
                // same shared palette/picker commands as the tool-header swatches.
            }
            .padding(.top, 16).padding(.bottom, 12)
        }
        .scrollIndicators(.hidden)
        .scrollBounceBehavior(.basedOnSize, axes: .vertical)
        .frame(width: 56)
    }
}

struct CompositorStatusBar: View {
    @Bindable var session: EditorSession

    var body: some View {
        HStack(spacing: 16) {
            if let document = session.document {
                Text(session.viewport.zoom, format: .percent.precision(.fractionLength(0...1)))
                    .frame(width: 62, alignment: .leading).accessibilityIdentifier("zoomStatus")
                Text("\(document.width) × \(document.height) px").accessibilityIdentifier("canvasDimensions")
                Text("sRGB · Transparent")
            } else { Text("Ready when you are") }
            Spacer()
            if session.showsBusy {
                ProgressView().controlSize(.mini)
                Text("Working…")
            } else if session.isImporting {
                ProgressView().controlSize(.mini)
                Text("Importing images…")
            } else {
                Text(session.tool == .marquee ? (session.marqueeKind == .ellipse ? "Drag an ellipse · Shift add · Option subtract · Shift again mid-drag circle · Drag inside to move · Delete clears · ⌘D deselect" : "Drag a rectangle · Shift add · Option subtract · Shift again mid-drag square · Drag inside to move · ⌘-drag moves pixels · Delete clears · ⌘D deselect") : session.tool == .wand ? (session.wandMode == .object ? "Click an object to select its outline · Tab for Wand · Shift add · Option subtract · Drag inside to move · ⌘-drag moves pixels · Delete clears · ⌘D deselect" : "Click to select similar colors · Tab for Object · Shift add · Option subtract · Drag inside to move · ⌘-drag moves pixels · Delete clears · ⌘D deselect") : session.tool == .lasso ? (session.lassoKind == .freehand ? "Drag to select · Drag inside to move · Shift add · Option subtract · Delete clears · ⌥⌫/⌘⌫ fill · ⌘D deselect" : "Click corners · Click start, double-click or Enter to close · Delete removes corner · Escape cancel") : session.tool == .brush ? (session.brushMode == .erase ? "Drag to erase" : "Drag to paint") + " · [ ] size · Shift-[ ] hardness · 1–0 opacity · Escape cancel · Space to pan" : session.tool == .blur ? (session.blurMode == .blur ? "Drag to soften" : session.blurMode == .smudge ? "Drag to smudge" : "Drag to push pixels") + " · [ ] size · Shift-[ ] hardness · 1–0 strength · Space to pan" : session.tool == .cloneStamp ? "Option-click to set the source · Drag to clone · [ ] size · Shift-[ ] hardness · 1–0 opacity · Space to pan" : session.tool == .spotHealing ? "Drag over blemishes to heal · [ ] size · Shift-[ ] hardness · Escape cancel · Space to pan" : session.tool == .type ? "Drag a text box · Click text to edit · Drag box handles to resize · ⌘Return finish · Escape cancel" : session.tool == .shape ? "Drag to draw a shape on a new layer · Shift \(session.shapeKind == .line ? "45°" : session.shapeKind == .rectangle ? "square" : "circle") · Option from center · Shift-U or Tab for the next shape · Escape cancel · Space to pan" : session.tool == .gradient ? "Drag to draw · Drag ends to adjust · Shift 45° · 1–0 opacity · Enter apply · Escape cancel" : session.tool == .crop ? "Drag to crop · Enter apply · Escape cancel · Space to pan" : session.tool == .move ? "Drag to move · Handles to resize · Circle to rotate · 1–0 layer opacity · Space to pan" : session.tool == .hand ? "Drag to pan · Pinch to zoom" : session.tool == .idle ? "No tool selected · Press a tool's key to pick one · Space to pan" : "Click to zoom in · Option-click to zoom out · Drag right or left to zoom smoothly · Space to pan")
            }
        }
        .font(.system(size: 11).monospacedDigit()).foregroundStyle(.secondary)
        .padding(.horizontal, 18).frame(height: 30)
        .accessibilityElement(children: .contain)
    }
}

/// Resolves the named panel against `entry`'s session into a wire-ready tree, and refreshes `entry`'s action-handler
/// registry for it so a later Qt-side interaction can dispatch back to the right closure by node id.
@MainActor private func resolvePanel(_ panel: String, entry: Entry) -> RenderNodeWire? {
    let session = entry.editor.session
    let resolved: RenderNode
    switch panel {
    case "ToolHeaders": resolved = ViewResolver.resolve(CompositorToolHeaders(session: session))
    case "ToolRail": resolved = ViewResolver.resolve(CompositorToolRail(session: session))
    case "StatusBar": resolved = ViewResolver.resolve(CompositorStatusBar(session: session))
    case "NavigationToolHeader": resolved = ViewResolver.resolve(NavigationToolHeader(session: session))
    case "TransformInspector": resolved = ViewResolver.resolve(TransformInspector(session: session))
    case "BrushControls": resolved = ViewResolver.resolve(BrushControls(session: session))
    case "CropControls": resolved = ViewResolver.resolve(CropControls(session: session))
    case "LassoControls": resolved = ViewResolver.resolve(LassoControls(session: session))
    case "ShapeControls": resolved = ViewResolver.resolve(ShapeControls(session: session))
    case "GradientControls": resolved = ViewResolver.resolve(GradientControls(session: session))
    case "TypeControls": resolved = ViewResolver.resolve(TypeControls(session: session))
    case "ColorPaletteControls": resolved = ViewResolver.resolve(ColorPaletteControls(session: session))
    case "LevelsSheet": resolved = ViewResolver.resolve(LevelsSheet(session: session))
    case "HueSaturationSheet": resolved = ViewResolver.resolve(HueSaturationSheet(session: session))
    case "FilterSheet": resolved = ViewResolver.resolve(FilterSheet(session: session))
    case "LayersPanel": resolved = ViewResolver.resolve(LayersPanel(session: session))
    case "CurvesControls":
        var settings = CurvesSettings()
        let binding = Binding<CurvesSettings>(get: { settings }, set: { settings = $0 })
        resolved = ViewResolver.resolve(CurvesControls(settings: binding))
    default: return nil
    }
    var node = resolved
    node.assignIDs()
    var handlers: [String: [String: (Any) -> Void]] = [:]
    node.collectHandlers(into: &handlers)
    entry.actionHandlers[panel] = handlers
    return node.wire()
}


@_cdecl("compositor_session_render_tree")
nonisolated public func compositorSessionRenderTree(_ handle: UInt64, _ panel: UnsafePointer<CChar>?,
                                                     _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int64 {
    guard let panel, capacity >= 0 else { return -1 }
    let panelName = String(cString: panel)
    return withEntry(handle) { entry in
        guard let wire = resolvePanel(panelName, entry: entry) else { return -1 }
        guard let data = try? JSONEncoder().encode(wire) else { return -5 }
        if let output, capacity >= data.count { data.copyBytes(to: output, count: data.count) }
        return Int64(data.count)
    }
}

/// Draws a `Canvas` node's `draw` handler into a fresh `width`×`height` RGBA context (via the existing
/// Skia-backed `CGContext`, `Sources/Compat/CoreGraphics/CoreGraphicsCompat`) and returns the raw pixels, so the
/// Qt renderer can blit them without knowing anything about `GraphicsContext`/`Path` itself — the same "one
/// generic bridge, no per-panel Qt code" shape as every other node kind. Same size-query convention: called with
/// `output == nil` an implementation could size-probe, but a Canvas's byte count is always `width*height*4`, known
/// up front, so callers skip straight to the fill call.
@_cdecl("compositor_session_render_swiftui_canvas")
nonisolated public func compositorSessionRenderSwiftUICanvas(_ handle: UInt64, _ panel: UnsafePointer<CChar>?, _ nodeID: UnsafePointer<CChar>?,
                                                              _ width: Int, _ height: Int,
                                                              _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int64 {
    guard let panel, let nodeID, width > 0, height > 0, width * height <= 100_000_000, capacity >= 0 else { return -1 }
    let panelName = String(cString: panel), nodeIDString = String(cString: nodeID)
    return withEntry(handle) { entry in
        guard let draw = entry.actionHandlers[panelName]?[nodeIDString]?["draw"] else { return -1 }
        let cgContext = CGContext(width: width, height: height)
        let box = GraphicsContextBox(context: GraphicsContext(cgContext: cgContext), size: CGSize(width: Double(width), height: Double(height)))
        draw(box)
        let bytes = cgContext.buffer.bytes
        guard capacity >= bytes.count else { return Int64(bytes.count) }
        if let output { bytes.withUnsafeBufferPointer { output.update(from: $0.baseAddress!, count: $0.count) } }
        return Int64(bytes.count)
    }
}

/// Dispatches a Qt-side interaction (a button tap, a slider drag, ...) back to the Swift closure the last
/// `compositor_session_render_tree` call for this `panel` recorded for `nodeID`/`handlerKey`. `payload` is a JSON
/// fragment (`true`, `42.5`, `"text"`, or empty for a no-argument action like a button tap) matching what that
/// control's handler expects (see `Controls.swift`'s `_makeNode` implementations). Applying the change mutates
/// `EditorSession` directly (real `Observation`), so the *next* `compositor_session_render_tree` call already
/// reflects it — there is no separate "commit" step.
@_cdecl("compositor_session_dispatch_swiftui_action")
nonisolated public func compositorSessionDispatchSwiftUIAction(_ handle: UInt64, _ panel: UnsafePointer<CChar>?,
                                                                _ nodeID: UnsafePointer<CChar>?, _ handlerKey: UnsafePointer<CChar>?,
                                                                _ payload: UnsafePointer<UInt8>?, _ payloadCount: Int) -> Int32 {
    guard let panel, let nodeID, let handlerKey, payloadCount >= 0 else { return -1 }
    let panelName = String(cString: panel), nodeIDString = String(cString: nodeID), handlerKeyString = String(cString: handlerKey)
    let payloadData = payload.map { Data(bytes: $0, count: payloadCount) } ?? Data()
    return Int32(withEntry(handle) { entry in
        guard let action = entry.actionHandlers[panelName]?[nodeIDString]?[handlerKeyString] else { return -1 }
        let value: Any = payloadData.isEmpty ? () : ((try? JSONSerialization.jsonObject(with: payloadData, options: [.fragmentsAllowed])) ?? ())
        action(value)
        entry.rendered = nil
        return 0
    })
}
