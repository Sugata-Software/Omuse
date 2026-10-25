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

## 0.12.0 candidate — publication pending

The current 0.12.0 package candidate is tied to exact public source
[`9c684cae4a4ccdb763d036b65adc440cc61419c2`](https://github.com/Sugata-Software/Omuse/commit/9c684cae4a4ccdb763d036b65adc440cc61419c2),
[validation run 40161067985](https://github.com/Sugata-Software/Omuse/actions/runs/40161067985)
and [package run 40161068022](https://github.com/Sugata-Software/Omuse/actions/runs/40161068022).
Final run receipts and the reviewed download manifest are still pending, as are
source publication, download publication and anonymous verification of every
permanent asset. These identifiers describe the candidate under review; they do
not make an Actions artifact a permanent public download.

There are no announced 0.12.0 download links in this guide yet. Continue to use
the published 0.11.0 assets below until the 0.12 tag, notes, checksums, manifest,
installer pin and attached files pass final readback. The
[0.12 qualification record](release-0120-qualification.md) retains the open
gates and platform limits.

## Published 0.11.0

The [0.11 qualification](release-0110-qualification.md) identifies exact source
`e72bccdcef066cb6ab60a99faa9d7609de86c2e4`, validation
[38130013300](https://github.com/Sugata-Software/Omuse/actions/runs/38130013300)
and package build
[38130013292](https://github.com/Sugata-Software/Omuse/actions/runs/38130013292).
Both target archives passed integrity, source identity, notices and native
runner smoke. The Linux package also passed host Wayland/XWayland and
0.10 → 0.11 → rollback → forward checks.

[Source publication 38132530557](https://github.com/Sugata-Software/Omuse/actions/runs/38132530557)
and [download publication 38132594953](https://github.com/Sugata-Software/Omuse/actions/runs/38132594953)
both succeeded. The [0.11 release](https://github.com/Sugata-Software/Omuse/releases/tag/v0.11.0)
now carries these reviewed downloads:

- [Linux x86_64 bundle](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-linux-x86_64.tar.gz)
- [Windows x64 ZIP](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-windows-x86_64.zip), unsigned experimental
- [SHA-256 checksums](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-SHA256SUMS)
- [Download manifest](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-downloads.json)

All four permanent assets were downloaded anonymously and matched the reviewed
sizes and SHA-256 hashes. Manifest and checksum bytes matched exactly. The source
tag, notes and public installer pin also passed readback; the prior 0.10 release,
its tag, notes and all asset identities remain unchanged. The normal Omarchy
installation is verified at 0.11.0 with complete prior generations preserved.
See the [qualification record](release-0110-qualification.md) for scope and the
[installation guide](install.md) for the source-build curl path.

### Windows setup wizard — 12 October 2026

The [Windows installer](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-windows-x86_64-setup.exe)
packages the exact existing Windows ZIP, with Start menu/optional desktop
shortcuts, upgrade, repair and uninstall. Its [separate reviewed receipt](releases/installers/v0.11.0.json)
and [installer checksums](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-windows-x86_64-setup.exe.sha256)
identify the additive package; the original four assets and their checksums,
release tag and numbered notes remain unchanged. The runtime stays at 0.11.0.
[Publication 38196166372](https://github.com/Sugata-Software/Omuse/actions/runs/38196166372)
succeeded; all three new public downloads matched the reviewed bytes exactly.

[Windows build 38193823269](https://github.com/Sugata-Software/Omuse/actions/runs/38193823269)
passed all 14 lifecycle checks, including fresh installs, a real 0.10 → 0.11
upgrade, repair, downgrade refusal, shortcut targets, installed editing,
DLL loading and uninstall preserving project/profile sentinels. See the
[setup guide](windows-installer.md) for requirements, use and qualification limits.
Windows remains unsigned experimental; clean standard-user and interactive
Windows 10/11 acceptance are separate from Windows Server CI.

## Published 0.10.0 — historical

The [0.10 release](https://github.com/Sugata-Software/Omuse/releases/tag/v0.10.0)
is published from exact source `920ae0fddb9a58ba69526b7e7976b5d770435fd9`.
[Validation 38097640432](https://github.com/Sugata-Software/Omuse/actions/runs/38097640432)
and [package build 38097640470](https://github.com/Sugata-Software/Omuse/actions/runs/38097640470)
passed for that source. [Source publication](https://github.com/Sugata-Software/Omuse/actions/runs/38099819987)
and [download publication](https://github.com/Sugata-Software/Omuse/actions/runs/38099884639)
completed successfully. All four permanent assets were downloaded anonymously
and matched the reviewed sizes and SHA-256 hashes. The generated manifest bytes,
source tag and release notes are unchanged. See the
[release qualification](release-0100-qualification.md) for final native checks,
cross-version evidence and remaining platform limits. The prior
[0.9 release](https://github.com/Sugata-Software/Omuse/releases/tag/v0.9.0)
and its tags, notes and assets remain unchanged.

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
gh workflow run publish-downloads.yml --repo Sugata-Software/Omuse --ref main -f version=0.11.0
```

Alternatively, create the branch `publish-downloads/v0.11.0` at the **exact
reviewed public main commit**. This is an explicit publication request, not an
application development branch. The ordinary GitHub branch API or a normal push
can create it; no personal token, additional permission grant, or local `gh`
login is needed when the connected integration already permits repository branch
creation. For example, after reviewing the fetched main commit:

```sh
git fetch origin main
git push origin refs/remotes/origin/main:refs/heads/publish-downloads/v0.11.0
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

The historical 0.9.0 packages were built in
[run 37207585335](https://github.com/Sugata-Software/Omuse/actions/runs/37207585335)
and recorded in the [reviewed manifest](releases/downloads/v0.9.0.json).
Separate [download publication 37210058961](https://github.com/Sugata-Software/Omuse/actions/runs/37210058961) completed after source publication;
all four permanent assets passed anonymous size/SHA-256 verification.
The historical 0.8.0 manifest, tags, notes and assets remain unchanged.

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

## Published 0.9.0

The exact tested source is `65c95cc165f9d730a9f0bcea51c51a84cf4adb7e`. Complete validation 37207585304
and build 37207585335 passed before source publication. [Source publication 37209968347](https://github.com/Sugata-Software/Omuse/actions/runs/37209968347)
created the numbered prerelease, then [download publication 37210058961](https://github.com/Sugata-Software/Omuse/actions/runs/37210058961)
attached the four immutable assets. Anonymous verification matched each size
and SHA-256 and the original manifest bytes; tag and notes were unchanged.

Use the [0.9.0 release page](https://github.com/Sugata-Software/Omuse/releases/tag/v0.9.0),
[checksums](https://github.com/Sugata-Software/Omuse/releases/download/v0.9.0/omuse-0.9.0-SHA256SUMS)
and [installation guide](install.md). The curl command still builds the tested
source revision; downloadable bundles are a separate option. Windows remains
unsigned experimental, and the platform boundaries above still apply.

For each later release, repeat the exact-source build, declaration and separate
publication sequence above. Keep existing versions immutable and do not declare
download availability until anonymous asset verification completes.
