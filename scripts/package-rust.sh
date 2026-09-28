#!/bin/sh
# Build a checksum-verifiable, full-feature Linux x86_64 runtime bundle.
set -eu

usage() {
    cat <<'EOF'
Usage: scripts/package-rust.sh --binary PATH [--output ARCHIVE] [--runtime-dir DIR]

Packages an already-built omuse executable. This script does not build,
download, install, sign, or publish anything. A dirty source tree is rejected;
set OMUSE_ALLOW_DIRTY=1 only for an explicitly local development bundle.
COMPOSITOR_ALLOW_DIRTY remains a compatibility alias.
EOF
}

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary=
output=
runtime_dir=${OMUSE_RUNTIME_DIR:-${COMPOSITOR_RUNTIME_DIR:-$repo_root/rust/runtime}}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --binary) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; binary=$2; shift 2 ;;
        --output) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; output=$2; shift 2 ;;
        --runtime-dir) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; runtime_dir=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

[ -n "$binary" ] || { printf '%s\n' '--binary is required' >&2; exit 2; }
[ -x "$binary" ] || { printf 'Executable not found: %s\n' "$binary" >&2; exit 1; }
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ] || {
    printf '%s\n' 'This bundle recipe currently supports Linux x86_64 only.' >&2
    exit 1
}
command -v git >/dev/null 2>&1 && command -v sha256sum >/dev/null 2>&1 \
    && command -v python3 >/dev/null 2>&1 && command -v cargo >/dev/null 2>&1 || {
    printf '%s\n' 'git, cargo, python3 and sha256sum are required.' >&2; exit 1;
}

revision=$(git -C "$repo_root" rev-parse --verify HEAD)
dirty=$(git -C "$repo_root" status --porcelain --untracked-files=normal)
allow_dirty=${OMUSE_ALLOW_DIRTY:-${COMPOSITOR_ALLOW_DIRTY:-0}}
if [ -n "$dirty" ] && [ "$allow_dirty" != 1 ]; then
    printf '%s\n' 'Refusing to package a dirty source tree.' >&2
    printf '%s\n' 'Commit/stash all changes, or set OMUSE_ALLOW_DIRTY=1 for a local development bundle.' >&2
    exit 1
fi

for required in \
    "$runtime_dir/lib/libonnxruntime.so" \
    "$runtime_dir/lib/libraw.so" \
    "$runtime_dir/models/u2netp.onnx" \
    "$repo_root/rust/assets/omuse.png" \
    "$repo_root/rust/assets/omuse.svg" \
    "$repo_root/LICENSE" \
    "$repo_root/scripts/install-rust-bundle.sh" \
    "$repo_root/scripts/install-app.py" \
    "$repo_root/scripts/rust-license-inventory.py"
do
    [ -f "$required" ] || { printf 'Required bundle input is missing: %s\n' "$required" >&2; exit 1; }
done
for notice in "$repo_root"/rust/licenses/*.txt; do
    [ -f "$notice" ] || { printf '%s\n' 'Runtime license notices are missing.' >&2; exit 1; }
done

short_revision=$(printf '%s' "$revision" | cut -c1-12)
output=${output:-$repo_root/dist/omuse-$short_revision-linux-x86_64.tar.gz}
case "$output" in /*) ;; *) output=$PWD/$output ;; esac
mkdir -p "$(dirname -- "$output")"
work=$(mktemp -d "${TMPDIR:-/tmp}/omuse-bundle.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT HUP INT TERM
root=$work/omuse-bundle
mkdir -p "$root/bin" "$root/lib" "$root/models" "$root/licenses" "$root/share/icons"
inventory=$work/license-inventory
python3 "$repo_root/scripts/rust-license-inventory.py" "$inventory"
[ -f "$inventory/inventory.json" ] && [ -f "$inventory/THIRD_PARTY_NOTICES.txt" ] || {
    printf '%s\n' 'Rust dependency license inventory is incomplete.' >&2; exit 1;
}

install -m 755 "$binary" "$root/bin/omuse"
install -m 755 "$runtime_dir/lib/libonnxruntime.so" "$root/lib/libonnxruntime.so"
install -m 755 "$runtime_dir/lib/libraw.so" "$root/lib/libraw.so"
install -m 644 "$runtime_dir/models/u2netp.onnx" "$root/models/u2netp.onnx"
install -m 644 "$repo_root/LICENSE" "$root/licenses/Omuse-MIT.txt"
cp "$repo_root"/rust/licenses/*.txt "$root/licenses/"
install -m 644 "$inventory/inventory.json" "$root/licenses/rust-dependency-inventory.json"
install -m 644 "$inventory/THIRD_PARTY_NOTICES.txt" \
    "$root/licenses/Rust-THIRD-PARTY-NOTICES.txt"
install -m 644 "$repo_root/rust/assets/omuse.png" "$root/share/icons/omuse.png"
install -m 644 "$repo_root/rust/assets/omuse.svg" "$root/share/icons/omuse.svg"
install -m 755 "$repo_root/scripts/install-rust-bundle.sh" "$root/install.sh"
install -m 755 "$repo_root/scripts/install-app.py" "$root/install-app.py"

dirty_label=false
[ -n "$dirty" ] && dirty_label=true
cat > "$root/SOURCE-REVISION" <<EOF
source_revision=$revision
source_tree_dirty=$dirty_label
target=linux-x86_64
bundle_kind=full-feature
EOF

(
    cd "$root"
    LC_ALL=C find . -type f ! -name SHA256SUMS -print | LC_ALL=C sort |
        while IFS= read -r file; do sha256sum "$file"; done > SHA256SUMS
)

epoch=$(git -C "$repo_root" show -s --format=%ct "$revision")
archive_tmp=$output.new.$$
rm -f -- "$archive_tmp"
tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -czf "$archive_tmp" -C "$work" omuse-bundle
mv -f -- "$archive_tmp" "$output"
printf 'Created bundle: %s\n' "$output"
printf 'SHA-256: %s\n' "$(sha256sum "$output" | cut -d' ' -f1)"
