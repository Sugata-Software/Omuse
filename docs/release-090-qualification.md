# Omuse 0.9.0 release qualification

The exact 0.9.0 source passed Linux/Windows validation, native compact controls,
photo integrity and package compatibility checks. The normal development-host
installation is verified at 0.9.0, and reviewed installer/latest declarations
now target that source. **Numbered source publication and permanent asset
publication/anonymous verification remain separate pending steps.**

## Exact source and release gates

Runtime [`65c95cc165f9d730a9f0bcea51c51a84cf4adb7e`](https://github.com/Sugata-Software/Omuse/commit/65c95cc165f9d730a9f0bcea51c51a84cf4adb7e),
tree `abd6296cf31078e278ab7bb73dc799948443de4d`, matches all 576 intended runtime
files. Compared with preceding `8f72ce5c`, only a compact text-footer layout
constraint in `ui.rs` changed. Photo library source is unchanged; retained
photo evidence below identifies the library actually executed.

| Gate | Evidence and state |
| --- | --- |
| [Validation 37207585304](https://github.com/Sugata-Software/Omuse/actions/runs/37207585304) | Completed successfully, attempt 1: Linux job 111452172803 and Windows job 111452172633. |
| Core Linux application suite | 1,405 passed: 549 library, 394 GPUI and 462 integration; zero failures, four ignored. Editing/Create/80 templates/motion, media and interrupted-save recovery passed. The separate dependency helper has 20 Rust tests, outside this core count. |
| Core Windows application suite | 1,387 passed: 534 library, 392 GPUI and 461 integration; zero failures, four ignored. Complete Windows journeys and notice gates passed. |
| [Download build 37207585335](https://github.com/Sugata-Software/Omuse/actions/runs/37207585335) | Both original package smoke receipts passed nine checks. Linux has 619 dependency entries, Windows 421; zero notice findings. Overall build succeeded; the original generated manifest passed canonical/archive/receipt checks and is copied unchanged into the [download declaration](releases/downloads/v0.9.0.json). |
| Final Linux host package | Same final archive repeated all nine smoke checks on the development host. |
| Final native compact controls | 24 native checks passed on XWayland at 800 × 600 logical / 1200 × 900 device pixels. Visual review confirmed Apply/Cancel inside the text panel and a wrapping keyboard hint. |
| Exact final cross-version install | 0.8 → 0.9 → rollback → forward passed four installed-launcher editing journeys, both exact 22-file generations and 35 export comparisons across nine projects. |
| Normal installation | Final source/binary verified at 0.9; all 22 prior 0.8 files retained, profile/settings bytes preserved. Installed-launcher native journey passed 24 checks with an isolated profile. |
| Source and permanent downloads | Reviewed declarations are ready; numbered source publication and permanent asset/anonymous verification remain pending. |

The final Linux binary SHA-256 is
`da39ef88d0c6f08074051784974668841e62a2ac9c56c527b73c9ba4685e4ec6`.
The [receipt ledger](release-090-receipts.json) contains exact archive, binary,
receipt and screenshot hashes without personal filesystem paths.

## Photo integrity and reconstruction quality

The retained library at public `8f72ce5c` has SHA-256
`6efe5e06b4145bd2564c6ba90e11adf625a9956290b7f9aacc2572a07a4a6841`.
Its full corpus passed **16 cases and 128 retouch operations**. The corpus uses
three underlying photographs, including portrait PNG/JPEG variants and Nikon
D90/Canon 450D RAW, plus orientation, transparency, ICC and precision variants.

Full-resolution import, original save/reopen and retained 16-bit/RAW export
were checked. Editing and retouch used working copies no wider than 1,024
pixels. Operations covered Spot Heal, flat Blur/Smudge/Liquify, transformed
soft-selection retouch and mask Liquify. RAW comparisons use the same pinned
LibRaw engine through an independently configured call, not independent camera
colour science. The run occurred under concurrent compilation; its timings
are not a comparative speed benchmark.

Controlled removal separately preserved **256,052 unselected pixels** exactly,
with one Undo and exact Redo, atomic cancellation, retained source and 16-bit
source/result samples. The ContextualV1 recipe saved, reopened and re-evaluated
exactly. The reviewed mission-patch repair improves suit texture but retains
some seam/texture mismatch. Automatic Spot Heal repeated nearby lettering and
zipper detail on this large target. Numeric success does not establish a
natural repair: preview, select clean nearby donors, refine selection/feather,
and use Clone or reviewed AI when local texture is unsuitable.

Thirty focused standalone operation tests passed, including ten new donor,
context, cancellation, work-limit, 16-bit and serialization regressions. The
complete earlier local `8f72ce5c` suite passed 1,405 cases with four ignored;
its source scope remains distinct from final 65c95cc1 CI and native evidence.

## Projects, settings and rollback

The final cross-version journey matched **35 decoded RGBA exports across nine
projects**, covering formats 10, 12, 13 and 14. New ContextualV1 output and
editable curved text exported identically through both versions in all four
stages. Original and opened packages, profile files and workspace sentinels
remained byte-identical.

Headless export/self-test does not load or write native Preferences/Shortcuts.
Each runtime saved/reopened its own self-test project; this check did not edit
or resave the same external project across versions. Preserving profile bytes
is not proof that an older app understands newer settings.

**0.8 can show cached ContextualV1 output but can discard the algorithm choice
when editing/resaving.** Keep the 0.9 original and rasterize a separate copy
before backward editing. Container formats remain 10–14; unchanged numbers
alone do not guarantee new recipe preservation. Recipes lacking an algorithm
remain Legacy, and new controlled-removal recipes explicitly select ContextualV1.

## Native scope and practical limits

[Actual captures and credits](releases/images/v0.9.0/README.md) include the final
compact text panel and final app displaying the reviewed removal project.
The installed-launcher journey used an isolated profile, not the user's artwork.
Earlier 39243caa foreground Cua checks covered numeric focus, mask inspection,
grid and selected-letter font workflows; they remain historical. Visible
removal defects blocked that candidate and led to the current algorithm.

Native retouch requires an 8-bit raster layer or existing mask up to 16 MP,
with a 256 MiB working-buffer budget and bounded stroke work. Strokes compute
on release without interactive Escape cancellation. Controlled removal has a
separate 4-million-pixel source bound and needs useful donor context. Mask
inspection is read-only; safe ordinary-folder Ungroup refuses unsupported
appearance/lock/clipping cases. Fonts are installed locally, with up to 64
matches. These are bounded checks, not process-wide memory or universal quality
guarantees.

Windows remains unsigned experimental. Interactive Windows 10/11, live Windows
AI, clean-machine installation, mixed-DPI/accessibility, signing, long sessions
and broader photographic quality remain separate work. No new live provider
request is claimed. Tablet support is deferred; scheduling is excluded.
Published 0.8.0 records and earlier measurements remain unchanged.
