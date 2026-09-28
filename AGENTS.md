# Project guide maintenance

Keep the visual project guide current when feature scope, qualification results
or release readiness changes. The user uses it as the overview of Omuse.

- Update `docs/project-status.json` with the current behavior, limits and
  evidence. Distinguish implemented code, local test results, live provider
  qualification, planned work and release blockers.
- Refresh `docs/project-guide.fragment.html` with
  `python3 scripts/update-project-guide.py`, then run the same command with
  `--check` to detect stale embedded data.
- When showing the guide in chat, use the visualization skill and refresh a
  copy in the current conversation's permitted visualization directory with
  `--output`. Keep its theme-aware layout and local category controls.
- Keep historical measurements labeled with their candidate and scope. Do not
  promote a feature to qualified merely because its source or UI exists.
- If `omuse-dashboard-refresh` is installed on the current host, run it after
  regenerating the guide so the private hosted dashboard stays current too.
  Hosting details and credentials stay outside the source repository.

See `docs/project-guide.md` for the update workflow and
`docs/public-release-readiness.md` for the public-release gates.
