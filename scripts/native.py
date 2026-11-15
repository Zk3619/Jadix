"""Inspect native toolchains, identify sources and deliver local native applications."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
from datetime import datetime, timezone


ROOT = Path(__file__).resolve().parents[1]
EXCLUDED = {".git", "target", "node_modules", "__pycache__"}


def command(arguments, *, cwd=ROOT):
    try:
        result = subprocess.run(arguments, cwd=cwd, capture_output=True, text=True,
                                encoding="utf-8", errors="replace", timeout=30,
                                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        return {"arguments": [str(value) for value in arguments], "exit_code": result.returncode,
                "stdout": result.stdout.strip(), "stderr": result.stderr.strip()}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"arguments": [str(value) for value in arguments], "exit_code": None,
                "error": str(error)}


def file_identity(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return {"bytes": path.stat().st_size, "sha256": digest.hexdigest()}


def tree_identity(directory):
    directory = directory.resolve()
    rows = []
    for parent, directories, files in os.walk(directory, followlinks=False):
        directories[:] = sorted(name for name in directories
                                if name not in EXCLUDED and not (Path(parent) / name).is_symlink())
        for name in sorted(files):
            path = Path(parent) / name
            if path.is_symlink():
                raise ValueError(f"source identity requires regular files: {path}")
            rows.append({"path": path.relative_to(directory).as_posix(), **file_identity(path)})
    if not rows:
        raise ValueError(f"source directory is empty or unavailable: {directory}")
    rows.sort(key=lambda row: row["path"])
    encoded = json.dumps(rows, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return {"directory": str(directory), "excluded_directories": sorted(EXCLUDED),
            "file_count": len(rows), "manifest_sha256": hashlib.sha256(encoded).hexdigest(),
            "files": rows}


def sdk_identity(path):
    path = path.resolve()
    native = path if (path / "oh-uni-package.json").is_file() else path / "native"
    metadata_path = native / "oh-uni-package.json"
    if not metadata_path.is_file():
        return {"directory": str(path), "available": False}
    metadata = json.loads(metadata_path.read_text(encoding="utf-8-sig"))
    suffix = ".exe" if os.name == "nt" else ""
    compiler = native / "llvm/bin" / f"clang{suffix}"
    return {"directory": str(path), "native": str(native), "available": compiler.is_file(),
            "metadata": metadata, "metadata_identity": file_identity(metadata_path),
            "compiler": str(compiler), "sysroot": str(native / "sysroot"),
            "compiler_version": command([str(compiler), "--version"]) if compiler.is_file() else None}


def doctor(ohos_sdk=None):
    tools = {}
    versions = {"rustc": ["-vV"], "cargo": ["--version"], "rustup": ["target", "list", "--installed"],
                "adb": ["version"], "hdc": ["-v"]}
    for name, arguments in versions.items():
        executable = shutil.which(name)
        tools[name] = {"path": executable, "version": command([executable, *arguments]) if executable else None}
    deveco = os.environ.get("DEVECO_SDK_HOME")
    independent = os.environ.get("OHOS_NDK_HOME")
    candidates = []
    if deveco:
        candidates.append(Path(deveco) / "default/openharmony")
    if independent:
        candidates.append(Path(independent))
    if ohos_sdk:
        candidates.insert(0, ohos_sdk)
    sdks = [sdk_identity(path) for path in dict.fromkeys(path.resolve() for path in candidates)]
    selected = sdks[0] if sdks else None
    api_versions = {row["metadata"].get("apiVersion") for row in sdks if row.get("available")}
    android = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    android_sdk = Path(android) if android else None
    emulator = android_sdk / ("emulator/emulator.exe" if os.name == "nt" else "emulator/emulator") if android_sdk else None
    return {"system": platform.platform(), "machine": platform.machine(), "python": sys.version,
            "tools": tools, "ohos_sdks": sdks, "selected_ohos_sdk": selected,
            "ohos_selection": "explicit" if ohos_sdk else "DEVECO_SDK_HOME before OHOS_NDK_HOME",
            "ohos_sdk_api_conflict": len(api_versions) > 1,
            "android_sdk": str(android_sdk) if android_sdk else None,
            "android_avds": command([str(emulator), "-list-avds"]) if emulator and emulator.is_file() else None,
            "android_devices": command([tools["adb"]["path"], "devices", "-l"]) if tools["adb"]["path"] else None,
            "ohos_devices": command([tools["hdc"]["path"], "list", "targets"]) if tools["hdc"]["path"] else None,
            "limits": ["No devices were launched, installed, stopped, or removed.",
                       "Tool and SDK presence does not establish successful target compilation or runtime acceptance.",
                       "Select and record an actual system image and ABI before building or running."]}


def baseline(reference):
    reference_tree = tree_identity(reference)
    directories = ["crates/jadix", "crates/jadix_components", "crates/jadix_core", "crates/jadix_native", "crates/jadix_shell",
                   "crates/jadix_macros", "crates/jadix_tasks", "crates/jadix_layout", "crates/jadix_style_tokens",
                   "crates/jadix_taffy", "crates/jadix_theme", "crates/jadix_animation", "crates/jadix_hot_reload",
                   "examples/native-controls", "examples/portable-contract"]
    return {"reference": reference_tree,
            "candidate": {"git_head": command(["git", "rev-parse", "HEAD"]),
                          "lock": file_identity(ROOT / "Cargo.lock"),
                          "workspace": file_identity(ROOT / "Cargo.toml"),
                          "trees": [tree_identity(ROOT / name) for name in directories]},
            "ohos_prs": [
                {"number": 473, "head": "818506824a5ea1983f230ebc9a949492241b092e"},
                {"number": 474, "head": "e4d1b4ba43211baa6078516c0023e9a2154a42a1"},
                {"number": 475, "head": "4485db966db33be06412385ca6fc331686f8ed59"}],
            "limits": ["Local NSH tree identity is independent of the PR head identities.",
                       "Git HEAD does not identify uncommitted source; use the recorded file manifests.",
                       "Source hashes establish identity, not build, runtime, performance, or release acceptance."]}


def main():
    import native_capabilities
    import native_workflow
    import native_validation

    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="operation", required=True)
    inspect = subcommands.add_parser("doctor", help="Read tools, SDK metadata and connected target lists")
    inspect.add_argument("--ohos-sdk", type=Path)
    freeze = subcommands.add_parser("baseline", help="Hash the reference and current native source trees")
    freeze.add_argument("--reference", type=Path, default=ROOT / "Reference/nsh-main")
    inventory = subcommands.add_parser("ledger", help="Inventory reference exports and candidate schemas; parity remains unreviewed")
    inventory.add_argument("--reference", type=Path, default=ROOT / "Reference/nsh-main")
    inventory.add_argument("--scope", choices=["all", "foundation", "components"], default="all",
                           help="Include foundation guides and crate features; foundation does not depend on component export tests")
    for subcommand in [inspect, freeze, inventory]:
        subcommand.add_argument("--output", type=Path, required=True)
    native_workflow.register(subcommands)
    native_validation.register(subcommands)
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    if output.exists() or any((parent / "snapshot.json").exists() for parent in [output.parent, *output.parents]):
        parser.error("output must be new and outside archived snapshots")
    try:
        output.parent.mkdir(parents=True, exist_ok=True)
        if arguments.operation == "doctor":
            result = doctor(arguments.ohos_sdk)
        elif arguments.operation == "baseline":
            result = baseline(arguments.reference)
        elif arguments.operation == "ledger":
            if arguments.scope == "foundation":
                result = native_capabilities.foundation_ledger(arguments.reference)
            else:
                result = native_capabilities.ledger(arguments.reference)
                if arguments.scope == "all":
                    result["foundation"] = native_capabilities.foundation_ledger(arguments.reference)
        elif arguments.operation == "validate":
            result = native_validation.run(arguments, output)
        elif arguments.operation == "audit-validation":
            result = native_validation.verify(arguments.validation.resolve())
        else:
            result = native_workflow.execute(arguments, output)
        report = {"schema": 1, "operation": arguments.operation,
                  "recorded_at": datetime.now(timezone.utc).isoformat(), "reporter": file_identity(Path(__file__)),
                  "observations": result, "runtime_acceptance": False}
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open("x", encoding="utf-8", newline="\n") as stream:
            json.dump(report, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, f"native {arguments.operation}: {error}\n")
    print(json.dumps({"output": str(output), "sha256": file_identity(output)["sha256"],
                      "runtime_acceptance": False}, ensure_ascii=False))
    if arguments.operation == "validate" and not result["checks_passed"]:
        parser.exit(1, "native validate: checks failed or sources changed; retained report identifies failures\n")


if __name__ == "__main__":
    main()
