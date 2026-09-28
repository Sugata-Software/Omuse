# Photo editing hardening

28 September 2026. This pass prioritises ordinary photo editing and file
integrity on Linux. Tablet support is outside this release pass. The public
binary gates in [release readiness](public-release-readiness.md) still apply.

This record covers the `f066238` baseline. The subsequent
[real-photo qualification](photo-release-qualification.md) records RAW colour,
16-bit import and TIFF corrections, independent photo comparisons and measured
large-document limits.

## Corrections

- Feathered selections now blend destructive filters, blur/sharpen and content
  fill by coverage. Previously some paths treated every selected pixel as
  fully selected, producing a hard edge. Undo retains the original pixels.
- Mask erasing follows detached mask placement. Invalid opacity is rejected.
- Painting with Blend If now produces the same incremental preview as a full
  render, including when painting the backdrop beneath the affected layer.
- Layer import checks document limits before changing artwork or undo history.
  Text, shape, clipboard, RAW and generated-image callers report rejected
  insertion instead of claiming success. Assistant plans fail atomically.
- Flattened 8-bit RGB PSD files open as an editable merged image instead of an
  empty document. Raw, PackBits, ZIP and ZIP prediction are covered. Ambiguous
  spot channels are not silently interpreted as alpha. Unsupported PSD modes
  remain explicitly unsupported.
- Image decoders and PSD sections/decompression have explicit bounds.
- PSDs with a nonempty embedded ICC profile are rejected with conversion
  guidance. Previously the profile was silently ignored. Editable tagged-PSD
  color management remains unsupported; export a color-managed PNG/TIFF or
  save an sRGB PSD copy without its profile. Untagged PSD pixels are assumed
  sRGB. Converting individual layers would not preserve all blend/adjustment
  results, so this pass does not pretend to support that conversion.

## File operations and responsiveness

Ordinary photo open, import and export run on background workers. Cancellation
keeps an admission slot until the worker finishes, preventing repeated
Open/Cancel from accumulating decoders. Drag-and-drop accepts up to 16 images;
the batch commits as one undoable change, or leaves the original artwork intact.

Exports encode to a private staged file. A shared publication gate makes
cancellation and the final rename mutually exclusive: cancellation either
retains the previous destination or reports that publication already finished.
Failed encoding removes staging files.

JPEG preview has its own request identity. Changing preview fields cannot
strand a running export, and a late preview cannot populate a reopened dialog
or replace an active export's status. Preview workers retain their admission
slot until they finish.

Single-canvas saves now use the background snapshot path. Edits made while a
save runs remain dirty. A pending New/Open/Quit waits for the save; a failed
save returns that pending action to the unsaved-work dialog. Save completion
does not dismiss a newer dialog or steal focus from a new inline text edit.
Cancel and Escape cannot dismiss the Save dialog while a write is active. The
unsaved-work dialog disables Discard during that write and offers Keep editing
to cancel only the pending navigation; the save still completes.

Package identity is checked before and after opening, before reading lazy
pages/resources, and immediately before publishing a save. Identity includes
Unix inode and change timestamps, so replacing a package with equal-size files
and preserved modification times cannot bless stale content as current. Failed
lazy reads do not populate shared caches. Bulk package save/traversal uses one
source-check session to avoid repeatedly walking the whole package.

Normal, integer-aligned photo composition uses at most four row workers for
large surfaces. Small surfaces remain serial. Opaque rows can copy directly and
transparent rows can be skipped. The parallel and shortcut paths must match
the general renderer byte for byte.

On this development host, the final paired run of the existing synthetic
three-layer 2048x2048 compositor benchmark improved from a median
**126.166 ms to 48.669 ms** over 20 samples (about 2.6 times faster). An earlier
pair measured 130.509 ms to 39.171 ms (about 3.3 times faster); timings vary with
host load. Both final benchmark executables are retained with their checksums.
No compiler was running during these samples. This is CPU composition timing,
not whole-application latency; smaller images and transformed-layer cases
do not establish a comparable speedup. Byte-equality regressions cover clipped
edges, varied alpha, partial opacity, serial execution and four row workers.

Related hardening rejects duplicate CSV targets and over-budget bulk projects,
cancels stalled media probing, bounds AI response accumulation, and treats
uncertain provider submissions as unknown outcomes rather than retrying them.
No live or billable provider request is part of this pass.

## Verification

The release test suite passes **687 tests**: 313 library tests, 172 UI tests and
202 integration tests. Three manual timing tests remain excluded from the
ordinary suite; the compositor benchmark above was run separately. The
disposable edit/save/reopen/export journey, six-page Create export (including
PNG/PDF/MP4/GIF) and all 80 editable template variants also pass. Installer
regressions pass 11 tests; license-inventory tooling passes five tests.

The advanced recovery qualification passes six sustained revisions on a
768x512 document with three editable 16-bit layers. SIGKILL during revision 3
leaves project revision 2 intact and recovery revision 3 intact, with verified
pixel fingerprints and editable assets. This checks process interruption, not
power loss.

The normal production executable passes the native editing journey on Wayland
and XWayland. The XWayland run uses an 800x600 viewport and the light theme;
the Wayland run uses the system theme. Both captured windows were inspected.
The installed Omuse Preview executable matches the qualified binary, and its
editing self-test passes when launched through `omuse-preview` from `/tmp`.
The previous preview executable is retained as `omuse.previous`; the separate
original Omuse executable still has its pre-pass checksum. Existing app
windows were left open; relaunch Omuse Preview to use the updated executable.

The regression journey in `rust/tests/photo_workflow.rs` checks imported pixel
values, selected adjustment, editable mask, crop/resize, undo/redo, package
reopen, exact PNG/TIFF output and bounded opaque JPEG output. Additional tests
exercise failed/cancelled asynchronous operations, save revisions, external
package replacement, malformed PSDs and publication races.

The native journey also exercises the actual background photo Open/Export
dialog paths with a generated raster, adjustment, crop, resize and undo/redo.
It dispatches GPUI events within a native window; it does not qualify physical
keyboard/pointer delivery through the compositor.

Reproduce the automated suite with:

```sh
CARGO_BUILD_JOBS=2 OMUSE_TEST_KEEP=1 scripts/test-rust.sh
```

All test artwork and app state use isolated test/evidence directories. Existing
user documents and the older application remain available. This is local
qualification, not a completed clean-machine or physical-input release run.
Tagged PSD color management, independent macOS interchange fixtures and the
other public-release gates remain explicitly unqualified.
