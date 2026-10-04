# OmaPhoto comparison and improvement plan

## Latest release review — OmaPhoto 1.4.5, 4 October 2026

[OmaPhoto 1.4.5](https://github.com/ZacharyZhang-NY/OmaPhoto/releases/tag/v1.4.5)
was published at **05:10 UTC / 13:10 Perth**. The annotated tag resolves to
`4c7d98b17f80c3cbaecc17ac75c27af74f2f703b`, also the observed main head.
This supersedes the earlier 01:16 UTC review below, when 1.4.5 was unreleased.

The [new range](https://github.com/ZacharyZhang-NY/OmaPhoto/compare/28f6337d13f402378d025850accce04026a6df2b...4c7d98b17f80c3cbaecc17ac75c27af74f2f703b)
contains **four commits and 34 changed paths**, including three new acceptance
programs, commit-continuation regressions, picker fixes and refreshed screenshots.
Five Linux packages are attached: Arch x86_64, Ubuntu 24.04/26.04 and Fedora 43/44,
plus SHA256SUMS. The [release workflow](https://github.com/ZacharyZhang-NY/OmaPhoto/actions/runs/37178698737)
has seven successful jobs. The workflow runs packaging, not ctest; those green
jobs do not independently confirm the separately reported distro test results.
There are now five tags and five releases; every
older note hash and asset fingerprint matches the preceding watch baseline.

### What matters for Omuse

The comparison uses **Omuse 0.8.0**, installed runtime
[`eb558dc`](https://github.com/Sugata-Software/Omuse/tree/eb558dc59ccdf88a3d62dfc2d706a6b836da1eda)
and the same Rust source in integration `4888b00`. The older dirty worktree
is preserved, not used to judge the current application's features.

| Area | OmaPhoto 1.4.5 and current Omuse position | Useful next work |
| --- | --- | --- |
| Native-resolution retouch | OmaPhoto's brush tools operate on layer-resolution rasters. Omuse already has carried Smudge colour, untouched-source Liquify and bounded Blur regions, but its Blur/Smudge/Liquify path composites at canvas resolution and maps back. Ordinary paint and Clone already use local layer pixels. | Preserve fine detail on scaled-down/rotated layers, with fractional coordinates, masks, soft selections and exact Undo. Give Blur an independent radius; do not mistake the 0.8 algorithm improvements for full resolution parity. |
| Grid and snapping | OmaPhoto adds spacing, subdivisions and style controls, plus guide/grid snapping for selections and shapes with temporary bypass. Omuse has grid/guide snapping in Move/transform, but `ui.rs:6261` displays 64-pixel lines below 100% while `ui.rs:8496` still snaps to 8 pixels. | Use one grid model for display and snapping; add settings and selection/shape coverage. The mismatch is source-derived, not a reproduced native-GUI failure. |
| Masks and layer folders | OmaPhoto adds Alt-click mask-only inspection and ordinary folder Ungroup. Omuse already preserves soft selection coverage, PSD mask ground and growing/linked masks. It has vector Ungroup and layer Unnest, not an equivalent folder-dissolve command. | Add mask-only inspection and an ordinary Ungroup that preserves order, placement, locks and mask/effect semantics or explains unsupported cases. |
| Typography | OmaPhoto previews font choices on selected letters and restores the draft when the menu closes. Omuse supports per-range fonts/sizes through Rich Typography and live text rendering. | Make selected-letter font changes direct, with mixed-style feedback and reversible hover preview. Escape, outside-click and document changes must not lose later typing. |
| Curves and blending | OmaPhoto changes Camera Raw curves, Refine Saturation, Soft Light and clipping toward its Photoshop reference. Omuse's existing recipe and Soft Light formula differ. | Use independent reference pixels and version changed recipe semantics. Do not silently alter the appearance of saved artwork or claim Photoshop parity from release wording. |
| Selection, presets and JPEG | Colour-range selection, new-canvas presets and JPEG preview zoom feature in the release. Omuse already has these capabilities, including hue/luminosity ranges and encoded JPEG Fit/100% pan inspection. | Qualify interaction details; these are not missing whole features. |
| Retro finishes | OmaPhoto adds separate ASCII text-size control and Scanlines/CRT. Omuse has ten deterministic finishes, including ASCII, with cell-size controls. | Separate glyph size from cell size and add a distinct CRT finish; halftone lines are not the same effect. |
| Project navigation and previews | OmaPhoto adds draggable tabs/overflow and saves a QuickLook preview. Omuse has searchable recents and Create pages. | Design project tabs around dirty-document/recovery safety; add bounded saved previews and qualify Linux file-manager discovery separately. An embedded preview alone is not desktop thumbnail integration. |

Mask inspection was checked separately from mask editing: the current mask badge
(`ui.rs:5303`) selects the paint target without inspecting Alt, `refresh`
(`ui.rs:1055`) composites the artwork, and `canvas_view` (`ui.rs:6032`) selects
trace/vector/artwork displays without a mask-only route. The command catalogue
and mask controls have no isolation command. This supports the narrower missing
Alt-click/whole-canvas mask-view comparison; it does not question existing mask
painting or the subject-refinement preview.

Omuse's vector scope remains broader in the reviewed source: editable Pen/nodes,
image tracing, boolean construction, groups, gradients, advanced strokes, text
on curves and vector PDF. Its transparent-fill/stroked rectangle and ellipse
workflow already addresses the capability requested in new upstream
[issue #15](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/15). That issue is
open; it is not a feature shipped in OmaPhoto 1.4.5. Omuse's in-app AI and content
creation workflows are also a separate focus. Breadth does not establish
higher photographic quality or better performance.

### Stability and release lessons

The new [commit-continuation fix](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/8bd3fc3264d1d400b23b42cc2b6cc66614c35a77)
keeps a completed raster edit's follow-up work in the same event turn. Previously
the project could appear free while a gradient remained pending, or a chained
operation could read a newly selected layer. Omuse should test Apply followed
immediately by a new tool, layer selection, another command, Undo or close.
Its existing transaction/stale-result guards and Apply-to-keyboard-Undo tests
are useful coverage, not proof that this entire sequence matrix passes.

The [picker fix](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/c854e42116ff6c82b72d614de2c40ec8bd087ff7)
responds to newer Qt sending highlight signals while closed and dismissing a
popup without the old callback. Omuse uses GPUI, so the Qt patch is not a port
target. The portable lesson is to tie previews to visible popup lifetime and
restore state safely on every exit.

Pinned [TASKS](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/4c7d98b17f80c3cbaecc17ac75c27af74f2f703b/TASKS.md)
reports repeated parallel runs, six acceptance programs and 334/334 suites on
five distro checks. These are upstream reports, not tests executed here.
The original five parallel failures did not recur, but their cause remains
unknown; heavy 2-CPU/`-j32` stress still has timeout and one allocation-test
anomaly. Production test scripts remain serial. Passing suite counts are not
comparable to Omuse's individual test-case count.

The six acceptance programs exercise real controls and check pixels/state/files
under Qt offscreen; they are stronger evidence than screenshots alone. They
are still mostly synthetic scenarios, not native compositor, broad photographic
or hardware qualification.
At the release commit, fresh GitHub-package installs and the final checklist
remain unchecked. The live release/build evidence closes publication despite
the stale unchecked tag row; it does not close fresh-install acceptance.

Omuse's [0.8.0 release](https://github.com/Sugata-Software/Omuse/releases/tag/v0.8.0)
was checked again: it remains a source prerelease with **no permanent binary
attachments**, despite qualified CI archives. Public downloadable packages are
a clear delivery gap against OmaPhoto's release. Clean-machine acceptance,
signing and broader hardware remain separate gates.

The earlier captured-stroke endpoint defect and recoverable-memory work remain
planned and unfixed by this review. Prioritize them and native-resolution
retouch, then grid consistency/mask inspection, before cosmetic effects.
No app source, installed runtime, release tag or changelog was changed here.

### Watch scope

All five release/tag records, one branch, four PRs, eleven non-PR issues and ten
Actions runs were inventoried. The four PR records and previous ten issue
records are unchanged. OmaStore #14 and AppImage #8 remain open/unmerged;
neither is a shipped distribution channel. Upstream dependency locks, installer
and release recipes did not change in this four-commit range. The review is
source/test/release inspection, not an executed application comparison or
speed benchmark. Planned follow-ups remain visibly separate from shipped work.


## Historical source review — 4 October 2026, 01:16 UTC

The exact [6ccff8a → 28f6337 range](https://github.com/ZacharyZhang-NY/OmaPhoto/compare/6ccff8a9d79438d93ac7376341e5a54fbe6194e6...28f6337d13f402378d025850accce04026a6df2b)
contains **six commits and 49 changed paths**: 44 test files, one application
source file, two documentation files, `.gitignore` and `flake.nix`. The only
application change simplifies guarded document/mask reads in Blur sampling.
The Nix source filter now excludes the local Compositor reference checkout.
Dependency locks, release workflows, installer and distro recipes are unchanged.

**OmaPhoto 1.4.5 remains merged development work, not a published release.**
The four tags and four releases, including every release-note hash and asset
name, size, timestamp and digest, match the previous baseline. Latest remains
[v1.3.3](https://github.com/ZacharyZhang-NY/OmaPhoto/releases/tag/v1.3.3).
There is one active branch, four PRs and ten non-PR issues; issue metadata and
all nine Actions run records are unchanged. PR #14 is the new proposal below.

The upstream [status record](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/28f6337d13f402378d025850accce04026a6df2b/TASKS.md)
now reports a fresh build, targeted tests and mutation checks, and 330/330
suites at `-j8` before the last gesture-test commit. These are the author's
reports, not independently executed results or a new Actions run. Full `-j16`
isolation, acceptance screenshots, distro checks, packaging and live installs
remain unchecked in that record. The new tests use checked optional access,
correct two stale selection-mask expectations, exercise allocation failures,
and distinguish a second click from the first rather than clicking the same
position twice. No performance was measured here.

### Comparison with the current Omuse release

This review uses the clean integration source at `4888b00` before documentation
updates, with the same Rust tree as public runtime
[`eb558dc`](https://github.com/Sugata-Software/Omuse/tree/eb558dc59ccdf88a3d62dfc2d706a6b836da1eda).
The normal installed command and source receipt report **Omuse 0.8.0**. The
older dirty `feature/photo-vector-studio` checkout is preserved; its source
state is not treated as the released implementation.

Several high-priority findings from the historical 3 October review are now
implemented in 0.8.0: PSD Levels gamma/channel ranges and explicit mask ground,
soft selection-to-mask coverage, external format-11 font runs, retouch carry
and untouched-source Liquify, bounded Blur work, and Fit/100% encoded JPEG
inspection. The [0.8 qualification record](https://github.com/Sugata-Software/Omuse/blob/8f7526f1acee22de7068e143e65a7c4541c88b0c/docs/release-080-qualification.md)
retains its own 1,352-test, photo and native evidence. No application test or
benchmark was rerun during this scheduled review.

| Priority | Concrete Omuse follow-up and current evidence |
| --- | --- |
| Fix the captured-stroke endpoint limit | In `rust/src/ui.rs:1740`, pointer collection stops at 100,000 points, but mouse release appends another at line 1863. `rust/src/editor.rs:3555` rejects more than 100,000. A retouch stroke reaching that cap is therefore rejected on release. This is a source-derived defect, not an interactive reproduction. Reserve an endpoint slot or replace the last captured point; test 99,999/100,000 boundaries, endpoint retention and unchanged pixels/history on refusal. Inspect Spot Heal's analogous capture path too. |
| Make memory-pressure failures recoverable | The new upstream [WarpFailureTests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/28f6337d13f402378d025850accce04026a6df2b/tests/WarpFailureTests.cpp) pin failed Smudge carry, failed Liquify offsets, and scratch failure followed by recovery. Omuse already bounds pixels/work and commits successful results atomically, but `retouch_brush.rs:415,470,485` still allocates through `vec!` and `extend_from_slice`, with image clones elsewhere. Its core API accepts a 4096-pixel brush whose carried-colour buffer alone is about 256 MiB; this is an allocation estimate, not observed RSS or a reproduced crash. Add derived-buffer admission, fallible allocation and isolated failure injection proving the document and Undo remain intact. |
| Exercise actual focus transitions | [MoveBarPressTests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/28f6337d13f402378d025850accce04026a6df2b/tests/MoveBarPressTests.cpp) now types X then clicks Y without a canvas press. Omuse's numeric handler commits changed values on Blur and guards editor/layer identity; its visible typing test covers Enter and leaving an unchanged value. Add changed-value field-to-field clicks, immediate canvas presses, stale targets and exactly one Undo for artwork values. This is a coverage gap, not a reproduced input bug. |
| Strengthen gesture boundaries without copying unrelated mechanics | Omuse already cancels lost-button scrubs and stale Camera Raw gestures. Add successive curve clicks at different positions and direct path-walker tests for exact spacing, duplicates and corners. Upstream tab-drag and committed-point thinning tests concern different workspace/history designs; Omuse commits raster retouch results, so those mechanisms do not need a literal port. |
| Qualify parallel tests separately | `scripts/test-rust.sh` gives the run disposable XDG directories and uses `--test-threads=1`. Existing passing results do not establish concurrent settings isolation. Inventory mutable settings, isolate them per test/process, and run controlled parallel comparisons before changing CI concurrency. Upstream's `-j8` report does not close its own `-j16` gate or prove an Omuse speedup. |

The Blur cleanup needs no direct port: Omuse's mask-retouch path already selects
the mask explicitly. Translated/rotated mask Blur with black versus white
outside coverage is a useful additional fixture. Changed Camera Raw recipe
semantics, native-resolution retouch and broader photographic quality remain
separate work; old saved grades should retain their appearance.

### New store proposal and relocation lesson

[OmaStore PR #14](https://github.com/ZacharyZhang-NY/OmaPhoto/pull/14) is **open
and unmerged**, replacing closed/unmerged #12. Its only change is a 16-line
manifest for the existing x86_64 Arch package; the old manifest's meaningful
fields match, with an updated store URL in a comment. PR #8 remains open and
#10 remains closed/unmerged. Bot summaries do not establish a live listing.

The model-location concern in #14's review is supported by source inspection:
OmaPhoto uses `QStandardPaths::GenericDataLocation` for `omaphoto/u2net.onnx`.
OmaStore's pinned [extractor](https://github.com/KitsuneForgering/OmaStore/blob/9bcc8012b1ff121a61d93512be65652b769ec676/backend/internal/install/extract.go#L337)
preserves package `usr/` paths inside a per-version directory, and its
[launcher](https://github.com/KitsuneForgering/OmaStore/blob/9bcc8012b1ff121a61d93512be65652b769ec676/backend/internal/install/launcher.go#L18)
directly executes the relocated binary without adding a data search path.
Under ordinary data paths, a separately installed model could mask this
relocation gap. This review did not install either upstream application or
reproduce the store failure.

Omuse's `segmentation.rs:419` resolves its default runtime/model beside the
actual executable, and the earlier installed local-model journey has its own
receipt. Retain a clean-home, arbitrary-directory model/RAW/launcher test before
any store listing. Omuse's 0.8.0 release still has no permanent binary attachments
at this check; no listing, topic, release or package was published here.

Only local watch/comparison/guide documents changed. The integration guide is
the private dashboard's source so the installed 0.8.0 state is retained; the
canonical watch baseline is synchronized without replacing its older work.
Recommendations are planned work, not new shipped features or changelog fixes.


## Historical review — 3 October 2026, 01:10 UTC

OmaPhoto advanced from `802ae82` to
[`6ccff8a`](https://github.com/ZacharyZhang-NY/OmaPhoto/tree/6ccff8a9d79438d93ac7376341e5a54fbe6194e6):
**55 commits, 275 distinct changed paths** in the exact Git range. The full
path inventory includes 148 source files, 108 test files and eight kernel files.
Focused implementation and regression-test review covered editing, interaction,
file compatibility, export and packaging. No upstream code or installer was run.

**1.4.5 is now merged development work, still not a published release.**
The four tags and four releases retain identical note hashes and all asset names,
sizes, timestamps and digests; latest remains
[v1.3.3](https://github.com/ZacharyZhang-NY/OmaPhoto/releases/tag/v1.3.3).
There is one active branch and no new Actions run: all nine workflow records are
unchanged. The version bump in CMake, Arch and Nix metadata does not qualify a
release. Dependency lock, installer and workflow files did not change.

The upstream [status note](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/6ccff8a9d79438d93ac7376341e5a54fbe6194e6/TASKS.md#L460-L469)
explicitly leaves final review commits unbuilt/untested, reports five suites
failing together but passing separately, and leaves acceptance, distro checks,
packages and live installs pending. These are upstream reports, not failures
reproduced here. Existing screenshots are not fresh 1.4.5 acceptance evidence.

### Concrete fixes to prioritize in Omuse

These findings use the actual dirty `feature/photo-vector-studio` worktree at
`c902b72`, not an assumed release checkout. Installed Omuse reports **0.7.0**, with
public runtime receipt `5b3daefbb5258afff4a74a2ff3db5247b074972d`. The PSD parser,
external text normalizer and retouch engine are byte-identical in that runtime,
current public main `c2f2df7` and the inspected worktree. No application suite or
native editing journey was executed during this review.

| Priority | Source finding and next verification |
| --- | --- |
| High — PSD adjustment fidelity | `rust/src/psd.rs:1056` divides Levels gamma by 256; the format stores hundredths, so 100 should import as 1.0, not 0.390625. `hue()` at lines 1093–1100 always reads offsets 4/6/8, which are colorization values; ordinary version-2 master values begin at byte 10 and selective bands follow. This contradicts [Adobe's file-format specification](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/). OmaPhoto's [parser correction](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/cb85b86d589a862422b464dc94e4fe78b5eaeacb) and `PSDAdjustmentTests` supply useful cases. Correct version-aware parsing, retain or explicitly report selective-band limitations, and add independently authored PSD/PSB pixel references plus malformed-input cases. |
| High — PSD mask ground | Omuse discards the PSD mask default byte at `psd.rs:506`, then stores placement without `maskOutsideCoverage` at lines 815–820. Rendering infers the ground from border pixels (`effects.rs:892`), which can contradict the file's explicit default. Preserve the default; test white patches on black ground and the inverse, offset masks, adjustment masks, save/reopen and exact rendered edges. The same upstream commit adds `maskPatchSitsWhereItIsOnTheCanvas`; it is source evidence, not our executed fixture. |
| High — soft selection to mask | Omuse already creates masks from selections, but `Editor::add_mask` (`editor.rs:4430`) turns every selected sample into either 0 or 255 through `Selection::contains`, losing partial coverage. Keep actual selection coverage and invert it for Hide Selection. Add feathered and antialiased edges on rotated/scaled layers, and save/reopen/Undo cases. OmaPhoto's [mask change](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/0b0d36a7dffa20b0c67872e248724ae1232cf1e7) and `SelectionEditTests` explicitly cover soft edges. |
| High — external format-11 imports | OmaPhoto now writes ordinary version-11 projects with optional UTF-16 `text.fontRuns` ([format change](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/dcedfea53f5775d25dec1de658d2c6c432505678)). Omuse's `document.rs:845` requires a vector scene for version 11/12, so these ordinary projects are refused. `project_text.rs` converts only `colorRuns`; the existing format-10 regression intentionally rejects plain version 11. Separate external format semantics from Omuse vector extensions; validate and merge overlapping colour/font ranges into native runs, preserve cached pixels and the original file, and retain fail-closed vector-sidecar checks. A shared version number is not interoperability. |
| Retouch quality and speed | The previously identified Smudge carry/spacing and repeated Liquify resampling gaps now have a [CPU implementation upstream](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/cbd8612014568f633536321071f6856d675c80c8). Compare fading trails and repeated forward/back warps using untouched-source displacement sampling. Review bounded blur patches and clone tile reuse separately. Preserve alpha, soft selections, cancellation and Undo, and measure time/RSS before any speed claim. |
| Camera Raw compatibility | [New curve semantics](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/a77cce459861278a477150694f9aeaf72d01394b) add smooth parametric bends, per-channel master curves and different Refine Saturation behavior. Omuse still uses its older recipe. Add reference pixels and graph-gesture tests; version any changed saved-recipe meaning so existing grades keep their appearance. |
| JPEG inspection | OmaPhoto's [preview change](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/64ebd1cdf9208d21d4e085a288a1992d6f86ee96) adds Fit/100%, pan and remapped zoom keys. Omuse already previews genuinely encoded bytes with stale-result guards, but `ui.rs:7961` fits them into a fixed preview without inspection zoom and limits preview to 16 MP. Add bounded zoom/pan and a shared matte picker; keep encoded-byte, stale-result and export-independence regressions. |

Retouch resolution needs a specific comparison: ordinary paint and Clone already
use layer-local pixels in Omuse. Blur/Smudge/Liquify composite at canvas resolution
and map back (`editor.rs:3527`), so prioritize scaled-down detailed sources and
bounded blur regions. Broader native-resolution parity is not established by a
scaled-up-source Undo test. Levels also needs channel-rendering coverage:
`effects.rs:746` currently uses the master range only.

Other useful interaction work is configurable grid spacing/subdivisions/style,
broader resize/shape snapping with temporary bypass, mask-only inspection,
selected-text font hover previews and a distinct Scanlines/CRT finish. These are
not blanket feature absences: Omuse already has grid/guide snapping, colour-range
selection, rich text runs, vector grouping/ungrouping, ten Dither styles, inline text
Undo and pending-edit guards. Project-tab dragging is a workspace design choice;
Qt event-filter mechanics should not be copied into GPUI. Ordinary layer-folder
ungrouping is separate from vector ungrouping. A common close/quit matrix should
exercise text, gradients, floating pixels, transform drafts, modal jobs and a
save in progress through both Ctrl+Q and the window close button.

Do not mechanically replace Soft Light: upstream now uses a square-root bright
branch while Omuse's 8/16-bit renderers use the piecewise cubic/square-root
variant. Independent reference pixels and an explicit compatibility decision
should come before changing saved artwork. Upstream clipping-stack, adjustment,
canvas and export consistency tests are useful regression designs, not proof
of either editor's superiority.

### Repository proposals and qualification signals

- [Issue #13](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/13) reports RAW
  files missing from File → Import while drag/drop works. The upstream picker
  already includes RAW globs from the system MIME database; cause is unconfirmed.
  Omuse's native picker sets no image-extension filter and its RAW detector is
  case-insensitive. Add paired chooser/drop checks for upper/lowercase extensions
  and Unicode paths on a clean target; do not label this a reproduced Omuse bug.
- [OmaStore PR #12](https://github.com/ZacharyZhang-NY/OmaPhoto/pull/12) is now
  **closed without merging**. No manifest landed on main. AppImage PR #8 remains
  open; #10 remains closed/unmerged. Ten non-PR issues were inventoried; only #13
  is new or changed since the baseline.
- The five upstream parallel-test failures are a reminder to isolate mutable
  settings per test/process. Omuse's passing serial CI does not prove its tests
  can safely run concurrently. Test parallel isolation before using it to claim
  faster validation.
- `QuickLook/Preview.jpg` is a new optional upstream save artifact, bounded to
  a 1024-pixel long edge and omitted for large/unrenderable canvases. On Linux,
  prioritize file-manager thumbnails and Omuse's recent-project gallery rather
  than treating a Finder-specific folder as required parity.

Only this comparison, its reviewed baseline and the project guide were updated.
Recommendations are not changelog entries for shipped fixes. No source commit,
release, repository setting, desktop configuration or installed app was changed.

## Historical review — 1 October 2026, 01:12 UTC

Since `e94fd73`, OmaPhoto has two new default-branch commits. The first,
[`39dcbc7`](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/39dcbc770568af3772c1ee44a6dfaa883e3052c2),
changes `AGENTS.md` and `TASKS.md`: a survey and planned port through Compositor
1.4.5. The second, [`802ae82`](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/802ae82f72be1b6799098d521d940c4fa2e34407),
adds README guidance for garbled NVIDIA/Hyprland cursors and marks that
documentation task complete. **No application, test, dependency, build or packaging
implementation changed on main. OmaPhoto 1.4.5 is a plan, not a release.** The
four tags, four releases (including note hashes and every asset's metadata),
existing PRs and nine workflow results match the previous baseline.

[Issue #9](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/9#issuecomment-5922636193)
is now closed with a software-cursor workaround, distinguishing current Lua
configuration from older `.conf` setups. This is a documented user workaround,
not an app-side fallback or an independently reproduced fix. It leaves Omuse's
NVIDIA/fractional-scale acceptance check open; no desktop settings were changed.

Two new proposals were inspected:

- [PR #12](https://github.com/ZacharyZhang-NY/OmaPhoto/pull/12) adds only
  `omastore.toml`, identifying an x86_64 release package, executable, icon and
  screenshots. It is **open and unmerged**. The author's install/checksum claim
  was not independently rerun; an automated summary saying the app is already
  listed is not merge or installation evidence.
- [Issue #11](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/11) requests a
  Bézier pen tool. It contains no implementation. Omuse already has editable
  cubic paths, draggable anchors/handles, smooth-node editing, vector masks,
  Apply/Undo and persistence tests in `vector_ui.rs`, `vector_path.rs` and
  `vector_tests.rs`. A Photoshop-style direct canvas tool remains a separate
  interaction comparison.

### Most useful follow-up work for Omuse

Current Omuse source is `66f41b0`; its Rust subtree is identical to released
runtime `a8b7ac7`, and the installed command reports **0.6.0**. The following are
recommendations from source inspection, **not newly shipped fixes or reproduced
quality measurements**. Linked Compositor code/tests were read, not executed.

| Priority | Finding in Omuse and concrete next work |
| --- | --- |
| Smudge trail quality | `retouch_brush.rs` still uses 8%-diameter dab spacing and retains older carried colour. The [Compositor change](https://github.com/robbietilton/Compositor/commit/ad9ad7c94fe706f04bb930dcf576d95f77d0686a) changes both CPU/GPU spacing and carry, with a fading-dot-trail regression. Reproduce that visual case locally, then evaluate the smaller spacing and revised strength/carry together. Keep stroke-work limits, soft selections, alpha and Undo tests; tighter spacing increases work. |
| Liquify sharpness | Omuse `push()` repeatedly samples the previous dab's pixels. [Compositor's Metal change](https://github.com/robbietilton/Compositor/commit/7164dd49055f57828e59315887f4779ac954de21) moves offsets and resamples untouched source pixels. Evaluate a bounded CPU displacement field for Omuse using forward/back strokes on line art and real photos. The upstream GPU measurement does not establish an Omuse improvement or require adopting Metal. |
| Camera Raw curve behaviour | Omuse still applies a piecewise parametric lift to luminance before channel curves. The [new Compositor implementation and tests](https://github.com/robbietilton/Compositor/commit/60d9117907704443b991d7855a3ea454a535ee6e) use smoother parametric bends and an RGB master curve with changed Refine Saturation meaning. Add independent references and an explicit recipe/version migration so old saved grades retain their appearance. Omuse already captures point drags and preserves point order; parametric region/divider gestures and a selected-point readout are the remaining UI comparison. |
| Rendering regression coverage | Expand preview/Apply/export references for clipping stacks, fractional alpha, placed masks and transformed paint. Omuse's 8/16-bit Soft Light already uses the piecewise cubic/square-root formula; the roadmap is not evidence that it needs replacement. Compare actual pixels before changing compositing. |
| Everyday editing polish | Omuse already has colour-range selections and per-range font overrides in Rich Typography. Direct inline selection-to-font controls with mixed-style feedback remain useful. Dither currently lists ten styles; a distinct Scanlines/CRT mode is absent. These are proposals, not 0.6.0 features. |

OmaStore could become a discovery channel after Omuse has a qualified downloadable
package. The proposed manifest targets a package asset; Omuse currently publishes
source pre-releases. Finish dependency notices and clean-machine package checks
before listing it. This review makes no store submission, repository-setting
change or installation change.

The source audit also corrected stale guide entries: PSB/supported Photoshop text
and live text previews are implemented in 0.6.0, and format-10 regressions completed
the recorded release suite. The [0.6.0 qualification](release-060-qualification.md)
retains its own test scope; no application test suite was rerun for this review.

## Historical v1.3.3 release review

Reviewed 1 October 2026 against [OmaPhoto v1.3.3](https://github.com/ZacharyZhang-NY/OmaPhoto/releases/tag/v1.3.3),
published 30 September at exact tag commit
[`190414451148d9627e902ded53082dc3b3019913`](https://github.com/ZacharyZhang-NY/OmaPhoto/tree/190414451148d9627e902ded53082dc3b3019913).
The local source comparison uses Omuse 0.5.0, canonical `479ae31` and public
runtime `73dd0d4`; at review time the installed launcher reported 0.5.0
with that public source receipt. Source and regression tests establish intended behavior. OmaPhoto was
not installed or executed for this review, and neither application's tests were
rerun. This is not evidence that either editor is faster or more reliable.

The **whole-repository** watch includes the three post-tag commits through
[`e94fd73`](https://github.com/ZacharyZhang-NY/OmaPhoto/tree/e94fd73573aee5eefd8e955e72cda5b379d4120f).
Since the previous `838d5b1` review, 12 commits change 150 distinct paths: nine
commits enter 1.3.3 and three follow it. Four releases/tags, one branch, eight
issues, two pull requests and nine workflow results were checked. The three
older releases retain their notes and asset metadata. The
[watch baseline](omaphoto-watch.json) separates the release, updated packaging
and later application changes.

Omuse's strengths are its native Omarchy integration, searchable and executable
command reference, editable content collections, templates and reversible
in-app AI workflows. OmaPhoto has a more complete Camera Raw interaction surface
and a substantially broader downloadable Linux release. Both matter to a
credible public editor.

## Omuse 0.6 implementation following this review

The tables below retain the **0.5.0 review baseline**. The subsequent
`feature/omaphoto-133-editing` work implements the recommended editing backlog:
format-10 colour-run import, direct live text/colour previews, Open Recent,
numeric scrubbing, ten Dither styles and richer finishing effects, bounded
PSB/SVG imports and supported Photoshop text, growing masks, 3:4 crop,
capability-aware layer actions, background selection outlines, queued saves
and external-change decisions. Camera Raw gains stage-aware sampling and
targeted drags with coalesced, cancellable work.

This backlog is released in **Omuse 0.6.0**. See the
[implementation record](omaphoto-133-implementation.md) and
[0.6.0 qualification](release-060-qualification.md) for source-scoped test,
production, native, installation and compatibility evidence. These implementations do not establish interchangeable
project semantics, cross-app performance superiority, a larger safe document
budget or a portable binary release. Historical gaps in the tables should be
read with this newer implementation record.

## OmaPhoto 1.3.3 against Omuse 0.5.0

This release includes the previously reviewed finishing filters, crop and
clipboard work, plus PSB/SVG imports, editable Photoshop text, live text/effects,
growing masks, numeric scrubbing, selection-outline work and external reload.
They are now shipped features, rather than just development to watch. The
historical tables below retain the original review scope and limitations.

| Area | Source evidence and Omuse comparison | Priority |
| --- | --- | --- |
| Project format 10 | OmaPhoto now writes 10 and reads 1–10. Its optional `text.colorRuns` use UTF-16 offsets; Omuse's reader rejects versions above 9 and its rich runs use UTF-8 byte offsets. Renaming a project to `.omuse` cannot bridge this difference. | Highest interoperability priority: a bounded importer with Unicode range conversion, cached-pixel preservation, malformed-input tests and actual upstream fixtures. Do not simply increase the accepted version. Omuse collection schema v2 is a separate format. |
| Dither | Ten styles span Atkinson/Floyd–Steinberg, three Bayer sizes, halftone dots/lines/diamonds, patterns and ASCII, with pixel/cell sizes and colour choices. Source tests cover alpha, previews, committed output, picker cancel and worker rendering. Omuse has no equivalent Dither command or filter. | Strong creative addition for retro posters and social artwork. Start with deterministic diffusion/Bayer/halftone, reversible preview, selection/alpha correctness, bounded workers and Undo before expanding to ASCII. |
| Live text and selected-letter colour | Upstream renders draft artwork through the committed path and colours an actual text selection, with caret-linked swatches and picker cancel/focus restoration. Omuse supports mixed font/colour runs through its typography editor, but its inline textarea is an overlay and lacks this direct selection/picker integration. | High everyday usability value: the same render path for preview and Apply, plus native selection-aware colour editing. Preserve IME, Unicode, masks/effects and Undo. |
| Open Recent | A persisted ten-item menu deduplicates canonical paths, drops missing projects, supports Clear and only promotes successful opens/saves. Tests cover cancelled/failed actions and files disappearing while listed. Omuse has no shared recent-project list or command. | Add a keyboard-searchable recent list covering canvases and collections, with clear history, safe dirty-document navigation and missing-file handling. |
| Numeric and slider polish | Numeric scrubbing is now released. Black & White, Color Balance and Hue/Saturation also gain coloured slider tracks and double-click reset. Omuse's refined panels retain typed/button controls without shared label scrubbing. | High daily editing value. Combine visible drag affordances with exact typing, keyboard access, reset, Escape rollback and one Undo entry per drag. |
| Save, close and focus safety | Upstream adds snapshot saves while editing and queues save/reload/open/close operations behind one writer. Its new tests cover repeated saves, quit, failed writes, disappearing controllers and exporting beside a save. Omuse already has background snapshots, destination locks, conflict detection and publication checks; this is not a wholly missing feature. | Extend lifecycle stress coverage and external-change choices while preserving Omuse's existing save guard. Do not substitute the upstream manifest/image-size fingerprint for Omuse's metadata/inode checks. |
| Imports and masks | PSB, bounded SVG/SVGZ raster import, simple editable PSD text and paint-driven mask growth from the previous review are included in 1.3.3. Those remain concrete Omuse gaps, with upstream's own conversion limitations. | Keep the planned bounded import and mask work. Existing PSD support, rich typography and fixed-size mask painting are not equivalent. |
| Smaller parity details | Omuse already has anchored keyboard zoom, session layer Copy/Paste, context menus and selection-started crop with portrait 9:16 via orientation swap. It lacks a 3:4 preset, empty-layer Vignette and the richer spatial finishing-filter semantics described below. | Avoid rebuilding existing workflows; add the missing behaviours with explicit modes and visual references. |

Primary source references: [format specification](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/docs/project-format.md),
[Dither implementation](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/src/Document/Dither.cpp),
[Dither UI tests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/tests/DitherSheetTests.cpp),
[selected text tests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/tests/TextColorRunsTests.cpp),
[recent-project tests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/tests/OpenRecentTests.cpp)
and [save queue tests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/tests/ProjectSaveQueueTests.cpp).
Corresponding Omuse paths include `rust/src/document.rs`, `objects.rs`,
`rich_text_ui.rs`, `ui.rs`, `crop.rs`, `filters.rs` and `save_guard.rs`.

### Release evidence and changes after the tag

The [1.3.3 release workflow](https://github.com/ZacharyZhang-NY/OmaPhoto/actions/runs/36737522836)
passed all six jobs at `1904144`. The inspected workflow and Arch recipe build
and package the application; they do not run the full test suite. Upstream's
`TASKS.md` reports 291 passing test programs across distro checks and fresh
container install/start checks. These are upstream-reported results, not our
independent rerun or directly demonstrated by that release workflow.

The release currently has six application packages plus `SHA256SUMS`. Fedora
43 and 44 packages were added separately to handle their different LibRaw ABI;
upstream reports building the new Fedora 44 package from the unchanged tag.
The updated asset metadata is captured in the watch baseline. No AppImage or
Flatpak is published, and PR #8 remains unmerged.

The final post-tag [acceptance commit](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/e94fd73573aee5eefd8e955e72cda5b379d4120f)
adds three application-driven test programs, sixteen screenshots and a
[feature acceptance table](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/e94fd73573aee5eefd8e955e72cda5b379d4120f/docs/acceptance.md).
Those screenshots use Qt's offscreen platform. The same commit fixes window
teardown reaching already-destroyed menus/toolbars when panels retain focus
or pending field edits. That application fix is **after the 1.3.3 tag**, even
though the acceptance document is titled 1.3.3; it must not be attributed to
the tagged release binaries. Upstream reports 295 test programs at this later
head. Raw counts are not comparable with Omuse's individually counted cases.

The review recommended addressing format-10 import and save/focus lifecycle coverage;
then recent projects, numeric controls and accurate live text; then Dither,
mask growth and richer imports. The historical
[0.5.0 qualification](release-050-qualification.md) records 970 passing cases,
all 80 template variants, exact-source CI and installed update/rollback checks.
Those receipts remain separate from this source-only competitor review. The
subsequent [0.6.0 qualification](release-060-qualification.md) records completion
of that backlog and 1,105 passing cases on the final runtime. A
same-machine photo corpus and timing run is still required for any competitive
speed, quality or stability claim.

## Historical v1.2.3 editing and interaction review

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

## Development after v1.2.3

The first development review inspected source through
[`ffba175`](https://github.com/ZacharyZhang-NY/OmaPhoto/tree/ffba1753c62f5ad82a48248a598366c951faac8d),
five commits after the release. These changes were merged on upstream `main`
but **unreleased at that review**; they are included in 1.3.3. Source and tests were inspected;
neither editor was executed for this additional comparison.

| Upstream development | Omuse comparison and useful next work |
| --- | --- |
| [Finishing filters](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/fdb62978c54c1fdac95b1065919acf80a637982c): transparent-margin Bloom, richer Vignette and radius-based local Tonal Contrast | Omuse has all three names and tests in `rust/tests/raster_filters.rs`, but different semantics: Bloom/Vignette preserve transparent destination pixels and Tonal Contrast is per-pixel. Add explicit richer modes with pixel references, selection/cancellation checks and one-step Undo; do not silently break existing alpha guarantees. |
| [Stepped keyboard zoom](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/9ac3b1b81c9ff64bed214e270b7bd6531e296e65) preserves the document point at viewport center | Omuse 0.3.0 adds fixed 2%–1600% stops with centre anchoring, pointer-centred wheel zoom and round-trip tests; its automated/native qualification passed. The upstream document-tab sizing change has no equivalent in Omuse's current single-document window. |
| [Layer context menus and shared format version](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/fc076de71d1c873f94b69beb7db20b309e6826cc) | Omuse already targets the clicked row and retains multiselection, but its action labels/enablement are generic. Add capability-based menus and direct right-click tests, and centralize the format-version constant. The guide's stale writer version was corrected to 9 in this documentation pass. |
| [Whole-layer clipboard and multiple duplicates](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/7a873eb9c70b754060663bf83862eafcf2b5516e) | Omuse already duplicates multiple selected roots transactionally and remaps mask links. 0.3.0 retains whole selected trees in the running session with internal mask-link remapping, bounded admission and PNG interoperability. Automated/Cua checks passed; independent processes and clipboard managers receive PNG, not editable trees. Cross-document tabs are a separate design choice. |
| [Crop ratios, empty-layer Vignette and live text-color preview](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/ffba1753c62f5ad82a48248a598366c951faac8d) | Omuse 0.3.0 adds a movable ratio frame, corner resizing, orientation swap and reversible Apply/Cancel; automated/native/Cua checks passed. Vignette cannot paint an empty layer; text-color changes remain drafts until Apply. Explicit empty-layer Vignette and cancel-safe live color preview remain future work. |

Omuse 0.3.0 implements **anchored keyboard zoom**, **session layer Copy/Paste**
and **interactive crop ratios**. Its [qualification](editing-workflows-qualification.md)
records 901 passing cases, exact-source CI, production desktop checks,
clipboard exchange and installed rollback. This updates Omuse's status against
the first upstream snapshot above; those qualification results belong to that
Omuse runtime, not the newer upstream source reviewed below.
Remaining work includes bounded Camera Raw preview, missing Camera Raw gestures,
empty-layer Vignette, live colour preview and low-memory qualification. Relevant Omuse paths are
`rust/src/ui.rs`, `filters.rs`, `document.rs`, `shortcuts.rs`,
`rust/tests/raster_filters.rs` and `rust/tests/layer_canvas_parity.rs`.

### 30 September development review

The exact [nine-commit range](https://github.com/ZacharyZhang-NY/OmaPhoto/compare/ffba1753c62f5ad82a48248a598366c951faac8d...838d5b1459965e83f1693ff480fe60d14cff3377)
changes 201 paths. Relevant implementation, regression fixtures, test registration,
dependency and packaging changes were inspected. These changes were **merged but
unreleased on 30 September at that snapshot**; they are included in 1.3.3.
Upstream's references to Compositor versions 1.2.8–1.3 were not then new
OmaPhoto releases. Neither application's tests or runtime were executed for
this scheduled source review. The Omuse source comparison uses `488cf34` on
`feature/inspector-refinement`; earlier runtime evidence remains separately scoped.

| New upstream work | What the code/tests establish | Omuse gap and next qualification |
| --- | --- | --- |
| [PSB and oversized Photoshop imports](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/a2241601ec3e5ac10378d075689253b36dc257d8) | Version-2 lengths and RLE row counts, layerless merged images, and over-budget layer/mask cropping with a conversion notice. Tests compare PSD/PSB pixels and reject oversized/truncated data before large allocations. The reviewed channel decoder supports raw/RLE, with 8-bit RGB constraints. | `rust/src/psd.rs` explicitly rejects PSB. It already handles layerless PSD raw/RLE/ZIP/prediction with pixel tests, so that is not a new gap. Add bounded PSB decoding and optional, explicitly reviewed cropping; preserve off-canvas pixels by default. Test malformed lengths, masks, negative origins, all supported compression modes and actual Photoshop/Affinity exports. |
| [Editable Photoshop text](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/008493b0a3de461775014a4e8341e01fcd6d542b) | TySh descriptors become native point/paragraph text. Mixed styles use the first style with a notice; warps and faux styles are reported, missing fonts substitute, and unsupported placement/vertical/broken text falls back to cached pixels. This is not full text fidelity. | Omuse preserves cached text pixels and reports the loss of editability. Add supported descriptors with a comparison preview, explicit conversion notes and fallback; qualify fonts, rich runs, transforms, baseline placement and reopen. Omuse currently rejects embedded ICC profiles in PSD, whereas upstream's new merged-image path converts its profile; qualify that separately from layered colour fidelity. |
| [SVG/SVGZ import](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/8fbb104c485611e8eaa3c7805c5635d878b5d80a) | Qt SVG rasterizes to intrinsic size for a new document or to fit an existing canvas. Fixtures cover transparency, orientation, compressed files and size budgets. Shapes are not retained as editable vectors by this import. | Omuse uses SVG for packaged UI assets but has no document SVG import path. Add bounded raster import with selectable dimensions first, then consider editable conversion separately. Test external-resource rejection, malformed/compressed files, transparency and budget admission before advertising support. |
| [Live typed text and effect continuity](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/136ee65a48f60e2cf20f48f886b426a340fe86da) | Draft text uses its committed raster path, preserving layer order, opacity/blend and masks/effects; recent effect results support undo. Tests compare editing/committed output and cancellation, including moved text boxes and placed masks. The preceding commit adds baseline placement and Move-tool double-click editing. | Omuse's `inline_text_view` is a textarea overlay; commit/cancel/Undo and text metadata have tests, but this is not a matching live artwork preview. Add a reversible canvas draft rendered by the same text/effects pipeline, then verify exact preview/commit pixels across fonts, zoom, masks, blend modes and undo. |
| [Painting masks beyond their original bounds](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/f335818e533118c94ee1a8abde4671e1ca6c4b46) | Brush/fill/gradient growth preserves mask placement and its white/black outside coverage; placed-mask effect previews and thumbnails follow the same coverage rule. Other tools retain their existing mask bounds. | Omuse paints existing masks through their transform and clamps dabs to their raster bounds in `Editor::stamp_dab`. Add bounded mask growth without moving source artwork; test reveal/hide masks, linked/detached placement, rotated layers, effect previews, untouched corners and single-step Undo/save/reopen. |
| [Draggable numeric labels](https://github.com/ZacharyZhang-NY/OmaPhoto/compare/f335818e533118c94ee1a8abde4671e1ca6c4b46...aa253d97159ee6736307bab59bcd1ee9e0000116) | Shared scrubbing spans tools, opacity, transforms, sheets and selected Camera Raw rows. Tests cover clamping/rounding, disabled controls, lost releases, focused fields, layer switches and one-step opacity Undo. | Omuse's refined inspector still uses buttons/typed fields for these values and has no shared numeric-label drag control. Add a native control with visible affordance, precise typing/keyboard access, one transaction per drag, Escape restoration, loss-of-focus cleanup and target-change protection. The layout work alone does not qualify this interaction. |
| [Document budgets and selection outlines](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/8a9c56ee1fd677adca6c2c154fed0ee44d7911b5) | Adds 200 MP generated-surface limits and RAM-derived document budgets capped at 800 MP, with separate image/mask accounting in project storage. Complex zoomed-out ants are retraced asynchronously at reduced resolution; tests check stale outlines, fill rules and repaint pacing. | Omuse retains its fixed 100 MP limit and caches up to 200,000 contour points, scanning the mask on a selection revision. Add zoom-aware background outline generation with stale-result checks and measure large-selection UI latency. Higher pixel limits need peak-memory/low-memory qualification, including history, masks and concurrent previews; copying a larger constant is not a speed or stability improvement. |
| [External project changes](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/838d5b1459965e83f1693ff480fe60d14cff3377) | Watches/coalesces changes, reloads clean projects, asks Revert/Keep Mine for unsaved work and defers while busy. Tests include package replacement, partial writes, background tabs and concurrent notifications. The digest hashes manifest bytes plus sorted image names/sizes; same-size image-byte rewrites deliberately do not change it. | Omuse already has locked atomic saves, consistent-read checks and metadata/inode conflict stamps in `save_guard.rs`, including rechecks before publication. It offers Save as on conflict, not automatic reload. Add an in-app external-change/reload decision with save-copy preservation and generation checks; test same-size image replacement, partial writes, changes during the dialog and active edits/AI requests. Do not replace the existing save guard with the weaker image digest. |

The upstream evidence includes `PSBImportTests`, `CropToCanvasImportTests`,
`PSDTextTests`, `PSDTextReaderTests`, `SVGImportTests`, `TypedTextCanvasTests`,
`MaskPaintAnywhereTests`, `MaskEffectsCanvasTests`, `NumericScrubTests`,
`Scrubbable*Tests`, `MarchingAntsTests`, `ExternalChange*Tests`,
`ProjectWatcherTests` and `ProjectDigestRaceTests`, registered in `tests/Tests.cmake`.
These are inspected regression intentions, not a report that we ran them.

Prioritize numeric controls, live text and mask growth for everyday editing;
then bounded SVG/PSB/editable-text import and external-change recovery. Profile
selection-outline latency before choosing a performance target. These are
planned improvements in the guide, not implemented Unreleased features.

## Distribution

OmaPhoto 1.3.3 publishes Ubuntu 24.04/26.04 DEBs, Fedora 43/44 RPMs, an Arch
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

The broader watch also found these public proposals and reports:

- [AppImage PR #8](https://github.com/ZacharyZhang-NY/OmaPhoto/pull/8) is open and
  unmerged at `e2ad754`. Its current workflow makes Debian a release gate,
  permits Alpine failure and adds third-party glibc to Alpine. Its observed
  [PR run](https://github.com/ZacharyZhang-NY/OmaPhoto/actions/runs/36351361361)
  awaits approval (`action_required`); this is not evidence of a shipped or
  verified portable AppImage. For Omuse, require startup and real editing
  checks on each declared clean target before advertising support.
- [AppImage PR #10](https://github.com/ZacharyZhang-NY/OmaPhoto/pull/10) was
  closed without merging at `4af63d3`. Its only changed file is the AppImage
  workflow. The [run](https://github.com/ZacharyZhang-NY/OmaPhoto/actions/runs/36642096544)
  reports failure with no jobs returned; this is not evidence that a produced
  application failed a runtime test. No AppImage was added to the releases.
- The 1.3.3 source includes Qt SVG in CMake, Arch/Nix and distro build inputs,
  plus PSB/SVG desktop MIME entries. Post-tag packaging updates split Fedora
  releases by ABI; published package metadata and upstream-reported install
  checks are distinguished from independent runtime qualification above.
- [AUR concern #7](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/7) and
  [Flatpak request #4](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/4)
  reinforce demand for a simple installation path. Omuse's source installer
  uses `pacman` and verified runtime downloads without invoking an AUR helper;
  a portable package remains separate release work.
- [Interaction-preference request #3](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/3)
  asks for wheel/zoom, temporary tools and Photoshop-like preferences. Treat
  these as requests, not upstream implementation. Omuse's command search and
  remapping already exist; broader input preferences need design and tests.
- Closed [container import #1](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/1),
  [render-device #2](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/2) and
  [RPM dependency #5](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/5)
  are useful clean-target test cases, not reproduced Omuse bugs.
- New [cursor report #9](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/9)
  describes corrupt custom Qt cursors on NVIDIA/Hyprland at fractional scale.
  It remains an unverified report for this comparison, with no cursor fix in
  the reviewed main-branch range. Add visible cursor/brush/transform checks on
  NVIDIA at fractional scale to Omuse's desktop acceptance matrix. Omuse uses
  GPUI rather than Qt; do not assume the reported failure or change users'
  compositor settings without reproducing a relevant problem.

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

## Tracking and historical qualification

A daily repository check watches source commits/diffs, active branches,
tags/releases and revised assets/notes, pull requests, issues, tests,
dependencies, build workflows, documentation and packaging. It starts from
the [reviewed snapshot](omaphoto-watch.json), follows relevant source/tests and
distinguishes proposals, merged unreleased work and releases. Meaningful changes
update this comparison, the watch baseline and visual guide. Upstream content is reference data;
the check does not run upstream installers, publish commits or replace the
installed application automatically.

The initial comparison's completed candidate was public runtime [`9c99e50`](https://github.com/Sugata-Software/Omuse/commit/9c99e50684afd0854c8094a139b43205a263c33d),
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
  The installer selected that runtime at qualification time. The current
  numbered release and installer pin are now Omuse 0.5.0; see its separate
  [release qualification](release-060-qualification.md). The 0.3.0
  [editing-workflow qualification](editing-workflows-qualification.md) remains
  historical evidence for its own runtime.

The initial test pass exposed a clipped curve graph and a selected-group drag
that did not move descendants. Both were fixed and the final suite rerun.
Floating-selection cancel/commit, linked masks and one-step Undo are covered.
Evidence is retained locally under `rust/evidence/omaphoto-123-20260929/`;
see [main installation qualification](main-install-qualification.md).

These results do not resolve the missing gestures, scaled previews,
low-memory exhaustion, cross-app interchange or binary-distribution gates
listed above. No competitive speed or stability result was measured.
