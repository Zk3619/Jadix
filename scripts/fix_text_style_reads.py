#!/usr/bin/env python3
"""One-shot migration helper: use TextStyle *_resolved() for common read patterns."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates" / "rui" / "src"

SKIP = {
    "style/text_style.rs",
    "core/tree.rs",
}

# Order matters: longer patterns first.
READ_REPLACEMENTS = [
    (r"\.text_style\.color\[", ".text_style.color_resolved()["),
    (r"\.text_style\.color\b", ".text_style.color_resolved()"),
    (r"\bst\.color\[", "st.color_resolved()["),
    (r"\bst\.color\b", "st.color_resolved()"),
    (r"\.text_style\.size\b", ".text_style.size_resolved()"),
    (r"\bst\.size\b", "st.size_resolved()"),
    (r"\.text_style\.weight\b", ".text_style.weight_resolved()"),
    (r"\bst\.weight\b", "st.weight_resolved()"),
    (r"\.text_style\.align\b", ".text_style.align_resolved()"),
    (r"\bst\.align\b", "st.align_resolved()"),
    (r"\.text_style\.underline\b", ".text_style.underline_resolved()"),
    (r"\bst\.underline\b", "st.underline_resolved()"),
    (r"\.text_style\.overline\b", ".text_style.overline_resolved()"),
    (r"\bst\.overline\b", "st.overline_resolved()"),
    (r"\.text_style\.line_through\b", ".text_style.line_through_resolved()"),
    (r"\bst\.line_through\b", "st.line_through_resolved()"),
    (r"\.text_style\.decoration_style\b", ".text_style.decoration_style_resolved()"),
    (r"\bst\.decoration_style\b", "st.decoration_style_resolved()"),
    (r"\.text_style\.decoration_thickness\b", ".text_style.decoration_thickness_resolved()"),
    (r"\bst\.decoration_thickness\b", "st.decoration_thickness_resolved()"),
    (r"\.text_style\.underline_offset\b", ".text_style.underline_offset_resolved()"),
    (r"\bst\.underline_offset\b", "st.underline_offset_resolved()"),
]

# Assignments: color = X -> color = Some(X) when not already Some(
ASSIGN_PATTERNS = [
    (
        re.compile(r"(\.text_style\.color)\s*=\s*(\[[^\]]+\]|parse_text_color\([^)]+\)|fg|want_fg|want|custom|c|color)"),
        r"\1 = Some(\2)",
    ),
    (
        re.compile(r"(\.text_style\.size)\s*=\s*([0-9.]+)"),
        r"\1 = Some(\2)",
    ),
    (
        re.compile(r"(\.text_style\.weight)\s*=\s*([0-9]+)"),
        r"\1 = Some(\2)",
    ),
]


def should_skip(path: Path) -> bool:
    rel = path.relative_to(ROOT).as_posix()
    return rel in SKIP


def fix_reads(content: str) -> str:
    for pat, repl in READ_REPLACEMENTS:
        # avoid double _resolved()
        content = re.sub(pat + r"_resolved\(\)", repl, content)
        content = re.sub(pat + r"(?!_resolved\()", repl, content)
    return content


def fix_assignments(content: str) -> str:
    for pat, repl in ASSIGN_PATTERNS:
        content = pat.sub(repl, content)
    # undo Some(Some(
    content = content.replace("= Some(Some(", "= Some(")
    return content


def process_file(path: Path) -> bool:
    if should_skip(path):
        return False
    text = path.read_text(encoding="utf-8")
    new = fix_assignments(fix_reads(text))
    if new != text:
        path.write_text(new, encoding="utf-8")
        return True
    return False


def main() -> int:
    changed = 0
    for path in ROOT.rglob("*.rs"):
        if process_file(path):
            changed += 1
            print(path.relative_to(ROOT))
    print(f"updated {changed} files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
