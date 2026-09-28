# Contributing to Omuse

Omuse is a native Linux creative application in development preview. The active
implementation is in `rust/`; the preserved Swift/Qt source is retained for
attribution, format comparisons and compatibility work.

## Start here

- Read the [Rust development guide](rust/README.md) and
  [release-readiness checklist](docs/public-release-readiness.md).
- Use Rust 1.98.0 and the locked dependency graph. `rust-toolchain.toml` selects
  that toolchain when using rustup; distribution Rust packages must supply a
  compatible version explicitly.
- Install the documented Linux development libraries and FFmpeg, then run
  `scripts/build-rust.sh` and `scripts/test-rust.sh`.
- Use a branch and submit a focused pull request to `main`. Describe the user
  problem, resulting behaviour, validation and any remaining limitation.

Tests create disposable projects and separate XDG directories. Native desktop,
tablet and display checks are separate from headless tests. Live provider tests
are opt-in and may consume the selected subscription allowance; do not add
provider credentials to CI or silently invoke billable services from tests.

## Reporting a problem

Use the bug-report form with your source revision, Linux distribution, desktop
session, GPU, app steps and a small synthetic reproduction where possible.
Review logs and screenshots before attaching them. Remove account tokens,
provider transcripts, personal paths and private artwork. A failing operation
with an uncertain provider outcome must not be retried automatically.

For suspected security problems, do not post exploit details, credentials or
private documents in a public issue. A dedicated private reporting channel and
published security policy are release gates tracked in the readiness checklist.

## Keep changes reviewable

Preserve project compatibility, originals and undo/recovery behaviour. A fix to
an importer, save transaction or provider boundary needs a focused regression
case that demonstrates the failure. Record physical device testing honestly;
synthetic events are not proof that a tablet or desktop portal works.

Keep generated build output, downloaded runtimes, model weights, personal
configuration and local evidence out of Git. Third-party changes must retain
their legal notices and provenance. Follow the existing Omarchy theme adapter
rather than introducing a second colour source for editor controls.

Omuse is for creating content. Calendars, scheduling, social publishing and
analytics are outside the current product scope.
