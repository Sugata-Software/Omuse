# Versioned experimental downloads

Omuse has a separate build and publication path for numbered Linux and Windows
downloads. It does not move source tags, overwrite a release, replace an asset,
or publish automatically when application code changes. The existing source
publisher continues to own the numbered release and its notes.

The first supported qualification level is **unsigned experimental preview**.
Native GUI, mixed-DPI and live AI tests remain useful release work; they are not
prerequisites for making clearly labeled automated previews downloadable. A
preview passing these checks must not be described as a stable, signed, or
generally portable release.

## Build an exact candidate

1. Push the reviewed runtime commit and intended Rust package version to a
   public `release/X.Y.Z` branch. **Omuse downloadable preview builds** runs
   read-only on that exact push; it works even before this new workflow reaches
   the default branch. **Omuse Rust validation** must also run on that same
   commit, including the Linux, Windows and installer jobs. The two workflows
   use matching source/build path filters, so notes-only follow-up commits do
   not start a package build waiting for a validation run that will never exist.
2. A separate job waits for successful validation of that exact SHA while the
   packages build. It rejects failed CI and cannot borrow another commit's
   results. Alternatively, after the workflows and source reach `main`, dispatch
   `build-downloads.yml` on `main` with the full source SHA and its successful
   validation run ID. That route requires the source already in main history.
3. The two native jobs build on Ubuntu 24.04 x86_64 and Windows Server 2025 x86_64.
   They prepare checksum-pinned optional runtime assets, build from the selected
   clean source and package the default release feature set. Existing Cargo
   locks and byte-verified license provenance remain in force; unresolved
   dependency notice findings reject packaging.
4. Each job validates every archive member and checksum, the exact source
   record, clean-tree status, mandatory runtime files, and the notice inventory.
   The extracted binary must report the expected version, finish its headless
   editing journey and load both bundled native libraries. Disposable
   installation, replacement and rollback checks preserve unrelated profile
   files. No user installation or provider account is used.
5. After both jobs pass, the final job creates a candidate download manifest.
   It binds source/version, the successful validation run, the build workflow
   commit/run/attempt, both archive sizes and SHA-256 hashes, their executable and
   inventory hashes, and their native smoke-receipt hashes.

The Linux smoke uses the real per-user generation installer. Windows uses the
documented portable approach: extract into a new folder, retain the old complete
folder, and switch back when needed. The initial upgrade smoke reinstalls the
same candidate to exercise the replacement transaction. It does **not** claim a
cross-version document/settings migration, and that limitation is retained in
every generated manifest.

Artifacts are retained for 30 days, with attempt numbers in their names. They
are build candidates, not permanent download links. A rerun produces its own
attempt and must be reviewed again; it cannot silently substitute for a
manifest's recorded attempt.
Use **Re-run all jobs** when retrying validation or a package candidate. Required
jobs and both target archives must belong to that complete attempt; a rerun of
only failed jobs does not establish a new complete candidate.
The archive ledger records exact build bytes; it does not assume separate
builds will produce byte-identical archives.

## Review and publish permanent assets

1. Download `omuse-download-manifest-attempt-N` from the completed build. Review
   the manifest alongside both target smoke receipts and diagnostics. Copy the
   generated JSON **without editing its bytes** to
   `docs/releases/downloads/vX.Y.Z.json` for that version.
2. Run the offline checks:

   ```sh
   python3 scripts/test-release-downloads.py
   python3 scripts/test-release-notes.py
   python3 scripts/release-downloads.py --check docs/releases/downloads/vX.Y.Z.json
   ```

3. Merge the exact candidate history into `main` without squashing or rebasing
   away its source identity. Commit the reviewed manifest and release notes to
   public `main`. The
   notes must explicitly say **unsigned** and **experimental** and describe the
   bounded qualification. Publish the matching numbered source prerelease
   through the normal source-release workflow. The package version, source tag
   and tested source must agree.
4. Request **Omuse versioned download publication** (`publish-downloads.yml`)
   using either route below. Its only write permission belongs to the final
   publication job. It verifies the current public declaration, exact successful
   CI attempts, source ancestry, tag, published notes and prerelease state before
   downloading the original build artifacts and checking their bytes again.
5. Publication attaches the two archives, `omuse-X.Y.Z-SHA256SUMS`, and
   `omuse-X.Y.Z-downloads.json`. The manifest is uploaded last as a completion
   marker. GitHub's returned asset SHA-256 digests and sizes must match; final
   readback verifies every asset and the tag. Users can then use the public
   Releases page without signing in to download Actions artifacts.

A complete rerun is read-only. An interrupted upload can resume only with the
same manifest and identical already-uploaded assets. Unknown assets, different
bytes, modified notes, a moved tag or an expired build artifact stop publication.
There is no `--clobber`, asset deletion, release edit, tag creation or tag update
in this publisher. Corrections to an already published binary require a new
version and its own qualification.

### Request publication

With an authenticated maintainer CLI that can dispatch Actions, run:

```sh
gh workflow run publish-downloads.yml --repo Sugata-Software/Omuse --ref main -f version=0.8.0
```

Alternatively, create the branch `publish-downloads/v0.8.0` at the **exact
reviewed public main commit**. This is an explicit publication request, not an
application development branch. The ordinary GitHub branch API or a normal push
can create it; no personal token, additional permission grant, or local `gh`
login is needed when the connected integration already permits repository branch
creation. For example, after reviewing the fetched main commit:

```sh
git fetch origin main
git push origin refs/remotes/origin/main:refs/heads/publish-downloads/v0.8.0
```

Both routes require the workflow commit and checked-out commit to be the exact
current public main. The request branch must still point to that commit. A
modified workflow, a different version, a feature branch, a pull request, or
main advancing while the original artifacts download stops publication before
the first upload. The checks do not infer approval from an application push, and
the request never changes the release's source tag or installer pin.

If main advances, review it and submit a fresh main dispatch or fast-forward the
request branch to that reviewed commit. Do not force-push. When main has not
changed, re-run the existing publication workflow to resume an interruption;
identical assets remain untouched. A completed publication is a read-only no-op
when its request still identifies current main.

The 0.8.0 packages were built in
[run 37161936378](https://github.com/Sugata-Software/Omuse/actions/runs/37161936378)
and recorded in the [reviewed manifest](releases/downloads/v0.8.0.json).
Successful builds and a source prerelease do not start this separate publication
step. Confirm all four permanent assets by their recorded hashes before updating
documentation to say downloads are available.

The source publisher accepts existing download assets only when their names,
sizes and digests match the versioned declaration on public `main`. Existing
source-only releases retain their original no-assets requirement; this pipeline
does not retroactively qualify or alter their binaries.

## User downloads and verification

Permanent files use predictable, versioned names:

- `omuse-X.Y.Z-linux-x86_64.tar.gz`
- `omuse-X.Y.Z-windows-x86_64.zip`
- `omuse-X.Y.Z-SHA256SUMS`
- `omuse-X.Y.Z-downloads.json`

They appear under the corresponding
[GitHub release](https://github.com/Sugata-Software/Omuse/releases), alongside its
source archives and notes. Linux users extract the bundle and run its `install.sh`
against the downloaded archive; the installer defaults to the current user's
`~/.local` and retains the prior generation. Windows users extract the zip and
run `omuse.exe` while keeping its `lib`, `models` and `licenses` folders beside
it. Close Omuse before moving or replacing a portable folder.

The checksum file can be checked with `sha256sum --check` on Linux, or individual
archive hashes can be compared with PowerShell `Get-FileHash -Algorithm SHA256`.
Hashes establish byte identity with the reviewed declaration; the previews are
explicitly unsigned. Windows subject selection still requires the documented
Visual C++ runtime. Optional FFmpeg and provider tools retain their separate
installation and capability requirements.

The build runner establishes the Linux binary's tested baseline. It is not
proof that every Linux distribution supplies compatible system libraries, and
Windows Server CI is not a substitute for interactive Windows 10/11 testing.
Those boundaries remain visible rather than delaying useful preview downloads
until every optional operation is qualified.

## Preparing 0.9.0

Keep the existing 0.8.0 source, tag, download manifest and installer pin intact
while the new editing work is qualified. Select the final 0.9.0 runtime only
after its application checks and applicable desktop review are complete. Push
that immutable commit to `release/0.9.0` to start both exact-source workflows.
Record the completed validation run and complete package-build attempt; retain
their generated manifest and native smoke receipts without editing them.

Once both targets and the local checks pass, merge the candidate history into
main, preserving the tested commit. Prepare the numbered notes, source release
declaration, unchanged generated `v0.9.0.json` and installer source pin together.
The source publisher creates the matching prerelease first. Then dispatch the
download publisher for 0.9.0 or create `publish-downloads/v0.9.0` at that reviewed
main commit. Read back all four assets and verify their hashes before declaring
binary publication complete. Until then, CI artifacts are candidate downloads
and the easy curl command continues to install the previously tested source.
