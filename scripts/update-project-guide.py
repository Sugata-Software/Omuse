#!/usr/bin/env python3
"""Refresh the static project guide from its reviewed status catalogue."""

import argparse
import json
from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "docs/project-guide.fragment.html"
DATA = ROOT / "docs/project-status.json"
BLOCK = re.compile(r'(<script type="application/json" id="omuse-guide-data">)\s*.*?\s*(</script>)', re.S)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=TEMPLATE,
                        help="HTML fragment destination; defaults to the tracked guide")
    parser.add_argument("--check", action="store_true", help="Fail if the destination is stale")
    args = parser.parse_args()
    source = TEMPLATE.read_text()
    data = json.loads(DATA.read_text())
    # Keep the embedded data inert even if future copy contains HTML characters.
    payload = json.dumps(data, indent=2, ensure_ascii=False).replace("<", "\\u003c")
    rendered, count = BLOCK.subn(lambda match: match[1] + "\n" + payload + "\n  " + match[2], source)
    if count != 1:
        parser.error("The guide must contain exactly one status data block")
    if args.check:
        if not args.output.is_file() or args.output.read_text() != rendered:
            parser.exit(1, "The project guide is stale; refresh it with this script.\n")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered)
    print(args.output)


if __name__ == "__main__":
    main()
