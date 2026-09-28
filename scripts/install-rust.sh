#!/bin/sh
# Install Omuse per-user while preserving the older Compositor applications.
set -eu
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary=${1:-${CARGO_TARGET_DIR:-$repo_root/rust/target}/release/omuse}
local_prefix=${OMUSE_INSTALL_PREFIX:-${COMPOSITOR_INSTALL_PREFIX:-$HOME/.local}}
[ -x "$binary" ] || { printf 'Build the Rust executable first: %s\n' "$binary" >&2; exit 1; }
app_dir="$local_prefix/opt/omuse"
legacy_app_dir="$local_prefix/opt/compositor-rust"
mkdir -p "$app_dir" "$local_prefix/bin" "$local_prefix/share/applications" "$local_prefix/share/icons/hicolor/256x256/apps"
mkdir -p "$local_prefix/share/icons/hicolor/scalable/apps"
asset_dir=${OMUSE_RUNTIME_DIR:-${COMPOSITOR_RUNTIME_DIR:-$repo_root/rust/runtime}}
mkdir -p "$app_dir/licenses"
cp "$repo_root"/rust/licenses/*.txt "$app_dir/licenses/"
if [ -f "$asset_dir/lib/libonnxruntime.so" ] && [ -f "$asset_dir/models/u2netp.onnx" ] && [ -f "$asset_dir/lib/libraw.so" ]; then
    mkdir -p "$app_dir/lib" "$app_dir/models"
    for library in libonnxruntime.so libraw.so; do
        install -m 755 "$asset_dir/lib/$library" "$app_dir/lib/$library.new"
        mv "$app_dir/lib/$library.new" "$app_dir/lib/$library"
    done
    install -m 644 "$asset_dir/models/u2netp.onnx" "$app_dir/models/u2netp.onnx.new"
    mv "$app_dir/models/u2netp.onnx.new" "$app_dir/models/u2netp.onnx"
else
    printf '%s\n' 'Optional RAW/subject assets absent. Run scripts/prepare-rust-assets.sh and reinstall to enable them.' >&2
fi
install -m 755 "$binary" "$app_dir/omuse.new"
if [ -f "$app_dir/omuse" ]; then
    cp -p "$app_dir/omuse" "$app_dir/omuse.previous"
elif [ ! -f "$app_dir/omuse.previous" ] && [ -f "$legacy_app_dir/compositor-rust" ]; then
    cp -p "$legacy_app_dir/compositor-rust" "$app_dir/omuse.previous"
fi
mv "$app_dir/omuse.new" "$app_dir/omuse"
cat > "$local_prefix/bin/omuse" <<LAUNCHER
#!/bin/sh
set -eu
exec "$app_dir/omuse" "\$@"
LAUNCHER
chmod 755 "$local_prefix/bin/omuse"
cat > "$local_prefix/bin/compositor-rust" <<LAUNCHER
#!/bin/sh
set -eu
exec "$local_prefix/bin/omuse" "\$@"
LAUNCHER
chmod 755 "$local_prefix/bin/compositor-rust"
install -m 644 "$repo_root/rust/assets/omuse.png" "$local_prefix/share/icons/hicolor/256x256/apps/omuse.png"
install -m 644 "$repo_root/rust/assets/omuse.svg" "$local_prefix/share/icons/hicolor/scalable/apps/omuse.svg"
cat > "$local_prefix/share/applications/omuse.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Omuse
Comment=Native image editor
Exec="$local_prefix/bin/omuse" %f
Icon=omuse
Terminal=false
Categories=Graphics;2DGraphics;RasterGraphics;
StartupNotify=true
StartupWMClass=omuse
DESKTOP
legacy_desktop="$local_prefix/share/applications/compositor-rust.desktop"
retired_desktop="$legacy_desktop.retired-by-omuse"
if [ -f "$legacy_desktop" ] \
    && grep -qx 'Name=Compositor Rust' "$legacy_desktop" \
    && grep -qx 'Icon=compositor-rust' "$legacy_desktop" \
    && grep -qx 'StartupWMClass=compositor-rust' "$legacy_desktop"; then
    if [ ! -e "$retired_desktop" ]; then
        cp -p -- "$legacy_desktop" "$retired_desktop.new"
        mv -f -- "$retired_desktop.new" "$retired_desktop"
    fi
    rm -f -- "$legacy_desktop"
fi
if command -v desktop-file-validate >/dev/null 2>&1; then desktop-file-validate "$local_prefix/share/applications/omuse.desktop"; fi
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$local_prefix/share/applications"; fi
printf 'Installed Omuse: %s\n' "$local_prefix/bin/omuse"
printf 'Compatibility launcher: %s\n' "$local_prefix/bin/compositor-rust"
printf 'Previous executable, when present: %s\n' "$app_dir/omuse.previous"
