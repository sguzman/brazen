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

import argparse
import glob
import hashlib
import os
import re
import tomllib
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

def load_existing_items(path: str) -> dict[str, dict]:
    try:
        with open(path, "rb") as f:
            data = tomllib.load(f)
    except FileNotFoundError:
        return {}
    items = data.get("item", [])
    out: dict[str, dict] = {}
    for it in items:
        it_id = it.get("id")
        if isinstance(it_id, str):
            out[it_id] = dict(it)
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="Verify docs/roadmaps/coverage.toml is up-to-date without writing it.",
    )
    args = parser.parse_args()

    out_path = "docs/roadmaps/coverage.toml"
    checklist = iter_checklist_items()
    existing = load_existing_items(out_path)

    merged: list[dict] = []
    for it in checklist:
        it_id = it.stable_id
        prior = existing.get(it_id, {})
        merged.append(
            {
                "id": it_id,
                "file": it.file,
                "line": it.line,
                "text": it.text,
                "status": prior.get("status", "planned"),
                "tests": prior.get("tests", []),
                "planned_tests": prior.get(
                    "planned_tests", [f"todo::{it_id.split(':')[-1]}"]
                ),
            }
        )

    existing_ids = set(existing.keys())
    wanted_ids = {it.stable_id for it in checklist}
    extra_ids = sorted(existing_ids - wanted_ids)
    missing_ids = sorted(wanted_ids - existing_ids)

    lines: list[str] = []
    lines.append("# Auto-generated. Each roadmap checkbox must have an entry here.")
    lines.append("# status: planned | done")
    for it in merged:
        lines.append("")
        lines.append("[[item]]")
        lines.append(f'id = "{toml_escape(it["id"])}"')
        lines.append(f'file = "{toml_escape(it["file"])}"')
        lines.append(f'line = {int(it["line"])}')
        lines.append(f'text = "{toml_escape(it["text"])}"')
        status = it.get("status", "planned")
        if status not in ("planned", "done"):
            status = "planned"
        lines.append(f'status = "{status}"')
        tests = it.get("tests", [])
        if not isinstance(tests, list):
            tests = []
        lines.append("tests = [" + ", ".join(f'"{toml_escape(str(t))}"' for t in tests) + "]")
        planned = it.get("planned_tests", [])
        if not isinstance(planned, list) or not planned:
            planned = [f"todo::{it['id'].split(':')[-1]}"]
        lines.append(
            "planned_tests = ["
            + ", ".join(f'"{toml_escape(str(t))}"' for t in planned)
            + "]"
        )

    rendered = "\n".join(lines) + "\n"
    if args.check:
        problems: list[str] = []
        if missing_ids:
            problems.append(f"missing {len(missing_ids)} items (run generator)")
        if extra_ids:
            problems.append(f"stale {len(extra_ids)} items (run generator)")
        if problems:
            raise SystemExit(f"{out_path} is out of date: " + "; ".join(problems))
        print(f"{out_path} covers all checkboxes ({len(checklist)} items).")
        return

    with open(out_path, "w", encoding="utf-8") as f:
        f.write(rendered)
    suffix = ""
    if extra_ids:
        suffix += f" (dropped {len(extra_ids)} stale items)"
    if missing_ids:
        suffix += f" (added {len(missing_ids)} new items)"
    print(f"Wrote {len(checklist)} items to {out_path}{suffix}")


if __name__ == "__main__":
    main()
