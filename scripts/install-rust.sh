#!/bin/sh
# Install a locally built Omuse with a verified, complete rollback generation.
set -eu
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary=${1:-${CARGO_TARGET_DIR:-$repo_root/rust/target}/release/omuse}
local_prefix=${OMUSE_INSTALL_PREFIX:-${COMPOSITOR_INSTALL_PREFIX:-$HOME/.local}}
asset_dir=${OMUSE_RUNTIME_DIR:-${COMPOSITOR_RUNTIME_DIR:-$repo_root/rust/runtime}}
[ -x "$binary" ] || { printf 'Build the executable first: %s\n' "$binary" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { printf '%s\n' 'python3 is required.' >&2; exit 1; }
work=$(mktemp -d "${TMPDIR:-/tmp}/omuse-payload.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$work/icons" "$work/licenses"
install -m 755 "$binary" "$work/omuse"
install -m 644 "$repo_root/LICENSE" "$work/licenses/Omuse-MIT.txt"
cp "$repo_root"/rust/licenses/*.txt "$work/licenses/"
install -m 644 "$repo_root/rust/assets/omuse.png" "$work/icons/omuse.png"
install -m 644 "$repo_root/rust/assets/omuse.svg" "$work/icons/omuse.svg"
if [ -f "$asset_dir/lib/libonnxruntime.so" ] && [ -f "$asset_dir/models/u2netp.onnx" ] && [ -f "$asset_dir/lib/libraw.so" ]; then
    mkdir -p "$work/lib" "$work/models"
    install -m 755 "$asset_dir/lib/libonnxruntime.so" "$work/lib/libonnxruntime.so"
    install -m 755 "$asset_dir/lib/libraw.so" "$work/lib/libraw.so"
    install -m 644 "$asset_dir/models/u2netp.onnx" "$work/models/u2netp.onnx"
else
    printf '%s\n' 'Installing core editor without optional Camera RAW / subject-selection assets.'
fi
revision=$(git -C "$repo_root" rev-parse HEAD 2>/dev/null || printf local-build)
dirty=false
[ -z "$(git -C "$repo_root" status --porcelain --untracked-files=normal 2>/dev/null)" ] || dirty=true
printf 'source_revision=%s\nsource_tree_dirty=%s\ntarget=linux-x86_64\n' "$revision" "$dirty" > "$work/SOURCE-REVISION"
python3 "$repo_root/scripts/install-app.py" install --prefix "$local_prefix" --payload "$work" --revision "$revision"
