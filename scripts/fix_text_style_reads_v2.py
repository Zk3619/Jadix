#!/usr/bin/env python3
"""Migrate TextStyle field reads to *_resolved(); skip assignment LHS."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates" / "jadix" / "src"

SKIP = {
    "style/text_style.rs",
}

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

PREFIXES = [r"\.text_style", r"\bst", r"\.base_text_style"]


def fix_field_reads(content: str, field: str) -> str:
    resolved = f"{field}_resolved()"
    for prefix in PREFIXES:
        # reads: prefix.field[ or prefix.field) etc, not followed by _resolved or =
        pat = re.compile(
            rf"({prefix})\.{field}\b(?!_resolved)(?!\s*=)"
        )
        content = pat.sub(rf"\1.{resolved}", content)
    return content


def fix_opacity_color_mutations(content: str) -> str:
    old = """            if opacity < 1.0 - f32::EPSILON {
                let a = ((f32::from(text_style.color[3]) * opacity).round() as u16).min(255) as u8;
                text_style.color[3] = a;
            }"""
    new = """            if opacity < 1.0 - f32::EPSILON {
                let mut c = text_style.color_resolved();
                let a = ((f32::from(c[3]) * opacity).round() as u16).min(255) as u8;
                c[3] = a;
                text_style.color = Some(c);
            }"""
    return content.replace(old, new)


def process_file(path: Path) -> bool:
    rel = path.relative_to(ROOT).as_posix()
    if rel in SKIP:
        return False
    text = path.read_text(encoding="utf-8")
    new = text
    for field in FIELDS:
        new = fix_field_reads(new, field)
    new = fix_opacity_color_mutations(new)
    # undo mistaken resolved on assignments
    for field in FIELDS:
        new = re.sub(
            rf"(\.text_style\.{field})_resolved\(\)\s*=",
            rf"\1 =",
            new,
        )
        new = re.sub(
            rf"(\bst\.{field})_resolved\(\)\s*=",
            rf"\1 =",
            new,
        )
    new = new.replace("_resolved()_resolved()", "_resolved()")
    if new != text:
        path.write_text(new, encoding="utf-8")
        return True
    return False


def main() -> None:
    changed = 0
    for path in ROOT.rglob("*.rs"):
        if process_file(path):
            changed += 1
            print(path.relative_to(ROOT))
    print(f"updated {changed} files")


if __name__ == "__main__":
    main()
