#!/bin/sh
# Domain tests, headless GPUI interactions, and a disposable editing journey.
set -eu
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
jobs=${CARGO_BUILD_JOBS:-2}
case "$jobs" in ''|*[!0-9]*|0) printf '%s\n' 'CARGO_BUILD_JOBS must be a positive integer.' >&2; exit 2;; esac
scratch=$(mktemp -d "${TMPDIR:-/tmp}/omuse-tests.XXXXXXXX")
cleanup() {
    keep=${OMUSE_TEST_KEEP:-${COMPOSITOR_TEST_KEEP:-0}}
    if [ "$keep" = 1 ]; then
        printf 'Test evidence retained: %s\n' "$scratch"
    else
        rm -rf -- "$scratch"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
export XDG_DATA_HOME="$scratch/data"
export XDG_CONFIG_HOME="$scratch/config"
export XDG_CACHE_HOME="$scratch/cache"
export XDG_STATE_HOME="$scratch/state"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"
cd "$repo_root"
python3 scripts/generate-keyboard-shortcuts.py --check
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo test --manifest-path rust/Cargo.toml --release --locked --features ui-test --jobs "$jobs" -- --test-threads=1
cargo run --manifest-path rust/Cargo.toml --release --locked --features ui-test --jobs "$jobs" -- --self-test "$scratch/journey"
cargo run --manifest-path rust/Cargo.toml --release --locked --example create_acceptance --jobs "$jobs" -- "$scratch/create-journey" --motion
cargo run --manifest-path rust/Cargo.toml --release --locked --example catalog_acceptance --jobs "$jobs" -- "$scratch/template-catalog"
cat "$scratch/journey/results.json"
printf '\n%s\n' 'Omuse validation complete; test documents were created only in the temporary test directory.'
