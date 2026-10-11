# Omastrator: focused adoption review

Reviewed 11 October 2026. This is a **partial source review and development
plan**, not a runtime comparison or a claim that Omuse has reached feature
parity. The new Omuse work described below is unreleased; its completed local
checks and remaining release boundaries are recorded in
[canvas workflow qualification](canvas-workflows-qualification.md).

## Which version was reviewed?

- Public repository: [Omastrator](https://github.com/iretonsean/Omastrator),
  pinned main commit
  [`235f058af7b5de8ec3eb5ffc553f60c0e7f8d3a8`](https://github.com/iretonsean/Omastrator/commit/235f058af7b5de8ec3eb5ffc553f60c0e7f8d3a8),
  dated 1 October 2026. Its README describes an alpha.
- The cached GitHub release listing contains
  [v0.1.0](https://github.com/iretonsean/Omastrator/releases/tag/v0.1.0),
  published 30 September. This review did not resolve its tag to a source
  commit or download its packages.
- The [website](https://seanireton.com/omastrator) advertises **0.2.3 beta 1**,
  with `omastrator-beta` packages versioned `0.2.3_beta.1-1` for x86_64 and
  aarch64, installed alongside 0.1. We have not established which source
  produces those packages. Website demonstrations are product claims, not
  proof that the pinned main has the same behavior.

The source snapshot itself shows why this distinction matters: the main
commit makes the last artboard removable, while its README still says the
last artboard cannot be deleted. Source, documentation and released behavior
need separate checks.

## What is useful for Omuse?

| Area | Evidence inspected | Omuse adoption boundary |
| --- | --- | --- |
| Contextual actions | [TaskBar.cpp](https://github.com/iretonsean/Omastrator/blob/235f058af7b5de8ec3eb5ffc553f60c0e7f8d3a8/src/Canvas/TaskBar.cpp) selects a visible context, hides during gestures/text interaction, and positions or pins a bar near the selection. | Add a compact dock on the existing canvas, using existing photo, text, vector and layer commands. Keep placement stable during zoom/resize; guard busy work, masks, gestures, modals and draft state. Selection-following placement and a draggable grip are not part of this first scope. |
| Retained layout | [AutoLayout.cpp](https://github.com/iretonsean/Omastrator/blob/235f058af7b5de8ec3eb5ffc553f60c0e7f8d3a8/src/Document/AutoLayout.cpp) contains frame flow, padding, gaps, alignment, hug/fill sizing and horizontal wrapping. It excludes hidden/absolute children and restricts the frame transform it handles. | Develop bounded editable layout with explicit supported transforms, preserved source objects, transactional Undo and save/reopen tests. Do not label initial support a complete frame/constraint/component engine. |
| AI proposal review | Cached AI design documentation describes headless user-selected agents, proposed changes, acceptance/cancellation and status. [AgentLauncher+Run.cpp](https://github.com/iretonsean/Omastrator/blob/235f058af7b5de8ec3eb5ffc553f60c0e7f8d3a8/src/Agent/AgentLauncher%2BRun.cpp) contains process-group cancellation, timeout and log lifecycle code. | Make scope, pending output, Keep/Discard and failures clearer in Omuse. Retain explicit provider selection, existing stale-result fences and per-operation connection checks. An installed CLI or subscription does not establish image-generation access. |

The cached design-system documentation also describes tokens, component
instances, variants, overrides and code/theme synchronization. Those are
interesting future design tools; the review did not inspect enough of their
implementation to claim compatibility or completeness. No upstream source
was copied as part of this documentation review.

## Deliberate limits

Omuse remains focused on creating and editing content. Live browser/code
editing, deployment, desktop overlays, Hyprland workspace creation and theme
write-back are outside this adoption step. Social calendars and scheduling
remain outside the product scope.

No Omastrator application, test suite, benchmark or agent request was run.
The website video and package payloads were not independently qualified.
This review provides no evidence that either application is faster, more
stable, visually superior or universally better. Omuse's existing released
photo/vector qualification also does not qualify its new toolbar/layout/AI
changes.

The adopted workflows now pass local command/input-routing, narrow-viewport,
Undo, persistence, failure-atomicity and synthetic AI review tests. Native Cua
inspection covers the Linux XWayland interaction path. These checks do not
establish live-provider behavior, Windows interaction, broad hardware support
or performance at resource limits; release qualification remains separate.

## Ongoing comparison

[omastrator-watch.json](omastrator-watch.json) records this partial baseline,
cache hashes and unreviewed areas. The existing monitor now covers five
repositories, including Omastrator; that monitoring scope is broader than
this review's evidence. Future reviews should resolve the beta-to-source
mapping, inspect meaningful PR/issue/workflow changes and compare targeted
source/tests before promoting claims. Proposed PRs, merged development,
released packages and observed behavior must stay separate.
