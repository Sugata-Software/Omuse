# Omuse releases

Start with the [changelog](../../CHANGELOG.md) or
[GitHub releases](https://github.com/Sugata-Software/Omuse/releases).

| Version | Date | Scope | Notes |
| --- | --- | --- | --- |
| 0.6.0 | 1 October 2026 | Photo finishing, direct editing, Photoshop/SVG imports and save/reload safety; source pre-release | [Release notes](v0.6.0.md) |
| 0.5.0 | 30 September 2026 | Unified .omuse projects, legacy migration and safer Save As; source pre-release | [Release notes](v0.5.0.md) |
| 0.4.0 | 30 September 2026 | Selectable AI providers, shared creation workflows, desktop connection fixes and refined panels; source pre-release | [Release notes](v0.4.0.md) |
| 0.3.0 | 30 September 2026 | Interactive crop, anchored zoom, editable clipboard and safer Cut; source pre-release | [Release notes](v0.3.0.md) |
| 0.2.1 | 29 September 2026 | Safer AI image editing, finishing Undo and practical user manual; source pre-release | [Release notes](v0.2.1.md) |
| 0.2.0 | 29 September 2026 | Task-based Ask Omuse and editable AI photo review; Arch/Omarchy x86_64 source pre-release | [Release notes](v0.2.0.md) |
| 0.1.0 | 29 September 2026 | First public source release; Arch/Omarchy x86_64 | [Release notes](v0.1.0.md) |

## Version numbers

Omuse has its own sequence, beginning at **0.1.0**. Each release matches its
Rust package version; the current release is **0.6.0**. It does not inherit
another editor's release numbers.

- **Patch versions (for example 0.2.1):** compatible fixes and hardening within a minor version.
- **0.2.0 and later minor versions:** substantial new workflows or behavior.
  Document any project-format or configuration migration explicitly.
- **1.0.0:** a stable supported release after the
  [release gates](../public-release-readiness.md) have evidence. A date or test
  count alone does not establish readiness.

Until that milestone, GitHub releases are marked as pre-releases. There is one
application named Omuse; the qualification label does not create a second app.
Project-file versions are separate from application versions.

## What every release records

Write `docs/releases/vX.Y.Z.md` around user-visible changes: editing, keyboard
and canvas, Create/export, reliability, installation, tests and known limits.
Link exact source revisions and qualification evidence. Keep proposed work out
of shipped features, and keep earlier measurements tied to their candidate.

`latest.json` selects the release to publish: version/tag, title/date,
pre-release flag, exact public source revision, notes file and successful Rust
validation run. The tag points to that tested runtime, even when the notes are
written in a later documentation commit. The current source releases attach
no application binaries. GitHub's automatic archives are source only.

## Maintainer workflow

1. Record implemented changes under **Unreleased** in `CHANGELOG.md` as work
   lands; keep the roadmap and qualification gaps in the project guide.
2. Select the next version. For runtime changes, update the Rust package and
   lockfile version before testing. Complete the appropriate automated/native
   checks and GitHub Rust validation for that exact public runtime.
3. Advance the installer pin only after qualification. Write the full release
   notes, a concise changelog entry, this index and `latest.json`. Update the
   project status and regenerate its guide.
4. Run `python3 scripts/release-notes.py --check`,
   `python3 scripts/test-release-notes.py` and the guide/reference checks.
5. Publish reviewed changes to public `main`. The release-notes workflow checks
   the declared source and completed CI, then publishes the numbered GitHub
   release. Pull requests validate without publishing. Existing releases and
   tags are never silently overwritten or moved.
6. Verify the release page, tag target and notes after publication. A failed
   publication remains unfinished; do not label a draft as released.

The normal public source channel is the only publication source. Never push
private working history, local evidence folders, credentials or personal
artwork. Redistribution requires its own complete dependency and target-machine
evidence before any binary assets are attached.
