# Omuse 0.9 interface and photo examples

These files are actual captures and engine outputs, copied without image
retouching or UI compositing. They illustrate workflows and their limits;
they do not establish universal repair quality.

| File | Source and scope |
| --- | --- |
| `01-compact-text.png` | Public runtime `65c95cc165f9d730a9f0bcea51c51a84cf4adb7e`, native GPUI/XWayland on Omarchy, 800 × 600 logical / 1200 × 900 device pixels. Synthetic artwork; Apply and Cancel are inside the inline text panel and the keyboard hint wraps. |
| `02-portrait-removal.png` | The same final runtime and Linux binary, actual 1896 × 1150 device-pixel capture in an isolated profile. Saved editable removal is visible above the hidden original portrait. Some seam/texture mismatch remains. |
| `03-removal-before.png` | NASA portrait before the controlled-removal operation; 512 × 512 pixels. |
| `04-removal-after.png` | Actual ContextualV1 result from the photo library at public `8f72ce5c945261ba92d1490aee4374176f24cfb8`; that library source is unchanged in final `65c95cc1`. The final package reopens/exports its saved result exactly. |

The controlled-removal example selects an ellipse at **128, 344**, size
**88 × 88** source pixels. Its clean sampling rectangle starts at **100, 300**,
size **126 × 147**, with **Search radius 64**, **Patch radius 2**, **Feather 0**.
The name badge and zipper are excluded from that donor rectangle. The mission
patch is replaced with plausible suit texture, but a mild seam and texture
mismatch remain. Review each repair and refine the selection/source as needed.

Photograph: **NASA, Eileen Collins portrait**, obtained from the scikit-image
sample collection and [documented as public domain](https://scikit-image.org/docs/stable/api/skimage.data.html#skimage.data.astronaut).
The [pinned source manifest](../../../../rust/tests/fixtures/photo-sources.json)
records the original URL, size and SHA-256. The before/after pair demonstrates
an editing operation; it is not an unaltered photograph or an endorsement.

The final Linux executable SHA-256 is
`da39ef88d0c6f08074051784974668841e62a2ac9c56c527b73c9ba4685e4ec6`.
The [release receipt ledger](../../../release-090-receipts.json) records capture
and output hashes alongside their qualification scope.
