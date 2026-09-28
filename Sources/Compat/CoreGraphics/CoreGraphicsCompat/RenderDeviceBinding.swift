// CoreGraphicsCompat/RenderDeviceBinding.swift — Swift side of the render
// device injection and Skia Canvas C ABI bridge.
//
// Plan §3, §4, §5: The C++ SkiaBridge supplies the C ABI functions for Skia
// raster drawing. In order to keep the SwiftPM core build Skia-free (no Skia
// headers or link flags required for building the Swift module), symbols are
// resolved dynamically via dlsym or registered explicitly.
//
// SOLID: the shim depends only on the portable raster substrate (ISP) and
// dynamic C ABI binding (DIP); it does not import Skia or Qt.

import Foundation
#if canImport(Glibc)
import Glibc
#endif

/// Render function injected by the C++ SkiaBridge for simple composite.
public typealias CompRenderFn =
    @convention(c) (UnsafePointer<UInt8>, UnsafeMutablePointer<UInt8>,
                    Int, Int) -> Int32

nonisolated(unsafe) private var _compatRenderFn: CompRenderFn?

@_cdecl("compositor_compat_set_render_fn")
public func compositor_compat_set_render_fn(_ fn: CompRenderFn?) {
    _compatRenderFn = fn
}

func compositor_compat_current_render_fn() -> CompRenderFn? {
    _compatRenderFn
}

// MARK: - Canvas C ABI Bridge

/// The dlopen binding behind `SkiaCanvasBackend`. Internal on purpose: everything else reaches Skia through `CanvasBackend`.
final class CompCanvasBridge: @unchecked Sendable {
    public static let shared = CompCanvasBridge()

    public typealias CreateCanvasFn = @convention(c) (UnsafeMutablePointer<UInt8>?, Int, Int, Int) -> OpaquePointer?
    public typealias DestroyCanvasFn = @convention(c) (OpaquePointer?) -> Void
    public typealias SaveFn = @convention(c) (OpaquePointer?) -> Void
    public typealias RestoreFn = @convention(c) (OpaquePointer?) -> Void
    public typealias TranslateFn = @convention(c) (OpaquePointer?, Float, Float) -> Void
    public typealias ScaleFn = @convention(c) (OpaquePointer?, Float, Float) -> Void
    public typealias RotateFn = @convention(c) (OpaquePointer?, Float) -> Void
    public typealias ConcatFn = @convention(c) (OpaquePointer?, Float, Float, Float, Float, Float, Float) -> Void
    public typealias GetCTMFn = @convention(c) (OpaquePointer?, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?) -> Void
    public typealias ClipRectFn = @convention(c) (OpaquePointer?, Float, Float, Float, Float, Int32) -> Void
    public typealias ClipRectDiffFn = @convention(c) (OpaquePointer?, Float, Float, Float, Float) -> Void
    public typealias ClipPathFn = @convention(c) (OpaquePointer?, OpaquePointer?, Int32, Int32) -> Void
    public typealias ClipMaskFn = @convention(c) (OpaquePointer?, UnsafePointer<UInt8>?, Int, Int, Int, Float, Float, Float, Float, Int32) -> Void
    public typealias GetClipBoundsFn = @convention(c) (OpaquePointer?, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?, UnsafeMutablePointer<Float>?) -> Void
    public typealias SetAlphaFn = @convention(c) (OpaquePointer?, Float) -> Void
    public typealias SetBlendModeFn = @convention(c) (OpaquePointer?, Int32) -> Void
    public typealias SetInterpolationQualityFn = @convention(c) (OpaquePointer?, Int32) -> Void
    public typealias SetAntialiasFn = @convention(c) (OpaquePointer?, Int32) -> Void
    public typealias FillRectFn = @convention(c) (OpaquePointer?, Float, Float, Float, Float, Float, Float, Float, Float) -> Void
    public typealias ClearFn = @convention(c) (OpaquePointer?, Float, Float, Float, Float) -> Void
    public typealias DrawImageRectFn = @convention(c) (OpaquePointer?, UnsafePointer<UInt8>?, Int, Int, Int, Float, Float, Float, Float, Float, Int32, Int32) -> Void
    public typealias BeginTransparencyLayerFn = @convention(c) (OpaquePointer?, Float) -> Void
    public typealias EndTransparencyLayerFn = @convention(c) (OpaquePointer?) -> Void

    public typealias PathCreateFn = @convention(c) () -> OpaquePointer?
    public typealias PathDestroyFn = @convention(c) (OpaquePointer?) -> Void
    public typealias PathMoveToFn = @convention(c) (OpaquePointer?, Float, Float) -> Void
    public typealias PathLineToFn = @convention(c) (OpaquePointer?, Float, Float) -> Void
    public typealias PathAddRectFn = @convention(c) (OpaquePointer?, Float, Float, Float, Float) -> Void
    public typealias PathAddEllipseFn = @convention(c) (OpaquePointer?, Float, Float, Float, Float) -> Void
    public typealias PathCloseFn = @convention(c) (OpaquePointer?) -> Void
    public typealias PathResetFn = @convention(c) (OpaquePointer?) -> Void

    public let createCanvas: CreateCanvasFn?
    public let destroyCanvas: DestroyCanvasFn?
    public let save: SaveFn?
    public let restore: RestoreFn?
    public let translate: TranslateFn?
    public let scale: ScaleFn?
    public let rotate: RotateFn?
    public let concat: ConcatFn?
    public let getCTM: GetCTMFn?
    public let clipRect: ClipRectFn?
    public let clipRectDiff: ClipRectDiffFn?
    public let clipPath: ClipPathFn?
    public let clipMask: ClipMaskFn?
    public let getClipBounds: GetClipBoundsFn?
    public let setAlpha: SetAlphaFn?
    public let setBlendMode: SetBlendModeFn?
    public let setInterpolationQuality: SetInterpolationQualityFn?
    public let setAntialias: SetAntialiasFn?
    public let fillRect: FillRectFn?
    public let clear: ClearFn?
    public let drawImageRect: DrawImageRectFn?
    public let beginTransparencyLayer: BeginTransparencyLayerFn?
    public let endTransparencyLayer: EndTransparencyLayerFn?

    public let pathCreate: PathCreateFn?
    public let pathDestroy: PathDestroyFn?
    public let pathMoveTo: PathMoveToFn?
    public let pathLineTo: PathLineToFn?
    public let pathAddRect: PathAddRectFn?
    public let pathAddEllipse: PathAddEllipseFn?
    public let pathClose: PathCloseFn?
    public let pathReset: PathResetFn?

    public var isAvailable: Bool { createCanvas != nil }

    private init() {
        #if canImport(Glibc)
        let candidates = [
            ProcessInfo.processInfo.environment["COMPOSITOR_SKIA_BRIDGE"] ?? "",
            // Bare name: the system loader's search path only. Never the working directory, where a stray
            // library would be picked up.
            "libCompositorSkiaBridge.so",
            "/app/lib/libCompositorSkiaBridge.so"
        ]
        for path in candidates where !path.isEmpty {
            if dlopen(path, RTLD_NOW | RTLD_GLOBAL) != nil {
                break
            } else if path == ProcessInfo.processInfo.environment["COMPOSITOR_SKIA_BRIDGE"], let reason = dlerror() {
                // An explicitly requested bridge that cannot load is worth saying so.
                FileHandle.standardError.write(Data("COMPOSITOR_SKIA_BRIDGE: \(String(cString: reason))\n".utf8))
            }
        }
        #endif

        self.createCanvas = Self.lookup("compositor_canvas_create")
        self.destroyCanvas = Self.lookup("compositor_canvas_destroy")
        self.save = Self.lookup("compositor_canvas_save")
        self.restore = Self.lookup("compositor_canvas_restore")
        self.translate = Self.lookup("compositor_canvas_translate")
        self.scale = Self.lookup("compositor_canvas_scale")
        self.rotate = Self.lookup("compositor_canvas_rotate")
        self.concat = Self.lookup("compositor_canvas_concat")
        self.getCTM = Self.lookup("compositor_canvas_get_ctm")
        self.clipRect = Self.lookup("compositor_canvas_clip_rect")
        self.clipRectDiff = Self.lookup("compositor_canvas_clip_rect_difference")
        self.clipPath = Self.lookup("compositor_canvas_clip_path")
        self.clipMask = Self.lookup("compositor_canvas_clip_mask")
        self.getClipBounds = Self.lookup("compositor_canvas_get_clip_bounds")
        self.setAlpha = Self.lookup("compositor_canvas_set_alpha")
        self.setBlendMode = Self.lookup("compositor_canvas_set_blend_mode")
        self.setInterpolationQuality = Self.lookup("compositor_canvas_set_interpolation_quality")
        self.setAntialias = Self.lookup("compositor_canvas_set_antialias")
        self.fillRect = Self.lookup("compositor_canvas_fill_rect")
        self.clear = Self.lookup("compositor_canvas_clear")
        self.drawImageRect = Self.lookup("compositor_canvas_draw_image_rect")
        self.beginTransparencyLayer = Self.lookup("compositor_canvas_begin_transparency_layer")
        self.endTransparencyLayer = Self.lookup("compositor_canvas_end_transparency_layer")

        self.pathCreate = Self.lookup("compositor_path_create")
        self.pathDestroy = Self.lookup("compositor_path_destroy")
        self.pathMoveTo = Self.lookup("compositor_path_move_to")
        self.pathLineTo = Self.lookup("compositor_path_line_to")
        self.pathAddRect = Self.lookup("compositor_path_add_rect")
        self.pathAddEllipse = Self.lookup("compositor_path_add_ellipse")
        self.pathClose = Self.lookup("compositor_path_close")
        self.pathReset = Self.lookup("compositor_path_reset")
    }

    private static func lookup<T>(_ name: String) -> T? {
        #if canImport(Glibc)
        guard let sym = dlsym(nil, name) else { return nil }
        return unsafeBitCast(sym, to: T.self)
        #else
        return nil
        #endif
    }
}