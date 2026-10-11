# Photoshop exchange

**Omuse 0.10.0.** Layered PSD export and 16-bit merged PSD/PSB import are
included with explicit conversion and resource limits. The
[release qualification](release-0100-qualification.md) records exact-source
CI, independent-reader checks and package/publication status. These checks
do not establish general Photoshop or Affinity interoperability.

Keep an **`.omuse` master** for your editable Omuse project. PSD exchange is a
conversion with explicit limits; it is not a lossless Photoshop round trip.
Opening or exporting a PSD never modifies the original input file.

## Opening PSD and PSB

For **8-bit RGB**, Omuse imports the supported layer structure, cached pixels,
blend modes and masks. Supported text and shape descriptors can stay editable;
unsupported descriptors use their cached appearance and report the conversion.
An embedded ICC profile is still refused on this layered path. Use a
colour-managed PNG/TIFF from the original editor when appearance matters, or
make an sRGB PSD copy without an embedded profile. Do not strip an arbitrary
profile from a non-sRGB document.

For **16-bit RGB**, Omuse opens the source editor's **merged composite as one
image**. Its interpreted 16-bit samples become an editable Advanced source; the
canvas uses an 8-bit display cache. When the file declares merged transparency,
Omuse removes Photoshop's white matte before colour conversion. This preserves
alpha but cannot recover colour information already lost when the source editor
quantized the matted composite. Untagged samples are interpreted as sRGB.
Embedded RGB ICC profiles are applied to the whole merged image through
LittleCMS, retaining a 16-bit sRGB result. Saving the imported image as `.omuse`
keeps that high-precision source.

The import report explicitly states that the original Photoshop layers, masks,
text, adjustments and metadata remain in the original PSD/PSB. They are not
reconstructed from the merged image. A file saved without a real merged preview
is refused; enable **Maximize Compatibility** when saving in the source editor.

To review the conversions for the current document, press **Ctrl+K**, search
for **Import conversion report**, and execute that command.

The 16-bit path accepts PSD and PSB files up to **64 MiB** and merged images up
to **16,777,216 pixels**. It supports RGB with an explicitly declared optional
transparency channel. Extra/spot channels, CMYK, Lab, invalid or incompatible
ICC profiles, and 32-bit/HDR Photoshop files are refused. These limits are
checked before decoding large image buffers.

## Exporting a layered PSD

Choose a **new `.psd` filename** for Photoshop export. Omuse prepares the file
privately and publishes it only when complete. An existing file, directory or
symlink at that destination is never replaced; choose another filename.
The destination filesystem must support hard links for this final safe publish
step. If export to a removable drive such as exFAT is refused, export to a local
Linux or NTFS folder first, then copy the completed PSD to that drive.

The exported document contains **8-bit, untagged sRGB pixel layers** and a merged
preview rendered from those same converted layers. If the receiving editor asks
which profile to assign, choose sRGB. Export does not embed an ICC profile. The compatibility preview uses Photoshop's
white-matte convention for translucent edges; the individual pixel layers retain
straight colour and alpha. The 8-bit preview can incur rounding at those edges.

Supported structure includes:

- Layer names, visibility, integer pixel offsets, supported blend-mode keys
  and Photoshop's 8-bit opacity values.
- Nested, unmasked pass-through groups at full opacity.
- Linked, opaque grayscale pixel masks with unscaled, unrotated integer
  placement, including disabled masks and their outside coverage.
- A visible canvas background, added as the bottom pixel layer.

The export report lists conversions. Live text and vector artwork become pixel
layers. Editable filter recipes and high-precision sources use their current
8-bit appearance. Supported local layer effects and their active mask appearance
are rendered into pixels. Editing locks and original metadata are not retained.
Opacity values are rounded to Photoshop's 256 available steps where necessary.

After a successful layered PSD export, press **Ctrl+K**, search for **Export
conversion report**, and execute it to view the destination filename and all
recorded warnings. The report belongs to the **most recent successful layered
PSD export in this application session**; it is not saved in the `.omuse`
document. A cancelled or failed export does not replace the previous report.
**Import conversion report** is separate and describes conversions made while
opening the current document.

Some compositing cannot yet be represented safely. Export refuses scaled,
flipped, rotated or fractionally positioned layers, adjustment and clipping
stacks, Blend If, group opacity or masks, and unsupported mask placement.
**Rasterize layer does not bake its transform**, so that action alone does not
resolve the transform refusal.

To exchange the appearance of an unsupported document:

1. Save the editable `.omuse` master.
2. Use **Save As** to create a separate `.omuse` copy.
3. Unlock any locked layers or groups in that copy, then press **Ctrl+K**,
   search for **Flatten image**, and execute it. Only the visible artwork is
   included; confirm that the layer list now contains one **Flattened** layer.
4. Export the copy to a new PSD filename. Use PNG/TIFF instead when separate
   Photoshop pixel layers are unnecessary.

Export is limited to 30,000 pixels per side, 1,024 layer records (a group uses
two records), a 256 MiB output file, and a conservative 768 MiB temporary-memory
estimate. The already-open document uses additional memory. Large documents may
reach the memory limit before the file-size limit.

## Validation scope

The exchange tests check explicit pixel references, round trips through Omuse's
separately implemented 8-bit importer, 16-bit sample retention, ICC conversion,
original-file preservation, cancellation, malformed input and safe publication.
Small, untouched PSD and PSB fixtures come from the MIT-licensed psd-tools corpus;
their identities are recorded in
[`HIGH-DEPTH-PROVENANCE.md`](../rust/tests/fixtures/photoshop/HIGH-DEPTH-PROVENANCE.md).

These checks do not establish Photoshop or Affinity visual interoperability.
Opening exported files in those applications and comparing their rendered output
remains a separate qualification step. The feature's current test/release status
is tracked in the [project guide](project-guide.md).
