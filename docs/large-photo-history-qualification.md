# Large-photo region-history qualification

This 28 September 2026 change replaces full painted-image history snapshots
with reversible 256×256 tiles for unselected Brush, Pencil and Eraser strokes
on ordinary 8-bit raster layers of at least 1 MiB. A small stroke now retains
only its touched tiles while preserving the complete surrounding editor state.
The [architecture record](large-photo-history-plan.md) describes the transaction
and accounting invariants.

## Correctness and content workflows

The final `scripts/test-rust.sh` run passed **721 automated tests**: 326 library,
175 GPUI/UI and 220 integration tests. Three explicitly manual timing tests
were excluded. The pinned Nikon RAW fixture and LibRaw runtime were enabled.
The 21 additional tests cover:

- clipped tile edges, hidden pixel bytes, repeated capture and symmetric swaps;
- exact differential pixels and composites against snapshot-only history;
- hard/soft Brush, Pencil and Eraser, pressure, opacity and transformed layers;
- no-op allocation preservation, cancellation and bounded-stroke failure;
- atomic rejection of invalid geometry/revisions and noncanonical buffers;
- mixed metadata, selection, masks, layer removal/reinsertion, saved revisions,
  redo branching, mode changes and budget eviction;
- unchanged frozen background owners and zero full-raster detachment when the
  edited raster is uniquely owned;
- queued GPUI frames, cancellation, Undo and Redo with region history active.

The disposable editing journey passed save/reopen and PNG/JPEG/WebP/TIFF
exports. Create produced an editable six-page collection, resized story,
six-page PNG/PDF export, MP4 and GIF. All **80 editable template variants**
retained their native text and exact composite pixels after reopening.

The earlier [photo qualification](photo-release-qualification.md) contains the
16-case independent photo corpus at `ac8874d`. That entire external corpus was
not repeated for this history-only change; the RAW-enabled regression suite
and the new size runs are separate evidence.

## Three paired performance runs

All **18 size runs** passed: three repetitions of baseline/current at each
size. Each pair used the same photograph, workload and final composite hash.
The baseline executable was preserved at `ac8874d` before rebuilding. No
compiler or other test workload ran during the comparisons.

The table gives the median of the three per-process medians and p95 values.
Undo counts were identical across all three repetitions.

| Canvas | Stroke median, before → after | Stroke p95, before → after | Retained undo, before → after |
| --- | ---: | ---: | ---: |
| 12 MP | 10.74 → 2.00 ms | 42.58 → 2.86 ms | 5 → 100 |
| 24 MP | 41.49 → 2.13 ms | 190.75 → 3.16 ms | 2 → 100 |
| 48 MP | 90.43 → 0.42 ms | 407.67 → 0.63 ms | 1 → 100 |

Every individual 48 MP pair improved both median and p95. The 12 MP case also
improved. These fresh baselines supersede the older single-run timings for
this comparison; host load can materially change absolute times.

First-stroke copying remains visible. This table gives the median first-stroke
cost, the worst stroke over all repetitions, and the median process peak RSS:

| Canvas | First stroke, before → after | Worst stroke, before → after | Peak RSS, before → after |
| --- | ---: | ---: | ---: |
| 12 MP | 9.60 → 30.18 ms | 103.86 → 34.49 ms | 418.8 → 326.3 MiB |
| 24 MP | 13.81 → 61.44 ms | 219.02 → 68.17 ms | 581.6 → 609.7 MiB |
| 48 MP | 51.49 → 44.40 ms | 445.01 → 62.33 ms | 1168.5 → 1199.6 MiB |

The first stroke regressed at 12 and 24 MP in these samples. Total process
memory also rose by about 28 and 31 MiB at 24 and 48 MP. This qualifies routine
stroke speed and history depth, not a universal latency or memory reduction.

The final 100-entry history retained 30.37 / 30.62 / 30.96 MiB at 12 / 24 /
48 MP, including 123 / 124 / 125 captured tiles. At 48 MP, patch storage was
30.80 MiB; the maximum history accounting during the sequence was 213.82 MiB
while earlier metadata snapshots were still retained. Instrumentation recorded
one whole-raster detachment per run (48 / 96 / 192 million bytes), consistent
with the initial shared owner. This counter is cumulative copied raster bytes,
not RSS or display latency.

## Measured scope and limits

The stress workflow uses a pinned photograph tiled into two-layer 12, 24 and
48 MP documents, ten metadata changes and 100 short synthetic 24 px strokes.
Each fresh process checks its retained undo/redo sequence, native `.omuse`
save/reopen, PNG pixels and atomic over-budget rejection. The wrapper enforces
a 3 GiB RSS ceiling, 4 GiB address-space ceiling and a 900-second timeout per
size. It records all expected/completed cases and rejects partial receipts.

These measurements concern CPU editor operations, not physical input-to-display
latency, native high-resolution camera decode, GPU residency or long sessions.
The 256 MiB setting limits retained history, not total application memory.
The existing 100-entry limit remains in effect.

Masks, selected strokes, clone/heal, small rasters and non-stroke transactions
retain full snapshots. Live and advanced sources retain their existing editing
restrictions. Region history uses contiguous live images; it does not change
the project format or renderer.

At this candidate, Create kept a synced active-document owner between strokes. Recovery holds
a document during its 150 ms debounce and package write; explicit background
saves also retain immutable snapshots. Those owners can still force a full
copy on the next paint. The isolated fixture does not model their lifetimes.
The later [desktop ownership qualification](desktop-history-qualification.md)
records the separate work to reduce those copies while preserving frozen
saves, recovery and exact undo. The measurements here remain historical.

Use the editor's mutation methods. Direct mutation of the public document
field bypasses history and dirty tracking and is outside the transaction
contract. Invalid patch geometry or revision refuses undo without changing
pixels or consuming history.

## Production desktop and installed preview

The normal locked release build, without `ui-test`, passed all 22 native
journey checks on both Wayland with the system theme and XWayland with the
light theme at the 800×600 logical minimum. These include background photo
open/export, crop/resize, undo/redo, retained 16-bit import/export, filters,
masks, live text/effects and project reopening. Both owned-window captures
were visually reviewed.

The native harness injects events through GPUI. The Omarchy Cua plugin is
installed but inactive in this desktop session, so this does not qualify
physical keyboard/pointer delivery. No desktop restart or configuration change
was performed, and no live provider request was made.

Production and installed preview executable SHA-256:
`e255e86449d4970f9094b0c225b25d3881ddc1f9dc3525d698101fed8fa5a36f`.
The actual `omuse-preview` launcher resolved correctly and passed its editing
self-test from `/tmp` with isolated user state. RAW/ONNX libraries and the
subject model match the prepared runtime assets. Both prior preview binaries
were preserved before installation, and the separate original app was unchanged.

## Reproduction and retained evidence

Run `scripts/test-rust.sh` with the optional pinned RAW fixture/runtime enabled.
Build the ordinary release executable and stress example without `ui-test`:

```sh
cargo build --manifest-path rust/Cargo.toml --release --locked --jobs 2 \
  --bin omuse --example photo_stress
python3 scripts/photo-stress.py --input /path/to/astronaut.png \
  --evidence /new/current-run --require-region-history
```

Preserve the baseline executable and its matching fixture source before
rebuilding. For each baseline run, pass `--binary /path/to/baseline/photo_stress`
and `--fixture-source /path/to/baseline/photo_stress.rs`, without the region
requirement. Repeat baseline/current sequentially three times, away from
compilers and other test workloads. Require matching input, workload and final
pixel hashes as well as complete passing receipts before comparing timing.

Local evidence is under `rust/evidence/region-history-20260928/`:

- `tested-sources.json`: the 262 build-input hashes frozen before qualification;
- `tests.log`, `regression-results.json` and `acceptance/`: completed tests and
  disposable content workflows;
- `baseline/`: the preserved `ac8874d` stress executable, source and hashes;
- `run-{1,2,3}-{baseline,current}/report.json` and `comparison.json`: all paired
  size receipts and aggregate calculations;
- `production-build.json`, `native-{wayland,xwayland}/verified-native.json`,
  `installed-preview.json` and `rollback.json`: executable identity and runtime
  acceptance;
- `implementation-review.json`: independent transaction and ownership review.

Machine-local logs and paths are not public release assets. Public binary
distribution still needs the gates in
[public release readiness](public-release-readiness.md).
