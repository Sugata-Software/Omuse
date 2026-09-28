#!/bin/sh
# Verify and install an Omuse bundle without sudo or network access.
set -eu

usage() {
    cat <<'EOF'
Usage: install-rust-bundle.sh ARCHIVE [--prefix DIR]

The default prefix is $HOME/.local. Use --prefix for an isolated test install.
Updates retain the previous complete Omuse installation for rollback.
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
    licenses/rust-dependency-inventory.json licenses/Rust-THIRD-PARTY-NOTICES.txt install-app.py
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

# Reuse the same verified generation install/rollback path as source installs.
payload=$work/payload
mkdir -p "$payload/icons"
install -m 755 "$bundle/bin/omuse" "$payload/omuse"
cp -R "$bundle/lib" "$bundle/models" "$bundle/licenses" "$payload/"
cp "$bundle/share/icons/omuse.png" "$bundle/share/icons/omuse.svg" "$payload/icons/"
cp "$bundle/SOURCE-REVISION" "$payload/SOURCE-REVISION"
revision=$(sed -n 's/^source_revision=//p' "$bundle/SOURCE-REVISION")
python3 "$bundle/install-app.py" install --prefix "$prefix" --payload "$payload" --revision "$revision"
