// swift-tools-version: 6.2
// Compositor — Linux port package manifest.
//
// The macOS app remains an Xcode project (Compositor.xcodeproj). This SwiftPM
// manifest builds the portable Swift core for Linux (and is the home of the
// @_cdecl C ABI seam, ENG-1). Per the autoplan provisional decision (ENG-12),
// the port ships in Swift 5 language mode; a Swift 6 compile spike is a
// tracked follow-up, not a blocker.
//
// Build (under the Freedesktop Swift runtime extension): `swift build`
// This host has no Swift toolchain; `swift build` is run in the SDK.

import PackageDescription
import Foundation

// The pinned OpenCV (third_party/opencv.pinned: 4.14.0, static core + imgproc) lives under /app in the Flatpak build and
// under build/opencv/install when built locally with the manifest's options. The OpenCV tier of the layer-effects chain
// compiles only when its headers are there, and links the static libraries from the same prefix.
let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let opencvPrefix: String? = ["\(packageRoot)/build/opencv/install", "/app"].first {
    FileManager.default.fileExists(atPath: "\($0)/include/opencv4/opencv2/imgproc.hpp")
}
let opencvLibrary: String? = opencvPrefix.flatMap { prefix in
    ["lib64", "lib"].map { "\(prefix)/\($0)" }.first { FileManager.default.fileExists(atPath: "\($0)/libopencv_core.a") }
}
import Foundation

let package = Package(
    name: "Compositor",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        // Static library so the C++ host (ENG-2 composition root) links the Swift
        // core's @_cdecl symbols in-process; built with --static-swift-stdlib in
        // the Flatpak manifest so the Swift runtime is bundled, not a system dep.
        .library(name: "CompositorCore", type: .static, targets: ["Compositor"]),
    ],
    targets: [
        // Layer effects (stroke / shadow / overlay / inner shadow): the C tier and the Vulkan tier of the chain that
        // Sources/Overrides/MetalLayerEffects.swift fronts. Same nine passes as upstream's Metal kernels.
        .target(name: "CompositorEffectsBackend", path: "backends/effects",
            exclude: ["shaders/effects.comp"],
            sources: ["EffectsCPU.cpp", "EffectsVulkan.cpp", "EffectsOpenCV.cpp"], publicHeadersPath: "include",
            cxxSettings: opencvPrefix.map { [.unsafeFlags(["-DCOMPOSITOR_HAS_OPENCV", "-I\($0)/include/opencv4"])] } ?? [],
            linkerSettings: [.linkedLibrary("vulkan")] + (opencvLibrary.map { library in [
                .unsafeFlags(["-L\(library)", "-L\(library)/opencv4/3rdparty"]),
                .linkedLibrary("opencv_imgproc"), .linkedLibrary("opencv_core"), .linkedLibrary("zlib"),
                .linkedLibrary("dl"), .linkedLibrary("pthread"),
            ] } ?? [])),
        .target(name: "CompositorBrushBackend", path: "backends/brush",
            exclude: ["shaders/continuous_brush.comp"],
            sources: ["BrushCoverageCPU.cpp", "VulkanBrushCoverage.cpp"],
            publicHeadersPath: "include", linkerSettings: [.linkedLibrary("vulkan")]),
        // The portable C pixel kernels (file-map "Keep" tier), reused verbatim from
        // the macOS source tree. The same .c files are ALSO built by CMakeLists.txt
        // for the C++ host and tests; this target makes them callable from the Swift
        // core (brush_alpha_bounds, spot_heal, …) so the two sides share one kernel
        // implementation. COMPOSITOR_PORTABLE turns on the ENG-17 canonical-buffer
        // entry asserts (no-ops on macOS/Xcode, which never defines it).
        .target(
            name: "CompositorKernels",
            path: "Compositor/Rendering",
            exclude: [
                "AdjustmentSurface.swift",
                "BrushCursorOverlay.swift",
                "CanvasViewport.swift",
                "DownsampleCache.swift",
                "EditorCanvas.swift",
                "EffectsPreviewCache.swift",
                "InlineTextEditor.swift",
                "LayerEffectsSurface.swift",
                "LayerRenderer.swift",
                "LiveMaskRenderer.swift",
                "MetalBrushCoverage.swift",
                "MetalLayerEffects.swift",
                "RasterSnapshot.swift",
                "SampleRingOverlay.swift",
                "SeparableBlend.swift",
                "TiledLayerRenderer.swift",
                "TransformOverlay.swift",
            ],
            publicHeadersPath: ".",
            cSettings: [
                .define("COMPOSITOR_PORTABLE"),
            ]
        ),
        // Apple CoreGraphics API surface for Linux (CGContext/CGImage/CGPath/... over Skia, pure-Swift failsafe).
        // Named exactly like the Apple framework so upstream macOS sources compile with `import CoreGraphics`.
        // It must not depend on any document/domain type (see docs/upstream-shim-worklist.md).
        .target(
            name: "CoreGraphics",
            dependencies: ["CompatSupport"],
            path: "Sources/Compat/CoreGraphics",
            swiftSettings: [.unsafeFlags(["-swift-version", "5"])]
        ),
        // ServiceSlot: the one mechanism the compat modules use to expose a replaceable implementation (install / override).
        .target(name: "CompatSupport", path: "Sources/Compat/CompatSupport",
                swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        // Foundation gaps (FileWrapper, NSFileCoordinator, security-scoped URLs), UTType and the headless AppKit
        // surface upstream's model code touches. Each is one Apple framework (interface segregation).
        .target(name: "LinuxFileOperations", path: "Sources/LinuxFileOperations", publicHeadersPath: "include"),
        .target(name: "FoundationCompat", dependencies: ["UniformTypeIdentifiers", "LinuxFileOperations"], path: "Sources/Compat/FoundationCompat",
                swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "Accelerate", path: "Sources/Compat/Accelerate",
                swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "ImageIO", dependencies: ["CoreGraphics", "Accelerate", "UniformTypeIdentifiers", "CompatSupport"],
                path: "Sources/Compat/ImageIO", swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "Vision", dependencies: ["CoreGraphics", "CoreVideo", "CompatSupport"], path: "Sources/Compat/Vision",
                swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "Combine", path: "Sources/Compat/Combine", swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "SwiftUI", dependencies: ["CoreGraphics", "AppKit", "Combine", "FoundationCompat", "CompatSupport"], path: "Sources/Compat/SwiftUI",
                swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "CoreVideo", path: "Sources/Compat/CoreVideo", swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "CoreImage", dependencies: ["CoreGraphics", "CoreVideo"], path: "Sources/Compat/CoreImage",
                swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "UniformTypeIdentifiers", path: "Sources/Compat/UniformTypeIdentifiers",
                swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "AppKit", dependencies: ["CoreGraphics", "FoundationCompat", "UniformTypeIdentifiers", "ImageIO", "CompatSupport"],
                path: "Sources/Compat/AppKit", swiftSettings: [.unsafeFlags(["-swift-version", "5"])]),
        .target(name: "_Testing_AppKit", path: "Sources/Compat/TestingOverlays/_Testing_AppKit"),
        .target(name: "_Testing_CoreGraphics", path: "Sources/Compat/TestingOverlays/_Testing_CoreGraphics"),
        .target(name: "_Testing_CoreImage", path: "Sources/Compat/TestingOverlays/_Testing_CoreImage"),
        // The unmodified macOS editor core (Compositor/{Document,IO,Rendering}) as the module `Compositor`, so upstream's
        // own tests (`@testable import Compositor`) run against it. Files are reached through symlinks in
        // Sources/UpstreamCore; nothing under Compositor/ is edited. Excluded: view/app code the Qt shell replaces
        // and the two Metal files, which Sources/Overrides replaces with same-API Vulkan/CPU implementations.
        // Xcode's own settings apply: Swift 5 mode, default actor isolation MainActor, approachable concurrency.
        .target(
            name: "Compositor",
            dependencies: ["CoreGraphics", "AppKit", "SwiftUI", "Combine", "CoreImage", "ImageIO", "Accelerate", "CoreVideo", "Vision", "UniformTypeIdentifiers", "FoundationCompat"] + ["CompositorKernels", "CompositorBrushBackend", "CompositorEffectsBackend", "CompatSupport"],
            path: "Sources/UpstreamCore",
            exclude: ["Rendering/AdjustPixels.c",
                     "Rendering/AdjustPixels.h",
                     "Rendering/BrushPixels.c",
                     "Rendering/BrushPixels.h",
                     "Rendering/ContentFill.c",
                     "Rendering/ContentFill.h",
                     "Rendering/HealPixels.c",
                     "Rendering/HealPixels.h",
                     "Rendering/LensPixels.c",
                     "Rendering/LensPixels.h",
                     "Rendering/LevelsPixels.c",
                     "Rendering/LevelsPixels.h",
                     "Rendering/NoisePixels.c",
                     "Rendering/NoisePixels.h",
                     "Rendering/WandPixels.c",
                     "Rendering/WandPixels.h",
                     "IO/CompositorApplicationDelegate.swift",
                     "Rendering/MetalBrushCoverage.swift",
                     "Rendering/MetalLayerEffects.swift"],
            swiftSettings: [
                .swiftLanguageMode(.v5),
                .defaultIsolation(MainActor.self),
                .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
                .enableUpcomingFeature("InferSendableFromCaptures"),
                // Upstream's Swift reaches the C kernels through a bridging header (no imports), and Foundation-only
                // files use the Foundation gaps FoundationCompat fills; both become implicit module imports.
                .unsafeFlags(["-Xfrontend", "-import-module", "-Xfrontend", "FoundationCompat",
                              "-Xfrontend", "-import-module", "-Xfrontend", "CompositorKernels",
                              // Apple's closure-taking APIs (Timer, Dispatch) do not check main-actor state against
                              // Sendable in Swift 5 mode; corelibs' do. Minimal checking matches Xcode's behaviour.
                              "-strict-concurrency=minimal"]),
            ]
        ),
        // Upstream's own test suite, unmodified (Swift Testing), run against the module above.
        .testTarget(
            name: "CompositorUpstreamTests",
            dependencies: ["Compositor", "_Testing_AppKit", "_Testing_CoreGraphics", "_Testing_CoreImage"] + ["CoreGraphics", "AppKit", "SwiftUI", "CoreImage", "ImageIO", "Accelerate", "CoreVideo", "Vision", "UniformTypeIdentifiers", "FoundationCompat"],
            path: "CompositorTests",
            // Tests of macOS-only UI code (ObjC-runtime NSSlider swizzling, floating panels, SwiftUI thumbnails):
            // listed in linux/UPSTREAM_TEST_EXCLUSIONS.md, never edited.
            exclude: ["SliderSnapTests.swift", "CameraRawSliderTests.swift", "FloatingPanelTests.swift", "CanvasThumbnailTests.swift",
                      "LayerTests.swift", "CursorTests.swift", "GuideTests.swift", "LevelsTests.swift",
                      "CanvasEntryTests.swift", "ColorPickerTests.swift", "SelectionTests.swift",
                      "BlendShortcutTests.swift", "TransformPressTests.swift"],
            swiftSettings: [
                .swiftLanguageMode(.v5),
                .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
                .enableUpcomingFeature("InferSendableFromCaptures"),
                .unsafeFlags(["-Xfrontend", "-import-module", "-Xfrontend", "FoundationCompat"]),
            ]
        ),
        // Tests of Linux-owned code that lives inside the Compositor module (the Sources/Overrides stand-ins), checked
        // against upstream's own behaviour.
        .testTarget(
            name: "LinuxOverrideTests",
            dependencies: ["Compositor", "_Testing_AppKit", "_Testing_CoreGraphics", "_Testing_CoreImage", "CompatSupport"] + ["CoreGraphics", "AppKit", "SwiftUI", "CoreImage", "ImageIO", "UniformTypeIdentifiers", "FoundationCompat"],
            path: "Tests/LinuxOverrideTests",
            swiftSettings: [
                .swiftLanguageMode(.v5),
                .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
                .enableUpcomingFeature("InferSendableFromCaptures"),
                .unsafeFlags(["-Xfrontend", "-import-module", "-Xfrontend", "FoundationCompat"]),
            ]
        ),
        .testTarget(
            name: "CompatTests",
            dependencies: ["CoreGraphics", "CompatSupport", "CompositorBrushBackend", "AppKit", "FoundationCompat", "UniformTypeIdentifiers", "Accelerate", "ImageIO", "CoreImage", "CoreVideo", "Vision"],
            path: "Tests/CompatTests"
        ),
        // ENG-2 composition root, inverted: a Swift `@main` bootstraps the Swift
        // runtime + Foundation (which a C++ main cannot on the Freedesktop Swift
        // 6.3 SDK — see docs/linux-port-file-map.md "composition-root constraint"),
        // then drives the Qt host through a C ABI. This bootstrap target proves
        // the architecture by running the session journey through the real
        // compositor_session_* ABI from a Swift entry point, AND initializes Qt
        // (via the HostRun C++ target) to prove the full Swift-main -> Qt link.
        //
        // HostRun compiles host/host_run.cpp (moc-free) as the Qt host entry
        // compositor_host_run; the Flatpak full build swaps in the CMake-built
        // CompositorHostRun lib (with the real MainWindow + app.exec()). Qt6
        // comes from the KDE SDK at the standard /usr paths.
        .target(
            name: "HostRun",
            dependencies: [],
            path: "host",
            sources: ["host_run.cpp", "SessionWindow.cpp", "DialogJourney.cpp", "SessionDialogs.cpp", "SizeDialog.cpp", "FilterDialog.cpp", "AdjustDialog.cpp", "ImageExporters.cpp", "TabletHandler.cpp", "QtImageIO.cpp", "FlatpakUpdateService.cpp", "SwiftUIQtRenderer.cpp", "moc_SessionWindow.cpp", "moc_FlatpakUpdateService.cpp"],
            cxxSettings: [
                .unsafeFlags([
                    "-I/usr/include/QtWidgets",
                    "-I/usr/include/QtCore",
                    "-I/usr/include/QtGui",
                    "-I/usr/include/QtDBus",
                    "-DQT_CORE_LIB", "-DQT_GUI_LIB", "-DQT_WIDGETS_LIB", "-DQT_DBUS_LIB",
                ]),
            ],
            // QtImageIO.cpp routes HEIF/HEIC/AVIF through libheif when its header is installed (`__has_include`), so
            // the link follows the same condition.
            linkerSettings: FileManager.default.fileExists(atPath: "/usr/include/libheif/heif.h") ? [.linkedLibrary("heif")] : []
        ),
        .executableTarget(
            name: "CompositorHostBootstrap",
            dependencies: ["Compositor", "HostRun", "ImageIO", "CoreGraphics"],
            path: "Sources/CompositorHostBootstrap",
            swiftSettings: [.unsafeFlags(["-swift-version", "5"])],
            linkerSettings: [
                .linkedLibrary("Qt6Widgets"),
                .linkedLibrary("Qt6Gui"),
                .linkedLibrary("Qt6Core"),
                .linkedLibrary("Qt6DBus"),
                .unsafeFlags(["-L/usr/lib/x86_64-linux-gnu",
                              "-Xlinker", "-rpath", "-Xlinker", "/usr/lib/x86_64-linux-gnu"]),
            ]
        ),
    ],
    cxxLanguageStandard: .cxx17
)
