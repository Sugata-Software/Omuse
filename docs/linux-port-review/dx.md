# Developer Experience (DX) Review: Linux Compositor

**Review Mode:** WORKFLOW SIMPLICITY & REPRODUCIBILITY  
**Baseline:** Freedesktop SDK 26.08, Swift 6 (org.freedesktop.Sdk.Extension.swift6), Qt 6.11.2  
**Target Audience:** Core contributors, external open-source contributors, CI/CD pipelines

---

## 1. Executive Summary & Developer Workflow Principles

A developer should be able to clone the repository on any modern Linux system, run a single command, and have passing tests within minutes. The build system must never silently link incompatible host libraries or require manual, undocumented environment variable setup.

### Core DX Principles
1. **Zero System Pollution:** The application and its dependencies build completely inside reproducible containers (Flatpak / Freedesktop SDK) without requiring host `sudo apt/dnf install` of bleeding-edge toolchains.
2. **Deterministic Toolchains:** Swift 6.3.3+ and Qt 6.11.2 are pinned to exact SDK releases. No floating `latest` tags in production builds.
3. **Fast Local Inner Loop:** Swift developers can run `swift test` in under 30 seconds; C++ developers can build and run CTest suites in under 5 seconds.
4. **Clear Diagnostic Fail-Fast:** If a build dependency is missing, CMake and SPM report the exact missing component, why it is needed, and how to resolve it.

---

## 2. Contributor Command Cheatsheet

### 2.1 Swift Core Development & Testing
Run all 412 unit tests inside the Freedesktop SDK container with the official Swift 6 extension:

```bash
# Run full Swift test suite inside container (no host Swift required)
flatpak run --user --devel \
  --env=FLATPAK_ENABLE_SDK_EXT=swift6 \
  --filesystem="$PWD" \
  --command=sh org.kde.Sdk//6.11 \
  -c 'cd "$PWD" && export PATH=/usr/lib/sdk/swift6/bin:$PATH && swift test'
```

### 2.2 C++ Host & Kernel Development
Build and run the C/C++ side, pixel kernels, and ABI seam tests:

```bash
# Configure and build with CMake
cmake -S . -B build -DCOMPOSITOR_BUILD_HOST=ON
cmake --build build

# Run headless CTest suite
ctest --test-dir build --output-on-failure
```

### 2.3 Full Flatpak Application Packaging
Build the complete application bundle for distribution or local testing:

```bash
# Build and install Flatpak bundle locally
flatpak-builder --disable-rofiles-fuse --user --install --force-clean \
  build-flatpak com.wonderassembly.Compositor.yaml

# Launch the installed Flatpak
flatpak run com.wonderassembly.Compositor
```

---

## 3. CMake Presets (`CMakePresets.json`)

To standardize developer tooling across VS Code, CLion, Qt Creator, and terminal environments, provide modern CMake presets:

```json
{
  "version": 3,
  "configurePresets": [
    {
      "name": "dev",
      "displayName": "Local Host Development",
      "description": "Builds C kernels, ABI shim, and Qt host against host Qt6",
      "binaryDir": "${sourceDir}/build-dev",
      "cacheVariables": {
        "COMPOSITOR_BUILD_HOST": "ON",
        "CMAKE_BUILD_TYPE": "Debug",
        "CMAKE_EXPORT_COMPILE_COMMANDS": "ON"
      }
    },
    {
      "name": "flatpak",
      "displayName": "Flatpak In-Sandbox Build",
      "description": "Builds inside Flatpak environment with vendored Skia and OpenCV",
      "binaryDir": "${sourceDir}/build-flatpak-cmake",
      "cacheVariables": {
        "CMAKE_PREFIX_PATH": "/app",
        "COMPOSITOR_REQUIRE_VENDORED_DEPS": "ON",
        "COMPOSITOR_ENABLE_SKIA_GPU": "ON",
        "CMAKE_BUILD_TYPE": "Release"
      }
    },
    {
      "name": "sanitizers",
      "displayName": "ASan / UBSan Verification",
      "description": "Builds tests with Address and UndefinedBehavior sanitizers",
      "binaryDir": "${sourceDir}/build-san",
      "cacheVariables": {
        "COMPOSITOR_BUILD_HOST": "ON",
        "CMAKE_BUILD_TYPE": "Debug",
        "CMAKE_CXX_FLAGS": "-fsanitize=address,undefined -fno-omit-frame-pointer",
        "CMAKE_C_FLAGS": "-fsanitize=address,undefined -fno-omit-frame-pointer"
      }
    }
  ],
  "testPresets": [
    {
      "name": "all-tests",
      "configurePreset": "dev",
      "output": { "outputOnFailure": true }
    }
  ]
}
```

---

## 4. Test Pyramid & Automation Strategy

```text
                     ┌───────────────────────────────┐
                     │   Flatpak Journey Smoke Tests │ (Qt session open/save/reopen)
                     ├───────────────────────────────┤
                     │    Skia Bridge & GPU Tests    │ (Vulkan / Raster parity)
                     ├───────────────────────────────┤
                     │      C ABI Seam Integration   │ (Swift ↔ C++ handoffs)
                     ├───────────────────────────────┤
                     │   CompositorCore Unit Tests   │ (412 tests: tools, history,
                     │                               │  transforms, selections, masks)
                     ├───────────────────────────────┤
                     │      C Pixel Kernel Tests     │ (Adjust, Heal, Lens, Wand, Fill)
                     └───────────────────────────────┘
```

| Test Tier | Target Executable / Command | Execution Time | Primary Focus |
|---|---|---|---|
| **C Kernels** | `test_kernels` | < 0.2s | Arithmetic accuracy of portable C kernels |
| **C ABI** | `test_composite_over` | < 0.1s | Memory alignment, premultiplied RGBA contracts |
| **Swift Core** | `swift test` | ~20s | Domain logic, undo history, brush math, layers |
| **Skia Bridge** | `test_skia_bridge` | < 0.5s | Skia Raster buffer round-trip & Vulkan device query |
| **Host Journey** | `test_host_journey` | < 0.2s | Headless Qt canvas manipulation & PNG export |

---

## 5. Dependency Management & Pinning Policy

- **Pinned Records:**
  - `third_party/skia.pinned`: CanvasKit `0.42.0` with Ganesh + Vulkan + GL/EGL enabled.
  - `third_party/opencv.pinned`: OpenCV `4.14.0` (minimal `core` and `imgproc` modules only).
- **Offline Build Compliance:**
  - Release archives are pre-fetched by the Flatpak manifest with verified SHA-256 checksums.
  - Network access is disabled during the compilation phase to guarantee zero surprise external fetches.

---

## 6. DX Sign-Off & Status

- [x] Tested one-command container test invocation: `swift test` runs cleanly and passes all 412 tests.
- [x] Verified CTest execution for host targets.
- [x] CMake configurations provide explicit, helpful error messages when dependencies are missing.
- [x] Flatpak packaging scripts updated to reflect Freedesktop 26.08 and Swift 6 toolchain requirements.
