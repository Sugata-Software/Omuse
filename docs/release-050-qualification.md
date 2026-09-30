# Omuse 0.5.0 qualification — 30 September 2026

This record covers the unified `.omuse` extension, legacy migration, collection
schema v2, safe Save As replacement and batch collection input. It does not
qualify new AI provider behavior or broader image-editing quality.

## Exact source and executable

- Public runtime: `73dd0d47c99b6718f640c430f47f4ea33c8d047d`.
- Git tree: `b8e1999e6dc5bcfabbff2126277439face30631d`.
- Production executable SHA-256: `17034c434f13c38304188ef7cfbd6b1a5ea785d8bf0bbfadcc5e68d0d08e503f`.
- Runtime: **Omuse 0.5.0**, built with locked production features, without `ui-test`.
- Local and public runtime trees match. Later release documentation and installer
  pin edits do not change the runtime.

## Automated checks

All **970 application cases passed** locally: 417 library, 274 UI and 279
integration cases in 31 batches. Four manual timing benchmarks were excluded.
Editing, Create/export, motion and all **80 editable template variants** passed.
The [exact-source GitHub workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36682319767)
also passed all-target checks, the complete suite, media export, interrupted
collection recovery, installer/reference fixtures and dependency-notice checks.
Offline installer, bundle and notice tests passed 25, 11 and 6 cases respectively.

The 21 new cases cover filename normalization, non-UTF-8 Linux names, loaded
trailing-slash paths, Save defaults, legacy copy preservation, replacement
confirmation and changed destinations, v1→v2 Save As and same-path migration,
lazy undo snapshots, pixels/metadata/brands/resources, missing or malformed
nested packages, unknown versions, external changes, recovery ownership, batch
collisions and active-page pixel limits. Malformed v2 content never falls back
to a legacy sibling. Compatibility identifiers remain unchanged.

## Previous-release interoperability

The verified 0.4.0 production executable created a legacy canvas. The new
production executable exported it under both `.comp` and `.omuse` names with
exact decoded RGBA equality and no source-file changes. Both production readers
also exported an independently constructed v1 six-page collection with identical
pixels. The old production reader rejected v2 without creating an output, matching
the documented downgrade boundary. These are synthetic fixtures, not an
independently reviewed Mac interchange corpus.

## Production desktop and installation

Production Wayland, production XWayland and the installed Wayland launcher each
passed **24 native checks** at the minimum 800×600 viewport with reduced motion.
They cover editing, Undo/Redo, modern save/reopen, retained 16-bit sources, export
and command search. Captures were inspected. The harness drives owned GPUI test
windows and does not establish every physical device or accessibility path.

The normal launcher selects a complete **19-file** installation with a clean
public-source receipt. The payload hash matches the production build. Both
runtime libraries and the subject model are retained. Rollback to 0.4.0 and back
passed with all payload hashes intact and isolated editing self-tests in both
directions. Desktop-file validation passed. Existing artwork windows and settings
were left intact; save and reopen to use the update. See the
[compact receipt](release-050-receipts.json) for the exact identities and scope.

## Compatibility and release boundaries

Single-canvas schema v9 retains `com.compositor.project`; legacy `.comp` projects
remain readable and their first UI Save offers an `.omuse` copy. Collections
read v1 and v2 and write v2. **Older app releases cannot read v2 collections.**
Use Save As to preserve a v1 original; app rollback alone does not downgrade data.

This is an Arch/Omarchy Linux x86_64 source pre-release, with no application
binaries attached. The two outstanding dependency texts, clean-machine install,
broader hardware, mixed-DPI/accessibility and independent interchange/image
corpora remain release gates. No live AI request was sent for this migration;
earlier provider receipts retain their original runtime identities.
