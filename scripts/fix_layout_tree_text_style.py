#!/usr/bin/env python3
"""Fix layout_tree.rs TextStyle test locals and remaining style.* reads."""

from __future__ import annotations

import re
from pathlib import Path

p = Path(__file__).resolve().parents[1] / "crates/jadix/src/layout/layout_tree.rs"
text = p.read_text(encoding="utf-8")

# Reads in push_input_chrome (TextStyle param named `style`)
read_repls = [
    (r"style\.size_resolved\(\)", "style.size_resolved()"),  # idempotent anchor
    (r"(?<![.\w])style\.size\b(?!_resolved)(?!\s*=)", "style.size_resolved()"),
    (r"(?<![.\w])style\.weight\b(?!_resolved)(?!\s*=)", "style.weight_resolved()"),
    (r"let c = style\.color\b", "let c = style.color_resolved()"),
]
for pat, repl in read_repls:
    text = re.sub(pat, repl, text)

# Test / local TextStyle assignments (not element.style)
assign_repls = [
    (r"(\n        style\.size) = ([0-9.]+);", r"\1 = Some(\2);"),
    (r"(\n        style\.weight) = ([0-9]+);", r"\1 = Some(\2);"),
    (r"(\n        style\.color) = (\[[^\]]+\]);", r"\1 = Some(\2);"),
    (r"(\n        style\.align) = (crate::style::TextAlign::[A-Za-z]+);", r"\1 = Some(\2);"),
    (r"(\n        style\.size) = (size);", r"\1 = Some(\2);"),
]
for pat, repl in assign_repls:
    text = re.sub(pat, repl, text)

text = text.replace("_resolved()_resolved()", "_resolved()")
p.write_text(text, encoding="utf-8")
print("fixed layout_tree.rs")
