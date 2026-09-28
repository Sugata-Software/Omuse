#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/compositor-color-noise.XXXXXXXX")
trap 'rm -rf "$work"' EXIT

cc=${CC:-clang}
common=(-std=c11 -O1 -g -Wall -Wextra -Werror -Wno-unused-parameter -I"$repo/Compositor/Rendering")
harness="$repo/tests/upstream_color_noise_repro.c"
allocator="$repo/tests/upstream_color_noise_allocator.h"
source="$repo/Compositor/Rendering/AdjustPixels.c"

build() {
    local source_file=$1 output=$2
    "$cc" "${common[@]}" -include "$allocator" -Dmalloc=repro_malloc -Dfree=repro_free \
        -c "$source_file" -o "$work/adjust.o"
    "$cc" "${common[@]}" "$harness" "$work/adjust.o" "$repo/Compositor/Rendering/LensPixels.c" -lm -o "$output"
}

mkdir -p "$work/fixed/Compositor/Rendering"
cp "$source" "$work/fixed/Compositor/Rendering/AdjustPixels.c"
patch --quiet -d "$work/fixed" -p1 < "$repo/patches/color-noise-zero-alpha.patch"

build "$source" "$work/buggy"
build "$work/fixed/Compositor/Rendering/AdjustPixels.c" "$work/fixed-bin"

for variant in buggy fixed-bin; do
    for pattern in 0 63; do
        REPRO_FILL_BYTE=$pattern "$work/$variant" transparent
        REPRO_FILL_BYTE=$pattern "$work/$variant" opaque
    done
done

bug_mixed_zero=$(REPRO_FILL_BYTE=0 "$work/buggy" mixed)
bug_mixed_pattern=$(REPRO_FILL_BYTE=63 "$work/buggy" mixed)
fixed_mixed_zero=$(REPRO_FILL_BYTE=0 "$work/fixed-bin" mixed)
fixed_mixed_pattern=$(REPRO_FILL_BYTE=63 "$work/fixed-bin" mixed)
bug_opaque_zero=$(REPRO_FILL_BYTE=0 "$work/buggy" opaque)
bug_opaque_pattern=$(REPRO_FILL_BYTE=63 "$work/buggy" opaque)
fixed_opaque_zero=$(REPRO_FILL_BYTE=0 "$work/fixed-bin" opaque)

printf '%s\n' "$bug_mixed_zero" "$bug_mixed_pattern" "$fixed_mixed_zero" "$fixed_mixed_pattern"

if [[ "$bug_mixed_zero" == "$bug_mixed_pattern" ]]; then
    echo "FAIL: buggy mixed output did not react to scratch-memory contents" >&2
    exit 1
fi
if [[ "$fixed_mixed_zero" != "$fixed_mixed_pattern" ]]; then
    echo "FAIL: fixed mixed output still reacts to scratch-memory contents" >&2
    exit 1
fi
if [[ "$bug_opaque_zero" != "$bug_opaque_pattern" || "$bug_opaque_zero" != "$fixed_opaque_zero" ]]; then
    echo "FAIL: opaque output changed with scratch contents or candidate patch" >&2
    exit 1
fi

if "$cc" -fsanitize=memory -fPIE -pie -O1 -g -std=c11 \
    -I"$repo/Compositor/Rendering" "$harness" "$source" "$repo/Compositor/Rendering/LensPixels.c" -lm -o "$work/msan" 2>/dev/null; then
    set +e
    MSAN_OPTIONS=halt_on_error=1:exit_code=86 "$work/msan" mixed >"$work/msan.out" 2>"$work/msan.err"
    detector_status=$?
    set -e
    if [[ $detector_status -eq 86 ]] && rg -q 'MemorySanitizer: use-of-uninitialized-value' "$work/msan.err"; then
        echo "MemorySanitizer: confirmed use of uninitialized value in current source"
        sed -n '1,20p' "$work/msan.err"
    else
        echo "FAIL: MemorySanitizer build ran but did not report the expected issue" >&2
        sed -n '1,80p' "$work/msan.err" >&2
        exit 1
    fi
else
    echo "MemorySanitizer: unavailable; allocator-poisoning checks still ran" >&2
fi

echo "PASS: transparent pixels and alpha are preserved; mixed output is stabilized; opaque output is unchanged"
