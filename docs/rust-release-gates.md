# Rust release gates

The Rust application has its own Linux validation workflow. It does not replace or modify the preserved Swift/Qt workflows.

## Pull request and push gate

`.github/workflows/rust-validation.yml` runs on Ubuntu 24.04 when Rust sources, the locked dependency graph, or the Rust build/test scripts change. It pins Rust 1.98.0, limits Cargo to two build jobs, verifies the documented Linux development libraries with `pkg-config`, and uses `--locked` for every Cargo build performed by the gate.

The gate performs:

1. `cargo check --release --locked --all-targets --features ui-test`.
2. `scripts/test-rust.sh`, which checks formatting, runs the release test suite including headless GPUI interaction tests with one test thread, and runs the disposable synthetic edit/save/reopen/export journey plus the editable six-slide Create acceptance example (brand, rich text, resize, save/reopen, PNG, multipage PDF, MP4 and GIF). It also saves, reopens and renders all 20 templates at their original, square, story and wide dimensions, retaining native text and byte-identical render comparisons. FFmpeg is required for this complete gate and installed on the CI runner.
3. `scripts/test-rust-bundle.py`, which checks isolated installation, rollback-file preservation, literal path handling and rejection of malformed or altered bundles.
4. `media_acceptance`, which retains native text sources, audio, imported/editable subtitles, soft-track and burned-subtitle MP4s, trim/split outputs and FFprobe stream receipts.
5. `scripts/create-recovery-check.py --fixture PATH FRESH_EVIDENCE`, which independently opens six-page collections after repeated saves and a writer interrupted at the staged-before-publish boundary. This is process/filesystem qualification, not a power-loss simulation. Media and recovery artifacts are retained by CI.
6. Dependency-notice override tests and an inventory of the locked dependency graph, retained for legal review.

The workflow does not download the optional Camera RAW or subject-detection assets. Tests that require those assets must continue to fail or skip according to their explicit test contract; the workflow must not present an absent optional backend as exercised.

## Gates required before publishing a Linux build

Passing CI establishes that the locked Rust graph builds and that the automated headless suite passes on its Ubuntu runner. A public Linux build still requires recorded evidence for:

- a native Wayland and X11/XWayland launch with real window presentation;
- physical pointer, keyboard, clipboard, file-portal, mixed-DPI, and multi-monitor behavior where supported (tablet support is deferred for this preview);
- installation and desktop-launch behavior on each supported distribution/package format;
- the pinned optional runtime asset preparation and Camera RAW/subject workflows when those features are included;
- save/recovery interruption qualification with `scripts/rust-release-qualification.py`;
- representative large-document memory and interaction measurements; and
- cross-platform `.comp` interchange with independently reviewed fixtures.

The CI workflow does not publish packages or releases. Native acceptance results and optional-backend evidence belong in release evidence from the exact candidate binary.

## Create and subscription qualification

Run `scripts/native-rust-check.py CANDIDATE FRESH_EVIDENCE --capture --panel create` against the actual candidate. Repeat the presentation checks for `templates`, `assistant`, `content-export` and `motion`, including the supported minimum window and system/light/dark themes. Inspect the captured artwork and controls; a rendered window alone does not establish usability.

The explicit `--ai` mode consumes the selected Codex subscription allowance: one generated image, one protected-background edit and one editable carousel plan, without retries. It exercises the app-owned request, returned preview, Keep, save, reopen and ordinary Undo/Redo paths, including a six-page branded plan with native text, captions and alt text. Keep its receipt and before/after artifacts with the candidate hash. A provider error or unknown submission outcome must remain visible; do not automatically repeat the request. Ordinary tests and startup never trigger live generation.

Claude and other runtimes require independent sign-in and operation qualification. An installed executable is not sufficient. Optional direct API access remains disabled until the user explicitly configures it. Record these external gates separately from native editing and local export results.

## Rust dependency notices

Create the candidate's Rust dependency inventory from the already-fetched, locked Cargo graph:

```sh
scripts/rust-license-inventory.py /path/to/new/license-inventory
```

The command runs `cargo metadata --locked --offline --all-features` for the Linux target, including the candidate UI-test feature. It does not build code or access the network. The output contains `inventory.json`, copied `licenses/` files, and a combined `THIRD_PARTY_NOTICES.txt` suitable for staging with a candidate bundle. Git dependencies are checked at both their package directory and Cargo's repository checkout root so repository-level notices are retained.

The inventory records application and validation dependency scopes separately and excludes the root application package. It records missing declarations, missing legal text, and unreadable or oversized declared files for human review. It does not decide license compatibility, satisfy attribution obligations by itself, or certify a package for release. Release review must resolve the findings against the exact dependency graph and candidate contents.

For archives that omit their upstream notices, version-specific overrides retain exact source text with source revisions and hashes. The inventory rejects changed notice bytes, crate versions or upstream identity; CI verifies the overrides and retains the resulting inventory. The [current notice review](rust-license-findings.md) documents recovered texts and the four unresolved upstream findings.
