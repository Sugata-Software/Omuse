// The JSON-codable mirror of `RenderNode`/`RenderModifier` that crosses the C ABI to the Qt host (see
// Sources/LinuxBridge/SwiftUIBridge.swift). `RenderNode` itself carries live Swift closures (`handlers`, and a few
// `RenderModifier` cases), which aren't `Codable`; this DTO drops them and keeps everything else, keyed by the
// same stable `id` so a later phase can dispatch a Qt-side action back to the right node without needing the
// closures to cross the wire too.

public struct RenderNodeWire: Codable {
    public var id: String
    public var kind: String
    public var stringParams: [String: String]
    public var doubleParams: [String: Double]
    public var boolParams: [String: Bool]
    /// Which `handlers` keys this node's live `RenderNode` had (the closures themselves don't cross the wire —
    /// they stay behind in `Entry.actionHandlers`, see SwiftUIBridge.swift). Tells the Qt renderer which dispatch
    /// key to use, since e.g. `TextField` carries either `"text"` or `"value"` depending on which upstream
    /// initializer built it.
    public var handlerKeys: [String]
    public var modifiers: [RenderModifierWire]
    public var children: [RenderNodeWire]
}

public struct RenderModifierWire: Codable {
    public var kind: String
    public var stringParams: [String: String] = [:]
    public var doubleParams: [String: Double] = [:]
    public var boolParams: [String: Bool] = [:]
}

extension RenderNode {
    /// Converts this (already `assignIDs()`-ed) node and its descendants to the wire format. Modifier cases that
    /// carry a closure (`.onAppear`, `.onSubmit`, `.onExitCommand`, `.sink`) or a nested node (`.overlay`, not yet
    /// needed by any wired-in panel) are dropped — see the plan's note on this phase's scope.
    public func wire() -> RenderNodeWire {
        RenderNodeWire(id: id, kind: kind, stringParams: stringParams, doubleParams: doubleParams, boolParams: boolParams,
                        handlerKeys: handlers.keys.sorted(), modifiers: modifiers.compactMap { $0.wire() },
                        children: children.map { $0.wire() })
    }
}

extension RenderModifier {
    func wire() -> RenderModifierWire? {
        switch self {
        case let .frame(width, height, minWidth, minHeight, maxWidth, maxHeight, alignment):
            var d: [String: Double] = [:]
            if let width { d["width"] = width }
            if let height { d["height"] = height }
            if let minWidth { d["minWidth"] = minWidth }
            if let minHeight { d["minHeight"] = minHeight }
            if let maxWidth { d["maxWidth"] = maxWidth }
            if let maxHeight { d["maxHeight"] = maxHeight }
            return RenderModifierWire(kind: "frame", stringParams: ["alignment": alignment], doubleParams: d)
        case let .padding(top, leading, bottom, trailing):
            return RenderModifierWire(kind: "padding", doubleParams: ["top": top, "leading": leading, "bottom": bottom, "trailing": trailing])
        case let .font(name): return RenderModifierWire(kind: "font", stringParams: ["name": name])
        case let .foregroundStyle(name): return RenderModifierWire(kind: "foregroundStyle", stringParams: ["name": name])
        case let .background(name): return RenderModifierWire(kind: "background", stringParams: ["name": name])
        case let .opacity(v): return RenderModifierWire(kind: "opacity", doubleParams: ["value": v])
        case let .disabled(v): return RenderModifierWire(kind: "disabled", boolParams: ["value": v])
        case let .fixedSize(h, v): return RenderModifierWire(kind: "fixedSize", boolParams: ["horizontal": h, "vertical": v])
        case let .buttonStyle(name): return RenderModifierWire(kind: "buttonStyle", stringParams: ["name": name])
        case let .toggleStyle(name): return RenderModifierWire(kind: "toggleStyle", stringParams: ["name": name])
        case let .controlSize(name): return RenderModifierWire(kind: "controlSize", stringParams: ["name": name])
        case let .textFieldStyle(name): return RenderModifierWire(kind: "textFieldStyle", stringParams: ["name": name])
        case let .multilineTextAlignment(name): return RenderModifierWire(kind: "multilineTextAlignment", stringParams: ["name": name])
        case let .contentShape(name): return RenderModifierWire(kind: "contentShape", stringParams: ["name": name])
        case let .clipShape(name): return RenderModifierWire(kind: "clipShape", stringParams: ["name": name])
        case let .offset(x, y): return RenderModifierWire(kind: "offset", doubleParams: ["x": x, "y": y])
        case let .cornerRadius(r): return RenderModifierWire(kind: "cornerRadius", doubleParams: ["radius": r])
        case let .shadow(r): return RenderModifierWire(kind: "shadow", doubleParams: ["radius": r])
        case let .scaleEffect(s): return RenderModifierWire(kind: "scaleEffect", doubleParams: ["scale": s])
        case let .rotationEffect(r): return RenderModifierWire(kind: "rotationEffect", doubleParams: ["radians": r])
        case let .help(text): return RenderModifierWire(kind: "help", stringParams: ["text": text])
        case let .accessibilityLabel(text): return RenderModifierWire(kind: "accessibilityLabel", stringParams: ["text": text])
        case let .accessibilityIdentifier(text): return RenderModifierWire(kind: "accessibilityIdentifier", stringParams: ["text": text])
        case let .identifier(text): return RenderModifierWire(kind: "identifier", stringParams: ["text": text])
        case let .scrollIndicators(name): return RenderModifierWire(kind: "scrollIndicators", stringParams: ["name": name])
        case let .keyboardShortcut(key, modifiers):
            return RenderModifierWire(kind: "keyboardShortcut", stringParams: ["key": key], doubleParams: ["modifiers": Double(modifiers)])
        case let .tag(text): return RenderModifierWire(kind: "tag", stringParams: ["text": text])
        case .overlay, .onAppear, .onSubmit, .onExitCommand, .sink: return nil
        }
    }
}
