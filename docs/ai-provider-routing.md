# Choose AI providers by task

> **Unreleased source candidate:** this routing and sequence UI is implemented on
> the feature branch. Candidate `f972adb` passed the complete public CI workflow
> with 945 application tests. See the [validation record](ai-routing-qualification.md).
> It is not part of the published Omuse 0.3.0 release or installer.

Open **Ask Omuse → Connections** to choose the subscriptions Omuse should
prefer. **Assistant** is the starting choice for text and layout work;
**Images** is the starting choice for generation and image edits. **Auto ·
allowed** lets a connection participate when Auto needs another capable route;
choose **Auto · excluded** to keep it out of automatic selection.

Each task also has an **Auto · provider ▾** control. Open it under **Provider for
…** and choose:

- **Auto** to use the preferred capable subscription, then another allowed,
  ready connection when necessary.
- A named provider to pin that task. A pin never falls back to another provider
  when the selected connection is unavailable or unsupported.

The choice is saved per task. Changing a preferred connection affects Auto
tasks; it does not replace pins. Failed requests are never resubmitted through
another provider automatically. A route marked **first use** runs only after
you explicitly submit and may consume subscription allowance while Omuse tests
that capability.

Current adapter limits are deliberate:

- **ChatGPT via Codex** is the current visual route for image generation,
  image editing, **Enhance photo**, and **Caption & alt text**.
- **Claude Code** can handle text and **Design & layout** without image input.
- Grok has no qualified runtime and remains unavailable.
- Omuse does not copy passwords or API keys and does not switch to a separately
  billed API fallback.

## Optional follow-on steps

For an image or image-edit task, **FOLLOW-ON STEPS · optional** can add **Then
arrange the layout** and **Then draft caption & alt text**. A sequence runs at
most three displayed requests in order: the chosen task, editable layout, then
caption. It uses one brief, one variation, and produces one final review. Layout
finishing edits the current page with native text, shapes, placement, resize and
animation; collection-wide page, resource and component operations belong in a
standalone Design & layout request. Each
step shows its provider and may use that subscription's allowance. You can open
the step row to change that task's Auto or pinned choice before submitting.

The canvas stays unchanged while the private candidate moves between steps.
**Stop request**, **Local-only · on**, a changed source canvas, an invalid step,
or a connection/runtime change stops unsent steps. Completed work remains
available for review when it can be retained safely. Omuse never retries a
failed step through another provider.

Keep applies the complete editable candidate as one final action, so one Undo
restores the prior project. Saved sequences retain a bounded replay recipe and
the exact generated layer identities used by later layout edits. Reopen refuses
the sequence if those identities collide, disappear, or no longer match the
source instead of applying a later plan to the wrong layer.

## Saved choices and current boundary

Existing version 1 connection choices migrate to version 2 the first time the
new preferences are written. Valid Assistant and Images choices are preserved;
invalid legacy values return to Codex. The legacy direct-API flag cannot enable
API billing. Other open Omuse windows keep the choices they loaded at launch;
open a new window after changing saved routing elsewhere.

Malformed version 2 task pins fail closed. Ask Omuse blocks submission until
you use **Reset unreadable choices**. The [qualification record](ai-routing-qualification.md)
describes automated routing, cancellation and replay coverage and the
remaining release checks. Clean-host behavior, live failure/cancellation cases and
representative provider output still require broader qualification before
release.
