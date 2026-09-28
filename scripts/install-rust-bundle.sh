#!/bin/sh
# Verify and install an Omuse bundle without sudo or network access.
set -eu

usage() {
    cat <<'EOF'
Usage: install-rust-bundle.sh ARCHIVE [--prefix DIR]

The default prefix is $HOME/.local. Use --prefix for an isolated test install.
Omuse is installed separately from the older Compositor editions.
EOF
}

[ "$#" -ge 1 ] || { usage >&2; exit 2; }
archive=$1
shift
prefix=${OMUSE_INSTALL_PREFIX:-${COMPOSITOR_INSTALL_PREFIX:-$HOME/.local}}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; prefix=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done
[ -f "$archive" ] || { printf 'Bundle not found: %s\n' "$archive" >&2; exit 1; }
case "$prefix" in /*) ;; *) printf '%s\n' '--prefix must be an absolute path.' >&2; exit 2 ;; esac
case "$prefix" in
    *"'"*|*\\*) printf "%s\n" "--prefix cannot contain a single quote or backslash because Desktop Exec cannot represent it portably." >&2; exit 2 ;;
esac
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ] || {
    printf '%s\n' 'This bundle installer supports Linux x86_64 only.' >&2
    exit 1
}
command -v python3 >/dev/null 2>&1 && command -v sha256sum >/dev/null 2>&1 || {
    printf '%s\n' 'python3 and sha256sum are required.' >&2; exit 1;
}

work=$(mktemp -d "${TMPDIR:-/tmp}/omuse-install.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT HUP INT TERM
# Validate the complete table before extracting anything. tarfile.extract is
# intentionally not used: only preflighted directories and regular files are
# materialized, so links and device entries cannot redirect a later write.
python3 - "$archive" "$work" "$prefix" <<'PY'
import os
import pathlib
import shutil
import sys
import tarfile

archive, destination, prefix = sys.argv[1:]
for label, value in (("archive path", archive), ("prefix", prefix)):
    if any(ord(char) < 32 or ord(char) == 127 for char in value):
        raise SystemExit(f"{label} contains a control character")
if os.path.getsize(archive) > 1_073_741_824:
    raise SystemExit("bundle archive exceeds the 1 GiB compressed-size limit")

with tarfile.open(archive, "r:gz") as source:
    members = []
    expanded = 0
    names = set()
    for count, member in enumerate(source, 1):
        if count > 1_000:
            raise SystemExit("bundle archive has too many members")
        members.append(member)
        name = member.name
        if any(ord(char) < 32 or ord(char) == 127 for char in name):
            raise SystemExit("bundle member name contains a control character")
        path = pathlib.PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts:
            raise SystemExit(f"unsafe bundle member: {name}")
        if not path.parts or path.parts[0] != "omuse-bundle":
            raise SystemExit(f"unexpected bundle member: {name}")
        if name in names:
            raise SystemExit(f"duplicate bundle member: {name}")
        names.add(name)
        if not (member.isdir() or member.isfile()):
            raise SystemExit(f"links and special bundle members are forbidden: {name}")
        if member.isfile():
            expanded += member.size
            if expanded > 2_147_483_648:
                raise SystemExit("bundle expands beyond the 2 GiB limit")

    root = pathlib.Path(destination).resolve()
    for member in members:
        output = root.joinpath(*pathlib.PurePosixPath(member.name).parts)
        if member.isdir():
            output.mkdir(parents=True, exist_ok=True)
            continue
        output.parent.mkdir(parents=True, exist_ok=True)
        payload = source.extractfile(member)
        if payload is None:
            raise SystemExit(f"cannot read bundle member: {member.name}")
        with output.open("xb") as target:
            shutil.copyfileobj(payload, target, length=1024 * 1024)
PY
bundle=$work/omuse-bundle
[ -d "$bundle" ] || { printf '%s\n' 'Bundle root is missing.' >&2; exit 1; }
if find "$bundle" ! -type d ! -type f -print | grep -q .; then
    printf '%s\n' 'Bundle contains unsupported links or special files.' >&2
    exit 1
fi
for required in SHA256SUMS SOURCE-REVISION bin/omuse lib/libonnxruntime.so \
    lib/libraw.so models/u2netp.onnx share/icons/omuse.png share/icons/omuse.svg \
    licenses/rust-dependency-inventory.json licenses/Rust-THIRD-PARTY-NOTICES.txt
do
    [ -f "$bundle/$required" ] || { printf 'Bundle member is missing: %s\n' "$required" >&2; exit 1; }
done
(
    cd "$bundle"
    # Every regular payload file must be named by the manifest; an attacker
    # cannot add an unchecked file that is later copied from licenses/.
    LC_ALL=C find . -type f ! -name SHA256SUMS -print | LC_ALL=C sort > "$work/actual-files"
    sed -n 's/^[0-9a-f]\{64\}  //p' SHA256SUMS | LC_ALL=C sort > "$work/manifest-files"
    cmp "$work/actual-files" "$work/manifest-files" >/dev/null || {
        printf '%s\n' 'Checksum manifest does not cover exactly the bundle payload.' >&2
        exit 1
    }
    sha256sum --check --strict SHA256SUMS
)
grep -qx 'target=linux-x86_64' "$bundle/SOURCE-REVISION" || {
    printf '%s\n' 'Bundle target is not linux-x86_64.' >&2
    exit 1
}

app_dir=$prefix/opt/omuse
legacy_app_dir=$prefix/opt/compositor-rust
bin_dir=$prefix/bin
applications_dir=$prefix/share/applications
icon_dir=$prefix/share/icons/hicolor/256x256/apps
scalable_icon_dir=$prefix/share/icons/hicolor/scalable/apps
mkdir -p "$app_dir" "$bin_dir" "$applications_dir" "$icon_dir" "$scalable_icon_dir"

# Install support files only after every bundle member has passed verification.
for directory in lib models licenses; do
    mkdir -p "$app_dir/$directory"
    for source in "$bundle/$directory"/*; do
        [ -f "$source" ] || continue
        name=$(basename -- "$source")
        mode=644
        [ "$directory" = lib ] && mode=755
        install -m "$mode" "$source" "$app_dir/$directory/$name.new"
        mv -f -- "$app_dir/$directory/$name.new" "$app_dir/$directory/$name"
    done
done
install -m 644 "$bundle/SOURCE-REVISION" "$app_dir/SOURCE-REVISION.new"
mv -f -- "$app_dir/SOURCE-REVISION.new" "$app_dir/SOURCE-REVISION"

install -m 755 "$bundle/bin/omuse" "$app_dir/omuse.new"
if [ -f "$app_dir/omuse" ]; then
    cp -p -- "$app_dir/omuse" "$app_dir/omuse.previous.new"
    mv -f -- "$app_dir/omuse.previous.new" "$app_dir/omuse.previous"
elif [ ! -f "$app_dir/omuse.previous" ] && [ -f "$legacy_app_dir/compositor-rust" ]; then
    cp -p -- "$legacy_app_dir/compositor-rust" "$app_dir/omuse.previous.new"
    mv -f -- "$app_dir/omuse.previous.new" "$app_dir/omuse.previous"
fi
mv -f -- "$app_dir/omuse.new" "$app_dir/omuse"

install -m 644 "$bundle/share/icons/omuse.png" "$icon_dir/omuse.png.new"
mv -f -- "$icon_dir/omuse.png.new" "$icon_dir/omuse.png"
install -m 644 "$bundle/share/icons/omuse.svg" "$scalable_icon_dir/omuse.svg.new"
mv -f -- "$scalable_icon_dir/omuse.svg.new" "$scalable_icon_dir/omuse.svg"
python3 - "$app_dir/omuse" "$bin_dir/omuse.new" "$bin_dir/omuse" \
    "$bin_dir/compositor-rust.new" "$applications_dir/omuse.desktop.new" <<'PY'
import pathlib
import shlex
import sys

executable, launcher_new, launcher, compatibility_new, desktop_new = sys.argv[1:]
pathlib.Path(launcher_new).write_text(
    "#!/bin/sh\nset -eu\nexec " + shlex.quote(executable) + ' "$@"\n',
    encoding="utf-8",
)
pathlib.Path(compatibility_new).write_text(
    "#!/bin/sh\nset -eu\nexec " + shlex.quote(launcher) + ' "$@"\n',
    encoding="utf-8",
)
# Desktop Exec is parsed by the desktop-entry grammar, not a shell. Within a
# quoted argument these five characters require backslash escaping.
escaped = launcher.replace("\\", "\\\\")
for character in ('"', '`', '$'):
    escaped = escaped.replace(character, "\\" + character)
escaped = escaped.replace("%", "%%")
pathlib.Path(desktop_new).write_text(
    "[Desktop Entry]\n"
    "Type=Application\n"
    "Name=Omuse\n"
    "Comment=Native image editor\n"
    f'Exec="{escaped}" %f\n'
    "Icon=omuse\n"
    "Terminal=false\n"
    "Categories=Graphics;2DGraphics;RasterGraphics;\n"
    "StartupNotify=true\n"
    "StartupWMClass=omuse\n",
    encoding="utf-8",
)
PY
chmod 755 "$bin_dir/omuse.new" "$bin_dir/compositor-rust.new"
mv -f -- "$bin_dir/omuse.new" "$bin_dir/omuse"
mv -f -- "$bin_dir/compositor-rust.new" "$bin_dir/compositor-rust"
mv -f -- "$applications_dir/omuse.desktop.new" "$applications_dir/omuse.desktop"

legacy_desktop="$applications_dir/compositor-rust.desktop"
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

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$applications_dir/omuse.desktop"
fi
printf 'Installed Omuse: %s\n' "$bin_dir/omuse"
printf 'Compatibility launcher: %s\n' "$bin_dir/compositor-rust"
printf 'Previous executable, when present: %s\n' "$app_dir/omuse.previous"
