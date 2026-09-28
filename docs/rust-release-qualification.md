# Rust save and recovery release qualification

`scripts/rust-release-qualification.py` is a bounded destructive-process test that only uses generated documents inside a fresh evidence directory. It does not open user projects or use the normal recovery directory.

Run it from the repository root with an explicit evidence path:

```sh
CARGO_TARGET_DIR=/path/to/shared/target python3 scripts/rust-release-qualification.py /tmp/omuse-release-evidence
```

Add `--advanced` to create retained 16-bit sources and editable filter recipes on every layer. In this mode the fingerprint covers source/result samples and serialized recipes as well as cached pixels, and every reopened document must retain all advanced layers. Use a new evidence directory for each run:

```sh
CARGO_TARGET_DIR=/path/to/shared/target python3 scripts/rust-release-qualification.py /tmp/omuse-advanced-evidence --advanced
```

The gate first runs the Rust recovery worker tests, including bounded burst coalescing, clear versus in-flight writes, ownership release after process death, and latest-snapshot flush. The default sustained session then publishes twelve revisions of a three-layer 768×512 document. Every project and recovery publication is immediately reopened by the Rust document reader and checked for its revision, dimensions, layer count and full pixel fingerprint. The harness starts a second session, observes both a declared project-save window and an active staging directory, sends `SIGKILL` directly to the writer executable, and verifies that the visible project is a complete old or new revision and that the separately published recovery package is complete. Package and executable SHA-256 values, source hashes, working-tree status, timings, observed interruption phase, and any crash-left staging directory are recorded in `results.json`; recovery-test output is retained in `recovery-tests.log`.

The test demonstrates bounded repeated-save behavior, namespace-level atomicity during process death, and an independently readable recovery copy. Signal delivery is scheduler-dependent, so the visible project may be either adjacent revision. This does not simulate sudden power loss or prove storage-device persistence; those require a separate durability/fault-injection environment.
