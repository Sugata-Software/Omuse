# AI provider routing validation — 30 September 2026

This historical record covers source candidate `f972adb`, tested before
Omuse 0.4.0. The combined released runtime and installer are documented in the
[0.4.0 qualification](release-040-qualification.md); the measurements below
retain their original candidate identity.

## Public automated evidence

[GitHub run 36625728469](https://github.com/Sugata-Software/Omuse/actions/runs/36625728469)
passed **945 application tests**: 407 library, 263 UI and 275 integration cases.
Four manual timing benchmarks were excluded. The same workflow passed the
editing/Create/motion journeys, all 80 editable template variants,
interruption/recovery, installer and dependency checks. The project guide check
also passed.

The regression coverage includes:

- Per-task Auto or pinned choices, preferred roles and Auto exclusions.
- Missing or unsupported routes and bounded preference migration.
- Checking every step before dispatch and retaining each selected client.
- Cancellation and source changes stopping unsent steps.
- Keeping the last valid candidate for a single undoable Keep.
- Bounded saved workflow replay and generated-layer identity preservation.
- Naming recorded image outputs in restored reviews while preserving warnings
  for unrelated missing references and leaving caption/alt-text strings intact.

## Product behavior

Each task can use Auto or a named provider. Optional image/edit follow-ons can
add editable layout and caption/alt text, up to three disclosed requests from
one brief. Provider choices remain fixed while the sequence runs. The original
artwork changes only after the user reviews and keeps a valid result.

A pin never falls back silently. Failed requests do not switch providers or
activate separately billed API access. See the
[provider selection guide](ai-provider-routing.md) for controls and examples.

## Release boundary

Automated tests do not establish arbitrary prompt quality, every provider,
clean-host behavior or a numbered release. The current Claude adapter supports
text/layout without image input. Codex supports the visual/image task paths;
Grok remains unqualified. Broader output acceptance, live failure/cancellation
exercises, clean-host installation and desktop accessibility checks remain open.

Machine installation details, account state and private desktop captures are
excluded from this public record.
