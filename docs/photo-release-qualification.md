# Photo release qualification

This opt-in release pass uses pinned external photographs and separate decoder
implementations to check ordinary photo work. It complements the small synthetic
regressions in `rust/tests/photo_workflow.rs` and `photo_import_regressions.rs`.
It does not run on personal artwork or require a provider account.

## Corrections found by this pass

- Ordinary Open/Import now retains 16-bit PNG/TIFF samples as an editable
  source, including orientation and ICC conversion. RAW import retains both
  the developed 16-bit source and the original camera file. Painting requires
  an explicit conversion to 8-bit pixels; the open/import status explains that
  choice, including mixed batches where the last incoming layer is 8-bit.
- RAW white balance now preserves all four camera gains and adjusts both
  green sites consistently. Nikon D90 and Canon EOS 450D metadata uses green
  gains of 256 and 1024 respectively; the previous hardcoded second-green gain
  of 1 produced a gross magenta cast and noisy detail. Visual inspection caught
  this despite successful round trips. The new independent reference rejects
  both old outputs and matches the corrected outputs exactly.
- TIFF exports explicitly declare unassociated alpha in `ExtraSamples`, for
  both 8-bit and 16-bit data. Independent decoders previously warned about
  the omitted alpha declaration. Partially transparent precision fixtures now
  exercise the corrected output.
- Tagged sRGB 8-bit exports no longer drift on reimport through mismatched
  in-memory versus serialized destination profiles. TIFF profile extraction
  seeks only the header/directory/profile instead of reading the whole file
  into a second allocation.
- PNG uses a faster lossless compression setting. This trades some file size
  for lower export latency without changing decoded samples.

## Recorded photo and size results at `ac8874d`

On 28 September 2026 all **16 photo cases** completed successfully. The two
camera outputs matched every 16-bit sample of their independent reference
developments; both corrected photographs were also inspected visually.
Lossless output was exact, alpha declarations produced no decoder warnings,
and JPEG export PSNR stayed above 50 dB in this corpus.

All three synthetic size runs completed 100 brush strokes, exact retained
undo/redo, native `.omuse` save/reopen, PNG export and atomic rejection of
excess layer pixels. These are short, two-layer CPU workflows on this host:

| Canvas | Peak process RSS | Median brush stroke | 95th percentile | Undo steps retained at 256 MiB |
| --- | ---: | ---: | ---: | ---: |
| 12 MP | 419 MiB | 21.4 ms | 112.6 ms | 5 |
| 24 MP | 582 MiB | 50.6 ms | 221.7 ms | 2 |
| 48 MP | 1169 MiB | 301.0 ms | 478.2 ms | 1 |

These size measurements describe the snapshot-only history in `ac8874d`.
The subsequent [region-history qualification](large-photo-history-qualification.md)
records the implemented tile history and fresh paired measurements. Preserve
this table as the historical result of the photo-import pass. Bounded memory
and correct pixels do not imply smooth large-photo interaction.

A separate paired export used the same 4290×2856 16-bit image in both builds.
PNG export took **16.405 seconds before and 3.685 seconds after**, about 4.45
times faster in that one pair. Decoded outputs matched exactly; file size grew
from 71,045,373 to 75,001,702 bytes, about 5.6%. No compiler or other test workload
ran during the pair. This is one image and one sample per build, not a general
application speedup or a statistical benchmark.

Local receipts are retained under `rust/evidence/photo-qualification-final-20260928`,
`photo-stress-20260928` and `png-export-20260928`. The corpus receipt includes
the exact runner snapshot used for its completed run. The runner subsequently
gained an explicit completion field so an interrupted partial run cannot be
mistaken for a passing whole corpus.

## Automated regression and content acceptance at `ac8874d`

The full `scripts/test-rust.sh` run exited successfully with **700 passing
tests**: 318 library, 174 GPUI/UI and 208 integration tests. Three explicitly
manual timing benchmarks were excluded. The real Nikon RAW fixture and pinned
LibRaw runtime were enabled, including the exposure-control and retained-source
regressions. The new oversized 16-bit import rejection and mixed-precision drop
guidance regressions passed.

The disposable editing journey passed painting, undo/redo, save/reopen,
adjustment, gradient and PNG/JPEG/WebP/TIFF output. The Create journey retained
an editable six-page collection and resized story, and produced six PNG pages,
PDF, MP4 and GIF. All **80 editable template variants** passed their native
text/pixel persistence and layout checks. These are local bounded acceptance
workflows, not physical desktop input or broad design-quality qualification.

The final log, 256 build-input hashes, test result summary and acceptance
artifacts are retained in `rust/evidence/photo-release-20260928`. Six notice
inventory tool tests also passed; the two unresolved dependency legal texts
remain a separate binary-distribution gate.

## Production desktop and preview installation at `ac8874d`

The normal locked release build (without the `ui-test` feature) passed both
native desktop journeys: Wayland with the system theme and XWayland with the
light theme at the 800×600 logical minimum. Both runs exercised background
photo open/export, crop/resize, undo/redo, retained 16-bit import/export,
editable filters, masks, live text/effects and project save/reopen. Captures of
both owned test windows were visually reviewed. Events are injected through
the in-process GPUI test path; these are not physical Cua input receipts.

The executable SHA-256 is
`90238c85870bef2f38d108255fd1b723f62e42eef5b5fdc2a9aff7fdbf05d9db`.
The `omuse-preview` installation has the same hash and passed its isolated
editing self-test with `/tmp` as its working directory. The RAW/ONNX runtime
libraries and subject model match the prepared assets exactly. Both prior
preview executables were preserved in versioned rollback files before the
installer updated `omuse.previous`; the separate original installation was
left unchanged.

`native-wayland/verified-native.json`, `native-xwayland/verified-native.json`,
`installed-preview.json` and `rollback.json` under the final evidence directory
record those identities and results. This development-host installation does
not close the clean-machine installation or public binary-release gates.

## Sources and reference comparisons

[`photo-sources.json`](../rust/tests/fixtures/photo-sources.json) pins download
URLs, byte lengths, SHA-256 hashes and source licence evidence for:

- NASA's Eileen Collins portrait from the scikit-image sample collection,
  [documented as public domain](https://scikit-image.org/docs/stable/api/skimage.data.html#skimage.data.astronaut).
- Nikon D90 NEF and Canon EOS 450D CR2 photographs whose individual entries in
  the [RAW sample catalogue](https://raw.pixls.us/) are marked CC0. Their original
  catalogue rows and source checksums are retained in the manifest.

Photo fixtures retain their own terms; the application's MIT licence does not
relicense them. Large original files are downloaded into ignored evidence,
not stored in Git. The suite makes portrait JPEG, grayscale JPEG, three EXIF
orientations, transparent PNG, tagged sRGB and linear-RGB PNG/TIFF, and PNG/TIFF
opaque and partially transparent precision variants. The precision variants
deliberately contain nonzero low bits; they are test derivatives, not
camera-native 16-bit photographs.

Each application journey checks import, project save/reopen, selected brightness
boundaries, undo/redo, selection masks, crop, resize and raster export. Retained
16-bit sources are checked before an explicit conversion to pixels for the
destructive editing portion. Original RAW bytes remain embedded in the project.
The standard export path is 8-bit; explicit 16-bit PNG/TIFF export is tested
separately. Ordinary 8-bit imports stay on the lightweight raster path.

Pillow independently checks imported orientation/colour and lossless output.
ImageMagick checks retained and exported 16-bit samples. These tools use different
image decoders from Omuse's Rust image crate. The colour comparison uses
LittleCMS through two independent wrappers, so it is not an independent audit
of LittleCMS itself. RAW output must match an independently configured LibRaw
C++ development using camera white balance, including every retained 16-bit
sample. The reference bypasses Omuse's FFI and multiplier handling but uses the
same pinned LibRaw engine; it does not independently audit LibRaw's camera
colour science. The C++ reference selects camera white balance directly; Omuse
supplies the camera gains through LibRaw's C API. LibRaw can choose different
matrix or fallback paths for other formats, so exact agreement for these two
cameras is not proof that the configurations are equivalent for every camera.
The independent decoder must also accept TIFF output without
warnings about its alpha-channel declaration.

Lossless output must match exactly. JPEG import permits a maximum channel error
of four and mean error of 0.5 between decoders. Tagged colour conversions permit
one byte of rounding. JPEG export at quality 100 must have mean channel error at
most 2.5 and PSNR at least 35 dB against the explicitly matted reference.

## Reproduction

Build the examples with the locked Rust graph:

```sh
cargo build --manifest-path rust/Cargo.toml --release --locked --jobs 2 \
  --example photo_qualification --example photo_stress
```

The photo runner requires Python with Pillow and NumPy, ImageMagick and
LittleCMS. Its external downloads require an explicit `--fetch`. Subsequent
runs validate cached source hashes and work offline. Use a fresh evidence
directory; old application outputs cannot count as a passing rerun.

```sh
python3 scripts/photo-qualification.py rust/evidence/photo-candidate \
  --fetch --raw-library /absolute/path/to/libraw.so \
  --raw-reference-executable /absolute/path/to/raw-photo-reference
```

The RAW library is the optional pinned runtime prepared by
`scripts/prepare-rust-assets.sh`; its path must be supplied when it is not next
to the example executable. No missing optional backend is reported as tested.
The RAW reference is required for the two camera cases. Build
[`raw-photo-reference.cpp`](../scripts/fixtures/raw-photo-reference.cpp) with
the headers in the checksum-verified `LibRaw-0.22.2.tar.gz` prepared by that
script. For example, with the extracted headers in a fresh reference directory:

```sh
g++ -std=c++17 -O2 -Wall -Wextra -Wpedantic \
  -I /absolute/path/to/reference/LibRaw-0.22.2 \
  scripts/fixtures/raw-photo-reference.cpp \
  -L /absolute/path/to/runtime/lib '-Wl,-rpath,$ORIGIN' -lraw \
  -o /absolute/path/to/reference/raw-photo-reference
ln -s /absolute/path/to/runtime/lib/libraw.so \
  /absolute/path/to/reference/libraw.so.25
```

The pinned library has SONAME `libraw.so.25`. The reference runner records the
helper, source, input and LibRaw hashes and writes a fresh reference image for
each camera case. Its C++ helper caps both sensor and output dimensions at
12.5 megapixels, independently of Omuse's import limit.

## Large images and bounded history

The stress runner uses only Python's standard library and runs the three sizes
sequentially in separate processes:

```sh
python3 scripts/photo-stress.py \
  --input rust/evidence/photo-candidate/sources/astronaut.png \
  --evidence rust/evidence/photo-stress-candidate
```

The photograph is resized and tiled into synthetic 12, 24 and 48-megapixel
two-layer documents. The runner checks repeated brush edits, 256 MiB history
limits, metadata edits sharing pixel allocations, undo/redo, full composition,
save/reopen, PNG output and rejection of excess layer pixels. It records CPU
timings, current/peak process RSS, pixel fingerprints and file hashes. Each
child has a 4 GiB address-space ceiling, an actively monitored 3 GiB RSS limit
and a deadline. Those are harness safeguards, not a product memory guarantee.

The size fixture is not evidence of native high-resolution camera decoding,
physical input latency, GPU memory consumption or a multi-hour soak. High
precision editing retains its separate 16,777,216-pixel source limit. Oversized
16-bit/RAW sources are rejected explicitly instead of silently reduced to
8-bit. Ordinary 8-bit documents retain their existing limits.

## Release scope

This pass does not qualify physical keyboard/pointer delivery while the
Omarchy Cua plugin is inactive. It must be activated by a fresh desktop session;
the qualification does not restart or reconfigure a user's desktop.
Clean-machine installation, remote CI, independently reviewed camera colour and
macOS interchange, and the remaining gates in
[public release readiness](public-release-readiness.md) still require evidence.
Tablet support remains deferred.
