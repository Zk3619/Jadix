#!/usr/bin/env python3
"""Fix renderer TextStyle reads/assigns in selected files."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates/jadix/src"

FILES = [
    "renderer/text_hit.rs",
    "renderer/effect_pass.rs",
    "renderer/text_prepare_debug.rs",
    "text/visual_lines.rs",
    "text/measure.rs",
    "style/tailwind_leading.rs",
    "style/utility.rs",
    "components/button.rs",
    "components/badge.rs",
    "components/kbd.rs",
    "app/input_controller.rs",
    "app/state.rs",
    "utils/text_field.rs",
    "utils/font.rs",
    "style/text_style.rs",
]

FIELDS = ["color", "size", "weight", "align", "underline", "overline", "line_through"]


def fix_reads(content: str) -> str:
    for field in FIELDS:
        content = re.sub(
            rf"\bstyle\.{field}\b(?!_resolved)(?!\s*=)",
            f"style.{field}_resolved()",
            content,
        )
    return content.replace("_resolved()_resolved()", "_resolved()")


def fix_test_assigns(content: str) -> str:
    content = re.sub(r"(\n        style\.size) = ([0-9.]+);", r"\1 = Some(\2);", content)
    content = re.sub(r"(\n        style\.weight) = ([0-9]+);", r"\1 = Some(\2);", content)
    content = re.sub(r"(\n        style\.color) = (\[[^\]]+\]);", r"\1 = Some(\2);", content)
    return content


def main() -> None:
    for rel in FILES:
        path = ROOT / rel
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8")
        new = fix_test_assigns(fix_reads(text))
        if new != text:
            path.write_text(new, encoding="utf-8")
            print(rel)


if __name__ == "__main__":
    main()
