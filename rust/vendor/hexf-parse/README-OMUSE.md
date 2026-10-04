# Pinned upstream source update

This directory is the parser from authoritative upstream revision
[`41f0018229c1ee3d6fd813b6808d1ad1f506554c`](https://github.com/lifthrasiir/hexf/tree/41f0018229c1ee3d6fd813b6808d1ad1f506554c),
which declares and includes the author-supplied 0BSD license. It replaces the
older registry archive in Omuse's Cargo graph; it is not a license override for
the older CC0-1.0 archive.

`Cargo.toml` and `src/lib.rs` are byte-for-byte upstream `parse/` files. Upstream
`parse/LICENSE` links to `../LICENSE`; this directory retains the target's exact
text as a regular file so it also works in Windows source checkouts. The hashes
and source paths are recorded in `OMUSE-PROVENANCE.json` and verified by
`scripts/test-dependency-replacements.py`.

Upstream retained version 0.2.1. Relative to the published registry source, the
only runtime code change is upstream's replacement of `powf` with `libm::exp2`
when constructing the parsed float. `libm 0.2.16` was already in Omuse's lockfile.
The upstream unit/doc tests and additional bit-exact finite-value regression
samples cover the update. Full graphics and release qualification remain
necessary for a new application binary.
