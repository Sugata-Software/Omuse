# Linux parity: evidence and remaining work

Review date: 2026-09-25. The [UI integration follow-up](linux-ui-integration.md) now connects shortcut recording/persistence, native layer drops, shared color-picker presentation and the listed tool gestures. Follow-up interaction fixes and reproducible optimized
verification are described in [linux-release-readiness.md](linux-release-readiness.md).
This includes typed picker dispatch, focus-preserving header updates, bounded
window callbacks and real Qt pointer/key journeys. It does not close every gap
in the table below.

The reviewed Linux base is
`8ac58a71983ec36129e11179a9bee356b351f18b` (`GNU_Linux`). Its protected Mac
source trees match `75c4219`. Mac `main` has since reached
`679e66f` (v1.2.4). This document does not claim complete feature, visual,
performance or project-interchange parity.

## Source reuse is not runtime parity

The fork now compiles shared Mac code through `Sources/Compat` and platform
adapters. That reduces duplicated editing logic, but compiling a view does not
prove its interactions work. `MACOS_UI_PARITY_RULES.md` correctly requires actual
screenshots and interaction verification before claiming 100% parity. The older
percentage matrix should not be used as release evidence.

| Area | Evidence at the reviewed base | Acceptance work still required |
|---|---|---|
| Shared document/editing logic | Protected `Compositor/` and `CompositorTests/` match the pinned Mac revision | Execute supported tests with the actual renderer; record exclusions separately |
| Layer interactions | Native Qt reorder/nesting, folder trees, multi-layer moves and copy use shared validation/undo | Cross-document drops, mask/effect-row copying and Mac comparisons remain |
| Gestures and contextual menus | Qt layer menus and generic context-menu/tap/submit handlers are connected; concrete shape/gradient/crop paths are integrated | Generic `gesture`, `onDrop`, `simultaneousGesture` and popovers remain incomplete; broader reachable-control audit and physical gesture checks remain |
| Shortcut recording | Visible Qt recorder uses shared definitions, conflict validation and persistence; restart readback is tested | Remaining Mac-only responder commands are excluded from the Linux recorder |
| Text, typography and layout | Shared controls plus platform text/font adapters | Reference documents with installed/missing fonts, paragraph wrapping, transforms and glyph comparison on Mac and Linux |
| Camera Raw and native sliders | Camera Raw processing source is shared; the slider is a platform override | Known RAW fixtures, decode capability, slider hit-testing and developed-image comparisons; exclusion of AppKit slider tests is not parity |
| v1.2.4 finishing filters | Newer Mac source adds standalone Vignette, Bloom / Glow and Tonal Contrast | Merge upstream changes through the fork's normal process, expose controls and validate pixels/undo/export |
| Persistence | The reliability patch adds atomic replacement and failure tests in the Linux Foundation adapter | Filesystem/portal matrix, power-loss durability protocol and concurrent-writer policy are separate work |
| GPU behavior | The reliability patch bounds the brush fence wait and exercises failure/teardown | Effects-driver faults, prolonged sessions, device changes and end-to-end interaction latency |
| Clipboard, tablet and desktop integration | Platform adapters exist | Real Wayland/X11 clipboard peers, file portals, pressure/tilt/eraser, fractional scale and mixed-monitor transitions |
| Mac project interchange | Same source format reduces drift, but platform pixels/fonts/codecs differ | A Mac must open Linux outputs and vice versa; compare composites and editability using versioned fixtures |

These are source-derived gaps and verification requirements, not a complete
inventory of all bugs. A Qt host path may implement an operation separately from
an inert SwiftUI modifier; the visible route still needs to be demonstrated.

## Contribution boundaries

Contributions should target `GNU_Linux` first when they change compatibility
modules, Qt adapters, Vulkan, Flatpak packaging or Linux test infrastructure.
Shared-domain bugs should be reproduced against the current Mac source and
proposed separately upstream, without Linux-specific edits to protected trees.

Keep contributions reviewable:

1. Reproduce the problem at a pinned fork revision.
2. Keep the fix behind the appropriate platform adapter.
3. Include regression tests that exercise failure behavior as well as success.
4. Record the exact environment, skipped tests and broader-suite failures.
5. Avoid local deployment ledgers, private paths, signing keys, update services
   and unrelated feature bundles in a public patch.
6. Ask for maintainer review before treating the patch as a supported release.

The save/GPU contribution and focused commands are described in
[Linux reliability checks](linux-reliability-checks.md). Those checks establish
specific behavior; they are not certification of the whole app.
