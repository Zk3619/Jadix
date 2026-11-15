"""Run bounded engineering checks and retain evidence without asserting platform acceptance."""

import json
import os
from pathlib import Path
import subprocess
import sys
import time

import native


def register(subcommands):
    parser = subcommands.add_parser("validate", help="Run consolidated checks with source-bound evidence")
    parser.add_argument("--suite", action="append", choices=["python", "portable", "windows", "wasm", "uikit-controls", "uikit-notes", "uikit-selection", "uikit-popover", "field", "uikit-field", "wasm-field"])
    parser.add_argument("--timeout", type=int, default=1200, help="Maximum seconds per suite")
    parser.add_argument("--output", type=Path, required=True)
    audit = subcommands.add_parser("audit-validation", help="Check retained validation logs against current sources")
    audit.add_argument("--validation", type=Path, required=True)
    audit.add_argument("--output", type=Path, required=True)


def commands(suites):
    cargo = ["cargo", "test", "--locked", "--no-default-features", "--no-fail-fast"]
    portable = cargo + ["-p", "jadix_core", "-p", "jadix_native", "-p", "jadix_components",
                        "--features", "jadix_components/macros"]
    windows = cargo + ["-p", "jadix_core", "-p", "jadix_native", "-p", "jadix_components",
                       "-p", "jadix_shell", "-p", "jadix_native_controls", "--features",
                       "jadix_native/windows,jadix_native/motion,jadix_native/ohos,jadix_components/macros,"
                       "jadix_native_controls/native-shell,jadix_native_controls/native-code,"
                       "jadix_native_controls/native-gpu"]
    available = {
        "python": [sys.executable, "-m", "unittest", "discover", "-s", "scripts/tests", "-p", "test_native*.py"],
        "portable": portable,
        "windows": windows,
        "wasm": ["cargo", "check", "--locked", "--target", "wasm32-unknown-unknown", "-p", "jadix_native_controls"],
    }
    for application in ["controls", "notes", "selection", "popover"]:
        features = "uikit,extended" if application == "controls" else "uikit," + application
        available["uikit-" + application] = ["cargo", "test", "--locked", "-p", "jadix_native_controls",
                                            "--no-default-features", "--features", features, "--lib"]
    available["field"] = ["cargo", "test", "--locked", "-p", "jadix_native_controls", "--no-default-features",
                          "--features", "windows,field-workflow", "--lib", "--bin", "native-field"]
    available["uikit-field"] = ["cargo", "test", "--locked", "-p", "jadix_native_controls", "--no-default-features",
                                "--features", "uikit,field-workflow", "--lib"]
    available["wasm-field"] = ["cargo", "check", "--locked", "--target", "wasm32-unknown-unknown", "-p", "jadix_native_controls", "--features", "field-workflow"]
    if len(suites) != len(set(suites)):
        raise ValueError("each validation suite must be selected only once")
    if any(suite in suites for suite in ["windows", "field"]) and os.name != "nt":
        raise ValueError("the windows suite requires a Windows host")
    if "windows" in suites and "portable" in suites:
        raise ValueError("windows already includes portable checks; select it only once")
    return [(suite, available[suite]) for suite in suites]


def source_identity():
    trees = {name: native.tree_identity(native.ROOT / name)
             for name in ["crates", "scripts", "examples/native-controls"]}
    files = {name: native.file_identity(native.ROOT / name) for name in ["Cargo.toml", "Cargo.lock"]}
    return {"trees": trees, "files": files}


def run(arguments, output):
    if not 1 <= arguments.timeout <= 7200:
        raise ValueError("timeout must be between 1 and 7200 seconds")
    suites = arguments.suite or ["python", "windows" if os.name == "nt" else "portable"]
    invocations = commands(suites)
    output = output.resolve()
    for name in ["crates", "scripts", "examples/native-controls"]:
        if output.is_relative_to((native.ROOT / name).resolve()):
            raise ValueError("validation output must be outside identified source directories")
    directory = output.parent / f"{output.stem}-files"
    if directory.exists():
        raise ValueError("validation evidence directory must be new")
    before = source_identity()
    directory.mkdir(parents=True)
    environment = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    records = []
    for suite, command in invocations:
        log = directory / f"{suite}.log"
        record = {"suite": suite, "command": command, "log": str(log), "exit_code": None}
        started = time.monotonic()
        try:
            with log.open("xb") as stream:
                result = subprocess.run(command, cwd=native.ROOT, env=environment, stdout=stream,
                                        stderr=subprocess.STDOUT, timeout=arguments.timeout,
                                        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            record["exit_code"] = result.returncode
        except (OSError, subprocess.TimeoutExpired) as error:
            record["error"] = str(error)
        record["elapsed_seconds"] = round(time.monotonic() - started, 3)
        if log.is_file():
            record["log_identity"] = native.file_identity(log)
        records.append(record)
    after = source_identity()
    stable = before == after
    passed = stable and all(record["exit_code"] == 0 for record in records)
    return {"kind": "native-engineering-validation", "checks_passed": passed,
            "source_unchanged": stable, "source_before": before, "source_after": after,
            "checks": records, "engineering_signoff": False, "runtime_acceptance": False,
            "limits": ["Only selected commands are checked; missing implementations are not accepted.",
                       "Programmatic host tests do not accept physical input or assistive technology. UIKit suites on non-Apple hosts use mock ABI, not UIKit runtime.",
                       "No reference-framework performance superiority, signing or distribution is asserted."]}


def verify(path):
    report = json.loads(path.read_text(encoding="utf-8"))
    if (report.get("schema") != 1 or report.get("operation") != "validate"
            or report.get("runtime_acceptance") is not False):
        raise ValueError("requires a bounded native validate report")
    result = report["observations"]
    if (result.get("kind") != "native-engineering-validation" or result.get("checks_passed") is not True
            or result.get("source_unchanged") is not True or result.get("engineering_signoff") is not False
            or result.get("runtime_acceptance") is not False):
        raise ValueError("failed or overclaimed validation report")
    if result.get("source_before") != result.get("source_after") or result["source_after"] != source_identity():
        raise ValueError("validation sources have drifted")
    checks = result.get("checks", [])
    expected = commands([check["suite"] for check in checks])
    if not checks or len(expected) != len(checks):
        raise ValueError("validation has no executed checks")
    directory = path.resolve().parent / f"{path.stem}-files"
    for check, (suite, command) in zip(checks, expected):
        log = directory / f"{suite}.log"
        if (check.get("exit_code") != 0 or check.get("command") != command
                or Path(check.get("log", "")).resolve() != log.resolve()
                or check.get("log_identity") != native.file_identity(log)):
            raise ValueError("validation command or retained log differs from evidence")
    return {"integrity_verified": True, "checks": [check["suite"] for check in checks],
            "runtime_acceptance": False, "authenticity_verified": False}
