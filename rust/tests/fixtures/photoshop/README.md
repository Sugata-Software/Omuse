# Independent Photoshop fixtures

These fixtures come from [psd-tools/psd-tools](https://github.com/psd-tools/psd-tools)
under its MIT license, reproduced in `LICENSE.psd-tools`. Retrieved 1 October 2026.
They were created independently of Omuse's parser and synthetic test builders.

- `psd-tools-1layer.psb` is the unmodified
  [`tests/psd_files/1layer.psb`](https://github.com/psd-tools/psd-tools/blob/main/tests/psd_files/1layer.psb).
  Upstream Git blob: `bbad7086dcf28ae1f38f7db5e9c37c1160f00641`.
  SHA-256: `85317ccdb7ec11a51f1983ad2055544a490839c01f8b313db8070c1b0f0b83f4`.
- `psd-tools-text.TySh` is the exact 11,020-byte Type Tool payload at byte offset
  22,936 of upstream
  [`tests/psd_files/text.psb`](https://github.com/psd-tools/psd-tools/blob/main/tests/psd_files/text.psb).
  Original Git blob: `2f12579b5aee8c26387865889358b6da9bf28f63`.
  Original SHA-256: `d28350bc188655b1bfb4741877af946f07810f2eaa50c95d33939c210dd79e96`.
  Extract SHA-256: `b3fce210703647c44361770479041a6678e6ba5f1e9b0b80623eb08c776452ee`.
- `psd-tools-text-psd.TySh` is the exact 10,784-byte Type Tool payload at byte
  offset 23,018 of upstream
  [`tests/psd_files/text.psd`](https://github.com/psd-tools/psd-tools/blob/main/tests/psd_files/text.psd).
  Original Git blob: `c424e40ae25c8d05ab7652ce5ee83eab1ac56744`.
  Original SHA-256: `eb40ec54a5f270913463bcc847be65672ce6fff0d0d56288725f58e299e00f1c`.
  Extract SHA-256: `6f63945f1fcf1b089564285e124cbbc1b68171efa59c29620ceb05503980bdc8`.
  This record includes zero alignment bytes after its bounds rectangle, a
  layout detail not present in the PSB fixture.

The Type Tool test checks real UTF-16 EngineData, ArialMT at 13 pixels,
multiline content and unwarped horizontal layout. Extracting this descriptor
avoids bypassing Omuse's separate, explicit embedded-ICC import restriction.
No fonts are embedded in these fixture files.
