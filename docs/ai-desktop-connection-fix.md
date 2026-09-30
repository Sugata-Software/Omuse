# Omarchy desktop AI connection fix

30 September 2026. This historical record describes the connection correction
and its bounded local installed qualification. The fix is included in
[Omuse 0.4.0](releases/v0.4.0.md); its combined runtime and installer checks are
in the [release qualification](release-040-qualification.md). Omuse 0.3.0 did
not include the correction.

## Cause and correction

Omarchy's desktop PATH can find a provider through
`mise/shims/codex -> mise`. Omuse 0.3.0 canonicalizes that entry point before
checking its version, so it runs the manager as itself and rejects the result
as **Unverified**. The provider can already be signed in. The same discovery
problem affects other providers installed through these mise shims.

The correction recognizes the standard mise symlink layout, asks that manager
for the installed executable with a bounded `mise which` call in the private
probe directory, and applies all existing provider version, interface, account
and isolation checks to the returned runtime. General shell wrappers are still
rejected. Malformed, oversized, multiple, relative, missing and recursive
resolution results are not accepted. Cancellation and the original discovery
deadline cover the additional call.

## Evidence and scope

- All seven discovery regressions passed, including three new mise fixture
  tests and the existing arbitrary-shell-wrapper rejection test.
- A rebuilt discovery example, using the unchanged real desktop PATH, reported
  Codex **Ready / SubscriptionAllowance** with CLI 0.158.0. The Claude result
  in that earlier receipt was signed out and remains historical. After the
  existing Claude subscription login became available, corrected discovery
  reported Claude Code 2.1.283 as **Ready / SubscriptionAllowance**. Discovery
  checks submitted no model request. Grok remained unverified.
- The installed 0.3.0 application was separately opened with its Codex runtime
  directory prepended to PATH. Cua foreground XWayland input verified **Signed
  in**, one explicit user image request, a returned review and **Keep result**
  on the canvas. This is live evidence for that local workaround, not a GUI
  qualification of the newly corrected application build.
- A reversible, host-specific launcher adjustment provides the same path for
  ordinary desktop launches at that time. Reinstalling the unchanged 0.3.0 launcher could
  replace that workaround; the source correction is required in a future
  application update. That workaround did not change the application payload.
- The later local corrective generation `install-hytfk9hx` resolves the
  mise-managed runtimes from the normal launcher without that PATH workaround.
  It passed the bounded [Claude qualification](ai-claude-qualification.md).
  This local generation still reports 0.3.0 but is not the published release.

The user's artwork, prompt, account details and raw desktop captures are not
published. No credentials were copied and no separate API was enabled.
The [connection guide](ai-experience.md#connections-and-shared-context) explains
**Signed in**, **Unverified**, **Sign in needed** and operation-specific
first-use labels.

## Claude assistant protocol follow-up

The first synthetic Claude assistant request from rebuilt Omuse reached
**Submitted**, then failed because the adapter treated Claude Code's internal
`StructuredOutput` delivery as a disallowed external tool call. A separate,
bounded direct CLI capture used the same runtime and exact isolation flags. It
completed with validated `structured_output`; the initialization exposed only
`StructuredOutput`, an empty MCP server list and zero permission denials.

The source parser now permits only that exact internal tool name in a top-level
assistant envelope when Omuse requested a schema. All other normal, server and
MCP tool calls remain rejected. Omuse ignores the intermediate tool input,
requires the final validated structured result and no longer promotes prose
that merely parses as JSON. Six focused regressions were added, bringing the
Claude module to 14 tests. All 14 passed as part of a 387-test library run with
zero failures. The rebuilt qualification client then completed one synthetic
structured Claude request.

The exact installed local candidate also completed one synthetic **Design &
layout** journey through the Connections **Assistant** route: five editable
operations, Review, Keep, toolbar Undo and toolbar Redo passed. Codex remained
the **Images** route and the assistant required for canvas-based photo
assessment and caption drafting. No password or API key was copied, and no
separately billed API fallback was enabled.

This is bounded historical evidence, not qualification of the 0.3.0
application. At the time the public installer remained pinned to `ea187a9`; see the
[qualification record](ai-claude-qualification.md) for exact identities,
untested behaviors and the unresolved review-thumbnail aspect check.
