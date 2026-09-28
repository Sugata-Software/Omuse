# TODOS (collected by /autoplan, 2026-09-21; proposed, review before relying on)

- **Colour management**: ICC profiles, bit depth, blend space, export profiles. Deferred by CEO phase (R30 only states assumptions). Needs owner input. Depends on: R51 pixel contract.
- **Command palette**: Qt host Command Palette (`Ctrl+Shift+P` / `F1`) implemented with fuzzy action search, keyboard navigation, and smoke verification. (DONE)
- **Finger-touch large-target mode**: larger hit areas when touch is used. Deferred by design phase.
- **Crash-recovery autosave implementation (R61)**: Periodic background autosave snapshot (`autosave.comp` + `autosave.info`), startup recovery check, recovery restore, and clean shutdown clearing implemented and smoke verified. (DONE)
- **HEIC and TIFF import**: separately gated after decoder hardening (R56).
- **Research-risk features** (Content-Aware Fill, Remove Background, Liquify, Lens Correction, Healing): need their own decision per R6.
- **Cancelled new adjustment sheet discard (R26)**: Discard/undo newly created adjustment layer on dialog rejection implemented and smoke verified. (DONE)
- **Layers tree multi-selection (R22)**: `ExtendedSelection` in `m_layersView` + multi-layer selection deletion implemented and smoke verified. (DONE)
- **Stage 4 & 5 (CoreGraphicsCompat Shim & Skia Raster Backend)**: 100% complete (`3d617c7`, `39f1d88`, `02a8178`, `835a7f0`). 22 canvas ops mapped to Skia C ABI, dynamic `.so` loader, comprehensive parity tests passing, `COMPOSITOR_RENDERER=skia|swift` env switch verified.
- **README / Manifest alignment**: Resolved (updated README to KDE 6.11 and swift6//26.08 matching `com.wonderassembly.Compositor.yaml`).
- **All Stages 0 - 12 verification**: 100% green (426/426 Swift tests, 6/6 CTest tests, 5/5 Qt smoke test modes).
