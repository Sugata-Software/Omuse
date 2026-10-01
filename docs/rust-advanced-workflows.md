# Advanced workflows: implementation and supported limits

This ledger covers the thirteen advanced workstreams requested on 27 September 2026. The recorded verification below belongs to the pre-rename **Compositor Rust** checkpoint and remains historical evidence; the active product is Omuse. The recorded rollback baseline is `766fcd5`; the original Swift/Qt application remains separate. This checkpoint does not certify Mac parity or physical tablet/display support.

Most entry points are in the right-hand **Develop** inspector, under **Editable workflows** or **Paths & automation**. Refinement, controlled removal and range tools are in the **Selection** inspector. Press **Ctrl+K** to search and run these commands, including those without a default shortcut. **Ctrl+Alt+K** opens the shortcut recorder; the [keyboard reference](keyboard-shortcuts.md) lists every command and current default.

## Shared editing and persistence behavior

Choose an unlocked pixel layer and finish any floating selection first. Preview work uses a disposable draft; **Apply** publishes a validated document transaction with undo. Closing/cancelling a draft invalidates pending results. Asynchronous results are checked against their originating document revision. In Smart source, RAW, colour, automation and merge panels, choose an action, fill its fields, then press **Apply** to run the highlighted action. Record/Stop, recipe-builder buttons and Disable display proof act immediately.

Editable layer state retains a tiled 16-bit source and evaluated result, a compressed recipe, and embedded RAW bytes where applicable. `.omuse` saves these assets alongside its 8-bit cached layer image. Reopening in the Rust editor restores supported recipes. Painting directly on an editable source is guarded: paint on another layer or explicitly rasterize a copy. Rasterization sacrifices that copy's retained editing model; it is not a high-precision paint mode.

Use `.omuse` directory packages for both canvases and collections, keeping the whole directory when copying artwork. Their manifests identify the kind: `manifest.json` for a canvas, `project.json` for a collection. Legacy `.comp` projects remain readable; the first UI Save offers an `.omuse` copy and leaves the original intact. Collection saves use schema version 2 with nested `.omuse` pages/components; reading version 1 remains supported, but saving upgrades it. Earlier releases cannot read version 2, so use Save As to preserve a version 1 original when needed. The layer wire identifier remains `com.compositor.project`. See the [compatibility contract](omuse-rename.md).

The preserved editors do not understand these Rust-specific assets. Their cached image view does not establish equivalent rendering of backdrop-dependent features, nor preservation of recipes after saving there. Retain the Rust project when comparing another editor.

Most advanced operations and 16-bit compositing are limited to **16,777,216 pixels** (the UI calls this 16 MP). Editable retained assets have an aggregate **768 MiB** document budget. Tiled source storage makes snapshots shareable; evaluation and compositing still use CPU/full-image intermediate buffers. This is not an unrestricted large-document or GPU-compute pipeline.

## 1. Editable filter stacks

**Develop → Filter stack.** Add or select an effect, edit its parameters and opacity, use **Update selected effect**, reorder, enable/disable or remove nodes. **Use selection mask** attaches the current soft selection to a node; **Clear mask** removes it. Refresh the preview before applying.

The implemented menu includes exposure, Gaussian blur, unsharp mask, denoise, levels, hue/saturation, colour balance, noise, vignette, bloom, tonal contrast, invert, grayscale and curves. The original source, node order, enabled state, opacity and masks persist; reopening the workspace edits the retained stack. Editable layers accept at most 128 nodes; stack-mask storage is additionally bounded to 256 MiB.

The unreleased photo/vector branch adds **Target colour uniformity** with a reference colour, hue range/falloff and independent hue, saturation and lightness strengths. It retains native 16-bit evaluation, alpha and node masks. See the [step-by-step development guide](user-guide/photo-vector.md#make-a-product-colour-more-consistent), including older-reader and colour-space limits.

Filters, denoise, retouch and deformation have high-precision evaluation paths. The **Camera Raw compatibility node still evaluates through 8-bit pixels** before conversion back into the working space. It must not be described as a fully 16-bit Camera Raw filter stack.

## 2. Blend If

**Develop → Blend If.** Edit the four ordered 0–255 boundaries for **This layer** and **Backdrop**; split boundaries make soft transitions. The preview uses the actual composited backdrop. Settings persist per layer and remain separate from source pixels, including after explicit rasterization through typed layer metadata.

Independent source and backdrop channel buttons choose Luminance, Red, Green or Blue. This is backdrop-dependent compositing, not a permanently baked alpha mask.

## 3. Precision and colour

**Develop → Precision & colour.** Convert the selected editable source between **sRGB**, **Linear sRGB** and **Display P3**. Enter a destination path and choose **Export 16-bit PNG/TIFF** for document export. Ordinary Open/import and Smart source import retain supported 16-bit PNG/TIFF inputs and RAW masters; the canvas displays an 8-bit proxy. Destructive painting requires an explicit conversion to 8-bit pixels. The 16-bit compositor uses retained masters where present; promoting an existing 8-bit source does not create new captured detail. Imported ICC profiles are normalized into sRGB, and document compositing/16-bit exports currently target sRGB with an embedded profile. P3 working-space conversion is available, but end-to-end wide-gamut import/compositing/export is not implemented.

Clipping stacks retain their base coverage during 16-bit export; nested clipped groups retain independent child-mask links. The [photo integrity hardening record](photo-integrity-hardening.md) covers these regressions. The released **0.6.0** renderer uses bilinear sampling for transformed high-precision layers, while the canvas's **High quality** mode uses scale-aware Lanczos.

The unreleased photo/vector branch adds scale-aware, premultiplied Lanczos to high-quality 16-bit export. Reduced local/folder masks participate in the source tap accumulation, avoiding colour leakage from masked-out detail. That correction is not yet shared by the 8-bit canvas's independently filtered masks, so exact masked preview/export parity is still not established. Identity, Smooth and Nearest retain their existing behavior. The footprint is capped at 66 samples per axis with a 3/32 scale floor; extreme reductions remain approximate and masked high-quality transforms cost more CPU time.

Traditional live adjustment layers and traditional layer-effect records are **explicitly rejected by 16-bit export**. Supported editable-stack nodes are a different representation. Keep an editable project and use the ordinary export path, or deliberately rasterize a copy with the resulting precision tradeoff; do not claim those traditional effects export at full precision.

Monitor/proof ICC paths, rendering intent (0 Perceptual, 1 Relative colorimetric, 2 Saturation, 3 Absolute colorimetric) and black-point compensation (0 off, 1 on) control a software display transform; Disable display proof returns to the unproofed canvas. They do not alter saved artwork. **Proof settings are session/window state**, not persisted project settings. This code is not qualification of a calibrated monitor, compositor colour pipeline, HDR display or print proof. There is no persistent **32-bit floating-point HDR** document/export mode.

## 4. Smart and RAW sources

**Develop → Smart source** offers **Import / replace**, **Duplicate linked instance** and **Refresh same source**. Import retains decoded high-precision raster pixels; RAW import additionally embeds the original camera-file bytes. Linked paths are explicit references, not background file watchers. Refresh updates instances sharing the source identity as one validated transaction and reevaluates their recipes.

**Develop → Embedded RAW** redevelops the embedded original using exposure, temperature, tint and boost, with LibRaw producing a **16-bit source**. This is distinct from the 8-bit Camera Raw compatibility node. External files are not silently substituted during redevelop. RAW files are bounded to 512 MiB and advanced decoded sources to 16 MP. Importing a layered `.omuse` canvas, a legacy `.comp` canvas or PSD through Smart source retains its rendered image rather than nesting its full layer graph.

## 5. Vector paths and masks

**Develop → Vector paths / Vector mask.** Click to add cubic-path anchors; drag an anchor or its handle dots. **Delete node**, **Smooth node** and **Open/Close path** edit the draft; Apply commits one undo step. Vector geometry, fill/stroke fields and mask intent persist in the editable recipe. A vector mask is rasterized into layer-mask coverage while retaining its path for subsequent edits. Ordinary stack/source changes preserve later raster mask corrections or an explicitly removed mask; only path Apply rebuilds that coverage.

Fill and stroke colour fields accept #RRGGBB or #RRGGBBAA, with stroke width in source pixels (zero disables stroke). Preview style updates the path overlay. Applying reevaluates retained filters on the new geometry in a cancellable worker. Rasterization is bounded to 16 MP with additional curve-flattening and fill/stroke work limits.

The unreleased photo/vector branch adds corner nodes, exact curve-midpoint insertion, reverse/new subpaths, fill-rule switching and compound-hole previews. It also provides explicit **single-path editable SVG** exchange. The [development guide](user-guide/photo-vector.md#draw-and-refine-an-editable-path) explains these controls and the interchange limits. Direct-canvas editing, booleans, multi-object SVG, gradients and variable-width strokes remain planned.

## 6. Selection refinement

**Selection → Refinement workspace.** It starts from the selection, existing layer mask or source alpha. Paint foreground/background corrections on the source preview, adjust edge refinement, contrast, shift and defringing, and inspect against Original, Black, White, Checkerboard or Mask backgrounds.

Apply creates a separate refined cut-out and preserves the original layer; it does not rewrite the original pixels. The result is baked pixel/alpha output, not a persisted correction-stroke history. Limits include 16 MP, 4,096 corrections, a bounded brush-work budget and a 64-pixel maximum defringe radius.

## 7. Retouching

**Develop → Advanced retouch.** **Frequency layers** creates an editable tone/texture group, using Normal and Linear Light reconstruction, above its retained source. This path requires an opaque source in encoded sRGB and no Blend If; paint on a rasterized copy if required. **Dodge layer / Burn layer** creates a separate editable result with local tonal controls and the current selection as an optional node mask.

**New healing layer** creates a transparent layer, selects Heal and enables all-layer source sampling. Alt-click a source before painting. The final healing strokes are raster edits with undo; they are not a replayable source-linked stroke recipe.

## 8. Controlled removal

Select the target first, then **Selection → Controlled removal**. Enter the allowed sampling rectangle, search radius, patch radius and feather; preview before applying. Selected target pixels are excluded from the sampling pool. Pixels outside the target remain protected by the operation; this form does not expose a separate arbitrary protection-mask painter.

Apply creates a new editable removal result with source and recipe retained. This is deterministic bounded patch replacement, not generative fill. It has a tighter **4,000,000-pixel** input limit, search radius up to 64, patch radius up to 4 and an explicit work budget.

## 9. Brushes and Linux tablet input

**Develop → Brush studio.** Edit size, flow, spacing, hardness, scatter, angle/jitter, procedural texture strength and a monotonic pressure curve. The custom-tip image entry accepts up to **512×512 pixels**; luminance and alpha define coverage. Apply stores the brush configuration in per-user `brush.json`; **Use basic brush** removes that configuration. Settings persist as an application brush, not as editable document strokes.

The preview demonstrates a pressure ramp; mouse artwork uses constant pressure. Wayland **tablet-v2** events are wired into Brush, Pencil and Eraser, including mask targets, measured pressure, tilt, tool identity and stroke completion on proximity-out. **Physical tablet support remains unverified**: synthetic event wiring does not establish device mapping, driver behavior or X11 pressure/tilt support.

## 10. Editable deformation

**Develop → Mesh & pin warp.** The UI provides a **2×2 mesh** through four normalized corner positions, plus source/destination pin clicks, pin radius/strength and **Freeze selection**. Preview and Apply retain the original and warp settings; reopen to adjust the recipe. Clear pins removes draft pins.

The broader kernel can represent denser meshes, but the UI does not offer a general mesh-grid editor. Pins are bounded to 256. Selection freezing is the exposed protection control; a separate protect-mask painting workspace is not implemented. Evaluation is bounded to 16 MP and remains CPU-based.

## 11. Automation

**Develop → Recipes & batch.** Record supported whole-image filter and quick-adjust operations; add Resize, Crop, quarter-turn Rotate and Flip steps explicitly. Selection-dependent or unsupported edits pause recording rather than pretend to be captured. Recipes are portable versioned JSON, limited to 256 steps and 1 MiB on load; saving requires a new filename.

**Apply as new layer** evaluates through an 8-bit proxy and retains the previous layer. Folder batches enumerate supported inputs nonrecursively in sorted order, require a separate existing output directory, preserve existing files, and report individual failures/cancellation. Up to 10,000 files are enumerated; recipe input/output processing is bounded to 16 MP. Completed outputs remain after cancellation. Reopening the workspace shows the last run's summary and up to twenty filename/result rows; the full report remains in memory for the session.

Batches accept `.omuse` projects and legacy `.comp` packages alongside supported images. For a collection, only its saved active page is processed, matching `omuse --export INPUT.omuse OUTPUT.png`. Use Create's content-pack export for all pages. Output names retain the input suffix, such as `Artwork.omuse.png`, so projects and images with the same stem remain distinct.

```sh
omuse --batch RECIPE.json INPUT_FOLDER OUTPUT_FOLDER png CANCEL_FILE
```

The optional cancel-file path stops further processing when that file appears. This is a restricted editing recipe system, not arbitrary UI-event recording, scripting or a 16-bit batch pipeline.

## 12. Multi-image workflows

**Develop → Multi-image merge.** Choose a folder, alignment shift limit and **Focus stack**, **HDR merge** or **Panorama**. HDR requires one exposure-stop value per sorted input. The UI accepts 2–64 regular image files and caps retained input pixels at 256 MiB; use a folder containing only the intended inputs. Results are inserted as new editable image sources and the canvas expands when needed.

Registration is **translation-only**, with bounded overlap/score checks and a maximum shift of 1,024 pixels; no rotation, perspective, lens-aware panorama projection or moving-subject deghosting is claimed. Output is limited to 16 MP and processing uses a memory budget. Although exposure merging uses floating-point radiance internally, the UI result converts to normalized **16-bit linear sRGB**, clipping values outside that range. It does not retain a 32-bit HDR radiance document. Source-frame alignment/exposure history is not persisted as a re-editable multi-image recipe; the merged master is retained.

## 13. Existing luminosity and colour ranges

**Selection → Luminosity range / Colour range.** Adjust bounds or sampled colour, tolerance/softness and inversion, then preview. Choose Replace/Add/Subtract selection or an editable layer-mask output. Apply is undoable; cancellation leaves the document untouched. Saved layer masks retain their resulting coverage. A temporary selection and its range-dialog parameters are not a persistent parametric mask recipe. Range processing is bounded to 16 MP.

## Verified local checkpoint — 27 September 2026

The source entry points are `studio_ui.rs`, `advanced_ui.rs`, `workflow_ui.rs`, `vector_ui.rs` and `tablet_ui.rs`; persistence/evaluation lives in `advanced.rs`, `advanced16.rs`, `precision.rs`, `raster16.rs`, `smart_source.rs`, `recipes.rs` and `multiframe.rs`.

The full release suite passed **470 tests**, with zero failures and three intentionally ignored timing benchmarks, followed by the eleven-step headless editing/export journey. Real LibRaw DNG, layered PSD/reference-render and ONNX subject fixtures were enabled. A separate vendored Wayland tablet frame-state test passed. Formatting and diff checks passed.

The production executable and installed launcher each passed the nineteen-step native Wayland journey, including exact 16-bit source import, editable-stack preview/Apply, save/reopen/undo and 16-bit export pixel equality. Thirteen advanced-panel layouts were captured and visually inspected at the minimum 800×600 logical window size across dark and light themes. These repeat the same common journey; they are not thirteen independent end-to-end tests of every feature. Per-workspace interactions are also covered by the headless UI suite. The synthetic transparent artwork deliberately triggers the frequency-separation opaque-source guard; the removal fixture also demonstrates the insufficient-sampling-area guard.

The production executable and installed launcher passed seven CLI batch checks: exact output pixels/dimensions, per-file reporting, unchanged inputs, collision preservation, cancellation before processing, same-folder rejection and unsupported-recipe rejection. Save/recovery qualification published twelve revisions of a three-layer 16-bit project and checked retained source/result samples and recipes. Killing its writer during save left project revision 3 and recovery revision 4 intact. This tests process interruption, not sudden power loss.

The installed executable SHA-256 is `764b56838627cb8e3d56a8a59a182a4b4cd97f30407c713388808fba7cc2d6bc`. Machine-local evidence is retained under ignored `rust/evidence/`: `advanced-release-gate.log`, `advanced-tablet-backend.log`, `advanced-native-production/`, `advanced-native-installed/`, `advanced-cli-production/`, `advanced-cli-installed/` and `advanced-release-qualification/`. `advanced-release-receipt.json` ties the final source revision and fingerprints to those runs and the preserved baseline executable.

Physical tablet testing, calibrated-display/proof qualification, broader real-image quality evaluation and Mac comparisons remain separate. Most advanced operations have a 16 MP ceiling, document compositing/export targets sRGB, and persistent 32-bit HDR is not implemented. This is a locally qualified development checkpoint; no public release or upstream publication has been made.
