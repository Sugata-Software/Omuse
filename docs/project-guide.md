# Omuse project guide

The static visual guide groups implemented workflows and planned work
into eight categories. It distinguishes local test evidence from implemented
but unqualified routes, experiments, plans and release blockers. It never
calculates an invented percentage of release readiness.

[`project-status.json`](project-status.json) is the reviewed content catalogue.
[`project-guide.fragment.html`](project-guide.fragment.html) is the self-contained
conversation fragment. It uses local category selection and evidence
disclosures, makes no network requests and contains no provider credentials.
The host conversation supplies the theme and base controls; this fragment is
not a separately hosted public website.

When a feature changes or a qualification run completes:

1. Update its status, boundary and evidence paths in the JSON catalogue.
2. Record which tests or build the claim applies to. A source/UI path alone
   cannot establish successful live provider operation or release readiness.
3. Refresh the tracked fragment and the current conversation's copy:

```sh
python3 scripts/update-project-guide.py
python3 scripts/update-project-guide.py --check
python3 scripts/update-project-guide.py --output /absolute/path/to/current-conversation/omuse-project-guide.html
```

The visual is a dated snapshot. It does not poll the application or update from
unreviewed source automatically. Future work should update this catalogue and
refresh the conversation guide as part of its completion record.

The `Omuse project guide` CI workflow checks that the tracked fragment matches
the catalogue when either changes. It detects stale generated data; reviewing
feature claims and evidence remains part of the implementation work.
The same workflow checks the generated [keyboard reference](keyboard-shortcuts.md)
against `rust/src/shortcuts.rs`. Regenerate it with
`python3 scripts/generate-keyboard-shortcuts.py` when changing command definitions.

Numbered release notes live under [releases/](releases/README.md), with a concise
history in [CHANGELOG.md](../CHANGELOG.md). The guide describes the current
project state, while each release note retains its own tested runtime and limits.

## Private hosted copy

On a configured development host, `omuse-dashboard-refresh` validates the
generated fragment and atomically refreshes its standalone HTML export. Run
it after updating the guide. A Tailscale Serve route can point to that single
file and retain the same private URL as its contents change.

Keep the hostname, serving configuration, export location and verification
receipts in machine-local deployment evidence, outside the public source tree.
Use private Serve with public Funnel access disabled. Do not serve the
repository or evidence directory, or reset unrelated Tailscale routes.
