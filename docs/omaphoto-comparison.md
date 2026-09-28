# OmaPhoto comparison and improvement plan

Reviewed 29 September 2026 against [OmaPhoto v1.2.3](https://github.com/ZacharyZhang-NY/OmaPhoto/releases/tag/v1.2.3),
published 28 September. The reference source is the exact tag commit
[`96ce546508a920e10f2747ad3907da6eb87f4700`](https://github.com/ZacharyZhang-NY/OmaPhoto/tree/96ce546508a920e10f2747ad3907da6eb87f4700).
Release notes and tests in that source establish their intended behavior; we
have not run their application on this host. This comparison does not establish
that either application is faster, more reliable or visually superior.

Omuse's strengths are its native Omarchy integration, searchable and executable
command reference, editable content collections, templates and reversible
in-app AI workflows. OmaPhoto has a more complete Camera Raw interaction surface
and a substantially broader downloadable Linux release. Both matter to a
credible public editor.

## Editing and interaction

| Area in OmaPhoto v1.2.3 | Omuse implementation and evidence | Remaining work |
| --- | --- | --- |
| LibRaw import and development | Retained original RAW bytes and 16-bit source; two-camera, same-LibRaw reference comparisons in [photo qualification](photo-release-qualification.md) | Broader camera/profile corpus; this is not independent colour-science certification |
| Camera Raw's nine sections | Light/Color, Effects, Curve, Mixer, Grading, Detail, Optics, Geometry and Calibration; preserved-C pixel references in `rust/tests/camera_reference.rs` | More graphical controls and targeted canvas gestures; the compatibility operation is still 8-bit and capped at 16 MP |
| Histogram, vectorscope and clipping | This pass adds RGB histogram and hue/saturation vectorscope to the existing clipping preview. Analysis excludes fully transparent pixels, weights alpha and samples at most 512×512 cells | The full suite and light/dark native inspection passed; scopes describe the selected layer's sampled display RGB, not sensor RAW/HDR |
| White-balance/defringe eyedroppers and targeted adjustments | Existing point-colour sampling, geometry-guide dragging and channel curve graph | White-balance/defringe sampling, targeted curve and HSL/mixer drags are still missing |
| Interactive Camera Raw preview | Temporary editor, no artwork/history commit until Apply; this pass moves the temporary selection-aware composite off the UI thread and rejects stale analysis/preview results | Still computes the grade at full source resolution. Add scale-aware bounded previews and compare spatial detail/glow against a downsampled full-resolution oracle |
| Layered PSD | Groups, opacity/fill, masks, clipping and supported adjustment/blend mapping; this pass adds always-on synthetic nested/masked/clipped fixtures with exact pixels | Real Adobe/Affinity/OmaPhoto corpus, layered compressed-channel combinations, unsupported constructs and conversion-report review |
| Trim | Transparent, top-left and bottom-right colour trim, per-edge options; `rust/tests/layer_canvas_parity.rs` | Real-photo/manual acceptance of edge tolerance |
| Object/Subject selection and morphology | Local U²-Net path, refine, feather, expand/contract; selection-aware transactions | Independent people/product/hair/fur quality corpus; release feature names do not establish segmentation quality |
| New adjustment layers | Black & White, Color Balance, Invert, Gaussian Blur, Motion Blur and Add Noise are represented in the engine/UI | Broader layered interchange and visual references |
| Inner/Outer Glow and added blend modes | Existing glow kernels and blend implementations; this pass adds exact PSD import/render checks for Vivid Light, Linear Light, Pin Light and Hard Mix | Multiscale preview and translucent/grouped cross-app references |
| Duplicate folders and group opacity | Descendants preserved by grouped duplication; group rendering/opacity tests. This pass also fixes selected-group pointer moves with linked-mask and one-step Undo preservation | Group resize/rotation, large deeply nested editing and interchange corpus |
| Brush smoothing | Screen-space pulled-string Brush smoothing; this pass extends it to Eraser, including masks and release catch-up | Pointer/tablet quality remains separate; Pencil stays unsmoothed for precise work |
| Middle-button pan | Added in this pass with independent primary/middle release state and lost-release cleanup | Native foreground acceptance across mice/trackpads and window edges |
| Auto Select | Frontmost visible nested hit; this pass fixes an already-selected background move box intercepting a higher layer | Bounding-box hit testing, as in the reviewed OmaPhoto source; not per-pixel alpha picking |
| Remember guides/grid/snap/Auto Select | Atomic view preferences already exist; this pass fixes adding a guide while hidden failing to persist auto-show | Real-file preference serialization is tested; actual UI restart and broader profile migration acceptance |
| Shortcut reference | Ctrl+K searches and executes commands; Ctrl+Alt+K records/remaps shortcuts, with conflict handling and generated gesture documentation | Non-US layouts, IME and physical acceptance; no claim of universal shortcut delivery |
| Lazy, fixed-width font picker | This pass loads and caches installed font names on first text-dialog use; dialog width and visible results were already bounded | Large/missing/unusual font collections and long-name visual acceptance |
| Version-9 project format | `.comp` versions 1–9 are accepted and saved as 9, with bounded validation and roundtrip tests | Bidirectional files saved by the actual OmaPhoto v1.2.3 application; a shared version number is not interchange certification |
| Low-memory export failure | Validation, bounded document admission, staged output and cancellation protect existing destinations | OmaPhoto has allocation-starvation tests. Omuse has not qualified allocator exhaustion; Rust OOM can terminate the process. Add a controlled low-memory harness and recoverable admission/isolation before claiming parity |

The relevant upstream tests include
[`CameraRawCanvasTests.cpp`](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/96ce546508a920e10f2747ad3907da6eb87f4700/tests/CameraRawCanvasTests.cpp),
[`ProjectExportFailureTests.cpp`](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/96ce546508a920e10f2747ad3907da6eb87f4700/tests/ProjectExportFailureTests.cpp)
and the tagged source's brush, canvas-navigation and project-manifest tests.
Omuse's corresponding code lives in `rust/src/camera_raw.rs`,
`camera_canvas.rs`, `photo_scopes.rs`, `camera_scopes.rs`, `editor.rs`,
`psd.rs`, `raster.rs`, `document.rs`, `preferences.rs` and `ui.rs`.

## Distribution

OmaPhoto's release publishes Ubuntu 24.04/26.04 DEBs, a Fedora RPM, an Arch
package and checksums. Its source also provides a Nix flake. The Arch artifact
is about 174 MB; that download size alone says nothing about startup speed,
installed footprint or runtime memory.

Omuse has a one-command Arch/Omarchy source installer, complete-generation
update/rollback, and checksum bundle tooling. It does **not** yet have a
qualified downloadable native package. The two unresolved locked dependency
legal texts, clean-target installation and physical desktop acceptance remain
explicit [release gates](public-release-readiness.md).

Start with a qualified Arch package and AppStream metadata after the legal gate
is resolved. Build DEB/RPM/Nix support only with tests on each declared target;
an Arch-built dynamic executable is not automatically portable.

## Where Omuse offers a broader workflow

These are implemented Omuse capabilities, not claims that OmaPhoto can never
support them:

- Editable multipage content collections, reusable components, brand resources,
  data-driven variants, 20 templates / 80 preset-size variants and social-safe
  composition guides.
- PNG/JPEG/WebP/TIFF, PDF and bounded MP4/GIF content export, plus motion presets.
- Subscription-aware in-app assistant connections with reviewed changes, Keep
  and Undo. Live qualification is operation/provider specific; the current
  bounded Codex receipts do not qualify every provider or image operation.
- Native `gpui-omarchy` theming and a searchable action palette that also
  explains current custom bindings and gestures.
- Region-based painting history and retained 16-bit sources with editable
  operations. Previous measured improvements compare Omuse with earlier Omuse
  candidates, not with OmaPhoto.

See the [project guide](project-guide.md), [Create qualification](omuse-create-qualification.md),
[AI GUI qualification](cua-ai-qualification.md), and
[performance roadmap](rust-performance-roadmap.md) for the scope of the evidence.

## What would establish “as good or better”

1. Complete the missing Camera Raw gestures and bounded preview work; retain
   exact Apply/Undo output and prove stale/cancel behavior under repeated edits.
2. Run the same RAW, PSD and `.comp` fixtures through both exact builds. Keep
   source hashes, import reports and rendered references. Include damaged,
   oversized, transparent, transformed, nested, masked and clipped documents.
3. Measure cold/warm launch, 12/24/48 MP first and sustained strokes, zoom,
   selected adjustments, save/recovery, preview/export latency and peak RSS on
   the same machine. Alternate run order; retain at least five repetitions,
   medians and tail latency. Separate engine timing from physical input latency.
4. Exercise cancellation, forced failure and controlled memory pressure while
   preserving existing project/export files. No crash/data-loss superiority
   claim until both builds have run the same workload.
5. Qualify an actual install, upgrade, rollback and uninstall on a clean
   supported Arch/Omarchy target, with complete redistribution terms and hashes.
6. Review real photo-editing and content-creation journeys at 800×600 and common
   desktop/DPI configurations, with foreground keyboard and pointer input.

Calendars, scheduling, social publishing and tablet qualification are outside
this comparison's current product scope.

## Tracking and this candidate

A daily release check watches the official OmaPhoto releases from this pinned
baseline, compares meaningful changes against actual Omuse code/tests, and
updates this document and the visual guide. Upstream content is reference data;
the check does not run upstream installers, publish commits or replace the
installed application automatically.

The completed candidate is public runtime [`9c99e50`](https://github.com/Sugata-Software/Omuse/commit/9c99e50684afd0854c8094a139b43205a263c33d),
matching canonical source `fd4b47c` by tree
`e8fc449465c89902ecc7e9475bd64a8d3dd91113`.

- **808 tests passed:** 336 library, 227 UI and 245 integration; four manual
  timing benchmarks were excluded. This adds 19 tests to the earlier runtime.
- Editing and six-page PNG/PDF/MP4/GIF content journeys passed; all 80 editable
  template/size variants retained exact pixels after save/reopen.
- The production build passed its editing self-test and 24 native checks on
  both Wayland and XWayland. The installed main launcher passed 24 further
  Wayland checks. Light/dark Camera Raw captures were inspected, including the
  fixed footer at the minimum viewport and a complete graph at a larger size.
- Production SHA-256:
  `19bd5d4c6c682462f1a916ec7f04c86aa3a6d50da9c5d4ba75e435d0be4e19d6`.
- Complete installed rollback in both directions preserved all payload hashes
  and passed editing self-tests. A standard desktop launch resolved to the new
  main executable; the previous complete generation remains available.
- The exact public runtime passed its [full GitHub Rust/installer validation](https://github.com/Sugata-Software/Omuse/actions/runs/36492555743).
  The installer pin selects this runtime; later documentation commits do not
  change that source identity.

The initial test pass exposed a clipped curve graph and a selected-group drag
that did not move descendants. Both were fixed and the final suite rerun.
Floating-selection cancel/commit, linked masks and one-step Undo are covered.
Evidence is retained locally under `rust/evidence/omaphoto-123-20260929/`;
see [main installation qualification](main-install-qualification.md).

These results do not resolve the missing gestures, scaled previews,
low-memory exhaustion, cross-app interchange or binary-distribution gates
listed above. No competitive speed or stability result was measured.
