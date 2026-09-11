#!/usr/bin/env python3
"""Fix mistaken assignment rewrites from fix_text_style_reads.py"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates" / "rui" / "src"

ASSIGN_FIXES = [
    (r"\.text_style\.color_resolved\(\)\s*=", ".text_style.color ="),
    (r"\.text_style\.size_resolved\(\)\s*=", ".text_style.size ="),
    (r"\.text_style\.weight_resolved\(\)\s*=", ".text_style.weight ="),
    (r"\.text_style\.align_resolved\(\)\s*=", ".text_style.align ="),
    (r"text_el\.text_style\.color_resolved\(\)\s*=", "text_el.text_style.color ="),
    (r"text_el\.text_style\.align_resolved\(\)\s*=", "text_el.text_style.align ="),
    (r"label\.text_style\.color_resolved\(\)\s*=", "label.text_style.color ="),
    (r"child\.text_style\.color_resolved\(\)\s*=", "child.text_style.color ="),
    (r"child\.text_style\.size_resolved\(\)\s*=", "child.text_style.size ="),
    (r"child\.text_style\.weight_resolved\(\)\s*=", "child.text_style.weight ="),
    (r"indicator\.text_style\.size_resolved\(\)\s*=", "indicator.text_style.size ="),
    (r"indicator\.text_style\.color_resolved\(\)\s*=", "indicator.text_style.color ="),
    (r"indicator\.text_style\.align_resolved\(\)\s*=", "indicator.text_style.align ="),
    (r"self\.text_style\.color_resolved\(\)\s*=", "self.text_style.color ="),
    (r"self\.text_style\.weight_resolved\(\)\s*=", "self.text_style.weight ="),
    (r"t\.text_style\.size_resolved\(\)\s*=", "t.text_style.size ="),
    (r"t\.text_style\.align_resolved\(\)\s*=", "t.text_style.align ="),
    (r"el\.text_style\.color_resolved\(\)\s*=", "el.text_style.color ="),
    (r"el\.text_style\.size_resolved\(\)\s*=", "el.text_style.size ="),
    (r"el\.text_style\.weight_resolved\(\)\s*=", "el.text_style.weight ="),
    (r"el\.text_style\.align_resolved\(\)\s*=", "el.text_style.align ="),
    (r"target\.text_style\.color_resolved\(\)\s*=", "target.text_style.color ="),
    (r"target\.text_style\.size_resolved\(\)\s*=", "target.text_style.size ="),
    (r"target\.text_style\.weight_resolved\(\)\s*=", "target.text_style.weight ="),
    (r"k\.text_style\.color_resolved\(\)\s*=", "k.text_style.color ="),
    (r"c\.text_style\.size_resolved\(\)\s*=", "c.text_style.size ="),
]

# After fixing assignments, wrap bare values with Some() for Option fields.
# Lines already containing Some( are left alone.
VALUE_WRAP = [
    (re.compile(r"(\.text_style\.color)\s*=\s*(?!Some\()([^;\n]+);"), r"\1 = Some(\2);"),
    (re.compile(r"(\.text_style\.size)\s*=\s*(?!Some\()([0-9.]+);"), r"\1 = Some(\2);"),
    (re.compile(r"(\.text_style\.weight)\s*=\s*(?!Some\()([0-9]+);"), r"\1 = Some(\2);"),
    (re.compile(r"(\.text_style\.align)\s*=\s*(?!Some\()(crate::style::TextAlign::[A-Za-z]+);"), r"\1 = Some(\2);"),
    (re.compile(r"(\.text_style\.align)\s*=\s*(?!Some\()(want_align);"), r"\1 = Some(\2);"),
]


def process(content: str) -> str:
    for pat, repl in ASSIGN_FIXES:
        content = re.sub(pat, repl, content)
    for pat, repl in VALUE_WRAP:
        content = pat.sub(repl, content)
    content = content.replace("= Some(Some(", "= Some(")
    return content


def main() -> None:
    for path in ROOT.rglob("*.rs"):
        text = path.read_text(encoding="utf-8")
        new = process(text)
        if new != text:
            path.write_text(new, encoding="utf-8")
            print(path.relative_to(ROOT))


if __name__ == "__main__":
    main()
