#!/usr/bin/env bash
# Omuse installer. Run as your regular desktop user, never with sudo.
set -Eeuo pipefail

# Advance only to a public runtime commit with completed release validation.
# Documentation and ongoing work on main do not change the installed editor.
readonly omuse_release_revision=eb558dc59ccdf88a3d62dfc2d706a6b836da1eda

usage() {
    cat <<'EOF'
Omuse — native creative tools for Linux

Install or update:
  curl -fsSL https://raw.githubusercontent.com/Sugata-Software/Omuse/main/install.sh | bash

Omuse builds locally on Arch/Omarchy x86_64 from a tested source revision.
The first build takes time; later installs reuse downloads and compiled code.

Options (with a pipe, use bash -s -- OPTIONS):
  --prefix DIR          Install under DIR instead of ~/.local
  --jobs NUMBER         Concurrent build jobs (default: 2)
  --yes                 Accept package-manager prompts (sudo may still ask)
  --rollback            Restore the previous complete Omuse installation
  --uninstall           Remove Omuse; keep projects, settings and recovery
  --no-deps             Use existing build dependencies; skip pacman
  --no-runtime-assets   Skip Camera RAW / local subject-selection assets
  --source DIR          Build a development checkout instead of the tested revision
  -h, --help            Show this help
EOF
}

fail() { printf '\nOmuse: %s\n' "$*" >&2; exit 1; }
step() { printf '\n==> %s\n' "$*"; }

main() {
    local prefix=${OMUSE_INSTALL_PREFIX:-$HOME/.local}
    local jobs=${CARGO_BUILD_JOBS:-2} yes=0 deps=1 assets=1 action=install source_dir=
    while (($#)); do
        case "$1" in
            --prefix|--jobs|--source)
                (($# >= 2)) || fail "$1 requires a value."
                case "$1" in --prefix) prefix=$2;; --jobs) jobs=$2;; --source) source_dir=$2;; esac
                shift 2;;
            --yes) yes=1; shift;;
            --no-deps) deps=0; shift;;
            --no-runtime-assets) assets=0; shift;;
            --rollback|--uninstall)
                [[ $action == install ]] || fail 'Choose only one action.'
                action=${1#--}; shift;;
            -h|--help) usage; return;;
            *) fail "Unknown option: $1 (use --help).";;
        esac
    done
    [[ $prefix == /* && $prefix != / ]] || fail '--prefix must be an absolute user-owned directory.'
    [[ $prefix != *[$'\001'-$'\037'$'\177']* && $prefix != *"'"* && $prefix != *'\'* ]] || fail 'Unsupported character in --prefix.'
    [[ $jobs =~ ^[1-9][0-9]*$ ]] || fail '--jobs must be a positive integer.'
    [[ $(uname -s) == Linux ]] || fail 'Omuse is a Linux application.'
    ((EUID != 0)) || fail 'Run as your regular desktop user, without sudo. Only package installation uses sudo.'
    if [[ $action != install ]]; then
        [[ -f $prefix/opt/omuse/manage.py ]] || fail "No managed Omuse installation at $prefix."
        python3 "$prefix/opt/omuse/manage.py" "$action" --prefix "$prefix"
        return
    fi
    [[ $(uname -m) == x86_64 ]] || fail 'The installer currently supports Linux x86_64.'
    if [[ -n $source_dir ]]; then
        [[ -f $source_dir/rust/Cargo.toml && -f $source_dir/scripts/install-app.py ]] || fail '--source must be an Omuse checkout.'
        source_dir=$(cd -- "$source_dir" && pwd)
    fi
    local ID= ID_LIKE=
    [[ ! -r /etc/os-release ]] || . /etc/os-release
    if ((deps)) && [[ " $ID $ID_LIKE " != *' arch '* && $ID != omarchy ]]; then
        fail 'Automatic setup supports Arch/Omarchy. On another Linux distribution, install the dependencies in rust/README.md and use --no-deps.'
    fi
    step 'Install Omuse'
    printf 'Destination: %s\nFirst install: compiles Rust locally; allow time and about 12 GB of free disk space.\n' "$prefix"
    printf 'Includes Camera RAW and local subject tools unless --no-runtime-assets is set.\n'
    if ((deps)); then
        command -v pacman >/dev/null || fail 'pacman is required for automatic dependency setup.'
        local packages=(base-devel git curl python pkgconf wayland libxkbcommon libxkbcommon-x11 fontconfig lcms2 ffmpeg desktop-file-utils)
        local missing=() package
        for package in "${packages[@]}"; do
            pacman -Q "$package" >/dev/null 2>&1 || missing+=("$package")
        done
        if ((${#missing[@]})); then
            step "Install required system packages: ${missing[*]}"
            command -v sudo >/dev/null || fail 'sudo is needed to install missing system packages.'
            local flags=(--needed)
            ((yes == 0)) || flags+=(--noconfirm)
            # Never refresh databases alone or silently perform a system upgrade.
            local input=/dev/tty
            if ((yes)) && ! { true </dev/tty; } 2>/dev/null; then input=/dev/null; fi
            if ! sudo pacman -S "${flags[@]}" "${missing[@]}" <"$input"; then
                fail 'Package setup did not finish. Update your Arch system normally if mirrors are stale, then rerun this command.'
            fi
        fi
    fi
    local tool
    for tool in git curl python3 cc make pkg-config flock tee ffmpeg; do
        command -v "$tool" >/dev/null || fail "Missing $tool. See rust/README.md for development prerequisites."
    done
    local cache=${XDG_CACHE_HOME:-$HOME/.cache}/omuse/installer
    [[ $cache == /* ]] || fail 'XDG_CACHE_HOME must be an absolute path.'
    mkdir -p -- "$cache"
    exec 9>"$cache/install.lock"
    flock -n 9 || fail 'Another Omuse installer is already running.'
    local log=$cache/install-$(date -u +%Y%m%dT%H%M%SZ).log
    exec > >(tee -a "$log") 2>&1
    printf 'Install log: %s\n' "$log"
    # These task-specific variables stay available when EXIT runs after main unwinds.
    local work
    work=$(mktemp -d "$cache/run.XXXXXXXX")
    printf -v omuse_cleanup_command 'rm -rf -- %q' "$work"
    trap "$omuse_cleanup_command" EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'printf "\nOmuse installation stopped. Your previous app is preserved.\nLog: %s\n" "$log" >&2' ERR
    if [[ -z $source_dir ]]; then
        step "Fetch tested Omuse source ${omuse_release_revision:0:12}"
        source_dir=$cache/source
        local repository=https://github.com/Sugata-Software/Omuse.git
        if [[ ! -e $source_dir ]]; then
            git init --quiet "$work/source"
            git -C "$work/source" remote add origin "$repository"
            git -C "$work/source" fetch --quiet --depth 1 origin "$omuse_release_revision"
            git -C "$work/source" checkout --quiet --detach FETCH_HEAD
            printf '%s\n' "$repository" > "$work/source/.git/omuse-installer"
            mv -- "$work/source" "$source_dir"
        else
            [[ -f $source_dir/.git/omuse-installer && ! -L $source_dir ]] || fail "Unrecognized source cache: $source_dir"
            [[ $(git -C "$source_dir" remote get-url origin) == "$repository" ]] || fail 'The source cache has an unexpected origin.'
            [[ -z $(git -C "$source_dir" status --porcelain --untracked-files=normal) ]] || fail "The source cache has local edits. Preserve them before updating: $source_dir"
            git -C "$source_dir" fetch --quiet --depth 1 origin "$omuse_release_revision"
            git -C "$source_dir" checkout --quiet --detach FETCH_HEAD
        fi
        [[ $(git -C "$source_dir" rev-parse HEAD) == "$omuse_release_revision" ]] || fail 'Downloaded source does not match the tested revision.'
    fi
    local revision toolchain
    revision=$(git -C "$source_dir" rev-parse HEAD)
    toolchain=$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["toolchain"]["channel"])' "$source_dir/rust-toolchain.toml")
    step "Prepare Rust $toolchain (source ${revision:0:12})"
    if command -v rustc >/dev/null && command -v cargo >/dev/null \
        && [[ $(rustc --version | awk '{print $2}') == "$toolchain" ]] \
        && [[ $(cargo --version | awk '{print $2}') == "$toolchain" ]]; then
        printf 'Using the existing Rust %s toolchain.\n' "$toolchain"
    else
        if ! command -v rustup >/dev/null && [[ -x $cache/toolchain/cargo/bin/rustup ]]; then
            export CARGO_HOME=$cache/toolchain/cargo RUSTUP_HOME=$cache/toolchain/rustup
            export PATH=$CARGO_HOME/bin:$PATH
        fi
        if ! command -v rustup >/dev/null; then
            # Keep distro Rust and shell startup files untouched.
            export CARGO_HOME=$cache/toolchain/cargo RUSTUP_HOME=$cache/toolchain/rustup
            curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
                https://sh.rustup.rs --output "$work/rustup-init.sh"
            sh "$work/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain "$toolchain"
            export PATH=$CARGO_HOME/bin:$PATH
        fi
        rustup toolchain install "$toolchain" --profile minimal --component rustfmt
        # Explicit binaries also work if distribution Cargo precedes rustup proxies.
        local selected_cargo
        selected_cargo=$(rustup which --toolchain "$toolchain" cargo)
        export PATH=${selected_cargo%/*}:$PATH
    fi
    export RUSTUP_TOOLCHAIN=$toolchain CARGO_BUILD_JOBS=$jobs
    export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$cache/build}
    export OMUSE_RUNTIME_DIR=${OMUSE_RUNTIME_DIR:-$cache/runtime}
    [[ $CARGO_TARGET_DIR == /* && $OMUSE_RUNTIME_DIR == /* ]] || fail 'Build and runtime cache overrides must be absolute paths.'
    step 'Build Omuse — Cargo shows progress below'
    bash "$source_dir/scripts/build-rust.sh"
    if ((assets)); then
        step 'Prepare checksum-verified Camera RAW and subject-selection assets'
        local assets_log=${log%.log}-assets.log
        printf 'Runtime setup log: %s\n' "$assets_log"
        if ! bash "$source_dir/scripts/prepare-rust-assets.sh" >"$assets_log" 2>&1; then
            tail -n 20 "$assets_log" >&2
            fail "Runtime setup failed. Details: $assets_log"
        fi
    else
        # An explicit core-only install must not accidentally pick up old assets.
        export OMUSE_RUNTIME_DIR=$work/no-runtime
    fi
    step 'Verify and install Omuse'
    OMUSE_INSTALL_PREFIX=$prefix bash "$source_dir/scripts/install-rust.sh" "$CARGO_TARGET_DIR/release/omuse"
    printf '\nOpen Omuse from your application launcher.\n'
    printf 'Terminal: %s/bin/omuse\nUpdate: rerun the same curl command.\n' "$prefix"
    printf 'Rollback: %s/bin/omuse-manage rollback\nUninstall: %s/bin/omuse-manage uninstall\n' "$prefix" "$prefix"
    printf 'Your projects and settings are kept when uninstalling.\n'
    rm -rf -- "$work"
    trap - EXIT ERR INT TERM
}

main "$@"
