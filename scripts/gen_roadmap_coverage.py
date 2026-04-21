#!/usr/bin/env python3
"""
Generate docs/roadmaps/coverage.toml from docs/roadmaps/*.md checklists.

Policy:
- Every checklist item gets a stable id based on (file, line, text).
- New items default to status="planned" with a placeholder planned_tests entry.
- This script does NOT guess real test mappings; checked [x] items must be
  manually updated in coverage.toml to status="done" with tests=[...].
"""

from __future__ import annotations

import glob
import hashlib
import os
import re
from dataclasses import dataclass


@dataclass(frozen=True)
class ChecklistItem:
    file: str
    line: int
    text: str

    @property
    def stable_id(self) -> str:
        h = hashlib.sha1(f"{self.file}::{self.line}::{self.text}".encode("utf-8")).hexdigest()[:10]
        return f"{self.file}:{self.line}:{h}"


def iter_checklist_items() -> list[ChecklistItem]:
    out: list[ChecklistItem] = []
    for path in sorted(glob.glob("docs/roadmaps/*.md")):
        file = os.path.basename(path)
        with open(path, "r", encoding="utf-8") as f:
            for idx, line in enumerate(f, start=1):
                m = re.match(r"^\s*-\s*\[( |x|X)\]\s+(.+?)\s*$", line)
                if not m:
                    continue
                out.append(ChecklistItem(file=file, line=idx, text=m.group(2)))
    return out


def toml_escape(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


def main() -> None:
    items = iter_checklist_items()
    lines: list[str] = []
    lines.append("# Auto-generated. Each roadmap checkbox must have an entry here.")
    lines.append("# status: planned | done")
    for it in items:
        lines.append("")
        lines.append("[[item]]")
        lines.append(f'id = "{toml_escape(it.stable_id)}"')
        lines.append(f'file = "{toml_escape(it.file)}"')
        lines.append(f"line = {it.line}")
        lines.append(f'text = "{toml_escape(it.text)}"')
        lines.append('status = "planned"')
        lines.append("tests = []")
        lines.append(f'planned_tests = ["todo::{it.stable_id.split(":")[-1]}"]')
    with open("docs/roadmaps/coverage.toml", "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"Wrote {len(items)} items to docs/roadmaps/coverage.toml")


if __name__ == "__main__":
    main()

