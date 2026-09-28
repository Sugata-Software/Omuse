# Luminosity and colour ranges

The Select inspector provides **Luminosity range** and **Colour range**. Both sample the visible canvas, including its current layer compositing and background. The draft shows the source alongside a grayscale coverage preview. White means fully selected, grey means partial coverage, and black means excluded.

Luminosity range includes Shadows, Midtones and Highlights presets, editable From/To values from 0 to 255, and a soft falloff outside that interval. Colour range uses a sampled RGB colour, a fully selected tolerance and a soft falloff beyond it. Click a visible pixel in the source preview or use the shared Omarchy colour picker to choose the colour. Invert reverses range membership while continuing to exclude transparent pixels.

The output choices are:

- **Selection:** replace the current selection.
- **Add / Subtract:** combine soft coverage with the current selection.
- **Layer mask:** replace the selected layer's mask, including an independent mask or live source link. The dialog names the affected layer. Source pixels stay intact, and undo restores the previous mask and its metadata.

Preview work runs in the background. Changes coalesce for 120 ms, superseded work checks cancellation each row, and completions must still belong to the active draft. Cancel and Escape discard the draft. Apply uses the current validated settings, creating one selection or mask undo step. Selection changes do not mark artwork dirty. Layer masks are saved in the existing `.comp` format and remain paintable after reopening.

These are generated masks, not live parametric range nodes: changes to the source after Apply do not automatically regenerate them. Luminosity uses display-referred sRGB luma, not scene-linear luminance. Colour similarity uses normalized Euclidean RGB distance, not a perceptual colour-difference metric. Partial source alpha gates coverage once after inversion.

Both the sampled canvas and projected layer mask are limited to 16,777,216 pixels. Layer-mask projection samples soft coverage in layer coordinates, including translation, rotation and flips. Pixels outside the canvas receive zero coverage. Locked layers and locked parent groups reject mask replacement. Floating selections must be committed or cancelled first.

The implementation is covered by analytical mask-kernel tests, editor projection/undo/persistence tests and GPUI interactions. The native acceptance journey also checks luminosity selection undo and colour-mask save/reopen. `scripts/native-rust-check.py --panel luminosity-range` or `--panel color-range` captures the corresponding workspace, with the existing dark/light and minimum-window options.

## Local verification — 27 September 2026

This is a fixed pre-rename evidence record. Its installed executable name and
evidence filenames are retained as observed at that checkpoint.

The full Rust validation run passed 376 tests, with three benchmark tests ignored. A final focused run passed all seven range-dialog interaction tests, covering minimum-window geometry, colour sampling, soft-selection and layer-mask undo, stale/cancelled work, invalid input, and closing the window during an apply operation. The production executable was built without the `ui-test` feature.

The final candidate passed all 15 native acceptance checks in the light theme. The installed launcher passed the same 15 checks in the dark theme, including luminosity selection undo and colour-mask save/reopen pixel equality. Both final captures were visually inspected at the supported 800 × 600 logical minimum window size. These journeys dispatch synthetic GPUI input; they do not establish physical tablet behaviour or complete Photoshop/Affinity parity.

Machine-local evidence is retained under `rust/evidence/`: `range-validation.log`, `range-final-ui-tests.log`, `range-final-luminosity-20260927/`, `range-installed-colour-20260927/` and `range-release-receipt.json`. The previous installed executable is preserved as `compositor-rust.3954547` alongside the current installation. The generated-mask and 16,777,216-pixel limits above still apply.
