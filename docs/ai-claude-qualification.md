# Claude assistant qualification — 30 September 2026

This historical record covers one bounded Claude assistant candidate tested
before Omuse 0.4.0. It does not qualify the unchanged 0.3.0 runtime. The combined
0.4.0 release and installer are covered by the
[release qualification](release-040-qualification.md); this live receipt keeps
its original candidate identity.

## Exact candidate

- Public candidate: `31170322e0241380a10c337c3cf18ab11fb41ef0`, tree
  `98351ef8e7c0a43d5c95d098e985ac20e0c7dedd`.
- Production executable SHA-256:
  `598ebb81d475406fedbb3e8ba3a362c5c7ccce16ec4ac8cd96267c2d59061de8`.
- Release build completed with two locked build jobs.
- Installed normal-app generation: `install-hytfk9hx`, clean source receipt,
  19 payload files. It still reports version 0.3.0 because this is a local
  corrective build, not a numbered release.
- Previous complete generation: `install-o_h4lt_d`, public 0.3.0 runtime
  `ea187a9`, retained with all 19 payload files for rollback.
- At that qualification, the public 0.3.0 release and installer pin remained `ea187a9`.

## Automated and protocol evidence

- All 387 local library tests passed with zero failures, including all 14 Claude
  cases and seven provider-discovery cases. Complete exact-source
  [GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36612969908)
  then passed all 910 application tests: 387 library, 248 UI and 275 integration.
  Four manual timing benchmarks were excluded. Editing/Create/motion journeys,
  template variants, recovery, installer and dependency checks also passed.
- The rebuilt qualification client submitted one synthetic assistant request
  through Claude Code 2.1.283 using the existing subscription login and received
  a valid structured response.
- The connection showed **Ready / SubscriptionAllowance**. No password or API
  key was copied into Omuse, and direct API fallback remained disabled.
- Production native Wayland passed all 24 checks at an 800×600 logical viewport
  with reduced motion.

## Installed GUI journey

An exact installed XWayland window was opened without the former launcher PATH
workaround. In **Ask Omuse → Connections**, Claude Code was selected for
**Assistant** and ChatGPT via Codex for **Images**. One synthetic welcome-card
**Design & layout** request was submitted. It returned five editable operations;
Review, **Keep result**, toolbar Undo and toolbar Redo all passed visually.

The connection card then reported the Assistant operation as available and
tested on this runtime. Saved provider preferences read back as Claude Code for
Assistant, Codex subscription for Images and direct API disabled.

Existing artwork windows were left untouched. The new demonstration was not
saved or reopened. Account details, local paths, user artwork and raw captures
are intentionally absent from this public record.

## Boundaries and remaining checks

- This is one synthetic assistant and native-layout journey. Cancellation,
  refinement, follow-up, save/reopen and restored-history behavior were not
  tested with Claude.
- Claude remains unavailable for visual input and image operations. Use Codex
  for Images and for canvas-based Enhance photo and Caption & alt text tasks.
- Grok remains unavailable and unqualified.
- The review thumbnail appeared to compress horizontal proportions compared
  with the correctly kept canvas. Source already requests contain fitting, so
  the cause is unproven. Treat review-thumbnail aspect as a remaining visual
  check; no UI fix is claimed here.
- Clean-host installation, broader prompts and artwork, portability and the
  untested live Claude journeys remain open.
- [Pull request #1](https://github.com/Sugata-Software/Omuse/pull/1)
  originally carried the correction. Exact runtime `3117032` passed full Rust
  CI run `36612969908` and its project-guide check before the combined 0.4.0
  release. Its 910-test result is historical.
