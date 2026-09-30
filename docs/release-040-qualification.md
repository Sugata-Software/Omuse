# Omuse 0.4.0 qualification — 30 September 2026

The combined release includes desktop subscription discovery, Claude structured
assistant responses, per-task routing, bounded image/layout/caption sequences
and the refined editing, Create and AI panels. This is an Arch/Omarchy x86_64
source pre-release. Project formats remain compatible. Routing preferences
migrate from version 1 to version 2 when written; valid existing choices survive
and migration cannot enable separately billed API access.

## Exact source and executable

- Public runtime: `3f5ece38ba1ee84af2d80062412bd9e11e26fbe3`.
- Git tree: `a529fa9b55c45340137813dffa5079b4b340c8b3`.
- Production executable SHA-256: `b5d58b871b1c99dd7239e4a0033df80f303683f8a54fa1befb756c04ca02cd51`.
- Runtime version: **Omuse 0.4.0**, built with the locked Rust toolchain and
  ordinary production features, without `ui-test`.
- The public source tree exactly matched the local runtime tree before testing.
  Later release notes and installer-pin edits do not change the runtime.

## Automated checks

The complete local suite passed **949 application cases**: 407 library, 267 UI
and 275 integration. Four manual timing benchmarks were excluded. Format and
keyboard-reference checks, editing/Create/motion journeys and all 80 editable
template variants passed. The [exact-source GitHub workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36669607056) passed
both required jobs, including all-target compilation, the same full suite,
media export, interrupted collection recovery, installer/reference fixtures and
dependency-notice inventory. Inventory success does not resolve the two open
binary-distribution legal-text entries.

Routing tests cover explicit pins, Auto exclusions, preference migration,
unsupported tasks, preflight of all steps, cancellation, stale source state,
bounded saved replay and exact generated-layer identity. UI tests include
800×600 scrolling, retained AI briefs/results across Connections, contextual
controls and cancellation from the Connections footer. These are controlled
regressions, not a representative live-provider quality evaluation.

## Production desktop and installation

The exact production executable passed **24 native checks on Wayland** and
**24 on XWayland**. The installed normal launcher separately passed **24 on
Wayland**. Runs used the minimum 800×600 logical viewport and reduced motion.
They exercised synthetic painting, Undo/Redo, save/reopen pixel equality,
retained 16-bit samples, photo adjustment/crop/resize/export and command search.
Panel captures were inspected separately; the harness drives owned test
windows and does not prove every physical device or accessibility path.

The normal command and desktop entry select a complete **19-file** generation
with a clean public-source receipt. The executable hash matches the production
build. Both runtime libraries and the subject model are retained. Complete
rollback to the preceding panel candidate and back passed isolated editing
self-tests, with all 19 hashes unchanged in each direction. Desktop-file
validation passed. Existing artwork windows, user settings and provider
profiles were left intact; reopen after saving to use the new app.

The [compact receipt](release-040-receipts.json) separates launcher and payload
hashes and records the two installation generations. Private paths, account
state and raw desktop captures are excluded from the public receipt.

## Provider and release boundaries

No new live provider request was sent during this combined release check.
The [Claude Design receipt](ai-claude-qualification.md) belongs to `3117032`;
the [Codex image journeys](ai-image-editing-qualification.md) retain their own
runtime identities. Earlier [routing CI](ai-routing-qualification.md) belongs
to `f972adb`. None is relabeled as a new 0.4.0 live receipt.

Claude is text/layout only; Codex supplies the visual and image task paths.
Grok is unqualified. Broader live failures, cancellation, output acceptance,
clean-host installation, mixed-DPI/accessibility and independent photo or
interchange corpora remain open. This source release attaches no application
binaries; the [release gates](public-release-readiness.md) still apply.
