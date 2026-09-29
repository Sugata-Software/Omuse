# Creating with Ask Omuse

Open **Ask Omuse** from the toolbar or command search. Choose a task, write a
brief or select a starting point, then customise it. A starting point never
sends a request. **Ctrl+Enter** and the primary button submit the same selected
task; Enter adds a new line.

## Tasks

| Task | Result and controls |
| --- | --- |
| Design & layout | Editable text, shapes, layouts and content pages. Include or exclude a canvas preview. |
| Enhance photo | Exposure, brightness, contrast and saturation on the active ordinary pixel layer. Original pixels remain intact. |
| Caption & alt text | A caption and image description. Copy either directly or Keep both in the content project. |
| Generate image | A new image from a brief and optional references, reviewed before insertion. |
| Replace selection | New content within the selected area; pixels outside it stay protected. |
| Remove object | A natural fill for the selected area. |
| New background | A new background around the selected subject, with optional local finishing controls. |
| Expand canvas | Continue the scene into the pixel margins specified for each edge. |

Photo enhancement applies to the entire active layer. It does not interpret a
pixel selection as a local adjustment mask. It creates editable clipped
Exposure, Curves and Hue/Saturation layers. Exposure is bounded to ±3 stops;
brightness, contrast and saturation are bounded to ±100%. Locked, advanced,
already-clipped and non-pixel targets are rejected before submission. Camera
Raw-specific temperature, tint, highlight and shadow controls are not offered
through this operation.

## Connections and shared context

Connections lists official local subscription runtimes. Sign-in happens with
the provider. Omuse never copies a password or silently switches to a separately
billed API. Unsupported routes remain unavailable; first use of an untested
operation is explicitly labelled and can use subscription allowance.

Codex can optionally report remaining allowance and reset windows through its
[official app-server protocol](https://learn.chatgpt.com/docs/app-server).
The display is a snapshot from the last connection check. Missing information
stays unavailable; Omuse does not infer an unlimited allowance or spend reset
credits. Refresh Connections updates the snapshot.

The request-context card names the content to be sent: a bounded canvas preview
when enabled/required, editable layer details, project/brand summaries and
chosen references. An image-edit request includes the canvas and its edit mask.
A refinement also includes its prior proposal and available result image.
Reference images are re-encoded before sending, rather than sharing their
original file metadata. **Local-only** in Connections prevents remote requests
for the collection.

Photo assessment and caption drafting require a visual assistant connection.
The current implementation supports canvas images through ChatGPT via Codex;
Claude's assistant path does not silently receive image references.

## Review, refine and recover

The canvas stays unchanged while the provider works. Stop cancels the current
request and unsent variations; a provider may already have consumed allowance.
Unknown outcomes are never retried automatically.

Review the listed changes and compare Before/After. **Keep result** applies the
candidate as one undo step. Photo responses cannot affect other layers or
perform layout operations; caption responses cannot edit artwork. Empty or
invalid results do not qualify the operation or expose an applicable result.

Refine this result and Another direction attach the saved proposal to the next
brief. They preserve its task and selected references, and do not submit by
themselves. Start a new request clears the current brief/context while keeping
saved history. Results from a changed or previous-session canvas remain
reviewable; they cannot blindly replace current artwork.

Completed results are retained within entry, file and metadata budgets. History
writes merge current on-disk entries under a lock and publish the index before
pruning old artifacts, so another window or a failed write cannot silently
erase alternatives. A window's list refreshes on history operations/reopen.

## Evidence boundaries

The task interface, photo operations and provider/history regressions are part
of the 0.2.0 candidate. Current qualification results are recorded separately
in the project guide and release notes. A passing local test does not prove
image quality, every provider/account, or support for every image operation.
Direct API billing, calendars and scheduling remain outside this work.
