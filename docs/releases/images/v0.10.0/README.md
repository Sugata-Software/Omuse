# Omuse 0.10 packaged interface

These are actual captures of the **0.10.0 Linux CI package**, copied unchanged.
They are not mockups or edited composites. The exact public runtime source is
[`920ae0fddb9a58ba69526b7e7976b5d770435fd9`](https://github.com/Sugata-Software/Omuse/commit/920ae0fddb9a58ba69526b7e7976b5d770435fd9).
Package capture does not itself establish public download availability; the
[release qualification](../../../release-0100-qualification.md) records the
separate CI, package and publication checks.

Both captures use the packaged application on Linux/Omarchy through XWayland,
in an isolated profile, at **1568 × 954 device pixels**. The executable SHA-256
is `0f113edc4e6727d9f84ca99d85cbf5102739a18968673927badeb8a46d8eeab6`.
No personal filesystem paths or account details are shown.

| File | Capture and scope | SHA-256 |
| --- | --- | --- |
| `01-composition.png` | Native package evidence `01-composition.png`: the mixed composition is open in the native editor; live text, 34 vector objects and independent repeat copies remain editable. | `49e9faa6ca1dac07674c90158ea2d03199e8da97ddf1103b1e3cb9fac87f1918` |
| `02-shape-builder.png` | Native package evidence `07-merged.png`: two filled ellipses merged into an editable **Merged shape**. | `2b5c4e0d5050416f03bcdf48b97fcfc51bf56ec0de89e422bec59616a7a69780` |

The packaged journey exercised **Shift+P**, **Ctrl+A**, **Alt+M**, a foreground
merge drag, draft **Ctrl+Z / Ctrl+Shift+Z**, **Enter** to keep the artwork and
document **Ctrl+Z**. Evidence captures `08-draft-undo` through
`11-document-undo` record the history checks. Background keyboard input worked;
a background pointer attempt timed out without changing the artwork, and a
foreground drag succeeded. Both isolated test windows and the driver session
were closed afterward. This is a bounded native check, not acceptance of every
input method or platform.

The artwork is an original synthetic Omuse qualification composition generated
by [`photo_vector_workflow.rs`](../../../../rust/examples/photo_vector_workflow.rs).
It contains no borrowed photographs or private user artwork. Bundled Outfit
type retains its [font licence and provenance](../../../../rust/assets/fonts/README.md).
Captures and composition are provided as part of the Omuse project under its
[MIT licence](../../../../LICENSE); third-party typeface licensing is unchanged.

These views demonstrate the interface and named workflows. They do not establish
general design quality, accessibility, Windows interaction or performance. The
historical 0.7 studio film elsewhere in the README is a separate asset and does
not demonstrate these new controls. Earlier pre-version-bump captures remain
in the development evidence; these files now document the actual 0.10 package.
