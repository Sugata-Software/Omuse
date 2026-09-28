# Linux installer qualification — 28 September 2026

This change makes the active source tree Linux Omuse and adds the public
`install.sh` entry point. Old app implementations remain in Git history, with
attribution and the exact C comparison references preserved separately.

## Local evidence

- **31 installer tests pass:** 20 source/manager tests and 11 offline-bundle
  tests. The source tests include 28 before/after integration-write failure
  cases, persistent write failure, SIGTERM interruption, atomic rollback
  rejection, toolchain selection, escaped paths and synthetic piped installation. Real Git
  fixtures verify a stable cached source checkout and refusal to overwrite local edits.
- A real production executable was installed into an isolated prefix on this
  Omarchy host. The source build reused installed Rust 1.98.0 and the existing
  Cargo cache. Pinned assets were prepared; a second update reused verified
  assets and build outputs.
- The installed executable matched the production binary; both native runtime
  libraries loaded, and all three installed asset hashes matched their inputs.
- Installation and rollback ran the editor's isolated editing self-test.
  Rollback and the reverse switch selected the expected complete generations.
  The desktop file passed validation, and a fresh install created no legacy
  command alias.
- Uninstall removed the isolated application. The original user applications
  retained their exact executable hashes.
- The relocated C generators produced byte-identical Camera Raw fixtures.
  Both Rust camera-reference tests passed. The independent color-noise
  reproduction, including MemorySanitizer, passed from its new location.

The first public source snapshot's complete Rust workflow passed at `d8c926e`.
The installer change adds a fast installer/reference-fixture CI job alongside
the full Rust job. A green earlier run is not evidence for a later commit.

## Scope

These checks exercise the installation mechanism on the development host and
use fault injection for failures. The real run reused existing system packages,
Cargo downloads and source checkout. It does not establish clean-machine
package provisioning or a clean remote first-build duration.

Ordinary errors and SIGTERM restore installation state. The final application
activation uses one symlink replacement; rollback uses Linux's atomic rename
exchange. SIGKILL, power loss and filesystem failure recovery are not qualified.
Source-build access does not close the outstanding binary-distribution notices,
physical desktop acceptance or portability gates in the
[release checklist](public-release-readiness.md).
