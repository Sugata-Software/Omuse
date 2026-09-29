# Omarchy desktop AI connection fix

30 September 2026. Source correction under development; not included in the
published 0.3.0 executable or source installer pin.

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
  Codex **Ready / SubscriptionAllowance** with CLI 0.158.0. Claude resolved to
  its official runtime but was signed out. Grok remained unverified. These
  checks submitted no model request.
- The installed 0.3.0 application was separately opened with its Codex runtime
  directory prepended to PATH. Cua foreground XWayland input verified **Signed
  in**, one explicit user image request, a returned review and **Keep result**
  on the canvas. This is live evidence for that local workaround, not a GUI
  qualification of the newly corrected application build.
- A reversible, host-specific launcher adjustment provides the same path for
  ordinary desktop launches. Reinstalling the unchanged 0.3.0 launcher can
  replace that workaround; the source correction is required in a future
  application update. The installed application payload was not changed.

The user's artwork, prompt, account details and raw desktop captures are not
published. No credentials were copied and no separate API was enabled.
The [connection guide](ai-experience.md#connections-and-shared-context) explains
**Signed in**, **Unverified**, **Sign in needed** and operation-specific
first-use labels.
