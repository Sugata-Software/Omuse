# Omuse launch screen

The launch screen carries the selected Sunset Muse identity into the native
application: warm paper, a plum Muse, flowing sunset inlays and an outlined
Outfit wordmark. Faint curved bands frame the corners. Four small sunset accents
breathe in sequence while a workspace is being prepared, capped at 30 frames
per second. The emblem and typography remain still.

The layout fits the 800 × 600 minimum window and centres the same lockup in a
maximized workspace. The editor itself continues to follow Omarchy through
`gpui-omarchy`; the splash palette never changes image pixels or editor themes.

## Loading and handoff

The first splash frame paints before document preparation begins. File decoding,
initial compositing and CPU display-tile creation run on the background executor.
The status identifies the document being opened, or says “Preparing your
workspace” for a new canvas. There is no invented percentage or minimum display
time. Fast launches finish quickly.

The prepared editor appears under a 220 ms fade, timed from the first handoff
frame. After that the overlay is removed, its image textures are retired and its
animation stops. Keyboard focus and the editor's unsaved-document close guard
belong to the editor as before.

The desktop's reduced-motion preference disables the accent animation and makes
the handoff immediate. `OMUSE_REDUCED_MOTION=1 omuse` also disables these splash
effects for a single launch. This override is specific to the splash.

Closing during startup cancels the coordinator and prevents a late result from
creating an editor. A synchronous image decoder already working cannot be
interrupted internally; cancellation is checked between preparation stages.
Failed opens produce a usable new canvas with the existing error status, without
retaining the failed file or folder as its save destination.

The assets are embedded, with no network, video decoder or runtime font lookup.
The font source and licence are retained in [the font provenance guide](../../rust/assets/fonts/README.md).

## Verification

The standard Rust test gate includes startup coverage for first-frame ordering,
project and image pixels, failed-open save safety, cancellation, reduced motion,
duplicate completion, texture retirement and pointer input after dismissal.

The native harness captures only its own disposable Omuse window:

```sh
python3 scripts/native-rust-check.py rust/target/release/omuse \
  rust/evidence/splash-motion --capture-startup --capture --minimum-window
python3 scripts/native-rust-check.py rust/target/release/omuse \
  rust/evidence/splash-still --capture-startup --reduced-motion --capture
```

Each run needs a fresh evidence directory. `--capture-startup` holds only the
explicit `--ui-smoke` test launch while two frames are captured. It verifies
changing frames with motion enabled, identical frames with reduced motion, and
the paint → preparation → editor → dismissal sequence, then runs the native
editing/save/reopen journey. A missing capture controller records a test failure
and releases the hold after 20 seconds. Ordinary launches have no such gate.

An ungated harness run records the same stage timings without capture delays.
Timing observations describe this host and build, not a performance guarantee.
