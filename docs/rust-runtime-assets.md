# Rust native runtime assets

The native editor and `.omuse` projects work without downloaded inference libraries. Camera RAW import and local subject selection use optional native runtimes beside the Omuse executable. Images remain local; no system service is installed.

Run `scripts/prepare-rust-assets.sh`, then `scripts/install-rust.sh /path/to/omuse`. This Linux x86_64 preparation script downloads pinned, SHA-256-checked artifacts, builds LibRaw without OpenMP, and stages assets under ignored `rust/runtime/`. It requires a C++ compiler, make, pkg-config, JPEG/zlib/LittleCMS development libraries and curl. `OMUSE_RUNTIME_DIR` overrides the staging directory. The legacy variable name remains accepted as a compatibility alias. Existing system LittleCMS 2 is also a runtime dependency of ICC conversion.

| Component | Source | Version and terms |
| --- | --- | --- |
| ONNX Runtime CPU | https://github.com/microsoft/onnxruntime/releases/tag/v1.23.2 | 1.23.2, MIT plus bundled third-party notices |
| U2NETP ONNX model | https://github.com/danielgatis/rembg/releases/tag/v0.0.0 | Model distributed by rembg; upstream architecture/training repository https://github.com/xuebinqin/U-2-Net, Apache-2.0 |
| LibRaw | https://www.libraw.org/data/LibRaw-0.22.2.tar.gz | 0.22.2, CDDL/LGPL dual licensing; original notices retained |
| LittleCMS | https://github.com/mm2/Little-CMS | System library, MIT |

License texts are under `rust/licenses/` and copied into the installed application. The LibRaw source archive is retained in the staging directory, and the preparation script records the unmodified source/build recipe. Do not distribute a binary bundle without its notices and corresponding-source obligations. The downloaded model and native binaries are not committed to this repository.

Runtime resolution uses explicit environment overrides (`OMUSE_ONNX_RUNTIME`,
`OMUSE_SUBJECT_MODEL`, `OMUSE_LIBRAW`) followed by executable-adjacent `lib/`
and `models/` paths. The corresponding legacy names remain accepted as
compatibility aliases. No inference runtime is downloaded during an editing
operation. Subject inference is CPU-only, serialized through a cached session,
with two intra-operation threads and a 16-million-pixel input bound. LibRaw
reads bounded local files and uses separate development contexts.

Local masking and LibRaw development provide native Linux photo workflows. A functioning mask/decode is not evidence of accurate segmentation or calibrated camera colour. Tests distinguish deterministic portable kernels from platform-specific image processing. The model's accuracy needs a diverse image evaluation set; a single successful portrait smoke test is only runtime validation.

The real RAW smoke fixture is `syoyo/raw-images`'s `images/colorchart-iphone7plus-cloudy.dng`, whose repository declares its photographs CC BY 4.0. The local test records its SHA-256 as `5a4db5cbda08620494aa299e27a771e19b439a00f6143f65edb43139b9ba7392`; it is not included in the source tree. Credit: Syoyo Fujita, https://github.com/syoyo/raw-images.

## Development guided-mask pack

The experimental **Guided subject mask** workflow uses the same trusted ONNX
Runtime with an additional explicitly downloaded EfficientSAM-Ti encoder/decoder
pair. It is separate from the existing U2NETP one-click selector. The fixed author
source is [yunyangx/EfficientSAM](https://huggingface.co/spaces/yunyangx/EfficientSAM/tree/d8dbb1eee73bfb3392aa6f6e8944aeb13f3f4036),
revision `d8dbb1eee73bfb3392aa6f6e8944aeb13f3f4036`; architecture/source:
[yformer/EfficientSAM](https://github.com/yformer/EfficientSAM). Author metadata
states Apache-2.0. The author licence and model card are retained under
`rust/licenses/efficientsam/`; weights are not bundled in source or default packages.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `efficientsam_ti_encoder.onnx` | 24,799,761 | `84ed466ffcc5c1f8d08409bc34a23bb364ab2c15e402cb12d4335a42be0e0951` |
| `efficientsam_ti_decoder.onnx` | 16,565,728 | `a62f8fa5ea080447c0689418d69e58f1e83e0b7adf9c142e2bd9bcc8045c0b11` |

Acquisition accepts only the fixed pinned HTTPS locations and verifies byte
lengths, hashes, licence, card and manifest before publication. Repair retains
prior bytes. A project cannot nominate executable/runtime/model paths. Positive
clicks and boxes are supported; model negative prompts are not qualified. The
local runtime probe establishes execution and cancellation, not photographic
quality, calibrated confidence, a hard process-memory cap or Windows support.
