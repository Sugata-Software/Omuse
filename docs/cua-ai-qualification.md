# Cua and AI assistant GUI qualification

29 September 2026. The installed Preview passed a bounded live Codex assistant
request, result review, Keep and one-step Undo through Cua. This is evidence for
one synthetic design on this host, not every provider, image operation or
physical input device.

## Exercised production candidate

- Public runtime source: `1d5ccaa4d0037f2353d698611497ce5aff46a577`.
- Source tree: `694a3a9ec0dd424cd252607c8a6b3e2922103179`, matching the local runtime checkpoint.
- Production executable SHA-256: `df38a2dfa2f57cd6d663358a56e4d9ca3ef805214ca819ab93fee460d4d4d48b`.
- Normal release build with no `ui-test` feature; local compiler Rust 1.98.1.
- Cua Driver 0.29.1, foreground XWayland input; Codex CLI 0.158.0 using its official subscription login.

All **789 automated tests** passed: 330 library, 218 UI and 241 integration
tests, with four manual timing benchmarks excluded. The editing self-test,
six-page PNG/PDF/MP4/GIF export journey and all 80 editable template variants
passed. Twelve regressions were added across this pass.

The final production executable separately passed its editing self-test and
all 24 native Wayland checks at 800x600, including photo adjustment/crop/resize,
undo/redo, save/reopen, 16-bit retention/export and command search. Its minimum
window capture was visually inspected. These native checks dispatch GPUI
events inside the app; they are separate from compositor-delivered Cua input.

## Live Cua result

Cua entered the same workshop brief used to expose the earlier colour defect:
a 1080x1080 cream page, orange heading, teal rounded rectangle and subtitle,
using editable text and shapes. Ctrl+Enter submitted it. The active Stop
control and status remained visible.

The provider returned a valid native plan containing resize, background,
shape, text and content operations. The original transparent 1024x768 canvas
remained unchanged during review. Keep applied the design; one toolbar Undo
restored the original dimensions and blank artwork. A subsequent foreground
brush drag painted a continuous stroke, and Ctrl+Z removed it completely.
Ctrl+Q with canvas focus closed the owned test window normally. The named Cua
session was then ended.

Fresh screenshots grounded and verified each action. The fixed canvas region
in the pre-Keep review, AI Undo and brush Undo captures exactly matches the
original capture. This is screenshot-region equality; document/pixel equality
is covered separately by the automated and native tests. The after-preview,
history labels, image-action rows and grouped variation controls were visually
checked. Omuse saved an Assistant capability receipt for Codex CLI 0.158.0.

This pass sent two synthetic assistant model requests: the intermediate
candidate returned invalid text colours; the corrected candidate succeeded.
An earlier instruction-isolation failure and two diagnostic thread starts
submitted no model turn. No API key, separately billed API or image-generation
request was used. Earlier image-operation evidence remains tied to its named
candidate and operation.

## Reproduced defects and repairs

- On keyboard candidate `26211b5`, opening Ask Omuse from command search and
  typing `hello` left the prompt empty and selected Lasso. Toolbar, shortcut
  and command-search opening now focus the prompt. Cua verified typing,
  Select All, command-search re-entry, plain Enter as a newline and
  Ctrl+Enter submission.
- The prompt, primary action, status and Stop control now stay above the
  scrolling details. Preparation errors also appear beside the prompt.
  Long history labels use ellipsis with the complete accessible label and
  review description retained. Previews have bounded heights; secondary
  actions wrap and variation choices remain together.
- Review mutations are blocked while provider work or local result preparation
  is active. Selection recomposition retains busy ownership through completion
  or failure. Runtime identity/disconnection failures invalidate only the
  affected connection and reveal Connections; ordinary request rejection does
  not invalidate a healthy connection.
- Candidate `c0a7200` stopped before `turn/start` because its disposable job
  lived beneath a repository and Codex discovered workspace instructions.
  Omuse now sets `project_doc_max_bytes=0` in the isolated process and thread
  configuration, while rejecting nonempty, missing or malformed instruction
  evidence before submission. A paired CLI 0.158.0 diagnostic reported one
  source without the override and zero with it, without submitting model turns.
  See the [official configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).
- Candidate `920fcc0` completed a request but returned text RGB values such as
  `[218,83,36]`, while native text requires normalized 0–1 values. Preparation
  rejected the result and offered no Keep action. The assistant instructions
  now distinguish text/shape colours from background byte RGBA, and parsing
  validates complete text, rich-text and shape styles without guessing a
  conversion. Existing `add_shape` and `style_text` operations are now advertised;
  their omission had caused the provider to decline the requested rectangle.

## Installation and CI

Preview records clean public source `1d5ccaa`. Its installed executable matches
the tested production hash. The complete previous `920fcc0` generation is
retained; RAW/ONNX/model assets match that generation, and the separate older
application and Preview launcher wrapper are unchanged.

The intermediate `c0a7200` and `920fcc0` candidates passed their full GitHub
workflows. For the final runtime candidate, the
[guide/reference workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36483014813)
and [Rust/installer workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36483014873)
both passed. All-target checks, release tests and editing journey, installer
fixtures, template/motion/recovery qualification and dependency-notice checks
completed successfully on the final runtime source.

## Driver and coverage limits

The app exposes only window metadata through this driver's AT-SPI fallback.
Background pointer/key calls produced no visible effect; foreground pointer
focus enabled subsequent keyboard input. This does not establish complete
accessibility-tree coverage, physical-device acceptance, alternate keyboard
layouts or IME behavior. Ctrl+Q did not fire while the AI prompt owned focus;
Ctrl+K → Close worked there, and Ctrl+Q worked after canvas interaction.

The native Omarchy plugin remains inactive: the installed 0.29.1 kit pins
Hyprland 0.56.2-1/GCC 16.1.1, while the host has Hyprland 0.56.2-2/GCC 16.2.1.
The compatibility guard refused the old build. No plugin was forced into the
desktop, and no logout, driver upgrade or permission-mode change was performed.
Cua ran in standard mode. These results do not qualify every AI provider or
make the public binary-release gates complete.

Local evidence is retained under `rust/evidence/cua-debug-20260929/`, including
`validation-contract.log`, `production-contract-build.json`,
`contract-wayland/native-results.json`, `install-contract.json`,
`live-contract-failure.json`, `live-contract-success.json` and the numbered
Cua captures. Raw evidence is not uploaded wholesale.

## Published promo

The two-minute Sunset Muse film is in the public repository and featured in
the README. Its MP4 was downloaded back from GitHub and matched the local
SHA-256 and 18,512,367-byte size. The poster was visually inspected. A GitHub
browser rendering check was unavailable because this session had no Browser
connection. See the [film and attribution record](media/omuse-sunset-muse-credits.md).
Publishing the trailer does not change the binary-release status.
