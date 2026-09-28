import Foundation
import CompositorBrushBackend

/// This adapter is the only Swift layer aware of the native brush ABI.
private func nativeCoverage(_ request: BrushCoverageRequest, context: OpaquePointer? = nil,
                            accelerated: Bool = false) throws -> BrushCoverageResult {
    guard (1...256).contains(request.width), (1...256).contains(request.height),
          request.settled.count <= 2048, request.tail.count <= 2048 - request.settled.count,
          request.permanent.count == request.width * request.height else { throw BrushCoverageFailure.invalidInput }
    let all = request.settled + request.tail
    let segments = all.map { CompositorBrushSegment(x0: $0.x, y0: $0.y, x1: $0.z, y1: $0.w) }
    var uniforms = CompositorBrushUniforms(a: Float(request.mapping.a), b: Float(request.mapping.b),
        c: Float(request.mapping.c), d: Float(request.mapping.d),
        origin_x: Float(request.origin.x), origin_y: Float(request.origin.y), radius: request.radius,
        hardness: request.hardness, canvas_width: Float(request.canvas.width), canvas_height: Float(request.canvas.height),
        antialias_width: request.antialiasWidth, spacing: request.spacing,
        width: UInt32(request.width), height: UInt32(request.height),
        settled_count: UInt32(request.settled.count), segment_count: UInt32(all.count))
    var permanent = [Float](repeating: 0, count: request.permanent.count)
    var preview = [UInt8](repeating: 0, count: request.permanent.count)
    let code = request.permanent.withUnsafeBufferPointer { source in
        segments.withUnsafeBufferPointer { segments in
            permanent.withUnsafeMutableBufferPointer { destination in
                preview.withUnsafeMutableBufferPointer { preview in
                    if accelerated {
                        return compositor_vulkan_brush_render(context, &uniforms, segments.baseAddress, segments.count,
                            source.baseAddress, source.count, destination.baseAddress, preview.baseAddress)
                    }
                    return compositor_brush_cpu(&uniforms, segments.baseAddress, segments.count,
                        source.baseAddress, source.count, destination.baseAddress, preview.baseAddress)
                }
            }
        }
    }
    guard code == 0 else { throw code == -1 ? BrushCoverageFailure.invalidInput : BrushCoverageFailure.unavailable }
    return BrushCoverageResult(permanent: permanent, preview: preview)
}

final class CPUBrushCoverage: BrushCoverageComputing {
    func render(_ request: BrushCoverageRequest) throws -> BrushCoverageResult { try nativeCoverage(request) }
}

final class VulkanBrushCoverage: BrushCoverageComputing {
    static let shared = VulkanBrushCoverage()
    private let context: OpaquePointer
    let deviceName: String
    let deviceType: UInt32
    let driverVersion: UInt32

    init?() {
        guard let context = compositor_vulkan_brush_create() else { return nil }
        self.context = context
        deviceName = String(cString: compositor_vulkan_brush_device(context))
        deviceType = compositor_vulkan_brush_device_type(context)
        driverVersion = compositor_vulkan_brush_driver_version(context)
    }
    deinit { compositor_vulkan_brush_destroy(context) }
    func render(_ request: BrushCoverageRequest) throws -> BrushCoverageResult {
        try nativeCoverage(request, context: context, accelerated: true)
    }
}
