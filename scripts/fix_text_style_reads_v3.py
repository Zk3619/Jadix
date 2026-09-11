#!/usr/bin/env python3
"""Replace remaining TextStyle value reads with *_resolved() (skip LHS assignments)."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates" / "rui" / "src"

FIELDS = [
    "color",
    "size",
    "weight",
    "align",
    "underline",
    "overline",
    "line_through",
    "decoration_style",
    "decoration_thickness",
    "underline_offset",
]

VARS = [r"text_style", r"st", r"base_text_style", r"ts", r"t\.text_style"]


def fix_content(content: str) -> str:
    for var in VARS:
        for field in FIELDS:
            pat = re.compile(rf"\b{var}\.{field}\b(?!_resolved)(?!\s*=)")
            content = pat.sub(f"{var}.{field}_resolved()", content)
    content = content.replace("_resolved()_resolved()", "_resolved()")
    # comparisons like color == Some(x) in tests should use .color not resolved
    return content


def main() -> None:
    changed = 0
    for path in ROOT.rglob("*.rs"):
        if path.relative_to(ROOT).as_posix() == "style/text_style.rs":
            continue
        text = path.read_text(encoding="utf-8")
        new = fix_content(text)
        if new != text:
            path.write_text(new, encoding="utf-8")
            changed += 1
            print(path.relative_to(ROOT))
    print(f"updated {changed} files")


if __name__ == "__main__":
    main()
