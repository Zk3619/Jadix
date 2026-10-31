"""Build the browser example and optionally serve its static output."""

import argparse
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serve", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--bind", default="127.0.0.1")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    manifest = root / "Cargo.toml"
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--manifest-path", str(manifest)],
        cwd=root, text=True,
    ))
    target = Path(metadata["target_directory"])
    output = target / "gallery-web"
    if not args.no_build:
        lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
        version = next(package["version"] for package in lock["package"] if package["name"] == "wasm-bindgen")
        tool = shutil.which("wasm-bindgen")
        if not tool:
            raise SystemExit(f"Install wasm-bindgen-cli {version} and add it to PATH (see docs/BUILD.md).")
        actual = subprocess.check_output([tool, "--version"], text=True).strip()
        if actual != f"wasm-bindgen {version}":
            raise SystemExit(f"Expected wasm-bindgen {version}, found {actual}.")
        environment = os.environ.copy()
        flag = "--cfg web_sys_unstable_apis"
        for key in ("RUSTFLAGS", "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"):
            environment[key] = f"{environment.get(key, '')} {flag}".strip()
        subprocess.run([
            "cargo", "build", "--locked", "--release", "--target", "wasm32-unknown-unknown",
            "--package", "gallery_web", "--manifest-path", str(manifest),
        ], cwd=root, env=environment, check=True)
        output.mkdir(parents=True, exist_ok=True)
        subprocess.run([
            tool, "--target", "web", "--out-dir", str(output / "pkg"),
            str(target / "wasm32-unknown-unknown/release/gallery_web.wasm"),
        ], check=True)
        shutil.copyfile(Path(__file__).with_name("index.html"), output / "index.html")
    if not (output / "pkg/gallery_web_bg.wasm").is_file():
        raise SystemExit("No browser bundle found. Run without --no-build first.")
    print(f"Web bundle: {output}", flush=True)
    if args.serve:
        handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(output))
        with http.server.ThreadingHTTPServer((args.bind, args.port), handler) as server:
            print(f"http://{args.bind}:{server.server_port}/", flush=True)
            try:
                server.serve_forever()
            except KeyboardInterrupt:
                pass


if __name__ == "__main__":
    main()
