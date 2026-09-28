# Linux save and GPU failure regression checks

These changes target the `GNU_Linux` compatibility layer. They leave the shared
`Compositor/` and `CompositorTests/` trees unchanged.

## Atomic package saves

`FoundationCompat.FileWrapper` stages all children in a private sibling directory.
Linux `renameat2(RENAME_EXCHANGE)` publishes a replacement without first deleting
the saved document. New documents use `RENAME_NOREPLACE` after an absent-destination
result, so a concurrent creator is not overwritten. Both operations return errors
without deleting either version. See the [Linux rename contract](https://man7.org/linux/man-pages/man2/rename.2.html).

An unsupported filesystem returns an error; there is no non-atomic fallback. This
is **namespace atomicity**, not a guarantee of persistence after power loss: no
new fsync/directory durability protocol is introduced. Multiple writers still use
last-successful-replacement semantics. This change is not file coordination or
stale-save detection.

Staging failures remove partial output. Successful exchange moves the previous
version into staging for cleanup. Cleanup is best effort after the commit point:
a failure there may leave a hidden `.compositor-save-*` sibling, but must not
report a successfully published save as failed. Child names must be single path
components; invalid names are rejected before writing.

Coverage:

- Six `FileWrapperAtomicTests`: create/replace, all-child publication, obsolete
  child removal, regular files, symlink destination, bad child names, staging
  failure, and injected publication failures.
- `CompositorCore.AtomicPublish`: real exchange/create calls plus link-time
  injection of ENOSPC/EACCES/EOPNOTSUPP and a concurrent destination creator.
- Existing `ProjectTests`: save/reopen, relocated packages, overwrite, missing
  images, corrupt metadata, operation state and effects metadata.

## GPU brush timeout

A tile fence wait is limited to two seconds. Failure leaves caller output buffers
unchanged and disables further submissions on that context, allowing the existing
adapter to use CPU coverage. Destruction does not perform an unbounded
`vkDeviceWaitIdle`. Pending resources are only destroyed after fence completion;
a still-pending or lost context is retained until process exit. The normal app
uses a shared brush context. Callers that repeatedly create failing contexts can
retain additional resources; this is a deliberate safety tradeoff, not a claim of
zero leaks or a complete GPU watchdog.

The Vulkan API may return a little after its requested timeout; this change does
not bound time spent inside other driver calls. See
[`vkWaitForFences`](https://docs.vulkan.org/refpages/latest/refpages/source/vkWaitForFences.html).

`CompositorCore.GpuTimeout.{stalled,completed,lost}` submits a real queue operation,
then interposes fence results. It checks the requested timeout, unchanged outputs,
disabled retries, usable CPU coverage, and pending-versus-completed teardown. The
shim first waits for the real work before reporting a fake failure. It is linked
only into tests, not enabled by a production environment variable.

## Running the focused checks

Inside the matching KDE SDK with the Swift SDK extension on PATH:

```sh
cmake -S . -B build -DCOMPOSITOR_REQUIRE_VENDORED_DEPS=OFF
cmake --build build --parallel 2
ctest --test-dir build -R 'GpuTimeout|AtomicPublish|Kernels' --output-on-failure
swift test --jobs 2 --no-parallel \
  --filter 'FileWrapperAtomicTests|AppKitCompatTests|ProjectTests' \
  --skip layerEffectsAreRenderedInExport
```

The excluded export-render test requires the Skia bridge. Run it as part of the
renderer-enabled suite, not as evidence of persistence correctness. A local
machine without a Vulkan compute device skips the GPU injection cases; the
dedicated CI job treats that skip as a failure. CI configuration is provided, but
local execution does not prove that GitHub Actions has run.

`CameraRawSliderTests.swift` is recorded in the Linux exclusion manifest because
it exercises AppKit-only slider cells. Camera Raw processing tests remain in the
build. The native kernel test now checks padded-row pixels and padding rather
than expecting the Linux-only assertion removed from the shared upstream kernels.
