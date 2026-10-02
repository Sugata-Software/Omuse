# Omuse development

Read [AGENTS.md](AGENTS.md) for project-guide maintenance and
[CONTRIBUTING.md](CONTRIBUTING.md) for the contributor workflow.

Omuse is a native Linux application. Its Rust source and tests live in `rust/`;
`rust/README.md` documents building, testing and architecture. A Windows build
is in development: keep it compiling (the `windows` CI job and
`scripts/build-rust.ps1`) and put platform differences behind `cfg` so Linux
behaviour and file formats do not change. The current tree
does not contain the earlier application. Preserve `.comp` compatibility,
upstream attribution and the independent reference fixtures when changing
image processing or document handling.

Keep private artwork, local qualification evidence, downloaded runtime assets
and credentials out of commits. Separate measured results from unqualified
features in the project guide and release notes.
