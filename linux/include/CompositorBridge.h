#ifndef CompositorBridge_h
#define CompositorBridge_h

/*
 * CompositorBridge.h — C ABI seam between the Swift core and the Qt/C++ host
 * (plan §9, ENG-1). This header defines the canonical buffer contract and
 * status codes that the Swift @_cdecl functions operate on. The Qt host
 * (host/host_run.cpp / CompositorHostRun) implements the SessionWindow
 * and drives the Swift core through these calls.
 *
 * Status codes:
 *   0  success
 *  -1  invalid argument (bad geometry, NULL required buffer, out-of-range size)
 *  -2  no document
 *  -3  busy (operation in progress)
 *  -4  unsupported version
 *  -5  operation failed
 *  -6  invalid/closed handle
 */

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Editor session commands (JSON-bound, UTF-8).
 * See EditorBridge.swift for the full command list and ownership rules.
 * Input is copied; JSON is NOT NUL-terminated.
 */
#define COMMAND_VERSION 1

/*
 * Command actions (match EditorBridge.swift enum cases):
 *   "new"                          — create new document
 *   "resizeCanvas"                 — resize canvas
 *   "cropCanvas"                   — crop canvas
 *   "resizeImage"                  — resize document image
 *   "addLayer"                     — add blank layer
 *   "selectLayer"                  — select a layer by UUID
 *   "deleteLayer"                  — delete active layer
 *   "renameLayer"                  — rename active layer
 *   "setVisible"                   — set layer visibility
 *   "setOpacity"                   — set layer opacity
 *   "setBlendMode"                 — set layer blend mode
 *   "setSelectedOpacity"           — set selected layers opacity
 *   "cycleBlendMode"               — cycle blend mode forward/backward
 *   "flipLayer"                    — flip layer horizontally
 *   "flipCanvas"                   — flip canvas horizontally
 *   "addShape"                     — add geometric shape
 *   "transform"                    — transform layer
 *   "transformBegin"               — begin transform operation
 *   "setMaskSelected"              — set mask selection state
 *   "transformPreview"             — preview transform
 *   "transformCommit"              — commit transform
 *   "transformCancel"              — cancel transform
 *   "undo"                         — undo last edit
 *   "redo"                         — redo last edit
 *   "invert"                       — invert selected pixels
 *   "deselect"                     — deselect selection
 *   "copy"                         — copy selection
 *   "copyMerged"                   — copy merged selection
 *   "paste"                        — paste from clipboard
 *   "cut"                          — cut selection
 *   "duplicateLayer"               — duplicate active layer
 *   "layerViaCopy"                 — create layer via copy
 *   "fillForeground"               — fill selection with foreground
 *   "fillBackground"               — fill selection with background
 *   "clearSelection"               — clear selected pixels
 *   "addRevealMask"                — add reveal mask
 *   "addHideMask"                  — add hide mask
 *   "deleteMask"                   — delete layer mask
 *   "invertMask"                   — invert layer mask
 *   "setMaskEnabled"               — set mask enabled state
 *   "setMaskLinked"                — set mask linked state
 *   "selectRectangle" / "selectEllipse" — select rectangular or elliptical area
 *   "brushBegin" / "brushMove" / "brushEnd" / "brushCancel" — brush stroke
 *   "warpBegin" / "warpMove" / "warpEnd" / "warpCancel" — warp brush
 *   "filterBegin" / "filterPreview" / "filterSetPreview" / "filterCommit" / "filterCancel" — filter
 *   "adjustmentBegin" / "addAdjustment" / "adjustmentPreview" / "adjustmentCommit" / "adjustmentCancel" — adjustment
 */

#ifdef __cplusplus
}
#endif

#endif /* CompositorBridge_h */