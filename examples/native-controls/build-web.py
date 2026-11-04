import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tomllib


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--wasm-bindgen", type=Path)
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/native-controls/web"))
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--selection", action="store_true", help="Build the native Select contract page")
    parser.add_argument("--popover", action="store_true", help="Build the native Popover contract page")
    parser.add_argument("--notes", action="store_true", help="Build the persistent notebook application")
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--large-list", action="store_true", help="Build the retained 1000-row sample instead of 100 rows")
    arguments = parser.parse_args()
    if arguments.popover and (arguments.notes or arguments.extended or arguments.large_list or arguments.selection):
        parser.error("--popover requires its own sample without other application modes")
    if arguments.notes and (arguments.extended or arguments.large_list or arguments.selection):
        parser.error("--notes cannot be combined with the controls sample features")
    if arguments.selection and (arguments.extended or arguments.large_list):
        parser.error("--selection cannot be combined with --extended or --large-list")
    root = Path(__file__).resolve().parents[2]
    output = arguments.output.resolve()
    if any((directory / "snapshot.json").exists() for directory in (output, *output.parents)):
        parser.error("output contains an archived snapshot; select a new output directory")
    output.mkdir(parents=True, exist_ok=True)
    executable = str(arguments.wasm_bindgen.resolve()) if arguments.wasm_bindgen else shutil.which("wasm-bindgen")
    if not executable:
        parser.error("Install wasm-bindgen-cli matching Cargo.lock, or pass --wasm-bindgen PATH")
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf8"))
    version = next(package["version"] for package in lock["package"] if package["name"] == "wasm-bindgen")
    observed = subprocess.check_output([executable, "--version"], text=True, encoding="utf8").strip()
    if observed != f"wasm-bindgen {version}":
        parser.error(f"wasm-bindgen-cli must match Cargo.lock ({version}); observed {observed}")
    command = ["cargo", "build", "--locked", "-p", "jadix_native_controls", "--target", "wasm32-unknown-unknown"]
    if arguments.extended:
        command += ["--features", "extended"]
    if arguments.selection:
        command += ["--features", "selection"]
    if arguments.popover:
        command += ["--features", "popover"]
    if arguments.notes:
        command += ["--features", "notes"]
    if arguments.large_list:
        command += ["--features", "large-list"]
    if arguments.release:
        command += ["--release"]
    subprocess.run(command, cwd=root, check=True)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=root, text=True, encoding="utf8"))
    profile = "release" if arguments.release else "debug"
    library = Path(metadata["target_directory"]) / "wasm32-unknown-unknown" / profile / "jadix_native_controls.wasm"
    subprocess.run([executable, "--target", "web", "--out-dir", str(output / "pkg"), str(library)], cwd=root, check=True)
    shutil.copyfile(root / ("examples/native-controls/web/notes.html" if arguments.notes else "examples/native-controls/web/index.html"), output / "index.html")
    report = {"target": "wasm32-unknown-unknown", "rust_profile": profile, "extended": arguments.extended,
              "selection": arguments.selection,
              "popover": arguments.popover,
              "selection_authoring": "compound-primary-options-disabled" if arguments.selection else None,
              "application": "notes" if arguments.notes else "controls",
              "retained_rows": 0 if arguments.notes or arguments.selection or arguments.popover else 1000 if arguments.large_list else 100,
              "wasm_bindgen": observed, "cargo_lock_sha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(),
              "library_bytes": library.stat().st_size, "library_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
              "files": [{"path": str(path.relative_to(output)), "bytes": path.stat().st_size,
                         "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                        for path in sorted(output.rglob("*")) if path.is_file() and path.name != "build.json"]}
    (output / "build.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print(output)


if __name__ == "__main__":
    main()
