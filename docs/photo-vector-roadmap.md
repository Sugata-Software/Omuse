# Omuse photo and vector studio roadmap

Approved direction, 1 October 2026. Omuse remains a native Rust/GPUI application
for Linux/Omarchy, with one `.omuse` project family and an emphasis on making
finished content. This roadmap extends the existing editor incrementally.
It is not a claim of Illustrator, Photoshop or camera-colour parity.

The [project guide](project-guide.md) tracks current implementation and evidence.
**11 October 2026 release target:** [Omuse 0.10.0](releases/v0.10.0.md), exact
source `920ae0fddb9a58ba69526b7e7976b5d770435fd9`, passed Linux/Windows validation
and package checks. Public source/asset publication is tracked separately in
the [release qualification](release-0100-qualification.md). Shape Builder,
curve finishing, independent repeat copies, precise node snapping, expanded
SVG conversion and bounded PSD exchange advance the workstreams below without
completing live procedural artwork, professional print or broad interchange.

**5 October 2026 baseline:** published and installed [Omuse 0.9.0](releases/v0.9.0.md),
qualified source `65c95cc165f9d730a9f0bcea51c51a84cf4adb7e`. Reviewed Linux and
unsigned experimental Windows archives, checksums and manifest are published
and anonymously verified. The normal installation and complete 0.8.0 rollback
have recorded checks in the [0.9 qualification](release-090-qualification.md).
Clean-machine, interactive Windows, signing and broader hardware/photographic
qualification remain open; publication does not make those gates complete.

[Omuse 0.8.0](releases/v0.8.0.md) shipped gradient fills and advanced strokes,
text on curves and outline conversion, bounded vector PDF, hue-range masks,
reference-colour matching, JPEG detail inspection, PSD adjustment/mask
corrections and external UTF-16 font-run reads. Its source
`eb558dc59ccdf88a3d62dfc2d706a6b836da1eda` and published archives retain their
own [qualification](release-080-qualification.md). These are partial
contributions to the larger workstreams below, not wholly completed phases.
Omuse 0.9 adds native-source retouch, independent Blur radius, versioned
controlled removal, configurable grid/snapping, mask inspection, safe folder
Ungroup and selected-letter font preview.

**Included in 0.10:** cancellable background Blur/Smudge/Liquify moves stroke
computation off the UI thread and adds cancellation/stale-result guards.
Eligible Camera Raw colour edits gain a quick draft followed by full refinement;
new versioned tone mapping and a colour-regression corpus protect appearance.
Texture removal remains opt-in experimental, with mixed real-photo outcomes.
The earlier 1,422-test and native Smudge/Undo/Redo checkpoint remains in the
[background-retouch record](background-retouch-qualification.md). Published 0.9
keeps synchronous strokes. These scheduling changes do not establish better
photographic quality or a measured end-to-end speed improvement.

Historical qualification records describe their named source checkpoints.
The [0.7.0 source-only release](releases/v0.7.0.md), runtime
`5b3daefbb5258afff4a74a2ff3db5247b074972d`, retains its
[qualification](release-070-qualification.md). The earlier selection, boolean
and SVG [development checkpoint](vector-workflow-qualification.md) passed
1,254 optimized application tests, editing/Create acceptance, 80 templates,
native checks on Wayland/XWayland and a bounded Cua keyboard/construction
journey. Those historical counts do not qualify later additions; use each
release's exact-source evidence. Calendars, scheduling, social publishing and
tablet qualification remain outside this work.

The approved [RAWmakase-inspired photo development plan](rawmakase-roadmap.md)
adds phased colour regression, progressive previews, retained RAW precision,
native AI command control, interactive selection and profile/preset migration.
Its phase status and qualification boundaries remain explicit.

## Build order

1. **Photo fidelity and vector foundations.** Correct preview/export sampling,
   retain precision, establish editable SVG path exchange, and improve path
   editing without destabilizing photo workflows.
2. **Useful everyday photo and vector tools.** Persistent selective masks,
   colour uniformity/matching, extend the implemented procedural scene and
   main-canvas path tools, then add shape construction, stroke controls,
   typography and broader editable SVG exchange.
3. **Advanced image intelligence and multi-image work.** Qualify restoration,
   semantic/depth masks, relighting, camera profiles and robust merges using
   independent images and measured resource budgets.
4. **Interchange and professional output.** Vector-preserving PDF, complete
   wide-gamut/HDR paths, advanced illustration and separately qualified print
   colour support.

Phases can overlap where modules are independent. Each useful slice must have
a native UI/command path, reversible edits, saved-project behaviour, documented
limits and focused tests. Engine-only work is not a completed user feature.

## Photo editing

| ID | Work | Existing foundation and required outcome |
| --- | --- | --- |
| P01 | Precision and sampling | Retained 16-bit sources exist. Replace bilinear-only downsampling with alpha-correct, scale-aware reconstruction; independently verify fine patterns, transforms, hidden RGB and retained precision. |
| P02 | Complete high-precision editing | Remove the Camera Raw compatibility node's 8-bit bottleneck incrementally. Unify supported adjustment/effect export, preserve old recipe appearance, and make deliberate raster conversion explicit. |
| P03 | Camera/lens profiles and colour | Add camera/lens identification, compatible measured profiles, distortion/vignetting/chromatic-aberration correction and reproducible colour-chart checks. Keep manual overrides; do not label generic corrections as a calibrated profile. |
| P04 | Persistent selective masks | Partial, shipped in 0.8: Hue range adds sampled circular hue, softness, saturation protection and inversion, with selection/add/subtract/intersect or layer-mask output. Reveal/hide masks retain soft coverage through transformed layers. Saved masks retain pixels; persistent hue/range recipes, semantic person/hair/clothing/face-region masks and broader edge qualification remain open. |
| P05 | Target-colour uniformity | User-chosen reference colour, adjustable range/falloff and independent hue/saturation/lightness uniformity. Preserve texture, alpha, masks and high-precision data; useful for skin and products without assuming a skin tone. |
| P06 | Reference-look matching | Partial, shipped in 0.8: Match reference colour is an editable Filter-stack operation with Amount, Preserve lightness, node opacity/masks and 8-bit/retained 16-bit sRGB/Display P3 evaluation. Bounded alpha-weighted Oklab statistics make the recipe independent of the reference file. Semantic/local matching, explicit exposure/white-balance normalization, campaign-wide batch preview and independent real-photo quality qualification remain open. |
| P07 | Retouch and removal fidelity | Partial: 0.8 added fractional footprints, consistent spacing, evolving Smudge carry and untouched-source Liquify. In 0.9, Blur/Smudge/Liquify use native layer/mask pixels with independent Blur radius, bounded work and atomic refusal; direct retouch requires 8-bit sources up to 16 MP and a 256 MiB working-buffer budget. Versioned controlled removal uses boundary context with a separate 4-million-pixel source limit. Recorded photo checks include visible seam/texture defects; broader clone/heal/removal quality remains open. 0.10 adds cancellable background retouch and opt-in Texture removal with separate source/region/selection/work limits. The Texture method retains original 16-bit donor precision; photographic seams remain a qualification limit. Published 0.9 retains synchronous strokes. |
| P08 | Learned restoration | Detail-preserving denoise, deblur and enlargement; optional scratch/face/text recovery where validated. Preserve originals and expose before/after detail crops and generated-detail disclosure. Check model redistribution, Linux execution and download integrity before integration. |
| P09 | Depth and relighting | Editable depth masks, near/far brightness and warmth, portrait/product relighting and edge/halo refinement. Keep depth correction accessible and avoid silently replacing original lighting or geometry. |
| P10 | Multi-image merging | Extend current translation-only registration with rotation/perspective and appropriate panorama projection; improve focus blending and HDR moving-subject handling. Persist source/alignment/exposure recipes and cancellation-safe results. |
| P11 | Wide gamut and HDR | Complete colour-managed working/compositing/export paths and persistent 32-bit floating-point radiance with explicit tone mapping. Qualify monitor/compositor behaviour independently from file correctness. |
| P12 | Product and campaign finishing | Connect consistent colour, masking, relighting and transparent/shadow output into reusable editable product-photo recipes and batch previews. Existing frames, templates, brands and bulk creation remain the foundation. |

## Vector, typography and exchange

| ID | Work | Existing foundation and required outcome |
| --- | --- | --- |
| V01 | Direct canvas Pen and Node tools | Released main-canvas workflow: P draws, A edits nodes, V picks/moves objects, double-click in Move mode enters nodes, and the shared Layers inspector provides insertion, open/close, corner/smooth and object controls. Arrow nudging uses source pixels. The unified canvas passed local regression/native checks. Pen click/drag corners and smooth curves, linked/Alt handles, Shift constraints, exact segment insertion and first-anchor closing are included in the released trace/curve workflow. 0.10 adds within-object split/join and six-screen-pixel node/handle snapping to visible anchors/guides/grid. Broader hardware and gesture coverage remain open. |
| V02 | Shape construction | Shipped in 0.8: background Unite, Subtract, Intersect, Exclude and Divide on selected filled paths, with Undo, fill-rule handling and bounded world-space flattening. Divide partitions the bottom object; 0.8/0.9 results are polygonal. 0.10 adds bounded Bézier operations and interactive Shape Builder for 2–8 consecutive opaque filled paths without strokes. Persistent live booleans and broader pathological-geometry qualification remain open. |
| V03 | Strokes and fills | Partial, shipped in 0.8: linear/radial fills have 2–16 alpha-capable stops, geometry, Pad/Repeat/Reflect spread and fitting controls. Uniform-width strokes have caps, joins, dash/gap patterns, offsets and miter limits. Saved scene version 3/project format 13 retains them. 0.10 adds Outline strokes and Offset path as ordinary editable geometry. Variable widths, persistent live corners/offsets and mesh/pattern fills remain open. |
| V04 | Typography | Partial, shipped in 0.8: single-line text on a retained curve has font/size/tracking/position/alignment, resolved-font reporting, explicit guide replacement and Convert to outlines. Scene version 4/project format 14 retains its recipe and portable saved outlines. External project format-11 UTF-16 font/colour runs also import safely. Styled spans on curves, paragraph flow, complete fallback/layout qualification and broad editable-text interchange remain open; SVG/PDF exports text as outlines. |
| V05 | Image tracing | Released local Colour/Gray/B&W tracing, presets, detail/smoothing/corner/noise/resolution/point controls, Source/Trace preview, retained original, saved settings/retrace, editable compound curves and one-step Keep/Undo. See the [qualification record](image-trace-qualification.md). Photo results are solid-colour approximations; broad photographic/topology/large-document qualification remains open. |
| V06 | Editable SVG exchange | Partial, shipped in 0.8: the scene importer/exporter handles multiple objects, organizational groups, names, visibility, object opacity, supported linear/radial fills and uniform-width cap/join/dash styles. Import appends artwork; scene export retains objects, with Omuse curved text converted to outlines. 0.10 adds supported text/tspan outlines, gradient/transformed-stroke expansion and single-painted-child group opacity with conversion warnings. General clipping/effects, isolated group compositing, patterns and external resources remain unsupported. Legacy single-path exchange and raster SVG import retain their narrower/separate contracts. |
| V07 | Vector-preserving PDF | Partial, shipped in 0.8: Export vector PDF writes one scene or legacy path at document DPI, retaining supported geometry, transparency and Pad gradients. Text exports as outlines; Repeat/Reflect are refused. A bounded 0.10 synthetic vector fixture passed independent Poppler visual comparison with recorded edge-rasterization differences. Mixed photo/vector document export, explicit raster-effect fallbacks, multipage/artboard vector output, editable text/font embedding and broader independent-reader qualification remain open. Existing multipage content PDF stays rasterized. |
| V08 | Illustration tools | 0.10 grid/radial repeats create independent editable copies with retained originals and Undo. Persistent linked repeats, reusable vector symbols/instances, live offsets, blends and mesh/pattern fills remain open; reuse the existing component/artboard model where compatible. |
| V09 | Illustrator interchange | Explore an explicitly bounded PDF-compatible `.ai` import after SVG/PDF foundations. Full proprietary Illustrator document round-trip is a separate research item, not an implied result of SVG support. |

## Shared quality and experience

| ID | Work | Required outcome |
| --- | --- | --- |
| Q01 | One document and reversible edits | Implemented foundations retain raster/vector/text in `.omuse` with bounded Undo and explicit unsupported-version refusal. Current scene versions 1/2/3/4 map to canvas formats 11/12/13/14; ordinary canvases remain format 10. Formats 12–14 and reference-match nodes require Omuse 0.8 or later. New 0.9 ContextualV1 removal recipes can display from cache in 0.8, but editing/resaving there can discard their algorithm choice; preserve the 0.9 original and rasterize a separate copy before backward editing. 0.10 Texture/SmoothV1 recipes require a reader that understands their versions; older editors may refuse the project. Broader migration/appearance qualification remains open; Save As preserves an older original rather than downgrading new semantics. |
| Q02 | Responsive rendering | A procedural scene now persists geometry/styles with one derived full-source image. Background draft previews use the document compositor and cancel obsolete jobs. The 0.8 release adds integer zoom rerenders up to 4× and 16 million pixels for admitted simple documents, with settled-preview fallback. 0.10 adds eligible per-pixel Camera Raw quick drafts and cancellable background retouch. Persistent visible/damaged-tile caching, regional invalidation and large mixed-document latency/peak-memory qualification remain open. Keep path/render-work budgets and a dependable CPU path; GPU work requires host-specific evidence. |
| Q03 | Coherent workspaces | Vector scenes edit on the main canvas with the shared Layers inspector, navigation, file handling and command search. Local Undo/Redo stays in the draft; Done/Enter keeps one document Undo step. Tool/layer/save changes keep valid drafts before continuing, while pending text fields require Update text first. Cancel/Escape discards the draft. Earlier main-canvas layouts, mode switching and themes passed their named local checks; new controls need candidate-specific qualification. Broader hardware/accessibility and long-session work remain open. |
| Q04 | Output and fonts | Partial, shipped in 0.8: JPEG inspection shows actual encoded pixels at Fit/100% with drag/arrow navigation, and curved-text controls report resolved fonts while saved outlines preserve appearance. PSD imports correct Levels gamma, distinguish master HSL/Colorize and retain mask outside coverage, with visible conversion limits. 0.10 adds converted layered PSD export, interpreted 16-bit RGB composite import and independent synthetic PSD/SVG/PDF reader checks. Non-Latin/fallback coverage and broader Photoshop interoperability remain open. |
| Q05 | Professional print | Separately design/qualify CMYK, spot colours, overprint and print proofing with appropriate fixtures. A vector editor or PDF export alone does not establish press readiness. |
| Q06 | Independent quality and stability | Keep untouched evaluation images, assess visible defects as well as numerical error, exercise save/reopen/Undo/cancel and long sessions, and measure latency/memory. A large unit-test count alone is not photographic or illustration quality proof. |
| Q07 | Distribution and dependencies | Notice findings for the previous mac/hexf-parse sources have exact-source or independently authored replacements. Packaging now rejects unresolved findings; separate candidate-build/publication workflows bind both target archives to reviewed source, checksums and smoke receipts. The 0.8 and 0.9 Linux/Windows archives and permanent publication passed their recorded checks; 0.9 also has normal-install and rollback evidence. Clean-machine acceptance, interactive Windows, signing and broader platform/hardware qualification remain open. 0.10 exact-source Linux/Windows and package checks passed; public asset verification remains separate. Later development needs its own candidate and publication gates. Keep optional models and subscription routes explicit. |

## First implementation batch

- P01: high-precision export sampling repair, with independent pixel/alpha cases.
- P05: editable target-colour uniformity with independent hue, saturation and
  lightness controls, selection masks and native 16-bit evaluation.
- V06: editable single-path SVG import/export, including supported styles,
  immutable source handling and clear refusal of unsupported artwork.
- V01: improve the original dialog path workflow's editing controls and
  integrate interchange. The subsequent main-canvas update is described below.

**Historical checkpoint:** these four bounded slices were locally qualified at
source `a4764f5`: 1,143 regression tests, native Wayland/XWayland journeys,
saved-project compatibility and the complete Create/template run. See the
[qualification record](photo-vector-foundations-qualification.md) and current
[user guide](user-guide/photo-vector.md). Those results precede the unified
canvas changes. The broader workstreams remain open; P01 still needs masked
canvas/export agreement. This remains historical source-specific evidence
within the qualified 0.7.0 source release.
No new model/provider account, system driver or commercial subscription is
required for this first batch.

## Architectural checkpoint: scalable vector documents

The existing path recipe retains a full-resolution 16-bit source and result,
plus an 8-bit display proxy. That is appropriate for a bounded path or mask,
but repeating it for every object would make illustration unnecessarily
expensive. Source-derived storage estimates are about 22.3 MiB per 1080-square
layer and 320.1 MiB per 4096-square layer, before transient render buffers and
small object/allocator overhead. These are allocation estimates, not measured
process memory. The current 768 MiB editable-asset budget counts source/result
surfaces but excludes display proxies; three independent 4096-square path
states already exceed that budget.

The shared Q01/Q02 architecture establishes a scene model before broadening V06
to multiple-object exchange. Its design and remaining work are:

- Implemented: a versioned procedural scene retains bounded objects, paths,
  transforms, paint and stacking order as the authoritative source. Released
  0.8 extensions add organizational groups, multiple-object editing, gradient/stroke
  styles and text-on-curve recipes with saved outlines. Isolated group
  appearance and reusable instances remain open.
- Implemented: objects share one derived RGBA8 image, rebuilt through bounded
  temporary tiles. Persistent visible/damaged-region caching keyed by revision,
  scale and working space remains open; temporary render tiles are not that
  persistent cache.
- Remaining: lazy raster-filter materialization and bounded tiled/strip export
  with cancellation, including a higher-precision procedural rendering path.
- Retained scene/cache accounting and admission limits remain in place;
  preserve legacy raster recipes and their admission rules for older projects.
- Qualify mixed photo/vector documents against fixed pixel, geometry, Undo,
  save/reopen, latency and peak-memory checks before broadening import claims.

The main-canvas tools now use this scene model. Broader object interchange can
extend it without introducing another document format or multiplying bitmap
layers.
The estimates follow `LayerState::retained_bytes`, `TiledRgba16::memory_bytes`,
the proxy in `editor_advanced.rs`, and the path rasterizer in `vector_path.rs`.

**Historical scene checkpoint — source `4e1da33`:** several solid paths/shapes
shared one authoritative geometry model and one derived RGBA8 layer image.
Its **Shift+P** dialog edited object order, visibility, opacity, movement and
geometry; format 11 persisted the scene and checked its saved cache. Rendering
used bounded temporary tiles, object culling, aggregate flattened-path work
checks and cancellation. See the
[scene qualification record](vector-scene-qualification.md).

That historical source passed 1,165 optimized regressions, both native backends,
the complete Create/media/template run and old-reader refusal. Its 4096-square
five-object example accounted for 64.002 MiB of scene/cache storage, against
320.125 MiB for the independent raster-backed source/result/proxy comparison.
This is retained allocation accounting, not a process-memory or rendering-speed
benchmark, and does not qualify the newer canvas interaction changes.

This is a partial Q01/Q02 checkpoint. The compositor still consumes one
full-source image per scene. Persistent visible/damaged-tile caching, isolated
group appearance, full SVG interchange, 16-bit procedural rendering and mixed-document
latency/peak-memory qualification remain open.
Legacy path recipes are preserved; they are not silently converted.

## Historical unified main-canvas checkpoint

This checkpoint replaced the artwork dialog with the ordinary
canvas and shared Layers inspector. **P** draws paths, **Shift+P** opens artwork,
**A** edits nodes and **V** picks/moves objects; double-clicking an object in
Move mode enters node editing. The inspector combines an object list, geometry controls,
stacking/visibility and live valid fill/stroke/opacity changes. **Update style**
remains available as an explicit fallback.

Draft artwork previews run through the exact document compositor, retaining
the surrounding layers and the existing artwork layer's placement, masks,
opacity, blend and supported effects. Space-drag, middle-drag and scroll use
the shared canvas navigation. **Ctrl+Z** / **Ctrl+Shift+Z** traverses local edit
history without leaving the draft, bounded to 64 entries and 32 MiB.
**Done** / **Enter** keeps a changed draft as one document Undo step;
**Cancel** / **Escape** discards it. Switching to another editing tool or layer,
or saving, first keeps a valid draft and then continues. Invalid input keeps
the draft open for correction.

Legacy single-path and vector-mask dialogs remain compatibility UI; **Edit text,
shape or vector artwork** reopens an older retained-path recipe, while **P**
opens the modern scene workflow.
At this checkpoint the scene schema used format 11, with resource limits of
1,024 objects, 100,000 total anchors, 4,096 subpaths and 16,777,216 source pixels,
plus existing render-work, cache and document budgets. Public 0.6.0 cannot read
projects containing these scenes; use Save As to preserve a compatible original.

Source `061602d` passed **1,177** regressions, the editing/Create/media run,
all 80 templates and 27 native checks per backend. Label-only polish `63048e4`
was rebuilt and passed repeat Wayland/dark and XWayland/light captures at an
800×600 logical viewport. See the
[qualification record](unified-vector-canvas-qualification.md) for evidence reuse
and limits. The qualified 0.7.0 source release includes this update; its
production/native and installation evidence is recorded in the release
qualification. At that 0.7.0 checkpoint no public binary was attached; current
0.9.0 downloads and their evidence are described above.

That checkpoint did not add gradients, boolean construction, scene text,
advanced strokes, persistent visible-tile caching or multiple-object SVG
exchange. The current partial implementations are described in the table above;
their evidence must not be inferred from this historical checkpoint.

## References reviewed

These document workflow inspiration, not benchmark results or permission to
reuse commercial engines, models, artwork or camera/lens databases.

- [Capture One look matching](https://support.captureone.com/hc/en-us/articles/22188770298269-Match-Look-Tool) and [skin-tone uniformity](https://support.captureone.com/hc/en-us/articles/360002596077-Adjusting-skin-tones).
- [DxO PhotoLab RAW, optical and colour features](https://www.dxo.com/en/dxo-photolab/features/).
- [Topaz Photo enhancement workflows](https://docs.topazlabs.com/topaz-photo/enhancements).
- [Lightroom selective masking](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/masking.html).
- [Luminar Neo depth-based relighting](https://support.skylum.com/luminar-neo-tips/how-to-make-a-photo-brighter).
- [ON1 effects and multi-image tools](https://www.on1.com/products/photo-studio/features/).
- [Affinity vector/pixel/layout workflow](https://www.affinity.studio/graphic-design-software).
- [Illustrator Shape Builder](https://helpx.adobe.com/illustrator/using/creating-shapes-shape-builder-tool.html).
- [CorelDRAW tracing](https://www.coreldraw.com/en/learn/tutorials/get-impressive-bitmap-to-vector-trace-results/).


## Approved Photocraft and Vectorcraft work — 10 October 2026

The user approved both tracks together. Keep the existing Omuse GPUI/Omarchy
canvas, `.omuse` documents and reversible editing workflow. These are delivery
requirements, not claims that the upstream applications or all features below
have been qualified in Omuse.

| Track | Approved work | Current delivery state |
| --- | --- | --- |
| Geometry | Curve-preserving booleans; measured comparison with existing polygon operations; split/join/simplify, snapping, offsets and outlined strokes; then interactive Shape Builder | 0.10 implements bounded cubic booleans, path finishing, split/join, main-canvas node/handle snapping and one-gesture Shape Builder. Local Linux application tests and bounded native merge/Undo checks passed; no speed superiority claim. |
| Vector illustration | Retained repeats, patterns and blends; richer stroke and corner controls; later envelopes and mesh workflows | Grid/radial copies are implemented as independent editable objects. Persistent live repeat recipes, patterns, blends, variable widths and meshes remain planned. |
| Typography and exchange | Paragraph layout, linked text, clipping and editable SVG text; stronger SVG/PDF fidelity and font embedding; bounded Illustrator/Affinity exploration | Supported SVG text/tspan and special stroke imports now convert to outlines with warnings. Paragraph flow, linked frames and broader interchange remain planned; outlined text is not live typography. |
| Photo removal | Texture-aware local removal comparison, original preservation, allowed-source masks, cancellation, 8/16-bit donor precision and real-photo evaluation | Opt-in TextureV2 source and UI are implemented. Isolated real-photo evidence is mixed, so it remains experimental. Integrated recipe tests and native Preview/Apply/Undo passed; native in-flight cancellation remains open. Legacy and ContextualV1 behavior is retained. |
| Photoshop exchange | Evaluate the independent Photocraft PSD reader/writer; prototype useful layered/high-depth exchange with explicit unsupported data reporting | Converted 8-bit layered export and ICC-managed 16-bit PSD/PSB composite import are implemented with reports and bounds. Existing 8-bit import remains separate. Local integrated tests and independent synthetic decoder checks passed; no external-editor visual fidelity claim. |
| Shared automation | Expose safe native photo and vector operations through typed assistant commands, using draft review and Undo | Typed vector commands are integrated with bounded object context and draft review. Live-provider qualification and broader operations remain open. No shell execution or extra provider spending implied. |
| Performance and precision | Measure tiled/damaged-region rendering, native precision and GPU opportunities on this machine; retain dependable CPU results | Planned; upstream performance claims are not Omuse measurements. |

Reviewed upstream snapshots: Vectorcraft
`9f659195c324419c087604a79c4e3b434874a62c` and Photocraft
`7722172585a01cbdb93c06f0f5ff2634fcb17999`. Their released versions and later
main-branch work are distinct. Reused source requires its original licence and
notices; the upstream branding and separately licensed assets are not imported.

Each integrated stage needs focused regression cases, visible UI review,
Undo/cancel/save/reopen/export evidence and updated common-task instructions.
The [0.10 release record](release-0100-qualification.md) identifies the exact qualified source, packages and publication status separately from earlier development runs.

These first 0.10 integrations advance V01/V02/V03/V06/V08 without replacing
the historical evidence for earlier releases. See the
[workflow guide](user-guide/photo-vector-development.md),
[final local qualification](photo-vector-qualification.md),
[photo removal evidence](texture-removal-qualification.md) and
[PSD exchange limits](psd-exchange.md). Full typography, persistent procedural
artwork, broader interchange and renderer improvements remain approved work;
they are not implied complete by these first integrations.
