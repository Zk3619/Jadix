import argparse
import json
from pathlib import Path
import re
import shlex
import subprocess
import tomllib

import yaml


SCRIPT = re.compile(r"(?:\./)?scripts/[A-Za-z0-9_./-]+\.(?:py|ps1|sh|mjs)")


def package_catalog(root):
    manifests = [root / "Cargo.toml"]
    for directory in (root / "crates", root / "vendor", root / "examples"):
        manifests.extend(directory.glob("*/Cargo.toml"))
    packages = {}
    for path in manifests:
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        if "package" in manifest:
            packages[manifest["package"]["name"]] = (path.parent, manifest)
    return packages


def inspect_workflow(root, workflow, tracked=None):
    data = yaml.load(workflow.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
    packages = package_catalog(root)
    issues = []
    commands = 0
    scripts = set()
    for job_name, job in data.get("jobs", {}).items():
        for step in job.get("steps", []):
            text = step.get("run", "")
            for name in SCRIPT.findall(text):
                path = name.removeprefix("./")
                scripts.add(path)
                if not (root / path).is_file() or (tracked is not None and path not in tracked):
                    issues.append({"job": job_name, "kind": "script-unavailable-after-checkout", "path": path})
            for line in text.splitlines():
                if not line.strip().startswith("cargo "):
                    continue
                commands += 1
                tokens = shlex.split(line)
                if "-p" not in tokens:
                    continue
                name = tokens[tokens.index("-p") + 1]
                if name not in packages:
                    issues.append({"job": job_name, "kind": "unknown-package", "package": name})
                    continue
                directory, manifest = packages[name]
                if "--features" in tokens:
                    features = tokens[tokens.index("--features") + 1]
                    if "$" not in features:
                        for feature in features.split(","):
                            package, separator, feature_name = feature.partition("/")
                            target = packages.get(package) if separator else packages[name]
                            selected = feature_name if separator else feature
                            if target and selected not in target[1].get("features", {}):
                                issues.append({"job": job_name, "kind": "unknown-feature", "package": name, "feature": feature})
                for selector, section, folder in [("--test", "test", "tests"), ("--example", "example", "examples")]:
                    for index, token in enumerate(tokens[:-1]):
                        if token != selector:
                            continue
                        target = tokens[index + 1]
                        declared = any(row.get("name") == target for row in manifest.get(section, []))
                        implicit = (directory / folder / (target + ".rs")).is_file() or (directory / folder / target / "main.rs").is_file()
                        if not declared and not implicit:
                            issues.append({"job": job_name, "kind": "missing-target", "package": name, "target": target})
    return {"jobs": len(data.get("jobs", {})), "cargo_commands": commands, "script_paths": len(scripts), "issues": issues, "passed": not issues}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path("target/governance/ci-workflow.json"))
    parser.add_argument("--tracked-only", action="store_true")
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    tracked = None
    if arguments.tracked_only:
        tracked = set(subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode("utf-8").split("\0"))
    report = inspect_workflow(root, root / ".github/workflows/ci.yml", tracked)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
