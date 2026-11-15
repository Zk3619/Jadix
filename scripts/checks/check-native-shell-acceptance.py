"""Run the Native/Hybrid/Shell foundation acceptance matrix and retain exact logs."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/native-shell/acceptance"))
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = root / arguments.output
    output.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy", "CARGO_HTTP_PROXY"):
        environment[key] = ""
    cargo = ["cargo", "--config", 'http.proxy=""', "--config", "build.jobs=1"]
    sdk = "crates/jadix_shell/sdk"
    checks = [
        ("contracts", cargo + ["test", "--locked", "-p", "jadix_core", "-p", "jadix_native", "-p", "jadix_shell", "--all-targets"]),
        ("vm", cargo + ["test", "--locked", "-p", "jadix_shell", "--features", "native-vm", "--test", "runtime"]),
        ("shell-baseline", cargo + ["run", "--locked", "-p", "jadix_shell", "--features", "native-vm", "--example", "shell_contract_baseline"]),
        ("portable", cargo + ["test", "--locked", "-p", "jadix_portable_contract"]),
        ("portable-run", cargo + ["run", "--locked", "-p", "jadix_portable_contract"]),
        ("reference-baseline", cargo + ["run", "--locked", "-p", "jadix_portable_contract", "--example", "native_contract_baseline"]),
        ("gpu-author", cargo + ["test", "--locked", "-p", "jadix_components", "--features", "macros", "--test", "native_projection", "--test", "portable_source"]),
        ("gpu-builder", cargo + ["test", "--locked", "-p", "jadix_components", "--no-default-features", "--features", "gpu", "--test", "native_projection"]),
        ("gpu-regression", cargo + ["test", "--locked", "-p", "jadix", "--lib", "--features", "a11y,image-gif", "--", "--test-threads=1"]),
        ("gpu-animation", cargo + ["test", "--locked", "-p", "jadix", "--lib", "--features", "a11y,image-gif,animation-ex", "--", "--test-threads=1"]),
        ("webview-existing", cargo + ["test", "--locked", "-p", "jadix", "--lib", "--features", "a11y,image-gif,webview", "webview", "--", "--test-threads=1"]),
        ("clippy", cargo + ["clippy", "--locked", "--no-deps", "-p", "jadix_core", "-p", "jadix_native", "-p", "jadix_shell", "-p", "jadix_portable_contract", "--all-targets", "--", "-D", "warnings"]),
        ("clippy-vm", cargo + ["clippy", "--locked", "--no-deps", "-p", "jadix_shell", "--features", "native-vm", "--all-targets", "--", "-D", "warnings"]),
        ("clippy-portable", cargo + ["clippy", "--locked", "--no-deps", "-p", "jadix", "-p", "jadix_components", "--no-default-features", "--features", "macros,theme-shadcn", "--lib", "--", "-D", "warnings"]),
        ("portable-no-macros", cargo + ["check", "--locked", "-p", "jadix", "-p", "jadix_components", "--no-default-features", "--lib"]),
        ("wasm", cargo + ["check", "--locked", "-p", "jadix_native", "-p", "jadix_shell", "-p", "jadix_portable_contract", "--target", "wasm32-unknown-unknown"]),
        ("dependencies", [sys.executable, "scripts/checks/check-native-shell.py", "--output", str(output / "dependencies.json")]),
        ("sdk-generated", ["node", f"{sdk}/scripts/generate.mjs", "--check"]),
        ("sdk-types", ["node", f"{sdk}/node_modules/typescript/bin/tsc", "--project", f"{sdk}/tsconfig.json", "--noEmit"]),
        ("sdk-tests", ["node", "--test", f"{sdk}/tests/runtime.test.mjs"]),
        ("sdk-build", ["node", f"{sdk}/scripts/build.mjs"]),
        ("public-docs", [sys.executable, "scripts/checks/check-public-docs.py", "--output", str(output / "public-docs.json")]),
    ]
    observations = []
    for name, command in checks:
        started = time.monotonic()
        repetitions = 3 if name in ("shell-baseline", "reference-baseline") else 1
        runs = [subprocess.run(command, cwd=root, env=environment, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
                for _ in range(repetitions)]
        result = subprocess.CompletedProcess(command, next((item.returncode for item in runs if item.returncode), 0),
                                             stdout="\n".join(item.stdout for item in runs))
        log = output / f"{name}.log"
        log.write_text(result.stdout, encoding="utf-8")
        if name in ("shell-baseline", "reference-baseline"):
            samples = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
            if name == "shell-baseline":
                valid = any(item.get("vm_samples") == 30 for item in samples) and any(item.get("handles_per_batch") == 1024 for item in samples)
            else:
                valid = {item.get("rows") for item in samples} == {100, 1000} and all(item.get("idle_measurements") == 0 and item.get("idle_commits") == 0 for item in samples)
            if not valid:
                result.returncode = result.returncode or 1
        counts = [dict(passed=int(match[0]), failed=int(match[1]), ignored=int(match[2]))
                  for match in re.findall(r"test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored", result.stdout)]
        observations.append(dict(name=name, command=command, repetitions=repetitions, exit_code=result.returncode,
                                 duration_seconds=round(time.monotonic() - started, 3),
                                 log=str(log.relative_to(root)), sha256=hashlib.sha256(log.read_bytes()).hexdigest(),
                                 rust_suites=counts, passed=result.returncode == 0))
        report = dict(passed=all(item["passed"] for item in observations), complete=len(observations) == len(checks), observations=observations)
        (output / "validation.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"{'PASS' if result.returncode == 0 else 'FAIL'} {name} ({observations[-1]['duration_seconds']}s)", flush=True)
        if result.returncode:
            print(result.stdout[-5000:], flush=True)
    return 0 if all(item["passed"] for item in observations) else 1


if __name__ == "__main__":
    raise SystemExit(main())
