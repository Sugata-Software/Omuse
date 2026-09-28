# Compositor Linux: autoplan intake, 2026-09-20

Status: NEEDS_CONTEXT. Intake evidence collected; premise clarification pending.
This is not a completed CEO/design/engineering/DX review or implementation approval.

## Review inputs

- New user brief: compatibility port preserving Swift + C, narrow CoreGraphicsCompat over Skia Raster/Vulkan, Qt Widgets presentation, CPU-addressable document state, no broad filesystem grants; baseline Freedesktop 26.08, Swift 6.4, Qt 6.11.2.
- Historical oracle: a19db9011282399785dc18efcfded904627bdcc2, 2026-09-18, Publish update feed for Compositor 1.0.4. Commit exists locally.
- Current checkout: eng-1-c-abi-seam at 95021eed3ef38533e3dad3bb099a3ab3bc38a22a.
- Base branch: origin/main, from origin/HEAD.
- Initial worktree change: host/SessionWindow.cpp adds Qt::ItemIsEditable in refreshLayers. Preserve this user change.
- UI scope: yes (Qt shell, canvas, layers, menus, dialogs, input).
- DX scope: yes (Flatpak build, Swift/C ABI, dependency pinning, tests, contributor workflow).

No existing plan file was overwritten. The current user message is the authoritative new brief; this file is an intake record, not a verbatim copy of that message. The older restore artifact is a different document and must not be represented as a copy of the new brief.

## Recovered decisions and their limits

Local question history at ~/.gstack/projects/chiddekel-Compositor/question-log.jsonl records approval of the earlier autoplan on 2026-09-19, a vertical-slice ABI approach, and a later explicit choice of SDK Qt6 plus vendored Skia/OpenCV. Those records are prior context, not approval of this revised brief. The newly requested Freedesktop runtime plus bundled Qt differs from the recorded SDK choice.

Earlier review logs and test counts are historical evidence only. No tests were rerun during this intake. Review consensus counts from earlier runs must not be reused for this run.

## Verified differences requiring scope clarification

| Area | New brief | Current evidence | Consequence for review |
|---|---|---|---|
| Packaging | Freedesktop 26.08, separately bundled Qt and Swift toolchain | com.wonderassembly.Compositor.yaml selects KDE 6.10 and Swift SDK extension 25.08 | This is a migration of an existing build choice, not a first scaffold |
| Rendering | CoreGraphics-shaped compatibility over one Skia contract | Sources/CompositorCore/Rendering/DocumentRenderer.swift renders/composites/masks through portable Swift buffers and LayerRenderer | Need inspect full rendering path and estimate convergence; do not declare existing code compliant merely because tests pass |
| GPU | Skia Vulkan plus mandatory Raster fallback | Manifest explicitly sets skia_enable_gpu=false and skia_use_vulkan=false | Existing manifest does not deliver the proposed GPU backend |
| Sandbox | No home/host grants | Manifest grants home, xdg-download and /tmp | Portal and package persistence work remains a release condition |
| Progress | Stage 0 starts with no Linux source | Branch diff against origin/main contains 164 changed files; current sources include portable core, Qt host, C ABI and tests | Preserve useful work and distinguish already implemented behavior from target architecture |

Graph discovery was used first. Coverage metadata matches DocumentRenderer.swift; SessionWindow.cpp metadata changed, so its local diff was read directly. These are bounded findings, not an exhaustive audit of the port.

## Dependency preflight

Official release announcements confirm the existence and stated release dates of [Swift 6.4](https://www.swift.org/blog/swift-6.4-released/) and [Qt 6.11.2](https://www.qt.io/blog/qt-6.11.2-released/). The [Freedesktop SDK tag listing](https://gitlab.com/freedesktop-sdk/freedesktop-sdk/-/tags) includes 26.08.1. This does not establish that the selected Swift archive runs inside that SDK, that the full offline build works, or that the versions are the newest available. OpenCV versions, exact Skia revision, checksums, transitive offline dependencies and runtime ABI remain to verify in the review.

## D1: pending premise clarification

Recommended interpretation: use the new brief as the destination and review a convergence plan from the current implementation, retaining applicable existing work. This includes reassessing the earlier KDE SDK decision and the portable Swift renderer against the newly requested CoreGraphicsCompat/Skia architecture.

Alternative: review only the historical macOS baseline and the supplied plan, treating current Linux implementation as outside the review scope. This answers the historical design question but does not yield a directly actionable migration plan for the current branch.

Common premises already explicit in the user's brief: retain Swift/C; Qt owns presentation; preserve macOS behavior as oracle; CPU state survives GPU failure; do not rewrite C algorithms into OpenCV; parity requires behavior and image evidence.

## Decision Audit Trail

| ID | Phase | Decision | Classification | Principle | Rationale | Rejected |
|---|---|---|---|---|---|---|
| I1 | Intake | Preserve current worktree and perform review only | Mechanical | Explicit over clever | User requested autoplan; local implementation changes predate this run | Resetting branch or altering product code |
| I2 | Intake | Keep historical oracle separate from current implementation baseline | Mechanical | Completeness | Both commits exist and answer different questions | Treating main-era plan as current implementation status |
| I3 | Intake | Do not reuse previous pass counts or consensus as current proof | Mechanical | Explicit over clever | Earlier tests/reviews cover different code and a different plan | Marking phases complete from historical logs |

## Continuation

After D1, capture the working plan restore point, load all four review methodologies and their referenced sections, complete CEO then Design then Engineering then DX, collect independent voices with truthful provider labels, and write the revised plan, registries, test matrix and task list. The current scope question is not final plan approval.

## GSTACK REVIEW REPORT

| Phase | Status |
|---|---|
| Intake | Evidence recorded; D1 pending |
| CEO | Not completed |
| Design | Not started |
| Engineering | Not started |
| DX | Not started |

Verdict: NEEDS_CONTEXT; no implementation or final approval claimed.
