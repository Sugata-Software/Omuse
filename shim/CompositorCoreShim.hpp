// CompositorCoreShim — thin C++ wrapper over the Compositor C ABI.
// See docs/linux-port-plan.md, ENG-1 (shim) and ENG-3 (result delivery).
//
// This header makes the C ABI C++-safe and adds defense-in-depth geometry
// validation (ENG-15) on the C++ side. ENG-3 result delivery (routing the
// Swift completion back onto the Qt thread via QMetaObject::invokeMethod with
// Qt::QueuedConnection) is wired at the composition root (ENG-2), not here:
// this header is dependency-free and Qt-free so it builds without Qt installed.

#pragma once

#include "CompositorCore.h"

#include <cstddef>
#include <cstdint>

namespace compositor {

enum class Status {
    Ok = 0,
    InvalidArgument = -1,
};

// Non-owning buffer views. Callers retain ownership; the Swift side owns result
// buffers across the seam (ENG-3).
struct BufferView {
    uint8_t *data;
};
struct ConstBufferView {
    const uint8_t *data;
};
struct ConstCoverage {
    const uint8_t *data;  // may be nullptr == full coverage
};

// Composite `src` over `dst` in place (premultiplied source-over) with optional
// coverage and opacity. Validates geometry before crossing the ABI.
Status composite_over_checked(BufferView dst,
                              ConstBufferView src,
                              ConstCoverage coverage,
                              std::size_t width,
                              std::size_t height,
                              std::size_t stride,
                              float opacity);

}  // namespace compositor