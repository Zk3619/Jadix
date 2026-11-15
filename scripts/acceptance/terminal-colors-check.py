#!/usr/bin/env python3
"""Validate explicit character color wire policies with a controlled pipe peer.

No terminal palette discovery, emulator appearance or scanout claim is made.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import unicodedata



def fixture_screen(data):
    """Decode only the initial fixture's CUP/SGR/cursor-visibility language.

    This is not a general emulator. Fail closed on new sequences or controls;
    the fixture uses only ASCII and CJK, so no grapheme/shaping claim is made.
    """
    screen = [[" " for _ in range(80)] for _ in range(24)]
    x = y = 0
    for token in re.split(r"(\x1b\[[0-?]*[ -/]*[@-~])", data.decode("utf-8")):
        if token.startswith("\x1b["):
            if token[-1] == "H":
                row, col = map(int, token[2:-1].split(";"))
                y, x = row - 1, col - 1
            elif token[-1] == "m" or token in ("\x1b[?25h", "\x1b[?25l"):
                pass
            else:
                raise AssertionError(f"unexpected fixture sequence {token!r}")
        else:
            for char in token:
                assert ord(char) >= 32 and not unicodedata.combining(char), repr(char)
                assert 0 <= x < 80 and 0 <= y < 24
                screen[y][x] = char
                width = 2 if unicodedata.east_asian_width(char) in ("W", "F") else 1
                if width == 2:
                    assert x + 1 < 80
                    screen[y][x + 1] = ""
                x += width
    return ["".join(row) for row in screen]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=False)
    results = {}
    screens = []
    env = dict(os.environ, TERM="xterm-256color", COLORTERM="truecolor")
    env.pop("NO_COLOR", None)  # Fixture-local; applications still honor user policy.
    for colors in ("rgb", "256", "16", "mono"):
        receipt = args.directory.resolve() / f"{colors}.json"
        run = subprocess.run([str(args.executable.resolve()), "--stdio", "--colors", colors,
                              "--receipt", str(receipt)], input=b"\x11", capture_output=True, timeout=10, env=env)
        (args.directory / f"{colors}.ansi").write_bytes(run.stdout)
        (args.directory / f"{colors}.stderr").write_bytes(run.stderr)
        assert run.returncode == 0, run.stderr
        value = json.loads(receipt.read_text())
        assert value["frames"] == 1 and value["error"] is None, value
        screen = fixture_screen(run.stdout)
        assert "Jadix character surface" in screen[0] and b"\x1b_G" not in run.stdout
        if colors == "rgb":
            assert re.search(rb"\x1b\[(38|48);2;", run.stdout)
        else:
            assert not re.search(rb"\x1b\[(38|48);2;", run.stdout)
        if colors == "mono":
            assert not re.search(rb"(?:38|48);", run.stdout), "mono emitted a color"
        if colors == "16":
            assert not re.search(rb"(?:38|48);", run.stdout), "16-color emitted extended SGR"
            assert any(30 <= int(n) <= 37 or 40 <= int(n) <= 47 or 90 <= int(n) <= 97 or 100 <= int(n) <= 107
                       for sgr in re.findall(rb"\x1b\[([0-9;]+)m", run.stdout) for n in sgr.split(b";") if n)
        screens.append(screen)
        results[colors] = {"wire_bytes": len(run.stdout), "receipt": value,
                           "wire_sha256": hashlib.sha256(run.stdout).hexdigest()}
    assert all(screen == screens[0] for screen in screens), "color depth changed text/geometry"
    invalid = subprocess.run([str(args.executable.resolve()), "--stdio", "--colors", "auto"],
                             capture_output=True, timeout=10)
    assert invalid.returncode != 0 and not invalid.stdout
    result = {"status": "pass", "controlled_pipe": True, "terminal_palette_observed": False,
              "executable_sha256": hashlib.sha256(args.executable.read_bytes()).hexdigest(), "results": results}
    (args.directory / "acceptance.json").write_text(json.dumps(result, indent=2))
    print(json.dumps({"status": "pass", "wire_bytes": {c: v["wire_bytes"] for c, v in results.items()}}))


if __name__ == "__main__":
    main()
