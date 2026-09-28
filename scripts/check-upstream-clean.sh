#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MANIFEST="$ROOT/linux/upstream-parity.json"
REF=${UPSTREAM_REF:-upstream/main}

if [ ! -f "$MANIFEST" ]; then
  echo "missing parity manifest: $MANIFEST" >&2
  exit 2
fi

if ! git -C "$ROOT" rev-parse --verify "$REF^{commit}" >/dev/null 2>&1; then
  echo "cannot resolve upstream reference: $REF" >&2
  exit 2
fi

set --
for root in Compositor CompositorTests; do
  while IFS= read -r path; do
    [ -n "$path" ] && set -- "$@" "$path"
  done <<EOF
$(git -C "$ROOT" diff --name-only "$REF" -- "$root")
$(git -C "$ROOT" ls-files --others --exclude-standard -- "$root")
EOF
done

if [ "$#" -eq 0 ]; then
  echo "UPSTREAM CLEAN: protected trees match $REF"
  exit 0
fi

echo "UPSTREAM DRIFT: protected trees differ from $REF" >&2
printf '%s\n' "$@" | sort -u >&2
echo "Move Linux-only changes behind Sources/Compat, Sources/Overrides, Sources/UpstreamCore/LinuxSupport, or host adapters." >&2
exit 1
