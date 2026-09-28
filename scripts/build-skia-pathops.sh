#!/bin/sh
# Skia only compiles src/pathops (SkPathOps: path booleans used by CGPath.union/intersection/...) when PDF/XPS are
# enabled. The compositor needs it without PDF, so build that GN source set and archive it next to libskia.a.
# Usage: scripts/build-skia-pathops.sh [skia-out-dir]   (default: build/skia-src/out/Raster)
set -e
OUT="${1:-$(cd "$(dirname "$0")/.." && pwd)/build/skia-src/out/Raster}"
ninja -C "$OUT" pathops
rm -f "$OUT/libskia_pathops.a"
ar rcs "$OUT/libskia_pathops.a" "$OUT"/obj/src/pathops/*.o
echo "built $OUT/libskia_pathops.a ($(ls "$OUT"/obj/src/pathops/*.o | wc -l) objects)"
