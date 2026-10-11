# RAWmakase-inspired photo development roadmap

Approved direction, 10 October 2026. This is a phased Omuse plan, not a release
announcement or a claim that the following work is complete. Omuse keeps its
native Rust/GPUI interface, Omarchy themes and shared photo/vector `.omuse`
workspace. The purpose is creating finished content; calendars, scheduling and
social publishing remain outside scope.

The reviewed upstream source is
[`pch/rawmakase` at `e64044c`](https://github.com/pch/rawmakase/tree/e64044cafd2b3bb2127d158017abeda66db5e3aa),
whose package declares version **0.2.2**. The
[versioned release](https://github.com/pch/rawmakase/releases/tag/v0.2.2) and
pinned source are separate references; this document does not equate later
source changes with that release. Upstream feature descriptions and test
reports are not Omuse execution evidence or comparative benchmarks.

## Phase 1 — Colour regression and progressive previews

**First slice included in Omuse 0.10.0; the full phase remains open.** The
repeatable colour corpus and sampled Camera Raw drafts have the exact-source
checks in the [release qualification](release-0100-qualification.md). Earlier
focused/full-suite tests, a bounded CPU measurement and native Linux inspection
retain their own source identities in the [development record](photo-preview-qualification.md).
Publication does not turn CPU-stage timings into end-to-end or GPU benchmarks.

Initial corpus review exposed legacy near-black colour spikes and nonmonotonic
shadow/highlight ramps. The first bounded correction is versioned `SmoothV1`
tone mapping for new edits, with explicit legacy rendering compatibility.

- Keep synthetic charts, alpha ramps, gradients, saturated colours and retained
  precision fixtures alongside licensed photographic holdouts. Record input
  hashes, reference provenance, colour spaces, operator versions and tolerances.
- Separate regression against Omuse's approved output from agreement with an
  independent reference. A self-generated snapshot is not colour-accuracy proof.
  Review any changed baseline and its visible effect before accepting it.
- First progressive drafts sample original pixels and use the same per-pixel
  colour operations on admitted simple documents. They refine automatically
  through the full-resolution path; spatial effects and unsupported documents
  retain exact rendering. Never average pixels before nonlinear grading and
  call the result equivalent. Label temporary aliasing/limited detail honestly.
- Keep full-resolution Apply, selection/transform semantics and sampler inputs.
  Scopes wait for the appropriate full-detail result. Cancel, newer requests,
  document changes and failures must never publish stale or partial artwork.

**Acceptance:** reviewed corpus differences; exact admitted sampled-pixel
equivalence; preview optimization leaves full Apply/export unchanged for the
same versioned recipe; one Undo; cancellation and stale-job
checks; native draft-to-detail inspection; measured first-feedback and settled
latency plus peak memory. The first local CPU measurement covers sampling and
grading only, and native memory observation covers one photo/history journey.
End-to-end latency, worst-case memory and broader photographic qualification
remain open. Before calling the interaction polished, keep the photo visible
while adjusting controls, prevent preview refresh from shifting them, and
qualify Escape with numeric-control focus in the native window.

## Phase 2 — Retained RAW and versioned high-precision recipes

**Planned.** Extend existing retained originals into one explicit development
contract shared by preview, editable layers and export. Preserve source
identity, decode choices, orientation, camera/working colour space and precision.
Version operator semantics; old projects must retain their prior appearance.
Make an 8-bit conversion a deliberate operation with a retained original.

**Acceptance:** supported RAW/16-bit fixtures survive save/reopen and recipe
re-evaluation; full-detail preview and export follow the same declared pipeline;
alpha, masks, transforms and highlight behaviour have independent reference
checks. Refuse unsupported recipes, missing sources and resource overruns
without damaging the document. Test old-reader behaviour and rollback copies;
unchanged container numbers alone do not establish backward compatibility.

## Phase 3 — Native AI editing commands and optional MCP

**Planned extension of Ask Omuse.** Expose typed native editing commands through
one validated interface shared by the in-app assistant and an optional local
MCP adapter. Keep generation, provider selection, review and control in Omuse.
Protocol access does not grant a provider subscription or image capability.

Require explicit local enablement, bounded schemas and operation budgets.
Bind each plan to its document, layer/page IDs and revision; preview changes,
then Keep as one reversible transaction. Reject stale targets and duplicate
commits. Keep arbitrary shell execution and unrestricted file writes outside
the editing interface. Export requires a chosen destination and overwrite
policy; a tool response cannot authorize another operation.

**Acceptance:** wrong-target, stale-revision, cancellation, malformed-command,
replay and resource-limit cases leave artwork intact. Read-only inspection and
editing have distinct permissions. Provider failures preserve local work;
subscription/API costs remain visible with no silent provider or billing switch.

## Phase 4 — Qualify local click and box segmentation

**Planned qualification and integration.** Evaluate point/box prompting as a
complement to existing subject selection, using local inference and editable
mask coverage. Keep positive/negative prompts, refinement and undo within the
usual canvas. Preserve the source; a model prediction is a starting mask.

**Acceptance:** representative hair, transparent objects, products, low contrast,
multiple subjects and empty targets receive independent visual review. Verify
coordinates on rotated/scaled layers, feathered edges, repeatability, source
replacement, cancellation, bounded memory and missing-runtime/model errors.
Saved masks must render without rerunning inference. Verify model licence,
redistribution rights, download hashes and actual supported CPU/GPU providers.

## Phase 5 — Camera/profile and preset interoperability

**Planned.** Add explicit camera/profile identification and bounded DCP/XMP or
other compatible profile support behind Omuse-owned adapters. Import XMP
presets with a readable mapping report: applied, approximated and unsupported
settings. Preserve profile identity/version and report fallbacks rather than
silently claiming a camera-matched look. Do not bundle proprietary profiles
without redistribution rights; importing a user's licensed profile is separate.

**Acceptance:** chart and real-photo comparisons cover each supported transform;
malformed/oversized profiles fail safely. Missing profiles, unsupported process
versions, crop coordinates and local adjustments produce explicit reports.
Preset application is reversible, saves its supported semantics and never
rewrites the source sidecar. Broader Lightroom/Camera Raw parity is not implied.

## Phase 6 — Optional catalog migration

**Later, optional.** Consider read-only import of selected catalog metadata,
ratings, keywords, collections and supported edits after the editing contract
is stable. Preserve Omuse's direct document workflow; a catalog is not required
to edit or export. Missing-file relinking and duplicate handling need previews.

**Acceptance:** import works from a consistent snapshot, preserves the source
catalog/photos byte for byte, reports unsupported edits and can be cancelled or
repeated without duplicate damage. Test offline files, large libraries, path
relocation and atomic destination publication before claiming migration support.

## Hardware, architecture and adoption gates

For every performance claim, retain exact source/build, fixture hashes, CPU,
RAM, GPU, driver/backend, viewport/DPI and working dimensions. Compare cold and
warm runs, first feedback, settled output, peak memory and cancellation latency
on the same host. CPU results do not qualify GPU execution. Test GPU output
against the CPU reference and force the fallback; unavailable hardware is an
unrun gate. GPUI texture presentation is not evidence of GPU image processing.

RAWmakase uses egui/eframe; Omuse uses GPUI. Its
[domain separation](https://github.com/pch/rawmakase/blob/e64044cafd2b3bb2127d158017abeda66db5e3aa/docs/architecture.md)
is useful architectural input, not a drop-in UI. Treat internal crate APIs and
control schemas as unstable integration surfaces: pin revisions, use narrow
adapters and test migrations before upgrades. Prefer independent bounded work
over importing a second application or bypassing Omuse's history/lifecycle.

RAWmakase's [MIT licence](https://github.com/pch/rawmakase/blob/e64044cafd2b3bb2127d158017abeda66db5e3aa/LICENSE)
requires retaining its notice when applicable code is borrowed. Its
[dependency notices](https://github.com/pch/rawmakase/blob/e64044cafd2b3bb2127d158017abeda66db5e3aa/README.md#license)
also identify separately licensed SDK data, fonts, icons and native libraries;
review the actual files and model rights before reuse. No upstream code is
copied by this documentation change. Record implementation, local evidence,
hardware qualification and release publication separately in the
[project guide](project-guide.md) and [photo/vector roadmap](photo-vector-roadmap.md).
