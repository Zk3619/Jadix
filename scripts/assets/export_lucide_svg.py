import argparse
import json
from pathlib import Path


SVG_OPEN = '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'


def documents(root):
    catalog = json.loads((root / "assets/lucide_paths.json").read_text(encoding="utf-8"))
    return {name + ".svg": SVG_OPEN + "\n  " + body + "\n</svg>\n" for name, body in sorted(catalog.items())}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2] / "crates/jadix_lucide"
    expected = documents(root)
    directory = root / "assets/svg"
    actual = {path.name: path.read_text(encoding="utf-8") for path in directory.glob("*.svg")}
    if arguments.check:
        mismatches = sorted(name for name in actual.keys() | expected.keys() if actual.get(name) != expected.get(name))
        print(json.dumps({"icons": len(expected), "mismatches": mismatches}))
        return bool(mismatches)
    directory.mkdir(parents=True, exist_ok=True)
    if actual.keys() - expected.keys():
        raise SystemExit("Unexpected SVG files; inspect them before regenerating.")
    for name, document in expected.items():
        (directory / name).write_text(document, encoding="utf-8", newline="\n")
    print(json.dumps({"icons": len(expected)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
