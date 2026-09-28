# Independent pixel reference kernels

These four files are exact copies of the portable C kernels from
[Compositor at revision 75c421980ad2d289ea8244c54cfa3a649678d259](https://github.com/robbietilton/Compositor/tree/75c421980ad2d289ea8244c54cfa3a649678d259/Compositor/Rendering).
Their original paths and SHA-256 hashes are recorded in
[provenance.json](provenance.json), and the original Wonder Assembly LLC MIT
copyright and permission notice is retained in [LICENSE](LICENSE).

They are independent test references. The Omuse application is implemented in
Rust and does not compile or link these files. They need only a Linux C
compiler when deliberately regenerating the Camera Raw golden fixtures:

```sh
python3 scripts/generate-rust-kernel-fixtures.py
git diff --exit-code -- rust/tests/fixtures
```

Run these commands from the repository root. Regeneration must leave the
committed JSON fixtures byte-for-byte unchanged for an unchanged reference.
The JSON files also record the C source hashes used to produce them.

Do not silently edit the reference to make a Rust test pass. A changed source
revision needs explicit provenance, fresh fixtures and a review of changed
pixels. Portable-kernel agreement does not establish agreement with Apple RAW,
Core Image, font rendering or the entire original application.

The color-noise reference has a known zero-alpha scratch-buffer defect. The
advanced fixtures deliberately omit those inputs; the basic fixtures still
cover zero alpha. The independent
[upstream contribution packet](../../../../docs/upstream-color-noise-reproduction.md)
includes a reproduction and a patch applied only to a temporary copy. Omuse's
Rust implementation initializes its scratch buffer deterministically.
