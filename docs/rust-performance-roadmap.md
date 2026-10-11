# Rust performance and stability

The performance direction is measurable input latency, bounded memory and correct pixels on Linux. GPUI accelerates presentation; the image engine currently runs on the CPU. No claim of beating OmaPhoto or the Mac application is justified without a controlled comparison on the same documents and hardware.

## Shared image ownership

`SharedImage` holds an `Arc<RgbaImage>`. Document clones, undo snapshots, duplicated layers, recovery requests and the Camera Raw source preview can share immutable pixel allocations. Mutable image access passes through `Arc::make_mut`, detaching only the edited image. Background saves and older undo states retain their original pixels. Import/export formats do not change.

The history budget charges an image allocation once across undo and redo, excluding allocations still owned by the current document. It charges pixel vector capacity rather than length, plus estimated per-snapshot metadata and selection storage. Undo plus redo is capped at 100 entries as well as the default 256 MiB retained-history budget. This bounds metadata-only edits that no longer consume large raster snapshots. It is a history budget, not a cap on process RSS or temporary processing buffers.

This is whole-image copy-on-write. The first changed paint pixel still copies the edited image when another owner retains it. A stamp that changes no pixels keeps the shared allocation, including zero-opacity paint, transparent erasing, and unchanged clone/heal or mask strokes. Tile-granular storage and GPU filters remain future work.

## Region history for large raster strokes

Unselected Brush, Pencil and Eraser strokes on ordinary 8-bit rasters of at
least 1 MiB retain reversible 256×256 preimage tiles. The history entry keeps
the complete editor-state snapshot with only the painted raster removed, so
history itself no longer forces a whole-image copy for each eligible stroke.
Undo and redo swap the same tile buffers after validating the target and
revision. The existing 100-entry and 256 MiB limits still apply.

Masks, selected strokes, clone/heal, live/advanced sources and non-stroke
transactions use the existing snapshots. Recovery, a collection checkpoint or
a background save can still hold the current raster and force copy-on-write.
This is tile history over a contiguous image, not tile-backed live storage.
See the [architecture](large-photo-history-plan.md) and
[bounded qualification](large-photo-history-qualification.md) for exact scope,
paired results. The later [desktop ownership work](desktop-history-qualification.md)
removes the persistent Create cache owner without changing live raster storage.

Active and cached inactive pages belong to their Editors. Save, export and
recovery materialize complete immutable Projects on demand; the live structural
Project cannot read or save a checked-out page until an Editor overlays it.
Older lazy snapshots are frozen before checkout. Fully cached snapshots also
remain usable after a same-path package exchange; uncached reads still validate
the source identity. Pending and in-flight recovery/save snapshots correctly
retain their pixels and can still force a full copy on the next edit.

## Rendering and asynchronous state

Integer-aligned Normal layers without masks use the existing direct blend path at partial layer/group opacity as well as full opacity. The path preserves the general renderer's alpha rounding. A differential test forces the general masked path and compares complete pixels across clipped placement, varied alpha and inherited opacity.

Camera Raw, RAW import, subject segmentation and refinement completions check generation and expected dialog before clearing the busy state. Cancelled work cannot release the UI state of a newer operation. This rejects stale results; it does not cooperatively stop CPU work already running.

## Incremental painting

Brush, pencil, eraser, clone and healing strokes track the bounds of pixels actually changed, in source-image coordinates. The UI merges pointer work until the next GPUI display frame. Finishing, cancelling or replacing a document invalidates queued previews; the final/restored document receives a full render. Switching layer rows or mask badges finishes an active gesture before changing its target.

For flat Normal raster layers with unit scale, zero rotation, integer offsets and no masks, the renderer restores the damaged canvas rectangle from the background and composites only intersecting pixels. It checks the entire document before modifying the target. Opacity, visibility, negative placement and alpha rounding match the full renderer. Ordinary saved/reopened raster projects and color-managed imports qualify, including stale structural metadata retained after edits. Unknown metadata, groups, effects, adjustments, masks, fractional placement and transformed layers use the full renderer.

This reduces CPU compositing work for qualifying strokes. The display cache below limits image conversion and upload to changed tiles, while the first mutation may still copy a whole shared layer. Tile-granular editing storage and proportional end-to-end input latency are not yet established. Rectangle-based damage may also cover unchanged pixels between distant dabs; future tile tracking can reduce that cost.

## Display tiles and GPU image lifetime

The canvas uses a 256-pixel display grid with a one-pixel border sampled from neighboring canvas pixels, clamped at the canvas edge. A terminal strip narrower than 64 pixels joins its preceding tile, avoiding poorly sampled one-pixel edge textures. These BGRA surfaces replace the prior whole-canvas display image; they do not change the document, history, exported pixels or painting storage. Regional paint updates rebuild only intersecting tiles, including neighboring borders. Full refreshes compare the composite to the cached surfaces and retain identical image identities, so mouse release does not upload a second copy of an unchanged preview.

GPUI uploads a surface on its first visible paint and reuses that image identity thereafter. The canvas explicitly evicts superseded and offscreen image identities from the atlas. Only the last painted generation is retained for retirement; intermediate previews that were never painted do not accumulate in a queue. Window/view release also clears owned image identities. Gradient, Camera Raw and matte previews use the same cache and retirement path.

Every tile edge derives from the whole canvas's snapped device-pixel endpoints, avoiding accumulated layout rounding and matching the full image's effective scale. The one-pixel border protects linear filtering from adjacent atlas allocations. GPUI independently snaps each tile to device pixels, so fractional zoom can differ slightly from one large image's sampling. The native comparison records exact differences against an independent monolithic texture with the same clamped outer border: an unpadded reference can sample unrelated atlas data at its edges. Canvas/export pixel equivalence remains exact in the model tests.

Native acceptance compares tiled and monolithic output over separate uniform black and white backgrounds. Each uses a local 3×3 reference RGB envelope with tolerance 2 and at most 0.01% outside pixels. Paired black/white captures also check derived alpha and the fixture's known alpha-175 bands. Flat bands crossing internal seams require zero outside-envelope pixels on every background. Checkerboard RGB differences are retained as diagnostics: fractional foreground sampling over a spatially changing background can produce valid colors outside a neighborhood of already-composited reference pixels. These checks do not assert identical screen pixels at every zoom.

The CPU cache retains the current composite's display tiles. Its exact texel count is `(W + 2*columns) * (H + 2*rows)`, using the coalesced grid's actual axis counts. Within the current 100-million-pixel and 30,000-per-axis limits, the maximum tile-buffer size is 406,447,104 bytes, excluding the RGBA composite, document/history, GPU copies and retained frame snapshots. GPU residency follows visible tiles; a zoomed-out view that shows the entire image can still require the full image's display data. This is not a process-memory cap.

## Next architectural steps

The next development build moves native Blur/Smudge/Liquify calculations into
a cancellable worker shared with image I/O admission. It captures shared source
pixels and publishes a verified result as one Undo transaction. The selection
capture, final history commit and canvas refresh still run on the UI thread;
this is not a measured end-to-end latency improvement or tile-backed storage.
See the [scoped qualification](background-retouch-qualification.md).

1. Measure rapid desktop strokes while recovery or save still owns a snapshot, and the full refresh at stroke completion. The persistent Create cache owner has been removed; immutable live raster tiles should follow only if the remaining copies are a material measured cost. Preserve frozen snapshots, selection state and exact undo. Expand region-history eligibility separately, with masks and selected strokes retaining their fallback until qualified.
2. Extend the verified regional compositor to masks and other semantics. Measure input-to-presentation latency on large documents and consider zoomed-out overview surfaces to reduce visible texture data.
3. Add bounded worker admission and cooperative cancellation for expensive previews and segmentation. Make recovery clear/shutdown asynchronous while retaining save-before-close guarantees.
4. Measure large multilayer documents, brush input latency, first-edit cost, sustained memory, zoom, save/recovery and export. Compare the same fixtures with OmaPhoto before any competitive speed claim.
5. Close the interaction gaps: transformed inline text, dedicated crop handles, graphical adjustment controls. Add cross-application `.comp` round trips and test-gated native Linux packaging.

## Reproduction

Run `scripts/test-rust.sh` for automated validation. Run the ignored `snapshot_benchmark` and `raster_benchmark` integration tests in release mode separately from builds and other CPU-intensive work. Timing results are observations, never timing assertions in CI. Use `scripts/rust-release-qualification.py` for bounded repeated publication and process-interruption checks, and `scripts/native-rust-check.py` for a separate synthetic Wayland window.

The ignored binary test `display_surface::tests::benchmark_regional_conversion_against_full_clone_swap` compares regional display conversion, full tile creation, unchanged refresh and the previous clone/swap conversion path. It does not measure GPU execution. `scripts/native-display-check.py EXECUTABLE NEW_EVIDENCE_DIRECTORY --desktop-env DESKTOP_ENV_JSON` runs the tiled/monolithic screenshot comparison in one disposable window. It needs Pillow in the Python environment, `grim`, and a live Hyprland session; it never changes desktop configuration. Only its own process and verified window are controlled.

The snapshot case uses four 2048×2048 RGBA layers and twenty layer renames. Its retained-history number is the editor's accounting, not a process-memory measurement. Compare undo depth as well as bytes: the old implementation can appear bounded only because it evicts almost all the requested history.

## Measured local checkpoint, 26 September 2026

On this host, the four-layer snapshot clone changed from a 5.932 ms median to 0.00036–0.00170 ms across two post-change observations. Twenty renames changed from 169.26 ms to 0.069–0.318 ms. Retained-history accounting changed from 201,330,723 bytes with only three retained undo steps to 27,530 bytes with all twenty retained. This is specifically a metadata-edit/snapshot benchmark, not a claim that the whole app is hundreds of times faster.

The final adjacent renderer comparison measured identity layers at 9.509 ms before and 10.174 ms after; the transformed case measured 182.916 ms before and 192.356 ms after, with substantial host-load outliers. Those cases do not establish a rendering speedup. The new partial-opacity identity case measured 15.152 ms, but no matching old-case timing was captured. An initial implementation that slowed full-opacity layers to 27.335 ms was rejected and corrected before installation. Gaussian timing is a single sample and is not used for a performance claim.

Machine-local raw logs are in `rust/evidence/snapshot-baseline.log`, `snapshot-after.log`, `snapshot-final.log`, `raster-before-final.log`, and `raster-after-final.log`. These observations are recorded with their limits so future work can improve the renderer against honest baselines.

## Incremental painting checkpoint, 27 September 2026

On this host, refreshing a 16×16 patch in a three-layer 2048×2048 document measured 0.018 ms median with the regional compositor versus 129.385 ms with the full compositor, over twenty samples. The benchmark uses three translucent raster layers and measures compositing only: paint mutation, first-write copy-on-write cost, display conversion, GPU upload and event-to-screen latency are excluded. It is not a whole-application speedup claim. The unchanged full-frame identity case measured 9.727 ms and the transformed case 186.732 ms.

All 335 automated application tests passed, including nineteen new damage, pixel-equivalence and frame-lifecycle tests. The native Wayland journey passed thirteen checks, including a new comparison of the in-progress stroke preview against the full renderer before mouse release. Full optional RAW, PSD and subject-inference fixtures were configured for this local run. Evidence is retained in `rust/evidence/region-validation.log`, `region-benchmark.log` and `region-native-20260927/`.

Cua Driver 0.29.1 captured a disposable baseline app window, but background and foreground input were refused; the Omarchy input plugin was not active in the current login. Those screenshots are independent visual inspection only. The successful interaction evidence above comes from the native GPUI harness, not Cua or physical hardware input.

## Display conversion checkpoint, 27 September 2026

Across eighty 24×24 edits on a 2048×2048 composite after eight warmups, regional display conversion measured 0.129 ms median versus 4.843 ms for the previous full-image clone/channel-swap path. Creating every tile measured 5.017 ms, and comparing an unchanged refresh measured 1.871 ms with no replacement images. The eighty regional updates created 25,028,064 bytes across 94 tile images, versus 1,342,177,280 bytes for the whole-image path: 98.1% fewer new display bytes. These are CPU conversion/allocation measurements, not GPU timing or end-to-end painting latency. The raw log is `rust/evidence/display-benchmark.log`.

The initial tile converter was rejected after measuring 22.301 ms for a full rebuild and 10.942 ms for an unchanged refresh. Bulk row copies and contiguous channel comparison removed that regression before installation. Full tile creation remains close to the old conversion cost; the main benefit is limiting work for regional updates and retaining unchanged texture identities.

The 30-capture native probe on this 1.6-scale Wayland display passed uniform-background color, derived-alpha and strict seam checks at zoom 0.37, 1.0, 1.25 and 2.0, plus a stroke across tile boundaries. At 1.25 zoom (two physical pixels per source pixel), tiled and monolithic captures were pixel-identical. The only uniform-background outliers were two pixels at 0.37 zoom in each matte, out of 3,388,792 compared pixels. All five derived-alpha comparisons and all seam strips had zero outside-envelope pixels; the source-alpha-175 checks were supported and passed. Checkerboard comparisons retained 481 and 1,431 outside-envelope pixels at 0.37 and 2.0 respectively, exposing the sampling limitation rather than treating that metric as a valid alpha-compositing invariant. Evidence is in `rust/evidence/display-native-mattes-20260927/`.

The candidate passed 345 automated tests with real RAW, layered PSD and ONNX fixtures, the synthetic save/reopen/export journey, thirteen native Wayland interaction checks and seven installer regressions. Ten additional tests cover display bytes and halos, regional invalidation, shared snapshots, terminal strips, GPU image retirement and unpainted previews. Logs are retained in `rust/evidence/display-validation.log`, `display-native-final-20260927/` and `display-installer-tests.log`. This does not establish physical-input acceptance, other display scales or Mac parity.
