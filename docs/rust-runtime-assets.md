# Rust native runtime assets

The editor and `.comp` editing work without downloaded inference libraries. Camera RAW import and local subject selection require two optional native runtimes. The installer copies them beside the separate Rust executable; it does not install system services, alter the old editor, or upload images.

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

These are Linux substitutes for Apple Vision and Core Image RAW processing. A functioning local mask/decode is not evidence of identical segmentation or camera color. Tests distinguish deterministic portable kernels from platform-specific image processing. The model's accuracy needs a diverse image evaluation set; a single successful portrait smoke test is only runtime validation.

The real RAW smoke fixture is `syoyo/raw-images`'s `images/colorchart-iphone7plus-cloudy.dng`, whose repository declares its photographs CC BY 4.0. The local test records its SHA-256 as `5a4db5cbda08620494aa299e27a771e19b439a00f6143f65edb43139b9ba7392`; it is not included in the source tree. Credit: Syoyo Fujita, https://github.com/syoyo/raw-images.
