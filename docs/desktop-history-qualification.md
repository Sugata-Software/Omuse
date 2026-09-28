# Desktop page ownership and large-photo editing

This change removes the Create project's redundant pixel owner between edits.
An active page and up to three cached inactive pages remain owned by their
Editors. The structural Project retains page IDs, order, names, dimensions,
templates, brands, components and resources. Its checked-out page storage has
no second live raster allocation and cannot silently supply stale artwork.

## Snapshot and recovery guarantees

Save, export, preflight and recovery materialize a complete Project from the
active and cached Editors. Overlay and validation finish before the live cache
or collection undo expectation changes. Structural collection mutations still
explicitly synchronize every Editor before cloning their mutable draft.
Page switching synchronizes before evicting an inactive Editor, so its latest
document survives even when its local undo history is released.

An incomplete Project rejects validation, document reads and save. The new
checked-out state is internal to an open desktop session; it is never written
to `.omuse`, and there is no file-format change. Page dimensions stay current
after crop/resize. Existing page names and template identity remain canonical.

Older Project clones keep their original pixels across a later save to the
same package path. Before checking out a shared lazy page, the old document is
loaded into its shared snapshot cache. The complete batch is checked before
any live page document is released.

The review also found an existing save-identity bug: a fully frozen snapshot
could be rejected after its own source package was atomically replaced. An
opened collection edited during background save could then fail recovery,
export or its next save. Package traversal now checks source identity only
when it actually reads an uncached disk asset, and checks it again before
publication. Fully cached snapshots have no remaining dependency on the old
package. Destination conflict checks remain separate and unchanged.

Recovery still captures completed edits immediately. Its existing worker
retains one pending snapshot plus one in-flight write, uses the same 150 ms
debounce, acknowledges clear operations, and flushes its final accepted
snapshot on shutdown. No UI timer delays snapshot capture. Background save,
export, recovery and collection undo snapshots remain immutable; a stroke
against one of these owners can still copy the full raster. The optimization
removes the persistent Create cache owner after those temporary jobs finish.

## Qualification status

Implementation, independent ownership review, the full regression gate,
all eighteen paired desktop comparisons, production launch and isolated
preview installation passed on 28 September 2026.

The final `scripts/test-rust.sh` run passed **734 automated tests**: 326 library,
179 GPUI/UI and 229 integration tests. Four manual timing tests were excluded
from the regression gate; the new desktop benchmark ran separately. The pinned
Nikon RAW fixture and LibRaw runtime were enabled. The editing journey,
six-page PNG/PDF/MP4/GIF workflow and all 80 editable template variants passed.
The thirteen added regressions cover checkout, transaction failure, frozen
snapshots, lazy-source replacement, cache eviction, page dimensions and
save/recovery/export after editing during a background save.

The earlier sixteen-case external photo corpus at `ac8874d` was not repeated
for this ownership change. These are local candidate results, not clean-machine,
camera-colour or physical-input qualification.

## Measured results

Each value below is the median of the corresponding statistic from three
fresh processes per strategy and size. Each process measured twelve short
strokes after one warmup. Both strategies used the same final release test
executable; only Create's cache ownership differed. All paired pixel hashes
matched, including exact full undo, redo and recovery.

| Canvas | Paint core median, previous → current | Headless frame/refresh median, previous → current | Frame/refresh p95, previous → current | Peak process RSS, previous → current |
| --- | --- | --- | --- | --- |
| 12 MP | 9.48 → 0.32 ms | 62.22 → 52.67 ms | 66.45 → 60.69 ms | 496.91 → 496.89 MiB |
| 24 MP | 18.84 → 0.31 ms | 105.73 → 75.72 ms | 432.44 → 313.73 ms | 957.49 → 957.82 MiB |
| 48 MP | 85.69 → 0.32 ms | 257.88 → 165.38 ms | 1128.84 → 736.31 ms | 1876.28 → 1876.99 MiB |

Measured full-raster detachment fell from 576 / 1152 / 2304 million copied
bytes to zero over the twelve strokes at 12 / 24 / 48 MP. This is cumulative
copy traffic, not resident memory. Both strategies retain the same tile undo
history. Peak memory is essentially unchanged; this does not establish a
process-memory reduction.

The measured headless frame/refresh median improved by 15.3%, 28.4% and 35.9%
at the three sizes. The full end-of-stroke refresh still dominates and has
large outliers. With twelve samples per process, the nearest-rank p95 is the
maximum; it is not a precise estimate of real-world tail latency. Rapid strokes
while recovery is writing and physical input-to-display latency remain future
measurements. Do not combine these results with the earlier isolated
100-stroke history workload as though they were the same benchmark.

## Production desktop and installed preview

The normal locked release build, without `ui-test`, passed all 22 native
journey checks on both Wayland with the system theme and XWayland with the
light theme at the 800×600 logical minimum. The journeys include painting,
undo/redo, save/reopen, background photo open/export, crop/resize, retained
16-bit import/export, selections, masks, live text and effects. Both owned-window
captures were visually reviewed. These synthetic journeys inject events
through GPUI; physical keyboard/pointer delivery is not qualified here.
No live provider request or desktop restart was performed.

Production and installed preview executable SHA-256:
`5e7d8b9c75a6c42eeee37047c51f1685fac4d9ea47a08cbe9d4f7e4974e8646e`.
The actual `omuse-preview` launcher resolved correctly and passed its editing
self-test from `/tmp` with isolated user state. RAW/ONNX libraries and the
subject model match the prepared runtime assets. Both prior preview binaries
were preserved before installation; the separate original app is unchanged.

## Reproducing the desktop comparison

Build the release test executable with `ui-test`, then pass its exact path:

```sh
cargo test --manifest-path rust/Cargo.toml --release --locked \
  --features ui-test --jobs 2 --bin omuse --no-run
python3 scripts/desktop-history-stress.py \
  --binary /absolute/path/to/rust/target/release/deps/omuse-TEST-HASH \
  --evidence /new/evidence/directory
```

The wrapper runs three paired repetitions at 12, 24 and 48 MP in fresh
processes, alternating order. Both modes use the same test executable. The
baseline is the previous **cache retention strategy** selected by a test-only
switch, rather than the complete previous production executable. Both use
region history, the same synthetic patterned image and the same strokes,
localized within the upper-left 520×520 pixels of each larger canvas.

One warmup stroke consumes the initial shared owner; twelve measured strokes
follow. Each executes editor painting, a queued GPUI frame and `changed()`.
Recovery finishes before each measured stroke, outside the timing window.
The result reports paint-core time separately from frame/refresh wall time.
Neither is physical input-to-display latency, a GPU benchmark, or a claim
about rapid strokes while recovery is still writing.

Each process checks exact full-undo/final/redo/recovered pixels and an unchanged retained
snapshot. It records detached raster bytes, retained tile history, undo depth
and process peak RSS. The wrapper rejects incomplete results, unequal paired
hashes, incorrect detach counts, a 3 GiB RSS ceiling, a 4 GiB address-space
ceiling, a 16 MiB diagnostic-log ceiling or a 900-second timeout. The limits bound this synthetic qualification;
they are not application memory guarantees.

Machine-local receipts are retained under
`rust/evidence/desktop-history-20260928/`. They are not public release assets.

- `tested-sources.json`: 265 frozen source/build/harness hashes;
- `tests.log`, `regression-results.json` and `acceptance/`: the completed gate;
- `comparison/report.json` and eighteen per-process logs: exact paired results;
- `production-build.json`, `native-{wayland,xwayland}/verified-native.json`,
  `installed-preview.json` and `rollback.json`: executable and runtime identity;
- `implementation-review.json`: independent ownership and regression review.

Public binary distribution still requires the gates in
[public release readiness](public-release-readiness.md).
