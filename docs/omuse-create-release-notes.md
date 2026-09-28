# Omuse Create + AI preview release notes

**Preview status — 28 September 2026.** Candidate
`b178ad273ae9627836b753d584925f3bf7e1a003` (binary SHA-256
`eeec22afb6c4eb5cf7f6840c4cece6b50c1c9827b21c3d929ba994cab1b73dca`)
is available for evaluation, not public release. Its automated, artifact,
native-window, bounded live-provider and separate preview installation gates
below passed. The remaining public-release gates are listed below.

## What is new

- **Create collections:** package multi-page work as `.omuse` projects with
  mixed page sizes, page reordering and duplication, shared backgrounds and
  collection undo/redo. Existing `.comp` projects continue to open.
- **Branded design:** use brand colours, font pairs, text styles and spacing;
  start from 20 editable templates; retain text, shapes, frames and reusable
  components as native objects.
- **Campaign production:** adapt layouts to new aspect ratios, create CSV-bound
  variants from approved fields, manage a searchable local asset library and
  keep placed resources inside the project package.
- **Review and export:** check a phone preview and composition guides, then
  create an ordered content pack with PNG, JPEG, WebP, PDF, captions, image
  descriptions and a manifest.
- **Motion:** add page and layer animation, transitions, audio and editable
  SRT/WebVTT subtitles; trim or split supplied clips; export MP4 or GIF through
  FFmpeg. Automatic transcription and generative video are not included.
- **Ask Omuse:** choose separate Assistant and Images connections, share only
  selected context and references, request native edit plans or image drafts,
  compare alternatives and keep a result through an explicit review step.

Local editing, restoration, project save and export do not require an AI
connection.

## Candidate evidence

- **Regression:** 636 tests passed and none failed across 20 groups. The three
  benchmark tests omitted from the standard suite were run separately and all
  passed.
- **Create journey:** the exact candidate completed a six-page editable
  collection, PNG/PDF content pack, story variant, MP4 and GIF under
  `gate-work-b178ad2/omuse-tests.UZcEpfOG/create-journey`. Decoded images and
  the PDF matched the visually reviewed prior output.
- **Templates and media:** all 80 variants across 20 templates preserved native
  text and pixels through render/save/reopen. Seventy-six unchanged renders
  matched reviewed output pixel-for-pixel; all four changed Video title renders
  passed visual review. Audio, soft/burned subtitles, trim and split passed the
  exact-candidate media journey.
- **Recovery and performance:** six-revision collection recovery and
  12-revision advanced-project recovery passed sustained and coordinated
  SIGKILL checks. The three bounded CPU/allocation benchmarks passed on this
  host; they do not predict GPU or cross-machine performance.
- **Native desktop:** Ask Omuse and the Create, Motion and Export panels passed
  native Wayland checks. Motion's **Play project** layout passed at 800×600.
  The corrected external XWayland harness passed a real GPUI 800×600 window and
  a byte-identical reduced-motion splash. Animated native startup also passed.
- **Bounded native AI:** the candidate reviewed retained provenance for a real
  generated asset; kept a new Background result with the source and protected
  subject byte-exact through save/reopen and Undo; and kept a six-page branded
  assistant plan with native text, captions and alt text through save/reopen,
  Undo and Redo. All six assistant pages passed visual review.

These results prove the named journeys on this candidate. They do not qualify
every template, provider operation, hardware path or Linux distribution. The
[candidate qualification record](omuse-create-qualification.md) is the source
for exact evidence, measurements, limitations and current gate status.

## Provider availability in this preview

| Route | Preview status |
| --- | --- |
| ChatGPT subscription through Codex | The bounded candidate journeys above passed through the in-app route. This does not yet qualify replacement, canvas expansion, removal, multi-variation cancellation, every failure/restart case or general provider support. Requests can consume the selected subscription allowance. |
| Claude Code subscription | The official runtime is installed, but it is signed out. The adapter remains unavailable until official subscription sign-in and an in-app assistant run are qualified. A direct Claude API route is outside this preview. |
| Grok Build | A provider adapter exists in source, but the detected `grok` command's official identity, account state and isolated ACP operation have not been verified. Grok remains unavailable. |
| OpenAI API | Optional direct API use is disabled pending the user's key and billing-route choice. No key is required for local work, and Omuse must never fall back from subscription access to separately billed API use silently. |

Provider sign-in remains with the provider's official runtime. Omuse does not
store provider credentials in artwork. Cancelling locally may not reverse usage
already accepted by a provider.

## Preview setup

For source evaluation without replacing the preserved installation:

```sh
scripts/build-rust.sh
rust/target/release/omuse
```

On this host, **Omuse Preview** is installed and verified through both the
`omuse-preview` command and its desktop entry. It uses the separate prefix
`~/.local/opt/omuse-preview` and separate settings, cache, history and recovery
locations. The original **Omuse** installation is unchanged.

Open `~/Pictures/Omuse Preview/Sugata Field Notes.omuse` for the local six-page
example or `AI Focus Carousel.omuse` for the six-page native assistant result.
The same folder contains a PNG/PDF content pack and a short README.

The bundle records clean source revision
`ee55127d3aa18ddb1344a9059ca126d2ffebe17b`: qualification-tool changes after the
application revision, with identical runtime source and the same executable
SHA-256 above. Installed self-test, native editing and 16-bit preservation
passed outside the source checkout.

Open **Create** from the header or press **Ctrl+Shift+E**. Save the complete
`.omuse` package when moving a collection. Motion export and clip tools require
working `ffmpeg` and `ffprobe` executables.

Open **Ask Omuse** with **Ctrl+Shift+J**. In **Connections**, use a signed-in
route whose required capability is verified, or explicitly choose the matching
**Try …** action when it is **not yet tested**. That first submitted operation
may use the selected provider's subscription allowance. Assistant and Images
routes remain independent; Omuse does not switch providers or enable separate
API billing after a failed request.

## Preservation and rollback

- Image results remain alternatives until **Keep**. Results created against an
  older document revision stay in review and cannot silently overwrite newer
  edits.
- Selection edits are staged through a mask. Background workflows keep the
  protected subject on its original layer, but generated pixels inside an edit
  region still require visual review.
- Restoration keeps the original as a hidden layer. Native edits apply as
  undoable transactions; exports and project saves publish only completed
  staged output.
- Provider failure, expiry or absence must not prevent local project access.
  Undo restores document state, although it cannot refund provider usage.
- Keep preview `.omuse` projects intact when returning to the preserved Omuse
  installation; older builds may not understand their Create metadata.

## Known limits

- Replacement, canvas expansion, removal, multiple-variation cancellation and
  several provider failure/restart paths do not yet have bounded live receipts.
- Image generation cannot promise exact subject identity or repeat an earlier
  result. Pixels inside a generated region require review.
- Layout preflight is conservative. It does not fully judge image backdrops,
  rich text colour runs, every missing-font substitution or all destination
  safe areas.
- Restoration enlargement interpolates existing pixels and does not recover
  unknown original detail.
- Motion depends on the installed FFmpeg build. Automatic transcription,
  generative video, scheduling, publishing, account management and analytics
  are outside this preview.

## Remaining public-release gates

Before public distribution, the qualification record must show:

- manual edge-case coverage for missing fonts, long copy, invalid CSV rows,
  unusual aspect ratios, damaged assets, cancellation, restart and offline use;
- real foreground keyboard and pointer input, tablet input and multiple DPI
  configurations. CUA captured the nested app, but foreground input injection
  was refused and does not close this gate;
- a clean build and packaging/CI run in the target environment;
- inclusion or resolution of the four outstanding dependency licence texts;
- a bounded receipt for every AI operation and provider route advertised as
  supported, including honest capability and billing labels.

The preview bundle targets this Arch host and its glibc 2.44 environment. The
current evidence does not establish compatibility with all Linux distributions.
