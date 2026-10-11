# Upstream adoption review

Omuse monitors five repositories: [OmaPhoto](https://github.com/ZacharyZhang-NY/OmaPhoto),
[PhotoCraft](https://github.com/storytold/photocraft),
[VectorCraft](https://github.com/storytold/vectorcraft),
[RAWmakase](https://github.com/pch/rawmakase) and
[Omastrator](https://github.com/iretonsean/Omastrator).

This is an adoption index, not a fresh, complete audit of all five repositories.
Each watch keeps its own reviewed identity and access failures. The monitor
checks whole repositories daily, including commits, branches, releases,
issues, pull requests, tests and packaging. It reports meaningful changes and
keeps routine unchanged results quiet.

## Current priorities — 11 October 2026

| Priority | User benefit and remaining gap | Effort / integration risk | Decision and evidence |
| --- | --- | --- | --- |
| Contextual canvas actions | Make existing photo, text, pixel selection and vector tools easier to reach. Existing command dispatch already owns prerequisites and Undo. | Small / input routing and focus need interaction tests. | Adopt in current development branch. [Source review and bounds](omastrator-adoption.md). |
| AI proposal comparison | Compare original/result and retained alternatives without repeatedly changing views. Existing provider routing, Keep/Discard and stale-source checks remain authoritative. | Medium / preview framing, memory and stale results. | Adopt locally bounded comparison and difference previews. No new provider entitlement or live-provider qualification is implied. [Review](omastrator-adoption.md). |
| Retained auto-layout | Keep mixed photo/text/vector spacing when content changes. Existing Create anchors and distribution do not retain row/column rules. | Larger / shared editor transactions, linked masks, resize semantics and native format compatibility. | Adopt a bounded direct-child frame model; qualify before release. Nested frames and general constraints remain deferred. [Review](omastrator-adoption.md). |
| Broader design systems | Extend existing Brand kits and components with richer variants and shared tokens. | Larger / component migrations and override semantics. | Investigate targeted gaps only; not part of this implementation. [Review](omastrator-adoption.md). |
| Browser and code deployment | Live web frames and deployment are separate product workflows. | Large / runtime, permissions, distribution and support costs. | Defer. Omuse prioritises native content creation. |

The first three adopted workflows were independently implemented in Rust;
no Omastrator source was copied. Its reviewed public source and website beta are distinct evidence.
Their [local qualification](canvas-workflows-qualification.md) now records
1,599 passing tests, native inspection, acceptance journeys and older-reader
compatibility. They remain unreleased development work.
The [partial watch record](omastrator-watch.json) lists unreviewed areas.

## Earlier source-specific work

- [OmaPhoto comparison](omaphoto-comparison.md) and [watch](omaphoto-watch.json).
- [PhotoCraft / VectorCraft roadmap](photo-vector-roadmap.md) and
  [qualification](photo-vector-qualification.md): released photo/vector work
  is tied to its own candidate and does not qualify these new canvas changes.
- [RAWmakase roadmap](rawmakase-roadmap.md): retain source and capability
  boundaries when evaluating RAW processing improvements.

Future monitor runs must inspect current sources and coverage before advancing
any baseline. Roadmaps, release announcements and test filenames alone do not
establish working behavior, performance or visual quality.
