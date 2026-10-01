# Photo and vector foundations: local qualification

Development checkpoint, 1 October 2026. This work lives on
`feature/photo-vector-studio`; it is not a numbered public release or an
installation update. The installed/public baseline remains 0.6.0.

The tested application source is
`a4764f5be28e3ad0fb0af421656fec140b06fe3e`, following the foundation commit
`0cea92efc6252bf44002e1cf104c870569f0387a`.
The [roadmap](photo-vector-roadmap.md) records the full approved scope, and the
[user guide](user-guide/photo-vector.md) explains the implemented controls.

## Implemented scope

- Target-colour uniformity in the editable Filter stack, with independent
  hue/saturation/lightness amounts, range/falloff, node masks and native 16-bit
  evaluation. Unaffected P3/linear-sRGB samples remain exact. Fully transparent
  pixels retain hidden RGB through zero-coverage and no-op node blending.
- Explicit single-path SVG exchange with cubic geometry, compound holes and
  supported solid styles. Input and output are bounded. Preparation writes and
  syncs a disposable staged file; the UI rechecks its exact draft immediately
  before no-replace publication. Cancelled/stale preparations are discarded.
- Corner/smooth nodes, exact curve-midpoint insertion, new/reversed subpaths,
  open/close and fill-rule controls, with one Apply/Undo transaction.
- Scale-aware, premultiplied Lanczos for high-quality transformed 16-bit
  exports. Local and folder masks participate in the source taps, protecting
  against masked-out colour leakage. Exact mask taps use a fast path.
- Cancellation checks during source promotion, colour conversion, clipping
  passes and sampling. Interrupted export preserves an existing destination.

## Evidence

The optimized regression run passed **1,143 tests**: 464 library, 336 UI and
343 integration cases. Four explicitly ignored timing benchmarks were not run.
The complete `scripts/test-rust.sh` run exited **0**. The disposable editing
journey, Create PNG/PDF/story workflow, MP4/GIF motion exports and all **80
editable template variants** also passed. Template checks compare saved/reopened
native text and rendered pixels exactly.

Focused checks cover the colour kernel, hue wrap, native sample precision,
source immutability, opacity/masks, transparent channels, save/reopen and
Undo/Redo. All six colour inputs are clicked and typed into at 800×600; no
manual focus injection is used. SVG cases include limits, refused content,
rendered round trips, existing-file preservation, preparation cleanup and
staleness at publication.

Native Wayland and XWayland runs each passed the same **24 common editing
checks** at an actual 800×600 GPUI viewport. These are repeated backend checks,
not 48 distinct tests of the new features. The owned-window captures were
visually inspected: the dark vector panel shows compound transparency without
double-painting the old source; the light colour panel keeps Apply/Cancel
visible while its settings scroll. Headless pointer/keyboard checks separately
exercise the new inputs and vector actions. The XWayland compositor capture
is 1200×900 because its device/compositor scale differs from the GPUI viewport.

Both native runs used the optimized **`ui-test`-enabled development binary**,
SHA-256 `6baf64e3a1f6437527971b1ab9712612ab94c200b86f8dd4d15076f6f1bb9c41`.
This is not a production-binary installation or clean-machine qualification.
Its installed/public 0.6.0 counterpart was not replaced.

Machine-local evidence:

- `rust/evidence/photo-vector-foundations-summary.json` (source/binary
  identities, aggregate counts and compatibility results).
- `rust/evidence/photo-vector-foundations-full.log` and its `.exit` marker.
- `rust/evidence/photo-vector-native-vector/verified-native.json` and
  `native-window.png` (Wayland, dark).
- `rust/evidence/photo-vector-native-colour/verified-native.json` and
  `native-window.png` (XWayland, light).
- `rust/evidence/photo-vector-foundations-release-artwork/` (final optimized
  synthetic acceptance, exported artwork and `reader-compatibility.json`).
- `/tmp/omuse-tests.yyFQlBBB/` (isolated editing, Create/motion and template
  acceptance outputs retained by `OMUSE_TEST_KEEP=1`).

The final optimized synthetic acceptance example writes before/after colour
swatches, a 16-bit PNG, a compound-path SVG and two editable projects. The
development CLI reopened and rendered both projects with exact RGBA equality
to the example's exports. The installed production
0.6.0 reader rendered the vector-only project with **exact RGBA equality** and
refused the unknown `targetColourUniformity` recipe without writing an output.
Both source projects remained unchanged. Its executable SHA-256 was
`bf3b8f5f510865fd832d3305b1b2ea40505c5eab6c93bf9fa2d49dc629b41a46`.
Encoded PNG bytes were not used as the pixel oracle; ICC metadata may differ.
The final exported swatches and compound path were also visually inspected.

## Limits and remaining qualification

- High-quality export uses a maximum 66×66 source footprint and a 3/32 scale
  floor. Extreme reductions and anisotropic rotation remain approximate.
  Non-aligned mask grids may add 6×6 reconstruction work per tap and per mask;
  deeply nested/placed masks can be expensive. No comparative speed claim is
  made. The 8-bit canvas still filters masks independently and can differ from
  corrected 16-bit exports on reduced masked detail.
- Uniformity uses encoded-sRGB HSL. Selected P3 colours can be gamut-mapped;
  this is not perceptual luminance matching, semantic skin selection or a
  complete wide-gamut pipeline. Camera Raw's compatibility node remains 8-bit.
- Editable SVG is one painted object, not a complete multi-object design.
  Export excludes photos, layer placement and retained filter effects.
  Direct-canvas tools, booleans, advanced strokes, typography, tracing and
  vector PDF remain roadmap work.
- Each current vector recipe retains full-canvas raster surfaces. The
  roadmap makes procedural scenes and bounded tile caches a prerequisite for
  larger illustrations and multi-object SVG; the allocation estimates there
  are source-derived, not process-memory measurements.
- Synthetic cases and this host's native checks do not establish quality on
  independent photographs, complete external-editor interchange, calibrated
  displays, broad hardware support or a stable public release. No live AI
  provider request is part of this pass.
- The installed Cua Omarchy wrapper reports changed Hyprland build
  compatibility. Cua input qualification is unavailable; the native application
  harness is used instead. No compositor/plugin configuration was changed.

## Reproduce

Use the system tools ahead of environment-manager shims for the isolated-XDG
test run. This avoids changing trust settings just to execute tests:

```sh
PATH=/usr/local/bin:/usr/bin:/bin OMUSE_TEST_KEEP=1 ./scripts/test-rust.sh
cargo run --manifest-path rust/Cargo.toml --release --locked \
  --example photo_vector_acceptance -- /tmp/omuse-photo-vector-artwork
```

Use a fresh acceptance directory. Machine-local evidence is retained under
ignored `rust/evidence/`; generated artwork does not include user documents.
