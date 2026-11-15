#!/usr/bin/env python3
"""Compare real Jadix GPU full/tiled streams by reconstructing every tile batch.

This is a controlled pipe peer, not a real terminal or a scanout benchmark.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("terminal_session", Path(__file__).with_name("terminal-session-check.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
Peer = module.Peer


def capture(executable, directory, tiled, compressed):
    peer = Peer(executable, directory, f"{'tiles' if tiled else 'full'}-{'zlib' if compressed else 'raw'}",
                timeout_ms=10000, compress=compressed, extra_args=["--tiles"] if tiled else [])
    snapshots = []
    try:
        peer.auto_ack = True
        peer.wait(lambda: bool(peer.frames))
        peer.send(b"\t")  # Stable button focus excludes the blinking caret.
        peer.wait(lambda: len(peer.frames) >= 2)
        peer.pump(.15)
        for count in range(1, 13):
            before = len(peer.frames)
            peer.send(b"\r")
            peer.wait(lambda: len(peer.frames) > before)
            peer.pump(.05)
            snapshots.append(peer.frames[-1]["rgba_sha256"])
        peer.send(b"\x11")
        result = peer.finish(True)
        assert result["receipt"]["count"] == "12", result["receipt"]
        return dict(snapshots=snapshots, result=result)
    finally:
        if peer.process.poll() is None:
            peer.process.kill()
            peer.process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/debug/examples/terminal_app.exe")
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=False)
    results = {}
    for compressed in (False, True):
        full = capture(args.executable, args.directory, False, compressed)
        tiled = capture(args.executable, args.directory, True, compressed)
        assert full["snapshots"] == tiled["snapshots"], "tiled composition differs from full GPU pixels"
        assert tiled["result"]["wire_bytes"] < full["result"]["wire_bytes"], "sparse workload did not reduce transport"
        results["zlib" if compressed else "raw"] = dict(full=full, tiled=tiled)
    for failure in ("timeout", "rejected", "disconnect"):
        peer = Peer(args.executable, args.directory, failure, timeout_ms=150, extra_args=["--tiles"])
        peer.wait(lambda: bool(peer.queries))
        if failure == "rejected": peer.ack(module.FIRST_FENCE, "ENOTSUP")
        if failure == "disconnect": peer.process.stdin.close()
        results[failure] = peer.finish(False)
    (args.directory / "acceptance.json").write_text(json.dumps(dict(status="pass", results=results), indent=2), encoding="utf-8")
    print(json.dumps({"status": "pass", "exact_compositions": 24, "failure_scenarios": 3,
                      "real_terminal_emulator": False}))


if __name__ == "__main__": main()
