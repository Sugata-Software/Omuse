# GPUI Linux integration patches

These are the published `gpui-pre` and `gpui-pre-linux` 0.3.6 sources, based on
Zed revision `bcf6582ce3500df93a8a39366640173e6786cea6`. Cargo selects them through
`[patch.crates-io]` in `rust/Cargo.toml`; the rest of the dependency graph stays
locked. Both packages retain their Apache 2.0 license files. Installed license
and provenance copies are in `rust/licenses`.

The local extension binds the compositor's tablet-v2 protocol and exposes
measured pressure, tilt, contact and proximity to GPUI. Events are coalesced at
Wayland frame boundaries. GPUI tablet listeners get first refusal; events left
unhandled fall back to primary-button pointer input for ordinary controls.
Omuse consumes stylus events for Brush, Pencil and Eraser, including masks,
and commits the stroke on release or proximity loss.

Modified upstream files:

- `gpui-pre/src/elements/div.rs`
- `gpui-pre/src/interactive.rs`
- `gpui-pre/src/window.rs`
- `gpui-pre-linux/src/linux/wayland/client.rs`
- `gpui-pre-linux/src/linux/wayland/clipboard.rs`
- `gpui-pre-linux/src/linux/x11/client.rs`
- `gpui-pre-linux/src/linux/x11/clipboard.rs`

The clipboard patch publishes image MIME types and bytes on Wayland, and calls
the existing image writer on X11 instead of publishing empty text. The X11
writer now uses the image's actual format atom. Text/primary selection behavior
is retained. Private in-process clipboard entries continue to roundtrip while
the application owns the clipboard; other applications receive the public PNG.
Clipboard tests and desktop evidence belong in the editing-workflow
qualification record; compilation alone does not establish interoperability.

Two trailing-space-only lines in `gpui-pre/src/_accessibility.rs` documentation
were trimmed for the repository whitespace check; this does not change code.

The app-level tests in `tablet_ui.rs` exercise pressure, proximity-out history,
and ordinary toolbar activation with synthetic events. The Linux client has a
separate pure frame-priority/pressure-normalization unit test. Neither replaces
physical-device qualification: mapping, pressure/tilt calibration, eraser-tip
identity, buttons, device removal and mixed-DPI behavior need a real tablet.
X11 pressure is not implemented by this patch.

When updating GPUI, rebase this small API/backend patch onto the new published
sources and rerun the editor's core/UI/native gates. Do not describe installation
or compilation alone as verified tablet hardware support.
