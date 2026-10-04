# Omuse photo and vector studio roadmap

Approved direction, 1 October 2026. Omuse remains a native Rust/GPUI application
for Linux/Omarchy, with one `.omuse` project family and an emphasis on making
finished content. This roadmap extends the existing editor incrementally.
It is not a claim of Illustrator, Photoshop or camera-colour parity.

The [project guide](project-guide.md) tracks current implementation and evidence.
The installed baseline is the [0.7.0 source pre-release](releases/v0.7.0.md). Its
qualified runtime is
`5b3daefbb5258afff4a74a2ff3db5247b074972d`; its local, exact-source CI,
native, installation and rollback evidence is recorded in the [0.7.0
qualification](release-070-qualification.md). No public binary is attached.
Historical qualification records describe their named source checkpoints;
the earlier selection, boolean and SVG checkpoint has its own
[development qualification record](vector-workflow-qualification.md).
That named checkpoint passed 1,254 optimized application tests, editing/Create
acceptance, 80 templates, native checks on Wayland/XWayland and a bounded Cua
keyboard/construction journey. Production-feature, exact-source CI and release
promotion checks remain separate from this local development result.
Calendars, scheduling, social publishing and tablet qualification remain
outside this work.

The current **unreleased** implementation extends that checkpoint with gradient
fills and stroke settings, text on curves and outline conversion, bounded
vector PDF, hue-range masks, reference-colour matching, JPEG detail inspection,
PSD adjustment/mask corrections and external UTF-16 font-run reads. The table
below marks these as partial contributions to their larger workstreams.
Historical test counts above do not qualify these later additions; release
notes and exact-candidate evidence remain separate.

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
| P04 | Persistent selective masks | Partial: the unreleased Hue range mode adds sampled circular hue, softness, saturation protection and inversion, with selection/add/subtract/intersect or layer-mask output. Reveal/hide masks retain soft coverage through transformed layers. Saved masks retain pixels; persistent hue/range recipes, semantic person/hair/clothing/face-region masks and broader edge qualification remain open. |
| P05 | Target-colour uniformity | User-chosen reference colour, adjustable range/falloff and independent hue/saturation/lightness uniformity. Preserve texture, alpha, masks and high-precision data; useful for skin and products without assuming a skin tone. |
| P06 | Reference-look matching | Partial: unreleased Match reference colour is an editable Filter-stack operation with Amount, Preserve lightness, node opacity/masks and 8-bit/retained 16-bit sRGB/Display P3 evaluation. Bounded alpha-weighted Oklab statistics make the recipe independent of the reference file. Semantic/local matching, explicit exposure/white-balance normalization, campaign-wide batch preview and independent real-photo quality qualification remain open. |
| P07 | Retouch and removal fidelity | Partial: Blur/Smudge/Liquify use fractional footprints and consistent path spacing; Smudge carries evolving paint and Liquify samples the untouched stroke source through accumulated displacement. Work limits retain transactional failure behavior. Broader clone/heal/removal fidelity, sampling/protection UX and independent real-photo evaluation remain open; removal is not a general reconstruction guarantee. |
| P08 | Learned restoration | Detail-preserving denoise, deblur and enlargement; optional scratch/face/text recovery where validated. Preserve originals and expose before/after detail crops and generated-detail disclosure. Check model redistribution, Linux execution and download integrity before integration. |
| P09 | Depth and relighting | Editable depth masks, near/far brightness and warmth, portrait/product relighting and edge/halo refinement. Keep depth correction accessible and avoid silently replacing original lighting or geometry. |
| P10 | Multi-image merging | Extend current translation-only registration with rotation/perspective and appropriate panorama projection; improve focus blending and HDR moving-subject handling. Persist source/alignment/exposure recipes and cancellation-safe results. |
| P11 | Wide gamut and HDR | Complete colour-managed working/compositing/export paths and persistent 32-bit floating-point radiance with explicit tone mapping. Qualify monitor/compositor behaviour independently from file correctness. |
| P12 | Product and campaign finishing | Connect consistent colour, masking, relighting and transparent/shadow output into reusable editable product-photo recipes and batch previews. Existing frames, templates, brands and bulk creation remain the foundation. |

## Vector, typography and exchange

| ID | Work | Existing foundation and required outcome |
| --- | --- | --- |
| V01 | Direct canvas Pen and Node tools | Implemented on the development branch: P draws, A edits nodes, V picks/moves objects, double-click in Move mode enters nodes, and the shared Layers inspector provides insertion, open/close, corner/smooth and object controls. Arrow nudging uses source pixels. The unified canvas passed local regression/native checks. Pen click/drag corners and smooth curves, linked/Alt handles, Shift constraints, exact segment insertion and first-anchor closing are implemented in the current trace/curve batch. Split/join, precise snapping and broader hardware qualification remain open. |
| V02 | Shape construction | Implemented in the unreleased vector workflow batch: background Unite, Subtract, Intersect, Exclude and Divide on selected filled paths, with Undo, fill-rule handling and bounded world-space flattening. Divide partitions the bottom object; resulting curves are polygonal. Interactive Shape Builder, retained live booleans and broader pathological-geometry qualification remain open. |
| V03 | Strokes and fills | Partial: unreleased linear/radial fills have 2–16 alpha-capable stops, geometry, Pad/Repeat/Reflect spread and fitting controls. Uniform-width strokes have caps, joins, dash/gap patterns, offsets and miter limits. Saved scene version 3/project format 13 retains them. Variable widths, expansion, live corners/offsets and mesh/pattern fills remain open. |
| V04 | Typography | Partial: unreleased single-line text on a retained curve has font/size/tracking/position/alignment, resolved-font reporting, explicit guide replacement and Convert to outlines. Scene version 4/project format 14 retains its recipe and portable saved outlines. External project format-11 UTF-16 font/colour runs also import safely. Styled spans on curves, paragraph flow, complete fallback/layout qualification and broad editable-text interchange remain open; SVG/PDF exports text as outlines. |
| V05 | Image tracing | Implemented in development: local Colour/Gray/B&W tracing, presets, detail/smoothing/corner/noise/resolution/point controls, Source/Trace preview, retained original, saved settings/retrace, editable compound curves and one-step Keep/Undo. See the [qualification record](image-trace-qualification.md). Photo results are solid-colour approximations; broad photographic/topology/large-document qualification remains open. |
| V06 | Editable SVG exchange | Partial: the unreleased scene importer/exporter handles multiple objects, organizational groups, names, visibility, object opacity, supported linear/radial fills and uniform-width cap/join/dash styles. Import appends artwork; scene export retains objects, with Omuse curved text converted to outlines. SVG text import, clipping/effects, group opacity, paint-server strokes, skewed/nonuniform stroke transforms and external resources remain unsupported. Legacy single-path exchange and raster SVG import retain their narrower/separate contracts. |
| V07 | Vector-preserving PDF | Partial: unreleased Export vector PDF writes one scene or legacy path at document DPI, retaining supported geometry, transparency and Pad gradients. Text exports as outlines; Repeat/Reflect are refused. Mixed photo/vector document export, explicit raster-effect fallbacks, multipage/artboard vector output, editable text/font embedding and independent-reader qualification remain open. Existing multipage content PDF stays rasterized. |
| V08 | Illustration tools | Reusable vector symbols/instances, repeats, live offsets, blends and mesh/pattern fills, reusing the existing component/artboard model where compatible. Add only after the core geometry and exchange contracts are stable. |
| V09 | Illustrator interchange | Explore an explicitly bounded PDF-compatible `.ai` import after SVG/PDF foundations. Full proprietary Illustrator document round-trip is a separate research item, not an implied result of SVG support. |

## Shared quality and experience

| ID | Work | Required outcome |
| --- | --- | --- |
| Q01 | One document and reversible edits | Implemented foundations retain raster/vector/text in `.omuse` with bounded Undo and explicit unsupported-version refusal. Current scene versions 1/2/3/4 map to canvas formats 11/12/13/14; ordinary canvases remain format 10. Formats 12–14 and reference-match nodes require the unreleased reader. Broader migration/appearance qualification remains open; Save As preserves an older original rather than downgrading new semantics. |
| Q02 | Responsive rendering | A procedural scene now persists geometry/styles with one derived full-source image. Background draft previews use the document compositor and cancel obsolete jobs. The unreleased workflow adds integer zoom rerenders up to 4× and 16 million pixels for admitted simple documents, with settled-preview fallback. Persistent visible/damaged-tile caching, regional invalidation and large mixed-document latency/peak-memory qualification remain open. Keep path/render-work budgets and a dependable CPU path; GPU work requires host-specific evidence. |
| Q03 | Coherent workspaces | Vector scenes edit on the main canvas with the shared Layers inspector, navigation, file handling and command search. Local Undo/Redo stays in the draft; Done/Enter keeps one document Undo step. Tool/layer/save changes keep valid drafts before continuing, while pending text fields require Update text first. Cancel/Escape discards the draft. Earlier main-canvas layouts, mode switching and themes passed their named local checks; new controls need candidate-specific qualification. Broader hardware/accessibility and long-session work remain open. |
| Q04 | Output and fonts | Partial: unreleased JPEG inspection shows actual encoded pixels at Fit/100% with drag/arrow navigation, and curved-text controls report resolved fonts while saved outlines preserve appearance. PSD imports correct Levels gamma, distinguish master HSL/Colorize and retain mask outside coverage, with visible conversion limits. Independent exported-pixel/geometry comparisons, non-Latin/fallback coverage and broader Photoshop interoperability remain open. |
| Q05 | Professional print | Separately design/qualify CMYK, spot colours, overprint and print proofing with appropriate fixtures. A vector editor or PDF export alone does not establish press readiness. |
| Q06 | Independent quality and stability | Keep untouched evaluation images, assess visible defects as well as numerical error, exercise save/reopen/Undo/cancel and long sessions, and measure latency/memory. A large unit-test count alone is not photographic or illustration quality proof. |
| Q07 | Distribution and dependencies | Notice findings for the previous mac/hexf-parse sources have exact-source or independently authored replacements. Packaging now rejects unresolved findings; separate candidate-build/publication workflows bind both target archives to reviewed source, checksums and smoke receipts. Execution and review of the new native candidate workflow, platform qualification and release publication remain separate gates. Keep optional models and subscription routes explicit. |

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
  transforms, paint and stacking order as the authoritative source. Unreleased
  extensions add organizational groups, multiple-object editing, gradient/stroke
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
qualification. No public binary is attached.

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
