# Source provenance

The public Omuse repository begins with a source snapshot of the existing
development project. Earlier development branches and legacy release tags are
preserved separately; they are not public Omuse releases.

## Application lineage

- Original application: [robbietilton/Compositor](https://github.com/robbietilton/Compositor).
- Earlier Linux fork: [chiddekel/Compositor](https://github.com/chiddekel/Compositor).
- Active application: the Rust/GPUI implementation under `rust/`, developed as
  Omuse by Sugata Software. The preserved Swift/Qt trees are historical reference
  and compatibility material; see the [legacy README](legacy-compositor-readme.md).
- The development snapshot before this publication preparation was
  `db34c1713a95213e59a87dc1b8f3b72828f2fff0`. The application runtime was last
  qualified at `b178ad273ae9627836b753d584925f3bf7e1a003`; later changes updated
  documentation and qualification tooling. These are development-history
  identifiers, not public release tags or a substitute for a fresh release build.

The original MIT copyright notice in [LICENSE](../LICENSE) is retained.
Third-party code, fonts, media and optional runtime assets retain their own
terms and attribution. The source snapshot does not change their ownership or
relicense them under the application licence.

## Dependency and media provenance

`rust/Cargo.lock` pins Rust dependencies. Vendored code retains its notices;
runtime notices are in `rust/licenses/`. The
[dependency notice findings](rust-license-findings.md) identify the four
unresolved legal-text entries that still block the planned binary distribution.
The [public media credits](media/README.md) cover the README screenshot and
separate promo media. Branding images retain embedded generation provenance
where present.

Historical design and test documents describe the state at their recorded
dates. They are not current release promises. Use the
[public release checklist](public-release-readiness.md) and its linked final
qualification records for the present evidence and remaining work.
