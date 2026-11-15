#!/usr/bin/env python3
"""Verify actual Jadix GPU pixels survive Ratatui/Kitty inline transport.

Requires Pillow for PNG comparison. This is an offline stream/GPU acceptance
check; it never reports a terminal emulator, SSH or monitor as tested.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]


def check(directory: Path) -> dict:
    frames = []
    image_ids = set()
    for step in range(3):
        png = directory / f"element-{step}.png"
        stream = (directory / f"element-{step}.ansi").read_bytes()
        chunks = []
        controls = []
        for part in stream.split(b"\x1b_G")[1:]:
            command, end, _ = part.partition(b"\x1b\\")
            assert end, "unterminated Kitty APC"
            control, semicolon, payload = command.partition(b";")
            assert semicolon and len(payload) <= 4096 and len(payload) % 4 == 0
            controls.append(dict(field.split("=", 1) for field in control.decode("ascii").split(",")))
            chunks.append(payload)
        assert controls, "missing image upload"
        first = controls[0]
        assert first["a"] == "T" and first["U"] == "1" and first["f"] == "32"
        assert first["s"] == "480" and first["v"] == "320" and first["q"] == "2"
        assert all(c.get("m") == "1" for c in controls[:-1]) and controls[-1]["m"] == "0"
        image_ids.add(first["i"])
        rgba = base64.b64decode(b"".join(chunks), validate=True)
        with Image.open(png) as image:
            assert image.size == (480, 320)
            assert image.convert("RGBA").tobytes() == rgba, "wire pixels differ from genuine GPU PNG"
        assert len(rgba) == 480 * 320 * 4
        assert all(alpha == 255 for alpha in rgba[3::4]), "output must be opaque"
        assert "\U0010eeee" in stream.decode("utf-8"), "missing Unicode placeholders"
        frames.append({"step": step, "image_id": int(first["i"]), "rgba_sha256": hashlib.sha256(rgba).hexdigest(),
                       "rgba_bytes": len(rgba), "wire_bytes": len(stream), "chunks": len(chunks)})
    assert len(image_ids) == 1, "replacement frames must reuse their owned image ID"
    assert len({f["rgba_sha256"] for f in frames}) == 3, "callback state must change genuine pixels"
    cleanup = (directory / "cleanup.ansi").read_bytes()
    owned_id = next(iter(image_ids))
    assert cleanup == f"\x1b_Ga=d,d=I,i={owned_id},q=2;\x1b\\".encode()
    result = (directory / "result.txt").read_text(encoding="utf-8")
    assert "rendered_frames=3" in result and "value=5" in result and "real_terminal=false" in result
    paths = ["Cargo.lock", "crates/jadix_tui/Cargo.toml", "crates/jadix_tui/src/lib.rs",
             "crates/jadix_tui/src/scene.rs", "crates/jadix_tui/src/surface.rs", "crates/jadix_tui/src/flow.rs",
             "crates/jadix_tui/examples/element_workbench.rs", "crates/jadix_tui/tests/scene.rs",
             "crates/jadix_tui/examples/workbench.rs", "crates/jadix/src/renderer/offscreen_scene.rs",
             "crates/jadix/src/renderer/offscreen.rs", "crates/jadix/src/renderer/path_pipeline.rs",
             "crates/jadix/src/renderer/resources.rs", "crates/jadix/src/app/state.rs",
             "crates/jadix/src/app.rs", "crates/jadix/src/app/pending_images.rs",
             "crates/jadix/src/app/pointer_events.rs", "crates/jadix/src/primitives/path.rs",
             "crates/jadix/src/text/measure.rs", "scripts/acceptance/terminal-scene-check.py"]
    return {"status": "pass", "real_gpu": True, "real_terminal": False, "ssh": False,
            "evidence_kind": "GPU rendering + real CrosstermBackend stream + callback replay",
            "frames": frames, "cleanup_owned_id": int(owned_id), "adapter_result": result,
            "sources": {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in paths}}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="new output directory with --run, existing export otherwise")
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--backend", choices=["dx12", "gles", "vulkan", "metal"], default="dx12")
    args = parser.parse_args()
    directory = args.directory.resolve()
    if args.run:
        if directory.exists():
            parser.error("--run requires a new directory; existing evidence is preserved")
        command = ["cargo", "run", "-p", "jadix_tui", "--features", f"backend-{args.backend}",
                   "--example", "element_workbench", "--locked", "--", "--export", str(directory)]
        subprocess.run(command, cwd=ROOT, check=True)
    report = check(directory)
    (directory / "acceptance.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: report[k] for k in ["status", "real_gpu", "real_terminal", "ssh"]}))


if __name__ == "__main__":
    main()
