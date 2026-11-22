"""Run native platform checks and retain logs without implying unobserved Apple coverage."""

import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def main():
    sys.stdout.reconfigure(errors="backslashreplace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/native-controls/runs") / datetime.now().strftime("%Y%m%d-%H%M%S-%f"))
    parser.add_argument("--wasm-bindgen", type=Path)
    parser.add_argument("--browser", default="msedge")
    parser.add_argument("--android-serial")
    parser.add_argument("--keystore", type=Path)
    parser.add_argument("--release", action="store_true", help="Use optimized Rust libraries for requested browser/Android packages")
    parser.add_argument("--large-list", action="store_true", help="Run requested platform packages with 1000 retained rows")
    parser.add_argument("--process-recovery", action="store_true", help="Verify Android task recovery after terminating its background process")
    parser.add_argument("--apple-rust-targets", nargs="+", choices=["aarch64-apple-ios", "aarch64-apple-ios-sim"], default=[],
                        help="Check Rust controls/notes/selection/popover for installed Apple targets; does not compile Swift, link an app or run UIKit")
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = (root / arguments.output).resolve()
    if any((directory / "snapshot.json").exists() for directory in (output, *output.parents)):
        parser.error("output contains an archived snapshot; select a new output directory")
    output.mkdir(parents=True, exist_ok=True)
    python = sys.executable
    features = "jadix_native_controls/uikit,jadix_native_controls/extended"
    checks = [
        ("contracts", ["cargo", "test", "--locked", "-p", "jadix_core", "-p", "jadix_native", "-p", "jadix_shell", "-p", "jadix_native_controls", "--features", features, "--lib", "--tests"]),
        ("portable", ["cargo", "test", "--locked", "-p", "jadix_portable_contract"]),
        ("clippy-native", ["cargo", "clippy", "--locked", "-p", "jadix_native", "-p", "jadix_native_controls", "--all-targets", "--features", features, "--no-deps", "--", "-D", "warnings"]),
        ("clippy-dom", ["cargo", "clippy", "--locked", "-p", "jadix_native_controls", "--features", "extended", "--target", "wasm32-unknown-unknown", "--no-deps", "--", "-D", "warnings"]),
        ("clippy-android", ["cargo", "clippy", "--locked", "-p", "jadix_native_controls", "--features", "extended", "--target", "aarch64-linux-android", "--no-deps", "--", "-D", "warnings"]),
        ("gpu-compatibility", ["cargo", "check", "--locked", "-p", "jadix_native_controls", "--features", "gpu"]),
        ("dependencies", [python, "scripts/checks/check-native-shell.py", "--output", str(output / "dependencies.json")]),
        ("public-docs", [python, "scripts/checks/check-public-docs.py", "--output", str(output / "public-docs.json")]),
        ("workflow", [python, "scripts/checks/check-ci-workflow.py", "--workflow", ".github/workflows/native.yml", "--output", str(output / "workflow.json")]),
    ]
    for target in dict.fromkeys(arguments.apple_rust_targets):
        for application, selected_features in (("controls", "uikit,extended"), ("notes", "uikit,notes"), ("selection", "uikit,selection"), ("popover", "uikit,popover")):
            checks.append((f"rust-{target}-{application}", ["cargo", "check", "--locked", "-p", "jadix_native_controls",
                           "--target", target, "--features", selected_features]))
    if arguments.wasm_bindgen:
        site = output / "web"
        checks.extend([
            ("web-package", [python, "examples/native-controls/build-web.py", "--extended", "--wasm-bindgen", str(arguments.wasm_bindgen.resolve()), "--output", str(site)]),
            ("dom-runtime", [python, "scripts/acceptance/native-dom-check.py", "--extended", "--site", str(site), "--channel", arguments.browser, "--output", str(output / "dom")]),
        ])
    if arguments.android_serial:
        package = output / "android"
        command = [python, "examples/native-controls/build-android.py", "--extended", "--target", "x86_64-linux-android", "--output", str(package)]
        if arguments.keystore:
            command.extend(["--keystore", str(arguments.keystore.resolve())])
        checks.extend([
            ("android-package", command),
            ("android-runtime", [python, "scripts/acceptance/native-android-check.py", "--extended", "--serial", arguments.android_serial, "--apk", str(package / "native-controls.apk"), "--output", str(output / "android-checks")]),
        ])
    observations = []
    if arguments.process_recovery:
        for name, command in checks:
            if name == "android-runtime":
                command.append("--process-recovery")
    if arguments.large_list:
        for name, command in checks:
            if name in {"web-package", "android-package", "dom-runtime", "android-runtime"}:
                command.append("--large-list")
    if arguments.release:
        for name, command in checks:
            if name in {"web-package", "android-package"}:
                command.append("--release")
    for name, command in checks:
        started = time.monotonic()
        environment = os.environ.copy()
        if name == "clippy-android" and environment.get("NDK_HOME"):
            windows = sys.platform == "win32"
            host = "windows-x86_64" if windows else "darwin-x86_64" if sys.platform == "darwin" else "linux-x86_64"
            llvm = Path(environment["NDK_HOME"]) / "toolchains/llvm/prebuilt" / host / "bin"
            environment["CC_aarch64_linux_android"] = str(llvm / ("aarch64-linux-android26-clang" + (".cmd" if windows else "")))
            environment["AR_aarch64_linux_android"] = str(llvm / ("llvm-ar.exe" if windows else "llvm-ar"))
        result = subprocess.run(command, cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf8", errors="replace")
        log = output / f"{name}.log"
        log.write_text(result.stdout, encoding="utf8")
        observations.append({"name": name, "command": command, "exit_code": result.returncode,
                             "passed": result.returncode == 0, "duration_seconds": round(time.monotonic() - started, 3),
                             "log": str(log.relative_to(output)), "sha256": hashlib.sha256(log.read_bytes()).hexdigest(),
                             "tests_passed": sum(int(value) for value in re.findall(r"test result: ok\. (\d+) passed;", result.stdout))})
        report = {"observed_at": datetime.now().astimezone().isoformat(),
                  "passed": all(item["passed"] for item in observations), "complete": len(observations) == len(checks),
                  "checks": observations,
                  "limits": ["Only requested local checks are represented; omitted platform runtimes remain unverified.",
                             "UIKit compilation, device signing, iOS keyboard and VoiceOver require Apple evidence.",
                             "Local success does not establish remote CI, minimum OS versions or full N1-N5 acceptance."]}
        (output / "validation.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
        print(f"{'PASS' if result.returncode == 0 else 'FAIL'} {name} ({observations[-1]['duration_seconds']}s)", flush=True)
        if result.returncode:
            print(result.stdout[-4000:], flush=True)
    return 0 if all(item["passed"] for item in observations) else 1


if __name__ == "__main__":
    raise SystemExit(main())
