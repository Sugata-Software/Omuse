# Omuse 0.12.0 release qualification

**Release qualification — 25 October 2026.** Exact-source Linux and Windows CI
passed. Source and all four archive/checksum/manifest downloads are published and
passed anonymous exact readback. The normal installed upgrade is verified. Windows
setup passed its separate qualification, publication and anonymous readback. The Linux package passed host package, native backend and actual
0.11 → 0.12 → 0.11 → 0.12 generation checks. Earlier release notes, tags and
assets remain unchanged. Final publication and installed-app readback are separate gates.

## Exact public runtime

| Identity | Status |
| --- | --- |
| Cargo package / lockfile version | 0.12.0 / 0.12.0 |
| Public runtime commit and tree | [`9c684cae4a4ccdb763d036b65adc440cc61419c2`](https://github.com/Sugata-Software/Omuse/commit/9c684cae4a4ccdb763d036b65adc440cc61419c2), tree `834184e996c0099342f06ee7a9506061efb21386` |
| Public history | Based on main `17f3d3811b633e6b916005f37eb679336f52e24f`; no private history published |
| Tag `v0.12.0` target | Exact runtime 9c684cae; source publisher [40163681631](https://github.com/Sugata-Software/Omuse/actions/runs/40163681631) passed |
| Rust validation run and attempt | [40161067985](https://github.com/Sugata-Software/Omuse/actions/runs/40161067985), attempt 1 — passed |
| Package run / reviewed manifests | [40161068022](https://github.com/Sugata-Software/Omuse/actions/runs/40161068022), attempt 1 — passed; [unaltered generated manifest](releases/downloads/v0.12.0.json) |
| Notes / latest declaration / installer pins | Exact anonymous readback passed; public curl pin selects runtime 9c684cae |

Application version, page-document formats 16–19, vector scene versions and
Create collection version 2 are separate identities. A later documentation
commit does not change the tested runtime or justify moving an existing tag.

## Release checks

| Gate | Required record / current status |
| --- | --- |
| Exact-source Linux tests | Passed: 1,881 application cases (645 library, 547 UI, 689 integration), zero failures, eight ignored, 90 targets. Editing, Create/motion, 80 catalog variants, strict legacy layout, motion/recovery and notices passed. Three RAW/two external PSD fixture cases returned early, separate from ignored benchmarks. Twenty dependency-helper cases are additional. |
| Exact-source Windows tests | Passed: 1,838 application cases (622 library, 545 UI, 671 integration), zero failures, eight ignored across 90 targets. Editing/Create/catalog/layout/motion, default build, package and notice gates passed. Twenty dependency-helper cases are additional. Optional external RAW/PSD fixture returns remain separate. |
| Installer/publisher/download tests | Local: 25 installer, 28 release-note, 31 download and 13 Windows-setup contract tests pass. Exact CI installer/reference job passed. |
| Native Linux package | Exact default-feature package: 24/24 scripted native checks on Wayland and 24/24 on XWayland, 800×600 logical viewport. Covers painting, Undo/Redo, guarded close/save/reopen, themes, text, effects, masks, retained precision, photo edits and command focus. Live AI excluded. |
| Windows archive/setup lifecycle | [Setup run 40163791906](https://github.com/Sugata-Software/Omuse/actions/runs/40163791906) passed all 14 native lifecycle checks, including actual 0.11→0.12 upgrade, repair, downgrade refusal, shortcuts, installed editing/DLL loading, uninstall and project/profile preservation. [Reviewed receipt](releases/installers/v0.12.0.json); [Publisher 40163989927](https://github.com/Sugata-Software/Omuse/actions/runs/40163989927) and all seven public downloads passed exact readback. Unsigned experimental; Windows Server 2025 CI is not interactive desktop qualification. |
| Package contents | Both generated archive/smoke hashes, binaries and inventories match the unaltered final manifest. Linux: 621 dependencies; Windows: 423; zero notice findings. Optional EfficientSAM-Ti weights are not bundled. |
| Save/access policy | Automated fault/recovery, destination identity and Linux visible-policy checks in final suite; native application checks confirm path Enter, replacement review and saved clipping contents. Final CI counts are recorded above. No physical power-loss claim. |
| Cross-version artwork | 37 owned fixtures across old/new/rollback/forward readers: 134 exact exports and 14 safe old-reader refusals. Bound to the final generated manifest. |
| Optional local model | Two real CPU model tests passed on final application code: point/box/cache and runtime cancellation/reuse. UI transactions covered by source tests/native review; mask quality remains experimental. No provider calls or bundled EfficientSAM-Ti weights. |
| Installation and rollback | Normal Omarchy app upgraded from reviewed public archive; version/source/executable and all 36 payload files match. Complete previous generations and pre-existing config/data retained byte-for-byte with mode/ownership. Normal launcher passed 24 scripted Wayland checks in an isolated profile. Isolated old/new rollback qualification is separate. |
| Public download readback | [Publisher 40163728420](https://github.com/Sugata-Software/Omuse/actions/runs/40163728420) passed; all four public files match exact sizes/hashes; manifest/checksum bytes, notes, tag, latest and curl pin match. Previous 0.11 release/tag/seven asset identities unchanged. |

## Automated coverage

Exact-source CI passed 1,881 Linux and 1,838 Windows application tests, with zero
failures and eight ignored on each platform; 20 dependency-helper cases per
platform are additional. CI run 40161067985, attempt 1, completed successfully.
The separate local optimized suite passed **1,881 tests** (645 library, 547 UI,
689 integration), zero failures and eight ignored across 90 targets, with host
Rust 1.99.1. CI is pinned to Rust 1.98.0. Editing, Create/motion, 80 catalog
variants and strict retain/release layout journeys passed afterward. Three
external RAW and two external PSD cases returned early without optional inputs;
these are separate from ignored benchmarks and fresh external-file qualification. A focused source/UI diagnostic run passed 547 tests with two
existing manual benchmarks ignored. The diagnostic uses unoptimized UI source
and a larger test stack, so it does not substitute for the optimized suite.

Physical Cua input against the final optimized application code exercised
Open/Save path Enter, Save/overwrite guards, brush/Undo, clipping contents-only
dragging with a stationary saved mask, and custom shortcut recording/persistence.
Those host checks use a UI-test build; the exact default-feature download has
separate scripted native backend checks above. One independent Poppler pixel
test passed; ordinary structural PDF tests do not count as that external render.
No broad Windows physical-interaction or hardware qualification is implied.

Relevant tests are part of the release source:

- [Vector clipping](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/vector_clipping.rs),
  [gradient rendering/export](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/gradient_quality.rs)
  and [SVG metadata](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/svg_metadata.rs).
- [Embedded look tables](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/look_lut.rs),
  [native photo processing](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/camera_native16.rs),
  [project versions](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/native_camera_project_versions.rs)
  and [native admission](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/native16_owned_admission.rs).
- [Save metadata](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/save_metadata.rs)
  and [adversarial replacement cases](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/save_metadata_adversarial.rs).
- [Guided source preservation](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/guided_mask_preservation.rs),
  [mask visibility](https://github.com/Sugata-Software/Omuse/blob/main/rust/tests/subject_mask_visibility.rs),
  [guided UI](https://github.com/Sugata-Software/Omuse/blob/main/rust/src/guided_mask_ui_tests.rs)
  and [advanced editing UI](https://github.com/Sugata-Software/Omuse/blob/main/rust/src/advanced_tests.rs).

Test presence is not a CI pass. Record final executable results and native scope
rather than inferring qualification from filenames, source or a screenshot.

## Published artifacts

Populate from reviewed final files and independent public readback:

| Artifact | SHA-256 / bytes / public receipt |
| --- | --- |
| Linux x86_64 archive and executable | 38,857,203 bytes; archive `9ad09f00a1110277b50c814e0684cffec6a56b799d7304827d7738578595f824`; executable `f9ff04fb73dff9ae0674b6a2c88d434fcf9a36583489621ff059c7af45159d83`. Public size/hash readback passed. |
| Windows x86_64 archive and executable | 27,094,634 bytes; archive `c0ea2b218ab80962e219dbfdfe912bd373ccb025f562d3e3a5b41c7c4825884b`; executable `a550a1ae06596413a37db4cfd190117f20adf7d4e4cbccee3cfdf3c2fdb408f3`. Archive and Windows runner smoke passed; bound to generated manifest; public size/hash readback passed. |
| Windows setup installer | 20,057,949 bytes; SHA-256 `1e88601c148b0fd94fc7a832e7bbb62a91e7c16e6796c7efc3dc7b2c1112e990`; native lifecycle and exact public readback passed. |
| Checksums and download manifest | Reviewed unaltered build manifest, exact archive/smoke/inventory binding passed. Public size/hash readback passed. |
| Source-installer runtime pin | `9c684cae4a4ccdb763d036b65adc440cc61419c2`; exact public bytes and shell syntax verified |

## Compatibility and limits

- Document 16: editable clips/scene 5. Document 17: embedded looks. Document 18:
  extended paints or inert SVG metadata/scene 6. Document 19: Native16 nodes,
  even disabled/zero-opacity. Old-reader refusal and lowered-manifest rejection
  need final checks. Ordinary retained 16-bit source alone uses format 10.
  Create collection version 2 remains separate from each page's document version.
- Clipping is bounded, with no nested clips/live-text masks. SVG/PDF reports do
  not establish arbitrary SVG, Illustrator or editable PDF/EPS parity. Dithered
  objects rasterize on export; `.omuse` retains original semantics.
- Native16 uses already-developed RGB, with a 16,777,216-pixel bound and additional
  admission. Policy estimates are not hard RSS limits. No calibrated profiles,
  sensor-linear RAW, HDR, Adobe appearance or GPU-acceleration guarantee.
- XMP is a reviewed partial intent mapping; local adjustments, crops and
  proprietary profiles remain unsupported. Existing byte-compatible grades are
  not silently upgraded.
- Guided masks remain experimental: six tokens, positive/box only, manual
  refinement and uncalibrated rankings. Download/repair is explicit and separate
  from runtime installation. Model licensing/provenance is distinct from runtime
  redistribution. Controlled removal and generated masks require visual review.
- Save policy covers bounded visible Linux ownership/modes/ACLs/xattrs. Different
  principals, hidden privileged metadata, timestamps, Windows ACLs and physical
  power-loss behavior remain unqualified unless separate public receipts say
  otherwise. Keep complete `.omuse` masters and backups.
- Local mask processing does not establish new cloud-provider or subscription
  entitlement. Broader hardware, physical input, photographic reconstruction,
  matting and print/editor interchange require their own qualification.

## Final publication

Source publisher [40163681631](https://github.com/Sugata-Software/Omuse/actions/runs/40163681631),
archive publisher [40163728420](https://github.com/Sugata-Software/Omuse/actions/runs/40163728420)
and setup publisher [40163989927](https://github.com/Sugata-Software/Omuse/actions/runs/40163989927)
completed successfully. Anonymous readback verified all seven attached files,
exact notes, source tag, latest declaration and public curl installer bytes/pin.
The previous 0.11 release, tag, notes and all seven asset IDs/sizes/digests are
unchanged. Normal installation and isolated generation rollback are separately
verified above. Numbered notes and the tested runtime remain immutable.

Broader Windows desktop/hardware, live providers, photographic reconstruction,
matting, physical power loss and broad print/editor interchange remain outside
these bounded checks. A successful automated or host-specific test does not
remove those limits.
