// OVERRIDE for `Compositor/UI/KeyboardShortcuts.swift`. The real file is blocked at the compiler level, not by
// missing compat coverage: `#selector`/`@objc` anywhere in a file fails type-checking for the *whole* file on
// Linux (Objective-C interop is absent from the compiler's target support entirely — confirmed empirically:
// `swiftc -enable-objc-interop` returns "unknown argument", not a missing-library error), and the offending piece
// (`ShortcutRecorder`, a private `NSViewRepresentable` button using `#selector`) is `private`, so it can't be
// cherry-picked around the way top-level files like `BlendModePicker.swift` could be.
//
// Shared shortcut definitions, conflict rules and persistence are retained here.
// The visible Linux recorder is host/ShortcutEditor.h, using native Qt key events
// and the saveShortcuts bridge command. SwiftUI's private fallback below is not
// used for keyboard capture on Linux.

import SwiftUI
import AppKit

struct ShortcutChord: Codable, Equatable, Hashable {
    var key: String
    var modifiers: Int
    init(_ key: String, _ modifiers: Int = 0) { self.key = key; self.modifiers = modifiers }
    init(_ event: NSEvent) {
        let flags = event.modifierFlags
        modifiers = (flags.contains(.command) ? 1 : 0) | (flags.contains(.option) ? 2 : 0)
            | (flags.contains(.control) ? 4 : 0) | (flags.contains(.shift) ? 8 : 0)
        switch event.keyCode {
        case 51, 117: key = "\u{7f}"
        case 36, 76: key = "\r"
        case 53: key = "\u{1b}"
        case 48: key = "\t"
        case 49: key = " "
        case 123: key = "\u{f702}"
        case 124: key = "\u{f703}"
        case 125: key = "\u{f701}"
        case 126: key = "\u{f700}"
        default:
            let typed = event.charactersIgnoringModifiers?.lowercased() ?? ""
            key = ["{": "[", "}": "]", "+": "=", "_": "-"][typed] ?? typed
        }
    }
    var eventModifiers: EventModifiers {
        var flags: EventModifiers = []
        if modifiers & 1 != 0 { flags.insert(.command) }
        if modifiers & 2 != 0 { flags.insert(.option) }
        if modifiers & 4 != 0 { flags.insert(.control) }
        if modifiers & 8 != 0 { flags.insert(.shift) }
        return flags
    }
    var cocoaModifiers: NSEvent.ModifierFlags {
        var flags: NSEvent.ModifierFlags = []
        if modifiers & 1 != 0 { flags.insert(.command) }
        if modifiers & 2 != 0 { flags.insert(.option) }
        if modifiers & 4 != 0 { flags.insert(.control) }
        if modifiers & 8 != 0 { flags.insert(.shift) }
        return flags
    }
    var label: String {
        let special = ["\u{7f}": "Delete", "\r": "Return", "\u{1b}": "Esc", "\t": "Tab", " ": "Space",
                       "\u{f702}": "←", "\u{f703}": "→", "\u{f701}": "↓", "\u{f700}": "↑"]
        return (modifiers & 4 != 0 ? "Super+" : "") + (modifiers & 2 != 0 ? "Alt+" : "")
            + (modifiers & 8 != 0 ? "Shift+" : "") + (modifiers & 1 != 0 ? "Ctrl+" : "")
            + (special[key] ?? key.uppercased())
    }
    func event(like event: NSEvent) -> NSEvent? {
        let codes: [String: UInt16] = ["\u{7f}": 51, "\r": 36, "\u{1b}": 53, "\t": 48, " ": 49,
                                       "\u{f702}": 123, "\u{f703}": 124, "\u{f701}": 125, "\u{f700}": 126,
                                       "=": 24, "-": 27]
        let shifted = modifiers & 8 != 0 ? (["[": "{", "]": "}", "=": "+", "-": "_"][key] ?? key) : key
        return NSEvent.keyEvent(with: event.type, location: event.locationInWindow, modifierFlags: cocoaModifiers,
            timestamp: event.timestamp, windowNumber: event.windowNumber, context: nil,
            characters: shifted, charactersIgnoringModifiers: shifted, isARepeat: event.isARepeat,
            keyCode: codes[key] ?? 0xffff)
    }
}

struct ShortcutDefinition: Identifiable {
    let title: String
    let group: String
    let original: ShortcutChord
    var id: String { "\(group):\(title)" }
    var isMenu: Bool { group == "Menus" }

    static let all: [ShortcutDefinition] = {
        func entry(_ title: String, _ key: String, _ modifiers: Int = 0, menu: Bool = false) -> ShortcutDefinition {
            .init(title: title, group: menu ? "Menus" : "Canvas & Layers", original: ShortcutChord(key, modifiers))
        }
        var result: [ShortcutDefinition] = [
            entry("Undo", "z", 1, menu: true), entry("Redo", "z", 9, menu: true),
            entry("New Canvas", "n", 1, menu: true), entry("Open Project", "o", 1, menu: true),
            entry("Save", "s", 1, menu: true), entry("Save As", "s", 9, menu: true),
            entry("Export PNG", "e", 9, menu: true), entry("Export JPEG", "s", 11, menu: true),
            entry("Close Project", "w", 1, menu: true), entry("Fit Canvas", "0", 1, menu: true),
            entry("Actual Pixels", "1", 1, menu: true), entry("Zoom In", "=", 1, menu: true),
            entry("Zoom Out", "-", 1, menu: true), entry("Show Transform Controls", "h", 1, menu: true),
            entry("Hide Compositor", "h", 3, menu: true), entry("Cut", "x", 1, menu: true),
            entry("Copy", "c", 1, menu: true), entry("Copy Merged", "c", 9, menu: true),
            entry("Paste", "v", 1, menu: true), entry("Fill with Foreground", "\u{7f}", 2, menu: true),
            entry("Fill with Background", "\u{7f}", 1, menu: true), entry("Content-Aware Fill", "\u{7f}", 8, menu: true),
            entry("Select All", "a", 1, menu: true), entry("Deselect", "d", 1, menu: true),
            entry("Inverse Selection", "i", 9, menu: true), entry("Select Subject", "a", 3, menu: true),
            entry("Curves", "m", 1, menu: true), entry("Levels", "l", 1, menu: true),
            entry("Hue/Saturation", "u", 1, menu: true), entry("Invert Pixels / Mask", "i", 1, menu: true),
            entry("Canvas Size", "c", 3, menu: true), entry("Image Size", "i", 3, menu: true),
            entry("Transform Layer / Selection", "t", 1, menu: true), entry("Duplicate / Layer via Copy", "j", 1, menu: true),
            entry("Toggle Clipping Mask", "g", 3, menu: true), entry("Group Layers", "g", 1, menu: true),
            entry("New Blank Layer", "n", 9, menu: true), entry("Move Layer Up", "]", 1, menu: true),
            entry("Move Layer Down", "[", 1, menu: true), entry("Merge Layers", "e", 1, menu: true),
            entry("Show Grid", "'", 1, menu: true), entry("Show Guides", ";", 1, menu: true),
            entry("Show Rulers", "r", 1, menu: true), entry("Snap", ";", 9, menu: true),
            entry("Lock Guides", ";", 3, menu: true)
        ]
        for (title, key) in [("Select tool", "a"), ("Move / Transform tool", "v"), ("Hand tool", "h"),
            ("Zoom tool", "z"), ("Brush tool", "b"), ("Eraser", "e"), ("Spot Healing", "j"),
            ("Clone Stamp", "s"), ("Type tool", "t"), ("Gradient tool", "g"), ("Shape tool", "u"),
            ("Eyedropper tool", "i"), ("Marquee / cycle shape", "m"), ("Magic", "w"),
            ("Lasso / cycle mode", "l"), ("Blur / Smudge / Liquify", "r"), ("Crop tool", "c"),
            ("Swap foreground/background", "x"), ("Reset colors", "d"), ("Cycle tool mode", "\t"),
            ("Temporary Hand tool (hold)", " "), ("Delete selection / layer / effect / lasso point", "\u{7f}"),
            ("Apply current canvas operation", "\r"), ("Cancel current canvas operation", "\u{1b}"),
            ("Decrease brush size", "["), ("Increase brush size", "]")] {
            result.append(entry(title, key))
        }
        result += [entry("Decrease brush hardness", "[", 8), entry("Increase brush hardness", "]", 8),
                   entry("Previous blend mode", "-", 8), entry("Next blend mode", "=", 8),
                   entry("Cycle shape kind", "u", 8)]
        for digit in 0...9 { result.append(entry("Opacity digit \(digit) (type two for exact %)", String(digit))) }
        for (direction, key) in [("Left", "\u{f702}"), ("Right", "\u{f703}"), ("Up", "\u{f700}"), ("Down", "\u{f701}")] {
            result += [entry("Nudge \(direction) 1 px", key), entry("Nudge \(direction) 10 px", key, 8),
                       entry("Move selected pixels \(direction) 1 px", key, 1), entry("Move selected pixels \(direction) 10 px", key, 9)]
        }
        result.append(.init(title: "Finish editing text", group: "Text Editing", original: ShortcutChord("\r", 1)))
        for (title, key) in [("Decrease tracking", "\u{f702}"), ("Increase tracking", "\u{f703}"),
                             ("Decrease leading", "\u{f700}"), ("Increase leading", "\u{f701}")] {
            result.append(.init(title: title, group: "Text Editing", original: ShortcutChord(key, 2)))
            result.append(.init(title: title + " by 10", group: "Text Editing", original: ShortcutChord(key, 10)))
        }
        result.append(entry("Toggle Levels preview", "p", 2))
        return result
    }()
}

@MainActor final class ShortcutSettings {
    static let shared = ShortcutSettings()
    private(set) var overrides: [String: ShortcutChord] = [:]
    private let panel = FloatingPanelController(name: "keyboardShortcuts")
    private static let storageKey = "keyboardShortcuts.v1"
    private init() {
        if let data = UserDefaults.standard.data(forKey: Self.storageKey),
           let saved = try? JSONDecoder().decode([String: ShortcutChord].self, from: data),
           Self.problem(in: saved) == nil { overrides = saved }
    }
    func chord(_ definition: ShortcutDefinition) -> ShortcutChord { overrides[definition.id] ?? definition.original }
    func menu(_ key: KeyEquivalent, modifiers: EventModifiers) -> ShortcutChord {
        let bits = (modifiers.contains(.command) ? 1 : 0) | (modifiers.contains(.option) ? 2 : 0)
            | (modifiers.contains(.control) ? 4 : 0) | (modifiers.contains(.shift) ? 8 : 0)
        let original = ShortcutChord(String(key.character), bits)
        guard let definition = ShortcutDefinition.all.first(where: { $0.isMenu && $0.original == original }) else { return original }
        return chord(definition)
    }
    func native(_ key: KeyEquivalent, modifiers: EventModifiers = []) -> ShortcutChord {
        let bits = (modifiers.contains(.command) ? 1 : 0) | (modifiers.contains(.option) ? 2 : 0)
            | (modifiers.contains(.control) ? 4 : 0) | (modifiers.contains(.shift) ? 8 : 0)
        let original = ShortcutChord(String(key.character), bits)
        guard let definition = ShortcutDefinition.all.first(where: { !$0.isMenu && $0.original == original }) else { return original }
        return chord(definition)
    }
    func show() { panel.show(title: "Keyboard Shortcuts", content: KeyboardShortcutsSheet(settings: self)) }
    func close() { panel.close() }
    @discardableResult func save(_ values: [String: ShortcutChord]) -> Bool {
        guard Self.problem(in: values) == nil, let data = try? JSONEncoder().encode(values) else { return false }
        UserDefaults.standard.set(data, forKey: Self.storageKey)
        guard UserDefaults.standard.synchronize() else { return false }
        overrides = values
        close()
        return true
    }
    static func problem(in values: [String: ShortcutChord]) -> String? {
        var assigned: [ShortcutChord: String] = [:]
        for definition in ShortcutDefinition.all {
            let chord = values[definition.id] ?? definition.original
            guard chord.key.count == 1, (0...15).contains(chord.modifiers) else { return "Choose a single key with optional modifiers." }
            if definition.group == "Text Editing", chord.modifiers & 7 == 0 {
                return "Text-editing shortcuts need Ctrl, Alt, or Super so they do not replace normal typing."
            }
            if [ShortcutChord("q", 1), ShortcutChord(",", 1), ShortcutChord("m", 3)].contains(chord) {
                return "\(chord.label) is reserved by the application."
            }
            if let other = assigned[chord] { return "\(chord.label) is assigned to both \(other) and \(definition.title)." }
            assigned[chord] = definition.title
        }
        return nil
    }
    func canvasEvent(_ event: NSEvent) -> NSEvent? {
        guard !overrides.isEmpty else { return event }
        let input = ShortcutChord(event)
        if let definition = ShortcutDefinition.all.first(where: { $0.group == "Canvas & Layers" && chord($0) == input }) {
            return definition.original == input ? event : definition.original.event(like: event)
        }
        if ShortcutDefinition.all.contains(where: { $0.group != "Text Editing" && $0.original == input && chord($0) != input }) { return nil }
        if input.modifiers == 8 {
            let plain = ShortcutChord(input.key)
            if let definition = ShortcutDefinition.all.first(where: { !$0.isMenu && $0.original.modifiers == 0 && chord($0) == plain }) {
                return ShortcutChord(definition.original.key, 8).event(like: event)
            }
            if ShortcutDefinition.all.contains(where: { !$0.isMenu && $0.original == plain && chord($0) != plain }) { return nil }
        }
        return event
    }
    func textEvent(_ event: NSEvent) -> NSEvent? {
        guard !overrides.isEmpty else { return event }
        let definitions = ShortcutDefinition.all.filter { $0.group == "Text Editing" || $0.original == ShortcutChord("\u{1b}") }
        let input = ShortcutChord(event)
        if let definition = definitions.first(where: { chord($0) == input }) {
            return definition.original == input ? event : definition.original.event(like: event)
        }
        if definitions.contains(where: { $0.original == input && chord($0) != input }) { return nil }
        return event
    }
}

extension View {
    func configuredNativeShortcut(_ key: KeyEquivalent, modifiers: EventModifiers = []) -> some View {
        let chord = ShortcutSettings.shared.native(key, modifiers: modifiers)
        guard let first = chord.key.first else { return keyboardShortcut(key, modifiers: modifiers) }
        return keyboardShortcut(KeyEquivalent(first), modifiers: chord.eventModifiers)
    }
    func configuredKeyboardShortcut(_ key: KeyEquivalent, modifiers: EventModifiers = .command) -> some View {
        let chord = ShortcutSettings.shared.menu(key, modifiers: modifiers)
        guard let first = chord.key.first else { return keyboardShortcut(key, modifiers: modifiers) }
        return keyboardShortcut(KeyEquivalent(first), modifiers: chord.eventModifiers)
    }
}

private struct KeyboardShortcutsSheet: View {
    let settings: ShortcutSettings
    @State private var draft: [String: ShortcutChord]
    @State private var search = ""
    @State private var recording: String?
    init(settings: ShortcutSettings) { self.settings = settings; _draft = State(initialValue: settings.overrides) }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Click a shortcut, then press its new key combination. Changes apply when you save.")
                .foregroundStyle(.secondary)
            TextField("Search shortcuts", text: $search).textFieldStyle(.roundedBorder)
            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(["Menus", "Canvas & Layers", "Text Editing"], id: \.self) { group in
                        Text(group).font(.headline).padding(.top, 8)
                        ForEach(ShortcutDefinition.all.filter { $0.group == group && (search.isEmpty || $0.title.localizedCaseInsensitiveContains(search)) }) { definition in
                            HStack {
                                Text(definition.title)
                                Spacer()
                                ShortcutRecorder(chord: draft[definition.id] ?? definition.original,
                                    recording: recording == definition.id,
                                    start: { recording = definition.id },
                                    finish: { chord in
                                        if let chord { draft[definition.id] = chord }
                                        recording = nil
                                    })
                                    .frame(width: 150, height: 26)
                            }
                        }
                    }
                    Divider().padding(.vertical, 8)
                    Text("Contextual keys & mouse gestures").font(.headline)
                    Text("Text fields keep standard macOS editing keys. Dialogs share the Apply/Cancel assignments above. Numeric fields use Up/Down, with Shift for larger steps. Standard macOS commands include ⌘Q to quit and ⌃⌘F for full screen. The shortcut editor itself always uses Return to save and Esc to cancel when not recording.")
                    Text("Option temporarily selects the eyedropper in painting tools. Shift constrains shapes/movement or adds to a selection; Option subtracts from selections or draws from center. Command-drag moves selected pixels; Command-Option-drag copies them. Option-drag duplicates layers/folders/effects; Option-click at a layer boundary toggles clipping. Command-click a thumbnail loads its selection. Control bypasses snapping. Right-drag adjusts brush size. Modifier-and-mouse gestures are fixed.")
                }.padding(.trailing, 8)
            }.frame(height: 465)
            if let problem = ShortcutSettings.problem(in: draft) {
                Text(problem)
                    .foregroundStyle(.orange).font(.callout).lineLimit(2)
                    .frame(height: 22, alignment: .topLeading)
            }
            Divider()
            HStack {
                Button("Restore Defaults") { recording = nil; draft = [:] }
                Spacer()
                Button("Cancel") { settings.close() }.keyboardShortcut(.cancelAction)
                Button("Save") { settings.save(draft) }.keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
                    .disabled(recording != nil || ShortcutSettings.problem(in: draft) != nil)
            }
        }.padding(24).frame(width: 660).fixedSize()
    }
}

/// Replacement for the blocked `ShortcutRecorder` (an `NSViewRepresentable` `NSButton` using `#selector`
/// target-action). Shows the current chord and flips to "Press keys…" on click, matching the real control's
/// states. **Honest gap, not swept under the rug**: actually capturing the next keystroke to complete the
/// rebinding isn't wired yet — it needs a key-capturing `NSView` (`keyDown(with:)`, no `#selector` needed, same
/// pattern as `LayerTableView`), but the Qt renderer doesn't host native (`HostedNativeContent`/`"_Native"`) views
/// yet either, so that plumbing wouldn't be visually exercisable this pass regardless. `ShortcutSettings.save`/
/// `.problem` and everything else in this file is real and fully wired.
private struct ShortcutRecorder: View {
    let chord: ShortcutChord
    let recording: Bool
    let start: () -> Void
    let finish: (ShortcutChord?) -> Void

    var body: some View {
        Button {
            start()
        } label: {
            Text(recording ? "Press keys…" : chord.label)
        }
        .buttonStyle(.bordered)
        .accessibilityLabel(recording ? "Press a shortcut" : chord.label)
    }
}
