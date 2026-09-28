#!/bin/sh
# Build only the native Rust editor. The existing Swift/Qt build is untouched.
set -eu
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
jobs=${CARGO_BUILD_JOBS:-2}
case "$jobs" in ''|*[!0-9]*|0) printf '%s\n' 'CARGO_BUILD_JOBS must be a positive integer.' >&2; exit 2;; esac
for tool in cargo rustc pkg-config; do
    command -v "$tool" >/dev/null 2>&1 || { printf 'Missing build tool: %s\n' "$tool" >&2; exit 1; }
done
if ! pkg-config --exists wayland-client xkbcommon xkbcommon-x11 fontconfig lcms2; then
    printf '%s\n' 'Missing Linux development libraries. Check Wayland, libxkbcommon (including X11), Fontconfig, and LittleCMS 2 with pkg-config.' >&2
    exit 1
fi
cd "$repo_root"
cargo build --manifest-path rust/Cargo.toml --release --locked --jobs "$jobs" "$@"
printf '%s\n' 'Omuse build complete. Run: cargo run --manifest-path rust/Cargo.toml --release --locked -- [optional-project.comp]'
