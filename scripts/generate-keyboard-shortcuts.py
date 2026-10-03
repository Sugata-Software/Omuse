#!/usr/bin/env python3
"""Generate the public keyboard reference from rust/src/shortcuts.rs."""

from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "rust/src/shortcuts.rs"
OUTPUT = ROOT / "docs/keyboard-shortcuts.md"
STRING = r'"((?:\\.|[^"\\])*)"'
COMMAND_RE = re.compile(
    rf"^\s*command!\({STRING}, {STRING}, {STRING}, {STRING}, {STRING}, {STRING}\);$",
    re.MULTILINE,
)
GESTURE_RE = re.compile(r'\(\s*"([^"\\]*)"\s*,\s*"([^"\\]*)"\s*,?\s*\),', re.DOTALL)
CATEGORY_ORDER = [
    "Application",
    "File",
    "Edit",
    "Selection",
    "Layer",
    "Mask",
    "Object",
    "Vector",
    "Image",
    "View",
    "Tools",
    "Adjustments",
    "Workspaces",
]


def decode(value: str) -> str:
    return json.loads(f'"{value}"')


def display_chord(chord: str) -> str:
    if not chord:
        return "Unbound"
    names = {
        "ctrl": "Ctrl",
        "alt": "Alt",
        "shift": "Shift",
        "super": "Super",
        "left": "Left",
        "right": "Right",
        "up": "Up",
        "down": "Down",
        "pageup": "Page Up",
        "pagedown": "Page Down",
        "backspace": "Backspace",
        "{": "Shift+[",
        "}": "Shift+]",
        ":": "Shift+;",
    }
    # A trailing hyphen represents the Minus key (for example, ctrl--).
    minus = chord.endswith("--")
    parts = chord[:-1].split("-") if minus else chord.split("-")
    if minus:
        parts[-1] = "-"
    return "+".join(names.get(part, part.upper() if len(part) == 1 else part.title()) for part in parts)


def escape_cell(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ")


def parse_source() -> tuple[list[tuple[str, ...]], list[tuple[str, str]]]:
    source = SOURCE.read_text(encoding="utf-8")
    commands = [tuple(decode(value) for value in match.groups()) for match in COMMAND_RE.finditer(source)]
    if not commands:
        raise SystemExit(f"No commands found in {SOURCE}")

    ids = [command[0] for command in commands]
    if len(ids) != len(set(ids)):
        duplicates = sorted({item for item in ids if ids.count(item) > 1})
        raise SystemExit(f"Duplicate command IDs: {', '.join(duplicates)}")
    defaults = [command[2] for command in commands if command[2]]
    if len(defaults) != len(set(defaults)):
        duplicates = sorted({item for item in defaults if defaults.count(item) > 1})
        raise SystemExit(f"Duplicate default chords: {', '.join(duplicates)}")

    gesture_block = source.split("pub const GESTURES", 1)[1].split("];", 1)[0]
    gestures = [tuple(decode(value) for value in match.groups()) for match in GESTURE_RE.finditer(gesture_block)]
    return commands, gestures


def render() -> str:
    commands, gestures = parse_source()
    by_category: dict[str, list[tuple[str, ...]]] = defaultdict(list)
    for command in commands:
        by_category[command[3]].append(command)
    bound = sum(bool(command[2]) for command in commands)

    lines = [
        "# Omuse keyboard shortcuts",
        "",
        "This reference is generated from Omuse's command catalog. Press **Ctrl+K** in the editor to search and run any of the "
        f"{len(commands)} commands. {bound} commands have a default shortcut; every unbound command remains searchable and executable.",
        "",
        "Open **Keyboard shortcuts** with **Ctrl+Alt+K** to assign, clear, or reset a binding. Custom bindings are stored per user. "
        "Omuse reserves **Super** for Omarchy and other Linux desktop shortcuts, and rejects Linux virtual-terminal chords such as Ctrl+Alt+F3.",
        "",
        "The command palette has **All**, **Bound**, **Unbound**, and **Gestures** views. Type to filter by command, category, ID, alias, or the current custom shortcut; use Up/Down or Page Up/Page Down to move, Enter to run, and Escape to close. Ctrl+K remains available while typing in sidebar fields; other canvas shortcuts stay suppressed there.",
        "",
        "Choose **Edit shortcut** from command search, or open the shortcut editor directly, then use **Record**, **Clear**, or **Default**. **Apply** validates and saves the complete map atomically; **Cancel** discards the draft. Omuse keeps explicit custom and unbound choices when later releases add defaults.",
        "",
        "Dialogs reserve Escape, Enter, Tab, and Space. Inline text uses **Ctrl+Enter** to commit, **Escape** to cancel, and Enter for a newline. Save and Save As honour your current bindings and commit the text draft first. Search and Save bindings require Ctrl, Alt or a function key to preserve text navigation.",
        "",
        "**Ask Omuse** focuses its prompt when opened from the toolbar, shortcut or command search. In that prompt, **Ctrl+Enter** sends one assistant request; **Enter** inserts a newline. Submission keeps the same connection, local-only and busy checks as the assistant button. Canvas shortcuts remain inactive while typing.",
        "",
        "Shifted punctuation is shown using US-layout key names: Shift+[ produces {, Shift+] produces }, and Ctrl+Shift+; produces Ctrl+:. Linux binds the resulting symbols, including Ctrl++ for zoom. On another layout, use Record to choose comfortable keys.",
        "",
        "The familiar single-key tools and several editing chords are inspired by Adobe's "
        "[Photoshop shortcut guidance](https://helpx.adobe.com/photoshop/desktop/get-started/settings-and-preferences/view-keyboard-shortcuts.html) "
        "and [printable shortcut reference](https://helpx.adobe.com/content/dam/help/en/photoshop/using/default-keyboard-shortcuts/photoshop-keyboard-shortcuts.pdf), "
        "adapted for native Linux and Omuse's actual commands.",
        "",
        "Legacy default changes: Export moved from Ctrl+E to **Ctrl+Alt+Shift+S**, Create workspace moved to **Ctrl+Alt+N**, and Ctrl+E now means Merge down. Tool defaults now include **H** Hand, **J** Spot healing, **P** Vector path workspace, and **Shift+B** Pencil.",
        "",
    ]
    for category in CATEGORY_ORDER:
        entries = by_category.pop(category, [])
        if not entries:
            continue
        lines.extend([f"## {category}", "", "| Command | Default | What it does |", "|---|---:|---|"])
        for _id, label, chord, _category, _keywords, notes in entries:
            lines.append(
                f"| {escape_cell(label)} | {escape_cell(display_chord(chord))} | {escape_cell(notes)} |"
            )
        lines.append("")
    if by_category:
        raise SystemExit(f"CATEGORY_ORDER is missing: {', '.join(sorted(by_category))}")

    lines.extend(["## Canvas and pointer gestures", "", "These temporary gestures are fixed so they remain available while other shortcuts are customized.", "", "| Action | Gesture |", "|---|---:|"])
    for label, gesture in gestures:
        lines.append(f"| {escape_cell(label)} | {escape_cell(gesture)} |")
    lines.extend(
        [
            "",
            "## Maintenance",
            "",
            "Regenerate this file after changing the catalog:",
            "",
            "```sh",
            "python3 scripts/generate-keyboard-shortcuts.py",
            "python3 scripts/generate-keyboard-shortcuts.py --check",
            "```",
            "",
            "The check also rejects duplicate command IDs and duplicate default chords.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if the checked-in reference is stale")
    args = parser.parse_args()
    expected = render()
    if args.check:
        actual = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if actual != expected:
            raise SystemExit(f"{OUTPUT.relative_to(ROOT)} is stale; regenerate it")
        print(f"{OUTPUT.relative_to(ROOT)} is current")
    else:
        OUTPUT.write_text(expected, encoding="utf-8")
        print(f"wrote {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
