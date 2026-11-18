#!/usr/bin/env python3
"""changelog.py VERSION [CHANGELOG]: VERSION's section of client/CHANGELOG.md as the launcher manifest's `changelog`,
JSON on stdout: [{"kind": "Fixed", "text": "...", "important": true}, ...]. A version with no section prints [].

The section is `## VERSION`, its kinds `### New` / `### Changed` / `### Fixed` / `### Removed`, one `- ` line per change
(indented lines continue it). A change written in bold (`- **...**`) is important. Markdown links keep their text;
backticks and emphasis are dropped, since the launcher draws plain text."""
import json
import os
import re
import sys


def parse(text: str, version: str) -> list[dict]:
    changes, kind, inside = [], "", False
    for raw in text.splitlines():
        line = raw.rstrip()
        if line.startswith("## "):
            inside = line[3:].strip().lstrip("v") == version.lstrip("v")
            kind = ""
            continue
        if not inside:
            continue
        if line.startswith("### "):
            kind = line[4:].strip()
        elif line.lstrip().startswith(("- ", "* ")) and not raw[:1].isspace():
            changes.append({"kind": kind, "text": line.lstrip()[2:].strip()})
        elif raw[:1].isspace() and line.strip() and changes:
            changes[-1]["text"] += " " + line.strip()
    out = []
    for c in changes:
        t = c["text"]
        important = t.startswith("**") and t.endswith("**") and len(t) > 4
        t = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", t)
        t = re.sub(r"(\*\*|__|`)", "", t).strip()
        if t:
            out.append({"kind": c["kind"], "text": t, **({"important": True} if important else {})})
    return out


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    path = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(__file__), "..", "CHANGELOG.md")
    text = open(path, encoding="utf-8").read() if os.path.exists(path) else ""
    print(json.dumps(parse(text, sys.argv[1]), ensure_ascii=False))
