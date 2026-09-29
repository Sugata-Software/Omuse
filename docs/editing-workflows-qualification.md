# Editing workflows qualification

30 September 2026 · Omuse 0.3.0 · Arch/Omarchy Linux x86_64.

Public runtime: [`ea187a900c06ecc68c8ea70635f2abb09cf933b2`](https://github.com/Sugata-Software/Omuse/commit/ea187a900c06ecc68c8ea70635f2abb09cf933b2).
Tree: `3513e06720fbc78ded3212380246c991a2e154e2`.
The exact public source passed all **901 application cases** in its complete
[GitHub Rust/installer run](https://github.com/Sugata-Software/Omuse/actions/runs/36594138936):
378 library, 248 UI and 275 integration, with four manual benchmarks excluded.
The normal production build, native desktop checks, foreground editing checks,
installation and complete rollback passed. The compact
[evidence manifest](editing-workflows-receipts.json) records their identities.

Production executable SHA-256:
`c61f4ff4544d73343ad3c68184b152131abd8517741055c27ef611a714d5fa03`.
This is the normal release build without the `ui-test` feature.

## Editing behavior

- Crop is a view-only frame until Apply. Free, Original, 1:1, 4:5, 3:2 and 16:9 presets support orientation swap, corner resize, movement and 1/10-pixel arrow nudges. Enter applies; Escape cancels. Apply changes canvas bounds in one history transaction and retains layer source pixels outside the cropped canvas. Selection bounds seed the frame. The retained crop stays in the same screen location. Integer dimensions may round a ratio by one pixel.
- Keyboard zoom uses fixed 2%-1600% stops around the viewport center. Actual pixels keeps that anchor; Fit recenters. Wheel zoom uses the pointer as its anchor. Invalid or nonfinite zoom values do not contaminate navigation state.
- Whole-layer copying retains selected trees, live objects, masks, advanced sources, transforms and locks in this process. Paste makes fresh identities, remaps internal mask links, preserves selection and inserts above the active root branch in one undo transaction. Selected children retain their local properties; unselected ancestors are not implicitly copied.
- Other applications receive a PNG. A random process-local clipboard identity prevents an unrelated external image from reviving an old editable tree. The editable payload is limited to 256 MiB and its PNG source canvas to 16 MP; missing mask sources and oversized payloads fail before edits.
- Rendered pixel Cut refuses appearances that could omit source pixels, including masks, visibility, opacity, live content and resampled transforms. Whole-layer Cut remains available when allowed. Refused/no-op pixel or mask cuts preserve the previous public and rich clipboard. Plain raster cuts retain off-canvas pixels and support Undo.
- Asynchronous paste rejects stale document/selection/layer/clipboard identity and active crop, gestures or dialogs.
- Wayland and X11 advertise/write image formats correctly. Wayland writes are nonblocking, bounded to 256 KiB or 64 attempts per callback, resume partial writes and terminate safely on failed/zero writes. An idle reader may retain its buffer until it closes; this is not a lifetime timeout.

## Automated validation

- Exact-source GitHub validation completed both jobs successfully. It passed
  901 application cases, editing/Create exports, all 80 editable template
  variants, motion/media acceptance and interrupted collection recovery.
- The same 901 application cases passed locally on `6491a61`. The final source
  changes only two Cut guidance strings and the manual. The local script was
  stopped after the tests to avoid a redundant UI-test build; its normal
  production editing self-test was run separately and passed. The complete
  final-source workflow above supplies the end-to-end CI result.
- After advancing the installer pin, all 25 installer tests, 11 bundle tests
  and 28 release-publication tests passed. The release declaration, keyboard
  reference, project guide, shell syntax and 113 relative documentation paths
  also passed their checks. No new dependency was introduced.

## Production desktop and clipboard

The exact executable passed **24 native checks on Wayland and 24 on
XWayland**, each at an 800×600 logical viewport. The crop controls were captured
and inspected in the system and light themes. The checks cover painting,
Undo/Redo, save/reopen, theme changes, text, adjustments/effects, masks,
retained 16-bit import/export, the photo journey and command-search execution.

Foreground Cua 0.29.1 input on XWayland, verified from screenshots, established:

- C → 1:1 → Enter changed a synthetic 64×80 document to 64×64; one Undo
  restored the original canvas.
- Group → Copy → Paste retained both groups and their children; one Undo
  restored the original tree.
- The public PNG decoded to the exact source RGBA pixels. An externally owned
  copy of those same bytes pasted as an ordinary pixel layer, without reviving
  the earlier editable tree.
- Pixel Cut at 90% opacity was refused with useful guidance, leaving the
  artwork and the previous PNG clipboard bytes unchanged.

Native Wayland clipboard publication was checked separately using a compositor
key dispatch to the exact owned window. `wl-paste` received `image/png` with
exact 64×80 source pixels. Its PNG SHA-256, also observed through XWayland, was
`c92437abe147ea5110f8c5a2f4c1bbb518b55666f095df1ab1f11add0c5089e3`.
This is not native Cua input qualification: the Omarchy driver still has its
recorded package/compiler compatibility mismatch. Only owned synthetic test
windows were closed; personal artwork and provider profiles were preserved.

## Installed application and rollback

The normal **Omuse** launcher selects generation `install-o_h4lt_d`, with a clean
source receipt naming `ea187a9` and the exact production executable above.
`omuse --version` returns **0.3.0**. The previous complete generation,
`install-ac5gtmjb` (0.2.1), remains available.

All **19 payload file hashes** matched through rollback to 0.2.1 and the reverse
switch to 0.3.0. Both versions returned the expected identity, and the desktop
entry passed validation. The installed launcher separately passed all **24
native Wayland checks** at 800×600 with reduced motion; its crop capture was
inspected. The harness hashes the launcher script, while the production payload
was hashed independently. See the [installation record](main-install-qualification.md).

## Boundaries

No live provider operation was sent for this editing pass. Earlier AI receipts
retain their original runtime identities. These synthetic journeys do not
establish arbitrary photographic quality. Physical mixed-DPI/non-US input,
broader photographic/PSD corpora, allocator exhaustion, portable downloadable
packaging and clean-target installation remain separate release gates.
