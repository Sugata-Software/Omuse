# Rust dependency license-text findings

`scripts/rust-license-inventory.py` inventories the locked, offline Cargo graph. It does not certify license compliance. The curated notices in [`rust/licenses/dependency-overrides/`](../rust/licenses/dependency-overrides/) exist only when a published crate archive omits a legal-text filename and upstream evidence ties retained text to the released source.

The current override manifest covers 29 crates in 22 exact published source revisions. Every entry records its repository URL, 40-character published VCS revision, allowed crate name/version/license expression, exact notice-source URL, and SHA-256 of the retained text. The inventory validates the package manifest repository (or upstream homepage where a repository is absent) and `.cargo_vcs_info.json` revision. The Pathfinder source-header override also validates the full published source-file hash.

Most overrides take legal text from the exact published revision. Two exceptional overrides use a legal-file-only commit whose direct parent is the exact published revision. Their manifests separately record the legal-text revision, direct parent, and commit URL; the inventory rejects malformed provenance or a legal-file URL that does not contain that revision. The evidence for accepting those two fixes is documented below rather than treating a later generic licence as interchangeable.

The locked Linux x86_64 graph was rechecked for public-release preparation on 28 September 2026. A fresh offline inventory recorded 608 dependencies and reduced `no_legal_text_found` from four dependencies to two. No generic licence template, guessed copyright line, or text from an unrelated project was added.

## Retained source-header notice

`pathfinder_geometry 0.5.1` declares `MIT/Apache-2.0`, identifies `https://github.com/servo/pathfinder`, and records VCS revision `a5e98fac00f433fe2ff4b2135383d82491b8bfc5`. The public GitHub source URL for that revision now returns HTTP 404, so no repository-root legal file can be recovered from it. Its published crate nevertheless contains an exact copyright and dual-license grant at `src/lib.rs` lines 1–9. The retained [`pathfinder-geometry-NOTICE.txt`](../rust/licenses/dependency-overrides/pathfinder-geometry-NOTICE.txt) is that unaltered header. The override records the source link, the notice SHA-256 `a830c3e1ca7ae32b804500428e0dd1fbaf34e997174af1ff2296227bd537c0d3`, and the full published-file SHA-256 `cedfb2d85e682e9b9e327a169347bf0cebecd52668e1ed91a3af05ec91b42090`; the inventory rejects it if either source identity or file content differs.

## Accepted direct-successor legal fixes

| Crate | Exact release source | Upstream legal-text provenance | Retained text |
| --- | --- | --- | --- |
| `seahash 4.1.0` | Declares `MIT`; repository `https://gitlab.redox-os.org/redox-os/seahash`; published VCS `94b632aeac099031c373599313d5b5f0acbbaec0`; crate SHA-256 `1c107b6f4780854c8b126e228ea8869f4d7b71260f962fefb57b996b8959ba6b`. | Commit [`3088c5c`](https://gitlab.redox-os.org/redox-os/seahash/-/commit/3088c5c912b70b586d27bf553fbe964e025a2c89) has the published revision as its sole direct parent, is titled `fix: add missing MIT license text`, and adds only `LICENSE`. Jeremy Soller then merged that exact change into the authoritative upstream branch in commit `74c02182146d1edd7c91a9e6eddefc7390682a70`. | [`seahash-MIT.txt`](../rust/licenses/dependency-overrides/seahash-MIT.txt) is byte-for-byte identical to that added file, SHA-256 `23f18e03dc49df91622fe2a76176497404e46ced8a715d9d2b67a7446571cca3`. |
| `simd_helpers 0.1.0` | Declares `MIT`; repository `https://github.com/lu-zero/simd_helpers`; published VCS and `v0.1.0` commit `ca1a2f84aa386d758e98f8a609d990263932fb85`; crate SHA-256 `95890f873bec569a0362c235787f3aca6e1e887302ba4840839bcc6459c42da6`. | Commit [`8204019`](https://github.com/lu-zero/simd_helpers/commit/82040194cd05affb060bf94d6f19f82a771d07fb) has the published revision as its sole direct parent and adds only `LICENSE`. The commit was accepted and committed by crate author Luca Barbato, and its text names `Copyright (c) 2019 Luca Barbato`. | [`simd-helpers-MIT.txt`](../rust/licenses/dependency-overrides/simd-helpers-MIT.txt) is byte-for-byte identical to that added file, SHA-256 `d69f24ad84ec2ade64c0b68bdb31b41170e997b158370342056918329cc9af1e`. |

These entries retain upstream-authored legal text and exact commit relationships. They do not assert licence compatibility or convert a licence declaration into replacement boilerplate.

## Remaining `no_legal_text_found` findings

| Crate | Published metadata and source identity | Complete upstream evidence | Reason it remains a finding |
| --- | --- | --- | --- |
| `hexf-parse 0.2.1` | `CC0-1.0`; repository `https://github.com/lifthrasiir/hexf`; VCS and `0.2.1` tag `4225763d744183d720f575ae96d04161b4d08ea0`; crate SHA-256 `dfa686283ad6dd069f105e5ab091b04c62850d3e4cf5d67debad1933f55023df`. | The complete published archive contains only `Cargo.toml` and `src/lib.rs`. The complete exact Git tree has no legal-text file or source grant. The author later relicensed the project from `CC0-1.0` to `0BSD` in commit `8a14eb63c3823b0ed5a04a4a200a03ccde648a0d`, then added 0BSD text; that different licence does not supply the missing CC0 terms for 0.2.1. | The exact source supplies only its SPDX declaration. There is no attributable CC0 legal text to retain. |
| `mac 0.1.1` | `MIT/Apache-2.0`; repository `https://github.com/reem/rust-mac.git`; public `0.1.1` tag and source commit `66afc663b68a65633ea165c742b5a9c6734581c2`; crate SHA-256 `c41e0c4fef86961ac6d6f8a82609f55f31b05e4fce149ac5710e439df7619ba4`. The older archive has no `.cargo_vcs_info.json`. | The complete tag and public repository history contain no legal-text file. Author Jonathan Reem's earlier commit `6b35d271ca8783a513aaa9bb47c2a926a68107d2` changes the declaration to dual MIT/Apache-2.0, and the packaged README says `MIT/Apache-2.0, like rust itself`, but neither supplies either licence's terms. | A declaration and comparison to another project are not substituted for the missing terms. |

The two findings remain visible in the generated inventory and keep the repository's current public-binary notice gate open. The safest closure for either crate is an upstream legal file or source-header grant explicitly tied to the published release. If that cannot be obtained, move the locked dependency path to a release whose published source includes attributable terms, or remove the dependency from the application graph. The current application path to `hexf-parse` is `gpui-kit 0.6.6 → gpui-pre-platform 0.3.6 → gpui-pre-linux 0.3.6 → gpui-pre-wgpu 0.3.6 → wgpu 29.0.4 → naga 29.0.4 → hexf-parse 0.2.1`; update that graphics stack only after confirming the resulting locked graph and graphics behavior. The shortest current path to `mac` is `gpui-kit 0.6.6 → gpui-base 0.6.6 → html5ever 0.27.0 → mac 0.1.1`; update or replace that HTML parsing path and re-run its regressions. A project-local fork is only suitable if the relevant upstream rights holder supplies the missing text; adding a licence file locally without that evidence would not close this finding.

## Windows x86_64 development package

`scripts/package-rust.ps1` runs the same inventory with `--target x86_64-pc-windows-msvc`. Overrides for crates that are locked but outside that target's graph, such as the Wayland and X11 backends, are skipped there; an override whose crate has left the lock is still rejected. On 2 October 2026 the Windows inventory recorded 527 dependencies. `accesskit_windows 0.34.0` was published from AccessKit revision `c88605b96d04431f9c3c792464a0f2f253480e94` with `MIT OR Apache-2.0`, the same source and expression as the existing `accesskit-platform` override, so it joined that allowlist. Four `no_legal_text_found` findings remain:

| Crate | Windows path and published identity | Reason it remains a finding |
| --- | --- | --- |
| `mac 0.1.1` | The same finding as Linux, above. | As above. |
| `lcms2-sys 4.0.7` | Windows-only direct dependency that compiles LittleCMS statically. Declares `MIT`; repository `https://github.com/kornelski/rust-lcms2-sys.git`; published VCS `78a8d8b20c37da2f7be38d2e39d6432fd15aa88d`. The repository's `LICENSE` is a symbolic link to the `vendor` submodule's LittleCMS licence, which the crate archive ships as `vendor/LICENSE` (SHA-256 `6dbd60437f8ef91d8de1f08ad75882547fd4931bfcc3566a0735f28db1484d31`). | The package includes that LittleCMS MIT text as [`rust/licenses/windows/LittleCMS-MIT.txt`](../rust/licenses/windows/LittleCMS-MIT.txt) for the compiled library. The submodule revision has no stable source URL for an override, so the binding crate's own entry stays visible. |
| `zune-jpeg 0.4.21` | `gpui-kit 0.6.6 → gpui-component 0.6.6 → resvg 0.45.1 → zune-jpeg 0.4.21`, resolved only for Windows with all features. Published VCS `fa2c767a01d7d9373911d0bf63e0588553d67e0e`. | Its `LICENSE.md` at that revision differs from the retained `zune-inflate` text; no reviewed override has been added for it yet. |
| `zune-core 0.4.12` | Through `zune-jpeg 0.4.21`. Published VCS `f8fbb123d5ed04441e8324a555bfcda0cb1bd28f`. | Its manifest declares no repository or homepage, so the inventory cannot verify an override's origin. |

The Windows zip is an unsigned development artifact. These findings keep the same public-binary notice gate open for it.

A future override needs the released crate's exact repository or tag/revision and one of these evidence paths:

1. legal text in the exact published revision;
2. a copyrighted source header in the published archive containing a licence grant; or
3. a direct-successor, legal-file-only upstream correction whose commit history clearly identifies the missing text and whose exact parent is the published revision.

Copying a licence from an unrelated later revision, another project, the SPDX catalogue, or a generic template does not establish that it applies to the release and is intentionally rejected by this inventory policy.
