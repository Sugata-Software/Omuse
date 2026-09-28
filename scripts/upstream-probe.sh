#!/bin/sh
# Phase-0 feasibility probe: compiles upstream Compositor/{Document,IO,Rendering} UNMODIFIED
# against stub Apple modules and prints the distinct missing Apple symbols (the shim work list).
# Usage: scripts/upstream-probe.sh [work-dir]   (run inside the Flatpak SDK, see docs/upstream-shim-worklist.md)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
W="${1:-${TMPDIR:-/tmp}/upstream-probe}"
rm -rf "$W"; mkdir -p "$W/Sources"
# Frameworks still stubbed (empty modules that only re-export Apple's umbrella imports).
# Frameworks the port already implements (real compat modules).
for m in AppKit FoundationCompat UniformTypeIdentifiers Accelerate ImageIO CoreImage CoreVideo SwiftUI Vision; do ln -s "$ROOT/Sources/Compat/$m" "$W/Sources/$m"; done
# CoreGraphics = the real compat module (it no longer depends on any document type).
ln -s "$ROOT/Sources/Compat/CoreGraphics" "$W/Sources/CoreGraphics"
# Upstream: only Document, IO, Rendering (UI/App files are the Qt shell's job); C kernels become their own target.
mkdir -p "$W/Sources/UpstreamProbe" "$W/Sources/UpstreamKernels/include"
for d in Document IO Rendering; do ln -s "$ROOT/Compositor/$d" "$W/Sources/UpstreamProbe/$d"; done
# Linux-original code the Metal overrides build on: the Vulkan/CPU brush backend, plus the overrides themselves.
ln -s "$ROOT/Sources/Overrides" "$W/Sources/UpstreamProbe/Overrides"
mkdir -p "$W/Sources/UpstreamProbe/LinuxRendering" "$W/Sources/CompositorBrushBackend"
for f in BrushCoverage NativeBrushCoverage; do ln -s "$ROOT/Sources/CompositorCore/Rendering/$f.swift" "$W/Sources/UpstreamProbe/LinuxRendering/$f.swift"; done
for f in "$ROOT"/backends/brush/*; do ln -s "$f" "$W/Sources/CompositorBrushBackend/$(basename "$f")"; done
for f in "$ROOT"/Compositor/Rendering/*.c "$ROOT"/Compositor/Rendering/*.h; do
  ln -s "$f" "$W/Sources/UpstreamKernels/$(basename "$f")"; ln -s "$f" "$W/Sources/UpstreamKernels/include/$(basename "$f")"; done
CEXCL=$(ls "$ROOT"/Compositor/Rendering | grep -E '\.(c|h)$' | sed 's#.*#"Rendering/&"#' | paste -sd,)
cat > "$W/Package.swift" <<PKG
// swift-tools-version:6.2
import PackageDescription
let v5: [SwiftSetting] = [.swiftLanguageMode(.v5)]
var targets: [Target] = [.target(name: "CoreGraphics", path: "Sources/CoreGraphics", swiftSettings: v5),
  .target(name: "UpstreamKernels", path: "Sources/UpstreamKernels", publicHeadersPath: "include"),
  .target(name: "CompositorBrushBackend", path: "Sources/CompositorBrushBackend", exclude: ["shaders/continuous_brush.comp"],
          sources: ["BrushCoverageCPU.cpp", "VulkanBrushCoverage.cpp"], publicHeadersPath: "include", linkerSettings: [.linkedLibrary("vulkan")])]
targets.append(.target(name: "FoundationCompat", dependencies: ["UniformTypeIdentifiers"], path: "Sources/FoundationCompat", swiftSettings: v5))
targets.append(.target(name: "UniformTypeIdentifiers", path: "Sources/UniformTypeIdentifiers", swiftSettings: v5))
targets.append(.target(name: "Accelerate", path: "Sources/Accelerate", swiftSettings: v5))
targets.append(.target(name: "CoreVideo", path: "Sources/CoreVideo", swiftSettings: v5))
targets.append(.target(name: "SwiftUI", dependencies: ["CoreGraphics","AppKit"], path: "Sources/SwiftUI", swiftSettings: v5))
targets.append(.target(name: "ImageIO", dependencies: ["CoreGraphics","Accelerate","UniformTypeIdentifiers"], path: "Sources/ImageIO", swiftSettings: v5))
targets.append(.target(name: "CoreImage", dependencies: ["CoreGraphics","CoreVideo"], path: "Sources/CoreImage", swiftSettings: v5))
targets.append(.target(name: "AppKit", dependencies: ["CoreGraphics","FoundationCompat","UniformTypeIdentifiers","ImageIO"], path: "Sources/AppKit", swiftSettings: v5))
targets.append(.target(name: "Vision", dependencies: ["CoreGraphics","CoreVideo"], path: "Sources/Vision", swiftSettings: v5))
let apple = ["Vision","SwiftUI","AppKit","FoundationCompat","UniformTypeIdentifiers","Accelerate","ImageIO","CoreImage","CoreVideo"]
targets.append(.target(name: "UpstreamProbe", dependencies: (apple + ["CoreGraphics","UpstreamKernels","CompositorBrushBackend"]).map { .byName(name: \$0) },
  path: "Sources/UpstreamProbe",
  exclude: [$CEXCL,"IO/CompositorApplicationDelegate.swift",
    "Rendering/EditorCanvas.swift","Rendering/InlineTextEditor.swift","Rendering/BrushCursorOverlay.swift","Rendering/SampleRingOverlay.swift",
    "Rendering/MetalBrushCoverage.swift","Rendering/MetalLayerEffects.swift"],
  swiftSettings: v5 + [.defaultIsolation(MainActor.self), .enableUpcomingFeature("NonisolatedNonsendingByDefault"), .enableUpcomingFeature("InferSendableFromCaptures"), .unsafeFlags(["-Xfrontend","-import-module","-Xfrontend","FoundationCompat","-import-objc-header","$ROOT/Compositor/Compositor-Bridging-Header.h","-Xcc","-I$ROOT/Compositor/Rendering"])]))
let package = Package(name: "Probe", products: [.library(name: "UpstreamProbe", targets: ["UpstreamProbe"])], targets: targets)
PKG
cd "$W" && swift build > build.log 2>&1 || true
grep "error:" build.log | grep -v '^ ' | sed -E 's#^.*Sources/UpstreamProbe/##' | sort -u > errors.txt
echo "distinct error lines: $(wc -l < errors.txt)  (see $W/errors.txt, $W/build.log)"
grep -ohE "cannot find (type )?'[A-Za-z0-9_]+' in scope" errors.txt | sed -E "s/.*'([^']+)'.*/\1/" | sort -u | tr '\n' ' '; echo
