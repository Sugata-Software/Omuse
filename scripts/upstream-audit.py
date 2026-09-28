#!/usr/bin/env python3
"""Generate an explicit upstream symbol inventory.

This is intentionally a conservative inventory: it records declarations and
candidate Linux name matches, but never calls a name match proof of parity.
The JSON report is the input to the later workflow and test-evidence stages.
"""
from __future__ import annotations

import argparse
import csv
import json
import re
import subprocess
import tempfile
from pathlib import Path

DECL = re.compile(
    r"^\s*(?:@[A-Za-z_][A-Za-z0-9_]*(?:\([^\n]*\))?\s+)*"
    r"(?:(?:public|internal|private|fileprivate|open|final|static|class|nonisolated|override|mutating|isolated)\s+)*"
    r"(?P<kind>func|init|struct|class|enum|protocol|actor|var|let)\b\s*(?P<name>[A-Za-z_][A-Za-z0-9_]*)?"
)

def area(path: Path) -> str:
    parts = path.parts
    if "UI" in parts:
        return "ui"
    if "Document" in parts:
        return "data_logic"
    if "IO" in parts or "Rendering" in parts:
        return "data_logic"
    return "other"

def declarations(root: Path):
    for path in sorted(root.rglob("*.swift")):
        rel = path.relative_to(root)
        depth = 0
        for lineno, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
            # Members and top-level declarations are useful inventory items;
            # locals inside function bodies are implementation details. This
            # lightweight depth filter avoids reporting thousands of locals
            # without pretending to be a full Swift parser.
            declaration_depth = depth
            match = DECL.match(line)
            if match and declaration_depth <= 1:
                name = match.group("name")
                # `var`/`let` declarations without a name are not symbols.
                if name:
                    yield {
                        "area": area(rel),
                        "kind": match.group("kind"),
                        "name": name,
                        "upstream_file": str(rel),
                        "line": lineno,
                    }
            # This is only a lexical approximation; strings and multiline
            # comments can affect it. The report explicitly remains a candidate
            # inventory and must be checked against compiler/API evidence.
            depth += line.count("{") - line.count("}")
            depth = max(0, depth)

def linux_names(roots):
    names = set()
    for root in roots:
        if not root.exists():
            continue
        for path in root.rglob("*.swift"):
            text = path.read_text(errors="replace")
            names.update(re.findall(r"\b(?:func|struct|class|enum|protocol|actor)\s+([A-Za-z_][A-Za-z0-9_]*)", text))
    return names

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--ref", default="upstream/main", help="git ref containing the upstream Compositor tree")
    parser.add_argument("--json", type=Path)
    parser.add_argument("--csv", type=Path)
    args = parser.parse_args()

    # Read the pinned upstream tree, not the working copy. This makes local
    # Linux edits visible to the source-integrity check without contaminating
    # the reference inventory.
    with tempfile.TemporaryDirectory(prefix="compositor-upstream-") as temp:
        archive = subprocess.run(
            ["git", "-C", str(args.root), "archive", args.ref, "Compositor"],
            check=True, stdout=subprocess.PIPE,
        )
        import tarfile
        archive_path = Path(temp) / "upstream.tar"
        archive_path.write_bytes(archive.stdout)
        with tarfile.open(archive_path) as tar:
            tar.extractall(temp, filter="data")
        upstream = Path(temp) / "Compositor"
        report_reference = args.ref

        linux_roots = [args.root / name for name in ("Sources", "host", "linux", "backends")]
        linux = linux_names(linux_roots)
        rows = []
        for row in declarations(upstream):
            row["candidate_linux_match"] = row["name"] in linux
            row["status"] = "candidate_match" if row["candidate_linux_match"] else "unmatched"
            rows.append(row)

    report = {
        "reference": report_reference,
        "method": "syntax-shaped declaration extraction plus conservative name candidates",
        "warning": "candidate_match is not functional parity; every row needs mapping and test evidence",
        "totals": {
            "all": len(rows),
            "ui": sum(r["area"] == "ui" for r in rows),
            "data_logic": sum(r["area"] == "data_logic" for r in rows),
            "candidate_matches": sum(r["candidate_linux_match"] for r in rows),
            "unmatched": sum(not r["candidate_linux_match"] for r in rows),
        },
        "symbols": rows,
    }
    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(encoded)
    else:
        print(encoded, end="")
    if args.csv:
        args.csv.parent.mkdir(parents=True, exist_ok=True)
        with args.csv.open("w", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=["area", "kind", "name", "upstream_file", "line", "candidate_linux_match", "status"])
            writer.writeheader()
            writer.writerows(rows)

if __name__ == "__main__":
    main()
