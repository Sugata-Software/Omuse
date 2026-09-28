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
The prompt, primary assistant action and active-request Stop control remain
above the scrolling details. Existing connection, local-only and busy guards
also apply to keyboard submission.

Review actions now refuse mutation while a provider request or local result
preparation is active. Selection-correction recomposition owns the busy state
until completion, including failure. Runtime identity and disconnection errors
invalidate the affected in-memory connection, reveal Connections and require a
refresh. Ordinary request rejection does not invalidate a healthy connection.
The draft prompt, artwork and saved history remain available.

## Current verification boundary

The updated candidate is undergoing regression and live GUI verification.
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
