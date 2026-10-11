# Small independent 16-bit Photoshop fixtures

Retrieved from the MIT-licensed psd-tools test corpus at immutable commit
`96eb134c17b2c65edf4c4151c0f00b802ada86c2`, 10 October 2026. The MIT text is
retained in `LICENSE.psd-tools`. These original files were not rewritten or
stripped of their embedded 3,144-byte sRGB ICC profile.

| File | Upstream path | Git blob | SHA-256 |
|---|---|---|---|
| `psd-tools-16bit5x5.psd` (22,592 bytes) | `tests/psd_files/16bit5x5.psd` | `424f7cdcede181d8f389db188d3df8ac3caf93ed` | `c78689ea7b576f23bbd8b6b4f4993a365266c3ffc9aaa02cc20bef4a36a9d7a0` |
| `psd-tools-16bit5x5.psb` (23,482 bytes) | `tests/psd_files/16bit5x5.psb` | `834c4dab9d11f528adb1ff3933ab513bbeb08812` | `e0c9b37ffb5981aaedbeec2b2dfa3ad561b57e86e5e04758ed859ca819cd427c` |

Source: <https://github.com/psd-tools/psd-tools/tree/96eb134c17b2c65edf4c4151c0f00b802ada86c2/tests/psd_files>.
Both are 5×5 RGB16 documents with `Lr16` layers and a raw merged RGB composite.
The compatibility test imports the merged composite only, retains 16-bit
colour-managed samples, and requires a visible layer-conversion report.

`linear-srgb16.icc` is an original, generated test asset (Omuse MIT licence),
568 bytes, SHA-256
`161d2ef4605701001891bfd948b17c4b37dd9b55d5e26afdd8dc66e0ac18cae8`.
It was created independently of Omuse's import path through the system LittleCMS
API: `cmsBuildGamma(NULL, 1.0)`, D65 white `(0.3127, 0.3290, 1)`, sRGB primaries
R `(0.64, 0.33, 1)`, G `(0.30, 0.60, 1)`, B `(0.15, 0.06, 1)`, then
`cmsCreateRGBProfile` and `cmsSaveProfileToMem`. Its test reference uses the
analytic IEC sRGB transfer function, not Omuse's colour-conversion output.
The maximum permitted deviation is 64 of 65,535 levels, accommodating encoded
ICC matrix/curve precision while detecting skipped colour conversion.

These tests are numeric/container checks. They do not claim a Photoshop UI
comparison, editable 16-bit layer import, CMYK/Lab conversion, HDR support or
preservation of original Photoshop descriptors.
