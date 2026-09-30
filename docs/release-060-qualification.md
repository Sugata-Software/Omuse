# Omuse 0.6.0 qualification — 1 October 2026

**Local and exact-source GitHub qualification passed.** Omuse 0.6.0 is installed
on the development host. The release declaration and curl installer select the
same tested public runtime.

The scope is the 0.6 editing, import, text, mask, save and workspace changes
described in the [release notes](releases/v0.6.0.md). This is not a new live AI
provider, comparative performance or broad hardware qualification.

## Exact final candidate

- Public runtime: `a8b7ac70e7513d305a671673a347eecaf2d6cc4c`.
- Git tree: `97ad18c5aa05a916b49af3dff9acc397e105cdfb`.
- [Exact-candidate GitHub workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36784517002): **completed successfully**.
- Production and installed Omuse executable SHA-256: `bf3b8f5f510865fd832d3305b1b2ea40505c5eab6c93bf9fa2d49dc629b41a46`.
- Version: **Omuse 0.6.0**, built with `scripts/build-rust.sh`, locked release
  dependencies and normal production features, without `ui-test`.
- Local and public committed runtime trees match. The installed source receipt
  records the exact public runtime with `source_tree_dirty=false`.
- The [compact receipt](release-060-receipts.json) records source identities,
  executable hashes, payloads and the separate verification runs.

Production code matches candidate
`c9a869df0f256404b6b06f2f9d6f1f3b83e824a0`; the final commit adds the missing
test-only trait import needed to compile its new UI regression.

## Automated checks

The complete GitHub run at `a8b7ac70` passed **1,105 cases: 457 library, 327 UI
and 321 integration**, with zero failures and four ignored timing benchmarks.
It also passed editing/Create journeys, all 80 editable template variants,
motion exports, interrupted collection-save recovery, dependency notice checks
and the separate installer/reference job.

The local aggregate is **1,105 passed, zero failed**, with **four timing
benchmarks ignored**. This combines completed runs with explicit source scope:

| Cases | Result and source |
| --- | --- |
| Library | **457 passed** at `a28ac448cf627f3ee80085d3249e6e5874b7ee0e`; unchanged core results reused |
| UI | **327 passed, zero failed, two ignored** in the final `a8b7ac70` rerun |
| Integration | **321 passed**, with two timing benchmarks ignored, at `a28ac448`; unchanged core results reused |

The earlier complete run covered **36 non-documentation batches** and passed
1,104 cases. The final UI rerun replaces its 326-case UI result; those earlier
UI passes are not counted twice. It includes the added compact-window finishing
regression. The final all-target check also passed. `full-suite.log`,
`final-ui.log` and `final-all-targets.log` are retained in the qualification
evidence.

The complete earlier run also passed the editing journey, Create/export and
motion journeys, and all **80 editable template variants**. It includes the
transparent-foreground Eraser/Clone/Heal fix. The exact-source GitHub run above
independently completed these checks after the final compact-window changes.

## Previous-release interoperability

The verified 0.5.0 production reader and the new 0.6.0 production reader produced
identical decoded RGBA pixels from the synthetic **128 × 96** prior-release
fixture. All **seven prior files stayed unchanged**. The old reader refused a
new format-10 canvas and created no output, matching the documented downgrade
boundary. This verifies the recorded synthetic fixture; it does not establish
independent Photoshop interchange or colour-quality qualification.

## Production desktop and installation

Production **Wayland**, production **XWayland** and the **installed Wayland
launcher** each passed **24 native checks**, including the **800 × 600 minimum
viewport** journey with reduced motion. Coverage includes painting, Undo/Redo,
save/reopen pixel equality, text, masks, editable filters, retained 16-bit source
and export, background photo operations and command-search keyboard execution.
The release owner inspected all three captures: Dither in the dark theme, Bloom
in the light theme and Vignette in the dark theme. The native harness drives
owned GPUI windows; these results do not qualify every physical device or
accessibility path.

The normal launcher selects **Omuse 0.6.0** in `install-brzel9z6`, a complete
**19-file** payload with a clean `a8b7ac70` source receipt. Its Omuse executable
matches the production hash above. The installed native receipt separately
hashes the launcher script as
`1f4792f0bdb0945c05532db4924e3b2a016aaf62b9d6423e2f17596cc7cadfab`.

Rollback to 0.5.0 (`install-p5qqp974`) and return to 0.6.0 both passed their
editing self-tests. Every payload hash was preserved across both generations,
and desktop-file validation passed. Original settings and artwork were left
intact. Existing windows retain their running executable; relaunch to use the
installed update. This is a development-host installation check, not a
clean-machine installation claim.

## Earlier Cua walkthrough — separate evidence

A Cua Driver **0.29.1** walkthrough passed on the earlier **UI-test** candidate
`0a417a81c1b008b94d09c2c3ef285b2695e39607`, executable SHA-256
`980bfd6318e75f86733f9c8f02475a33c40429032ad88a9dee277c72185492b2`.
It used **XWayland foreground input** and visually inspected exact-window
screenshots. It covered command search, Dither search and opening, ASCII style
selection, scrolling to lower controls, Cancel returning to unchanged synthetic
artwork and closing the isolated window. The receipt lists seven checks and six
captures at **1916 × 1170 device pixels**.

The local artifact is
`qualification-060-20261001/layout-review/cua/receipt.json`, SHA-256
`1907914c4fa76fea9dd461a19d6d2c7fdcdb767dce44d9b894ff508ce338686b`.
Background input was ineffective and an AT-SPI tree was unavailable. Tiling
ignored the resize request, so this is **not minimum-window, final production,
installed-launch or final responsive-layout qualification**. The completed
final-candidate checks above provide separate evidence for those paths.

## Compatibility and release boundaries

- Canvases read formats **1–10** and write **format 10**, including collection
  pages. Omuse 0.5.0 and earlier cannot read new format-10 saves. Opening an
  older file does not rewrite it; use **Save As** to preserve a previous copy.
  Application rollback does not downgrade artwork. Collection schema v2 is a
  separate version boundary.
- PSD/PSB import supports bounded **8-bit RGB** input. CMYK, 16/32-bit documents
  and embedded ICC profiles are refused. Supported simple Photoshop text is
  editable with cached appearance retained until an actual text edit;
  unsupported text uses a reported raster fallback. These checks do not
  establish complete Photoshop interchange.
- SVG/SVGZ imports produce **one raster layer**, preserve the source and do not
  import external resources, scripts or animation. The limits are **16 MiB**
  input/expanded data, **25 MP** output and **30,000 pixels per side**.
- Finishing effects and Camera Raw operate on **8-bit raster copies up to
  16 MP**. Retained RAW/16-bit originals do not make these paths sensor-RAW,
  HDR or high-bit-depth processing. Bloom stays inside the existing layer.
- This remains an **Arch/Omarchy Linux x86_64 source pre-release**. No
  comparative speed result, new live-provider success, portable binary support,
  broader hardware qualification or independent image-quality result is claimed.

## Publication decision

**Qualified for the declared source pre-release.** Exact-source GitHub, local
runtime, desktop, installation, interoperability and rollback checks passed.
The development host is on 0.6.0 and the curl installer selects the tested
runtime. The broader stable-release and binary-distribution gates remain in
the [release-readiness record](public-release-readiness.md).
