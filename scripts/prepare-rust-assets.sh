#!/bin/sh
# Optional, pinned Linux x86_64 runtime assets for local subject detection and RAW.
set -eu
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
asset_dir=${OMUSE_RUNTIME_DIR:-${COMPOSITOR_RUNTIME_DIR:-$repo_root/rust/runtime}}
[ "$(uname -m)" = x86_64 ] || { printf '%s\n' 'ONNX archive is for Linux x86_64.' >&2; exit 1; }
mkdir -p "$asset_dir/lib" "$asset_dir/models" "$asset_dir/sources"
fetch() {
    file=$1 url=$2 expected=$3
    if [ -f "$file" ] && printf '%s  %s\n' "$expected" "$file" | sha256sum --check --status; then return; fi
    curl --fail --location --retry 2 --proto '=https' --tlsv1.2 "$url" --output "$file.part"
    printf '%s  %s\n' "$expected" "$file.part" | sha256sum --check --status || { printf 'Checksum mismatch: %s\n' "$file" >&2; rm -f -- "$file.part"; exit 1; }
    mv "$file.part" "$file"
}
# Reuse complete verified assets; updates should not rebuild LibRaw needlessly.
recipe=$(sha256sum "$0" | cut -d' ' -f1)
if [ -f "$asset_dir/ASSET-RECIPE" ] && [ "$(cat "$asset_dir/ASSET-RECIPE")" = "$recipe" ] \
    && [ -f "$asset_dir/ASSET-SHA256SUMS" ] \
    && (cd "$asset_dir" && sha256sum --check --status ASSET-SHA256SUMS); then
    printf 'Using verified runtime assets: %s\n' "$asset_dir"
    exit 0
fi
fetch "$asset_dir/models/u2netp.onnx" https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx 309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8
fetch "$asset_dir/sources/LibRaw-0.22.2.tar.gz" https://www.libraw.org/data/LibRaw-0.22.2.tar.gz de86b035655accff8d4010f1a221fdf50d353cb7b1422ba26f14a0db92612cfa
# Runtime archive hash is pinned alongside its extracted library below.
fetch "$asset_dir/sources/onnxruntime-linux-x64-1.23.2.tgz" https://github.com/microsoft/onnxruntime/releases/download/v1.23.2/onnxruntime-linux-x64-1.23.2.tgz 1fa4dcaef22f6f7d5cd81b28c2800414350c10116f5fdd46a2160082551c5f9b
scratch=$(mktemp -d "${TMPDIR:-/tmp}/omuse-assets.XXXXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
tar -xzf "$asset_dir/sources/onnxruntime-linux-x64-1.23.2.tgz" -C "$scratch"
printf '%s  %s\n' 13ab8084954fa4a47c777880180b90810d6020f021441395712b48a75b74c68b "$scratch/onnxruntime-linux-x64-1.23.2/lib/libonnxruntime.so.1.23.2" | sha256sum --check --status
install -m 755 "$scratch/onnxruntime-linux-x64-1.23.2/lib/libonnxruntime.so.1.23.2" "$asset_dir/lib/libonnxruntime.so"
tar -xzf "$asset_dir/sources/LibRaw-0.22.2.tar.gz" -C "$scratch"
(cd "$scratch/LibRaw-0.22.2" && ./configure --prefix="$scratch/install" --disable-examples --disable-openmp && make -j "${CARGO_BUILD_JOBS:-2}" && make install)
install -m 755 "$scratch/install/lib/libraw.so.25.0.0" "$asset_dir/lib/libraw.so"
(cd "$asset_dir" && sha256sum lib/libraw.so lib/libonnxruntime.so models/u2netp.onnx > ASSET-SHA256SUMS.new && mv ASSET-SHA256SUMS.new ASSET-SHA256SUMS)
printf '%s\n' "$recipe" > "$asset_dir/ASSET-RECIPE.new"
mv "$asset_dir/ASSET-RECIPE.new" "$asset_dir/ASSET-RECIPE"
printf 'Runtime assets ready: %s\n' "$asset_dir"
