# Contributing to Omuse

Omuse is a native Linux creative application, written
in Rust with GPUI and direct Omarchy theme integration. Application code and
tests live in `rust/`. The earlier application is preserved in Git history;
see [source provenance](docs/source-provenance.md) for attribution and the
small independent reference suite retained for compatibility tests.

## Start here

- Read the [Rust development guide](rust/README.md) and
  [release-readiness checklist](docs/public-release-readiness.md).
- Use Rust 1.98.0 and the locked dependency graph. `rust-toolchain.toml` selects
  that toolchain when using rustup; distribution Rust packages must supply a
  compatible version explicitly.
- For normal installation on Arch/Omarchy, use the one-command installer in
  the [README](README.md). It prepares dependencies and builds the current
  tested source revision locally; the first installation takes longer than an update.
- For development, install the documented Linux libraries and FFmpeg, then run
  `scripts/build-rust.sh` and `scripts/test-rust.sh`.
- A Windows build is in development. Changes must keep the `windows` CI job
  passing; put platform-specific code behind `cfg` attributes and leave Linux
  behaviour unchanged. See [Build on Windows](rust/README.md#build-on-windows-in-development).
- Use a branch and submit a focused pull request to `main`. Describe the user
  problem, resulting behaviour, validation and any remaining limitation.
- Add user-visible changes to **Unreleased** in [CHANGELOG.md](CHANGELOG.md).
  Follow the [release policy](docs/releases/README.md) when preparing a numbered
  release; features, tests and published downloads must have separate evidence.

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
