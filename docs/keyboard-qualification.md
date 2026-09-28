# Keyboard and command search qualification

29 September 2026. This record covers the native Linux keyboard pass; it does
not qualify a public binary release or replace physical desktop acceptance.

## Delivered behaviour

The shared catalog exposes **171 commands**, **101 default bindings** and
**11 canvas/pointer gestures**. Ctrl+K opens the themed search. Names, command
IDs, aliases, categories and effective custom bindings are searchable. Arrows
and Page Up/Down move through results; Enter runs the selected action through
its normal editor guard and Undo path. Escape returns focus to the canvas.
Missing prerequisites, such as a crop selection, are explained without
changing the document. Unbound commands remain executable from search.

Ctrl+Alt+K opens the searchable shortcut editor. Record, Clear, per-command
Default and Restore defaults modify a draft; Apply validates and saves it
atomically. Cancel discards the draft. Explicit existing custom or unbound
choices survive newly introduced default bindings. Super and Linux virtual
terminal chords remain reserved for the desktop.

Tools, brush size/hardness/opacity, layer selection/reordering, ten-pixel
nudges and common photo adjustments have direct bindings. Basic brush and
Brush Studio settings stay synchronized. Nudges retain transforms and make
one Undo step; locked layers remain protected. Levels, Curves, HSL and Colour
Balance open editable drafts rather than applying immediately.

Input fields keep their normal editing keys. Command search can be opened
from a sidebar field. Inline text honours remapped Save and Save As, including
plain function keys: these commit the text first. The shortcut validator
rejects unmodified navigation keys for commands that can intercept text.

Linux's XKB path consumes Shift on punctuation. The actual bindings use
`{`, `}`, `Ctrl++` and `Ctrl+:`; displayed US-layout equivalents remain
familiar. Tests dispatch the symbols Linux delivers, not hypothetical
Shift-plus-base-key events.

The [complete reference](keyboard-shortcuts.md) is generated from the same
catalog as the application. CI checks for drift. It documents differences
from Photoshop, including C cropping an existing rectangle selection, P
opening the vector workspace and F7 toggling the inspector.

## Qualification record

The final functional suite passed **777 tests**: 326 library, 210 UI and 241
integration tests, with no failures and four manual timing benchmarks excluded.
This includes 20 additional tests over the 757-test photo-integrity baseline.
The editing self-test, six-page PNG/PDF/MP4/GIF export journey and all
80 editable template variants passed.

Coverage includes command search typing/navigation/execution, empty and
unavailable results, Undo, modal focus at 800×600, typing in sidebar fields,
remapped brush keys, actual Linux punctuation representations, 18 tool chords,
brush settings and mask-stroke history, transformed/locked-layer nudges,
layer navigation/reordering, Delete/Plus dispatch, four adjustment dialogs,
recorder clear/reset/conflict/cancel, and modified/function-key Save with
inline text. Model tests cover uniqueness, search ranking, reserved keys,
upgrade collisions, persistent unbound choices and failed-save preservation.

The only source change after this suite was wording in two gesture descriptions
to spell out the Move/Auto-select prerequisite; command behaviour and tests
were unchanged. The production build includes that clarification.

## Production native windows

The production executable passed **24 checks on Wayland** and the same
**24 checks through XWayland**, both at an 800×600 logical viewport. The
command search was visually inspected with the system theme; the shortcut
editor was inspected with the light theme. Content, result scrolling and
footer controls fit inside the window. The added native journey dispatches
Ctrl+K, searches New layer, presses Enter and then Ctrl+Z, checking layer
count, Undo depth and restored focus.

Executable SHA-256:
`fed8f6c81cb32994b3bdfa60bf8b83e2513e47fcc73dcacc4068ba6cf8d590c0`.
Native harness SHA-256:
`90c2e0168acd207d48a6699c8e6d97ae9f9c1fb1b441af92f4dc42d91d941d4f`.

These are native GPUI windows with events dispatched in process. They do
not establish physical keyboard delivery by the compositor.

## Published source and installed preview

The runtime is published as
[`26211b5`](https://github.com/Sugata-Software/Omuse/commit/26211b54140d8ebae5322d8a1347f743ef2455ae),
with Git tree `097c61ddeb935ee7b91e3d6fc7270a0919d0a86c`, matching the locally
built source checkpoint. The production executable was installed from the
clean public checkout into the separate Preview installation. Its self-test
and all **24 native checks through the installed launcher** passed, with
reduced motion. The resulting command-search capture was visually inspected.

The installed executable matches the production SHA-256 above. The launcher
receipt hashes the small launcher script separately; it is not the executable
hash. RAW/subject runtime assets were preserved and verified. The complete
previous installation remains the rollback generation, and the older separate
application was verified unchanged. Rollback machinery was qualified in the
[earlier installer pass](install-qualification.md); this pass verified the
retained generation rather than repeating an unchanged rollback implementation.

GitHub's [guide/reference run](https://github.com/Sugata-Software/Omuse/actions/runs/36461980133)
passed. The [Rust workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36461980019)
passed completely, including installer/reference checks, all-target checking,
the Rust suite, motion/recovery qualification and the dependency notice review.
This result was read back from GitHub on 29 September.

## Physical acceptance remaining

A later 29 September [Cua and AI GUI pass](cua-ai-qualification.md) established
foreground input through XWayland and reproduced an assistant focus defect.
The native Omarchy plugin is inactive because the host package/compiler
changed; a new login alone does not resolve that mismatch. The broader
physical-device pass remains open. Continue with a disposable project and verify:

1. Ctrl+K, typing, arrows/Page Up/Down, Enter and Escape; execute New layer,
   then Ctrl+Z, checking layer count and focus.
2. Type in an ordinary sidebar field and inline multiline text without
   switching tools. Check default and remapped Save/Save As, plus Ctrl+Enter
   and Escape, against reopened text and Undo.
3. B/Shift+B, G/Shift+G, J/Shift+J, H, brackets, shifted brackets, digits,
   Ctrl++, Ctrl+Shift+;, Delete, layer navigation and Shift-arrow nudges.
4. Record a new binding, reject a conflict, cancel a recording, clear/reset
   and Apply; restart and verify the effective binding and displayed reference.
5. Repeat on supported keyboard layouts, with IME and mixed-DPI displays.

These are remaining acceptance steps, not a claim that their physical input
paths have passed. Tablet qualification remains deferred.
