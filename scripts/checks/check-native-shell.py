"""Check target dependency isolation for the experimental native and shell contracts."""
import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/native-shell/dependencies.json"))
    parser.add_argument("--controls-features", default="", help="Additional native-controls application features to audit")
    parser.add_argument("--include-component-dev", action="store_true", help="Also audit GPU-free component normal/build/dev dependencies")
    parser.add_argument("--targets", nargs="+", default=[
        "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu",
        "aarch64-linux-android", "aarch64-apple-ios", "wasm32-unknown-unknown",
    ])
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    forbidden = {
        "jadix", "jadix_components", "jadix_renderer", "jadix_wgpu", "wgpu",
        "jadix_glyphon", "glyphon", "jadix_winit", "winit",
        "rquickjs", "rquickjs-sys", "quick-js", "quickjs-sys",
    }
    observations = []
    selected_packages = ["jadix_native", "jadix_shell", "jadix_portable_contract", "jadix_native_controls"]
    if arguments.include_component_dev:
        selected_packages.append("jadix_components")
    for package in selected_packages:
        for target in arguments.targets:
            edges = "normal,build,dev" if package == "jadix_components" else "normal,build"
            command = ["cargo", "tree", "--locked", "--offline", "-p", package,
                       "--target", target, "--edges", edges,
                       "--prefix", "none", "--format", "{p}"]
            if package == "jadix_components":
                command += ["--no-default-features", "--features", "macros,theme-shadcn"]
            if package == "jadix_native_controls" and arguments.controls_features:
                command += ["--features", arguments.controls_features]
            result = subprocess.run(command, cwd=root, capture_output=True, text=True, encoding="utf-8")
            packages = sorted({line.split()[0] for line in result.stdout.splitlines() if line.strip()})
            banned = forbidden - {"jadix", "jadix_components"} if package in {"jadix_portable_contract", "jadix_native_controls", "jadix_components"} else forbidden
            violations = sorted(set(packages) & banned)
            if package == "jadix_shell" and "jadix_native" in packages:
                violations.append("jadix_native")
            if package == "jadix_native" and "jadix_shell" in packages:
                violations.append("jadix_shell")
            passed = result.returncode == 0 and "jadix_core" in packages and not violations
            observations.append(dict(package=package, target=target, edges=edges, command=command,
                                     exit_code=result.returncode, packages=packages,
                                     violations=violations, stderr=result.stderr, passed=passed))
    report = dict(passed=all(item["passed"] for item in observations), observations=observations,
                  scope="Resolved dependency graphs with explicit edges per observation; not platform compilation or runtime evidence")
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{'PASS' if report['passed'] else 'FAIL'}: {len(observations)} target/package dependency graphs; {arguments.output}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
