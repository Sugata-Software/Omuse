# Cua and AI assistant GUI qualification

29 September 2026. This pass tests a disposable Omuse instance and synthetic
artwork. It does not qualify every provider, image operation or physical input
device.

## Driver route

Cua Driver 0.29.1 can launch and capture an exact Omuse XWayland window.
Foreground pointer input works. After a foreground click establishes focus,
keyboard input works too: Ctrl+K opened command search, typing filtered it,
and Enter ran Ask Omuse. Background pointer/key calls reported unverified
synthetic delivery and produced no visible change. An initial foreground
shortcut before the pointer focus also produced no visible change.

The app exposes only window metadata to this driver through AT-SPI, so this
pass uses fresh exact-window screenshots for grounding and verification.
It does not claim accessibility-tree coverage for the editor's controls.

The native Omarchy plugin remains inactive. The installed 0.29.1 kit pins
Hyprland 0.56.2-1 and GCC 16.1.1; the host now has Hyprland 0.56.2-2 and GCC
16.2.1. The compatibility guard correctly refuses the old build. No plugin
was forced into the desktop, and no logout, driver upgrade or permission-mode
change was performed. Cua runs in standard mode.

## Reproduced issue and fixes

On the installed keyboard candidate (`26211b5`), opening Ask Omuse from
command search and typing `hello` left the prompt empty and selected Lasso.
Canvas tool shortcuts still owned focus. Both opening paths now focus the
composer, and command search preserves that focus after dispatch.

The composer supports Ctrl+Enter submission with Enter retained for newlines.
The prompt, primary assistant action, bounded status feedback and active-request
Stop control remain above the scrolling details. Existing connection, local-only and busy guards
also apply to keyboard submission.

Review actions now refuse mutation while a provider request or local result
preparation is active. Selection-correction recomposition owns the busy state
until completion, including failure. Runtime identity and disconnection errors
invalidate the affected in-memory connection, reveal Connections and require a
refresh. Ordinary request rejection does not invalidate a healthy connection.
The draft prompt, artwork and saved history remain available.

## Current verification boundary

The updated source is published as `c0a7200c05a7c5e4a17e8ca5e35a5e6f0ae615a0`,
with tree `0dbbdd83df2f9d446f500e156f6711f789a62da8` matching the local
source checkpoint. All **784 automated tests** passed: 326 library, 217 UI
and 241 integration tests, with four manual timing benchmarks excluded.
The editing self-test, six-page PNG/PDF/MP4/GIF export journey and all
80 editable template variants passed. Seven added UI regressions cover prompt focus
and submission, command-search focus, toolbar/shortcut focus, the minimum
800x600 layout, busy review ownership, selection recomposition and connection
failure recovery. The focused AI group has 26 passing tests.

The first production executable (`dd1ccd5dfa3282d8c43979f4d6489dd9ef624312b922013d2041a67b47e70346`)
passed its self-test and 24 native Wayland checks at 800x600. Cua verified
foreground toolbar opening, prompt typing without a tool switch, Ctrl+K from
the prompt, search execution back into the prompt, Select All, ordinary Enter
as a newline and Ctrl+Enter submission in the installed Preview.

That first request stopped before `turn/start`: the job lived beneath a
repository and Codex reported an unexpected instruction source. The canvas
stayed unchanged. The runtime now sets `project_doc_max_bytes=0` in both its
isolated process and thread configuration, while still rejecting any nonempty,
missing or malformed instruction-source evidence before model submission.
The [official configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference)
documents the project instruction byte limit. A paired pre-submit check on
Codex CLI 0.158.0 reported one source without the override and zero with it;
neither diagnostic submitted a model turn. Two added regressions check the
response shape and prove that unexpected instructions submit no turn.

The follow-up full run passed **786 tests** (328 library, 217 UI and 241
integration; four manual benchmarks excluded), including both new isolation
regressions and the extended minimum-window status test. The editing, six-page
export and 80-template journeys passed again. After that run, three secondary
button rows were changed to wrap within the inspector; the final production
executable (`27f11cb99f7785ae579b6097d8b8cd6eaec5ca096cabccc6595baca70c16f0f3`)
then passed its editing self-test and all 24 native Wayland checks at 800x600.
The captured prompt, primary action and status were visible at that minimum
size. Live request verification in the installed Preview is still pending.

The first published AI GUI candidate also passed both GitHub workflows:
[Rust and installer checks](https://github.com/Sugata-Software/Omuse/actions/runs/36475325721)
and [guide/reference checks](https://github.com/Sugata-Software/Omuse/actions/runs/36475325753).
The baseline Cua captures and result notes are retained locally under
`rust/evidence/cua-debug-20260929/`; they are not uploaded wholesale.
Earlier provider evidence remains tied to its named candidate and operation.

## Published promo

The final two-minute Sunset Muse film is in the public repository and linked
from the README. Its MP4 was downloaded back from the published commit and
matched the local file's SHA-256 and 18,512,367-byte size. The poster was
visually inspected locally. A browser rendering check was unavailable because
no Browser connection was available in this session.

See the [film and attribution record](media/omuse-sunset-muse-credits.md).
Publishing this preview trailer does not change the binary-release status.
