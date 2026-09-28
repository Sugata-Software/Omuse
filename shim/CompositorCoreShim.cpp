// CompositorCoreShim — translation unit proving the C ABI header compiles under
// C++ and providing the C++-idiomatic wrapper. See CompositorCoreShim.hpp.

#include "CompositorCoreShim.hpp"

namespace compositor {

Status composite_over_checked(BufferView dst,
                              ConstBufferView src,
                              ConstCoverage coverage,
                              std::size_t width,
                              std::size_t height,
                              std::size_t stride,
                              float opacity) {
    // Defense-in-depth geometry validation (ENG-15) before crossing the ABI.
    if (width == 0 || height == 0 || stride < width * 4) {
        return Status::InvalidArgument;
    }
    if (dst.data == nullptr || src.data == nullptr) {
        return Status::InvalidArgument;
    }
    int rc = compositor_composite_over(dst.data,
                                       src.data,
                                       coverage.data,
                                       width,
                                       height,
                                       stride,
                                       opacity);
    return rc == 0 ? Status::Ok : Status::InvalidArgument;
}

}  // namespace compositor