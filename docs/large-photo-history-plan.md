# Large-photo region history

## Decision and evidence

Ordinary unselected Brush, Pencil and Eraser strokes now use reversible region
history when their editable 8-bit raster contains at least 1 MiB of pixels.
Smaller images, masks, clone/heal, selected strokes and other edits retain full
snapshots. This changes history and mutation, not rendering or project files.
The final bounded results are recorded in the
[region-history qualification](large-photo-history-qualification.md).

The bounded qualification in
`rust/evidence/photo-stress-20260928/report.json`, before this change at
development commit `ac8874d`, passed at all three sizes with
exact undo/redo, native `.omuse` reopen and PNG export. It also establishes the
problem:

| Canvas | Peak RSS | Stroke median | Stroke p95 | Undo at 256 MiB |
| --- | ---: | ---: | ---: | ---: |
| 4000×3000 (12 MP) | 419 MiB | 21.4 ms | 112.6 ms | 5 |
| 6000×4000 (24 MP) | 582 MiB | 50.6 ms | 221.7 ms | 2 |
| 8000×6000 (48 MP) | 1.14 GiB | 301.0 ms | 478.2 ms | 1 |

These are 100 synthetic 24 px brush strokes with a short continued segment,
not input-to-display measurements. The 48 MP run completed correctly in 42.8
seconds, but a small edit retained a roughly 192 MB raster revision. The
previous stroke `Snapshot` cloned the document before painting; `SharedImage` uses
`Arc::make_mut`, so a changed pixel can detach the whole active `RgbaImage`.
Metadata edits already share pixels and are not the target of this work.

## Implemented architecture

`Document`, `Layer`, package persistence and the raster renderer retain their
existing interfaces. The private history entry is:

```text
HistoryEntry
  Snapshot(existing full editor snapshot)
  Raster {
    state: full editor Snapshot with only the target image removed,
    layer_id, expected_revision,
    pixels: RasterPatch { dimensions, tiles: Vec<TilePatch> }
  }

TilePatch
  tile_x, tile_y, clipped_width, clipped_height, rgba_bytes
```

Tiles use the existing 256×256 display/damage grid. On the first changed pixel
in a tile, the collector copies that
tile's pre-stroke bytes into the patch collector. Subsequent dabs in the tile
reuse those bytes as the stroke's original-pixel source. This preserves the
accumulated-coverage behavior without retaining a full pre-stroke image. Tiles
are sorted in row order, with a clipped final row and column. A no-op captures
no tile. Noncanonical image buffers with trailing bytes select the snapshot
fallback before painting, because region swaps require an exact pixel layout.

Keeping the full state shell preserves document metadata, other layers, active
layer, selection and saved-revision semantics when mixed with old snapshots.
The target image's cloned owner is removed before mutation; other retained
owners remain immutable through `SharedImage` copy-on-write.

Undo applies every tile by swapping its stored bytes with the live raster. The
same entry then contains the redo bytes and moves to the redo deque. Redo uses
the same operation. A history entry therefore owns one tile copy, rather than
separate before and after copies. `history_bytes()` charges actual patch vector
capacities plus entry metadata, and the existing 100-step and 256 MiB trimming
rules continue to apply.

Normal Brush, Pencil and Eraser strokes on an ordinary 8-bit raster are the
first eligible operations. The patch is in layer-local coordinates. Existing
`StrokeDamage`/`PixelRect` remains the display invalidation contract, and
`raster::composite_region` remains the exact incremental compositor where it
already accepts the document. Full `raster::composite` remains the pixel oracle
and fallback.

The following operations initially retain `Snapshot`: clone/heal and tools
that take a canvas source, adjustments and filters, content fill, crop/resize,
layer insertion/deletion/reorder, floating selections, advanced/16-bit state
replacement, and whole-document transactions. Metadata snapshots remain cheap
because their raster allocations are shared, although such a retained owner
can still force one full detach on the next paint. The stress receipts report
the first stroke separately from steady-state strokes.

This slice does not change `SharedImage` storage. It removes the history
snapshot that normally forces whole-image detachment. A concurrent save,
preview or other owner can still make `Arc::make_mut` detach the live image.
The diagnostic counters record retained tile bytes and cumulative bytes copied
during strokes and patch undo/redo. Consider tile-backed `SharedImage` only if retained
external owners remain a material measured cost; that would be a separate
project with a much wider API and persistence audit.

## Required invariants

- **Atomic commit:** a completed eligible stroke creates exactly one history
  entry and one revision. A no-op creates none. Unsupported cases select the
  existing snapshot path before any pixel changes.
- **Cancel and failure:** cancellation, invalid input, overflow and abandoned
  strokes restore every captured tile, revision and editor state. They leave
  undo/redo depth, dirty state and damage empty, matching current behavior.
- **Undo and redo:** tile coordinates are unique and deterministic. Patch
  application preflights the layer ID, target kind and dimensions before the
  first write. A mismatch changes neither document nor history. Successful
  undo/redo is byte-exact and preserves the current saved-revision semantics.
- **Selection:** selection coverage is evaluated exactly as today. The first
  release may use region history only when no selection is active. Selected
  strokes stay on `Snapshot` until shared selection state can preserve the
  current selection restore behavior without copying a canvas-sized mask per
  entry.
- **Transforms:** patches always address source-image pixels. A transformed
  layer must produce the same local damage and pixels as the snapshot path.
  Later transform edits remain separate history entries, so reverse-order undo
  restores the transform before applying an older pixel patch.
- **Masks:** image and mask targets are explicit and cannot alias. Mask patches
  retain grayscale RGB equality and alpha 255. Independently placed masks use
  their local dimensions and current placement mapping.
- **Layer lifetime:** deletion, insertion, reparenting and replacement stay on
  full transactions. Undo order must restore a missing layer before a prior
  raster patch can be reached.
- **Background work:** saved/recovery snapshots keep immutable pixels. Editing
  while saving may take the current full COW fallback, but must never mutate the
  frozen job's raster.
- **Renderer and files:** regional display, full composite, save/reopen and
  exported PNG pixels remain identical. No tile-history representation is
  written to `.omuse`; only the current document state is persisted.
- **Budget:** rejected or trimmed entries release all patch buffers. Moving an
  entry between undo and redo does not duplicate its buffers.

## Scope and follow-up

The hybrid entries, diagnostics, unselected paint path and source-local
transformed strokes are implemented. `set_region_history_enabled(false)` keeps
the snapshot-only oracle callable and affects future strokes; existing mixed
history remains reversible. Eligibility defaults on for the qualified scope.
The old executable and stress source are frozen separately for paired runs.

Masks and selected strokes remain on full snapshots. Extending them needs
independent mask-placement and selection-ownership qualification. Clone/heal
and advanced/16-bit sources also retain their existing paths.

The later [desktop ownership work](desktop-history-qualification.md) removes
the persistent synced-document owner from Create. Active and cached pages
belong to their Editors; complete immutable Projects are materialized for
save, export, recovery and collection changes. Display tiles and thumbnails
hold derived pixels. Recovery still retains a document during its 150 ms
debounce and write, and background saves retain their own snapshots. These
temporary owners can force a whole-image copy even with region history.
The isolated editor fixture does not model these lifetimes. See the desktop
record for its separate comparison and limits; neither fixture measures
physical input-to-display latency or promises zero copies during all editing.

Use editor mutation methods for live edits. Direct writes to the public
`Editor.document` field already bypass undo and dirty tracking; they can also
invalidate a region entry. Geometry/revision mismatch refuses undo atomically.
Direct mutation during a stroke is outside the transaction contract.

Expected change and verification cost:

| Slice | Change surface | Verification cost |
| --- | --- | --- |
| Hybrid entry and accounting | Editor history internals and focused history tests | Moderate; mixed-stack and byte-accounting tests, no file migration |
| Unselected paint | Stroke preimage reads, stamp writes and cancel/finish | Highest initial correctness cost; differential coverage across every brush mode |
| Masks and transforms | Existing coordinate mapping and mask mutation paths | Moderate; tile-edge, placement and full-composite comparisons |
| Selected strokes | Selection ownership and restore semantics | High and optional; retain fallback until independently justified |
| Tile-backed live raster | `SharedImage` and all contiguous-image consumers | Very high; explicitly outside this implementation |

The existing three-size stress pass takes about 73 seconds on this host. Three
paired baseline/candidate repetitions therefore require roughly eight minutes
of isolated workload time, plus the ordinary test suite and artifact review.
Run those samples away from builds and other benchmarks.

## Verification cost and acceptance

The main engineering risk is not tile copying; it is preserving the stroke's
preimage, cancellation and mixed undo stack. Verification therefore needs more
than a microbenchmark.

Correctness acceptance:

- Differential tests cover hard/soft Brush, Pencil, Eraser, repeated dabs,
  self-crossing strokes, opacity, pressure, canvas edges and tile boundaries.
- No-op, outside-canvas and transparent strokes retain their raster allocation
  and create no history. Cancelling a changed region stroke restores exact
  pixels and state; it may retain a detached allocation when another owner
  holds the original. Frozen owners remain unchanged.
- Full undo then full redo is exact for mixed sequences of patch strokes,
  metadata, transforms, layer operations and document transactions.
- Mask painting, linked and independently placed masks, transformed layers and
  selected strokes remain exact before their eligibility gates are enabled.
- Save during and after edits, recovery snapshots, native `.omuse` reopen and
  PNG export retain exact hashes. Invalid patch preflight is atomic.
- Randomized small-document sequences compare the hybrid editor after every
  operation with a snapshot-only oracle and compare full composites.

Performance acceptance:

- Instrumentation proves an eligible small stroke captures bytes proportional
  to touched tiles and does not retain a canvas-sized history raster when the
  live image is otherwise uniquely owned.
- Under the existing 256 MiB budget, the 48 MP/100-stroke fixture retains at
  least 80 exact undo steps; the 100-step cap, rather than a full-raster entry,
  should be the expected limiter for this fixture.
- Repeat the 12, 24 and 48 MP cases in fresh sequential processes under the
  existing 3 GiB RSS, 4 GiB address-space and timeout limits. Record three
  paired runs and report median/p95/max stroke latency, peak RSS, patch bytes
  and undo depth. Do not infer physical-input latency.
- Enable the new path only if the paired 48 MP runs show a repeatable median and
  p95 improvement without a material 12 MP regression. Establish the observed
  result from the prototype; do not promise a fixed speedup in advance.

If correctness, memory accounting or mixed-history recovery fails, keep the
snapshot path enabled and ship no partial optimization. If region history is
correct but shared external owners still cause frequent full detaches, retain
the work and scope tile-backed live raster storage separately rather than
expanding this change into a renderer rewrite.
