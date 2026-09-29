# Ask Omuse 0.2.0 qualification

29 September 2026. This record distinguishes automated checks, production
desktop interaction and live subscription results. It does not qualify every
provider, prompt, image operation or physical input device.

## Runtime identity

- Public runtime: [`3dc3e46310e40dcf109b1bc695bb4e9dda6d2d24`](https://github.com/Sugata-Software/Omuse/commit/3dc3e46310e40dcf109b1bc695bb4e9dda6d2d24).
- Source tree: `1843e8e6242b210550365ad0aedf1e326cb06a38`.
- Production executable SHA-256: `2b4e19a54553ad1836f49da23373d1038420369c298d2384d0ca1bdc5bae16dc`.
- Normal release build without the `ui-test` feature, using Rust 1.98.1.
- The complete [GitHub Rust/installer validation](https://github.com/Sugata-Software/Omuse/actions/runs/36528288193) passed for this exact public runtime.

## Automated evidence

**843 application tests passed:** 365 library, 233 UI and 245 integration;
four manual timing benchmarks were excluded. The complete local suite passed
before the final review-layout and layer-label changes; all 233 UI tests then
passed on the final source. The editing self-test, six-page PNG/PDF/MP4/GIF
journey and all 80 editable template variants passed in the full validation
pass. The final production editing self-test passed separately. The exact
public source also passed its own complete remote validation run, including
motion exports, interrupted saves and dependency notice inventory.

New regressions cover strict photo/caption task contracts, prompt dispatch,
minimum-window review visibility, bounded refinement context, preserved removal
intent and references, provider events arriving before acknowledgements,
subscription evidence and allowance parsing, and concurrent history writes.
Photo adjustment tests verify protected targets, unchanged source pixels,
clipping and masks, save/reopen, rendered changes and one-step Undo. Review
labels resolve typed layer references without replacing arbitrary user text.

All 25 installer and 11 bundle regressions passed. The 28 release-publication
fixtures passed. A UI rerun using an already populated test recovery directory
failed a startup interaction assertion; a fresh isolated rerun passed all UI
tests. Public validation creates fresh test directories for each run.

## Production desktop checks

The final production executable passed **24 native Wayland and 24 native
XWayland checks**, each exercising the minimum 800×600 logical viewport.
These cover painting, Undo/Redo, unsaved guards, save/reopen, themes, text,
live adjustments/effects, selection masks, retained 16-bit import/export,
photo editing/export and command-search execution with focus restoration.
Dark/system and light Ask Omuse captures were visually inspected.

The installed normal launcher independently passed the same **24 Wayland
checks with reduced motion**; its Ask Omuse capture was inspected. The native
harness dispatches GPUI events inside its owned application. Cua input reaches
the application through the compositor; these are separate forms of evidence.

Cua Driver 0.29.1 used foreground XWayland input and fresh window screenshots.
Its AT-SPI fallback exposes window metadata, not a complete control tree. The
Omarchy-specific driver plugin was not activated, replaced or hot-loaded in
this pass. Its compatibility gate and broader physical input remain open.

## Three live Codex requests

All requests used **Codex CLI 0.158.0**, the existing official subscription
route and a synthetic 640×480 still life. No separately billed API, reset
credit or purchase was used. The visible connection allowance is only the
runtime's snapshot; it is not a promise that a subsequent request will run.

Two requests ran on intermediate public candidate `df15fd4`, production hash
`57b2978e4454c595f36c474a250930613e95f23f66f0cd59158f139b0f45e9a0`:

1. **Enhance photo:** Ctrl+Enter submitted +0.5-stop exposure, +10% contrast
   and −10% saturation. The returned plan contained only the requested active
   layer's `adjust_photo` operation. Before/After, Keep and one-step Undo
   passed. The original canvas stayed unchanged during review.
2. **Caption & alt text:** the primary button submitted a request for visible
   scene details without invented product claims. The returned `set_content`
   accurately described the synthetic scene. Copy caption was checked by
   pasting into the prompt without resubmitting. Keep and Undo changed content
   metadata while retaining the artwork. Close worked from the focused prompt.

The final `3dc3e46` differs from that intermediate build only in readable
layer-review labels and their regression test. A third live request and the
following history journey ran on the **exact final production executable**:

3. **Saved-result refinement:** reopening history retained both results. The
   old photo result remained readable, with an explicit changed-canvas notice
   and no Keep control. Refine restored the photo task and previous proposal
   without automatic submission. A new brief requested +0.25-stop exposure
   while retaining +10% contrast and −10% saturation. The returned plan used
   the new document's current layer ID. Review displayed the actual layer
   name, Before/After worked, Keep applied the changes and one Undo restored
   the clean source. Ctrl+Q closed the owned window from its focused AI prompt.

Completed reviews were retained in local history with provider/runtime and
task provenance. Display comparison found a clear applied adjustment and only
six one-level channel differences after the first Undo; it is not an exact
screenshot-equality claim. Exact source-pixel and document restoration are
covered separately by automated persistence and transaction checks.

## Installation and rollback

The clean public runtime was installed as the normal **Omuse** application,
without recompiling or creating a second Preview app:

- Active generation: `install-hly2uob3`, clean source receipt for `3dc3e46`.
- Previous generation: `install-y4k24szz`, the tested `9c99e50` application.
- All **19 payload files** were compared, including libraries, subject model,
  icons, legal texts, source receipt and executable. Unchanged assets matched
  the previous generation; the executable matched the final hash above.
- Complete rollback to the previous generation and the reverse switch both
  restored all 19 expected hashes. The final active generation is 0.2.0.
- The installed main launcher passed the independent native journey above.
  Its script hash is `1f4792f0bdb0945c05532db4924e3b2a016aaf62b9d6423e2f17596cc7cadfab`;
  the underlying executable was verified separately.
- Desktop-file validation passed. Launching `omuse.desktop` mapped a native
  Wayland window whose executable resolved to the final managed generation
  with the expected hash. Cua timed out waiting for GTK's activation handoff;
  compositor/process readback independently established successful launch.

The normal command and desktop entry remain Omuse. User documents, settings
and provider profiles were preserved. The public source installer selects
this tested runtime; downloadable binaries remain outside this release.

## Limits

The recorded live examples are synthetic. They do not establish photographic
taste, factual accuracy for arbitrary caption requests or all image-generation
and editing operations. Earlier generation, background and design receipts
remain tied to their original candidates.

Photo enhancement affects the whole active ordinary pixel layer. It exposes
exposure, brightness, contrast and saturation as editable adjustments; Camera
Raw temperature, tint, highlights and shadows are outside this operation.
Visual photo and caption input currently requires ChatGPT via Codex. Claude's
authentication fixtures are stronger, but this is not a new live Claude or
Grok qualification. Direct API billing remains disabled.

Downloadable binaries, clean-machine installation, memory-pressure testing and
broader physical-input/display coverage retain their separate release gates.
Tablet qualification remains deferred.

Local logs, screenshots and synthetic fixtures are retained under
`rust/evidence/ai-experience-20260929/`. Only this curated record is published;
raw evidence, provider state and personal paths are not uploaded wholesale.
