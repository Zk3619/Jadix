import argparse
import json
import re
import tomllib
from pathlib import Path


def inspect_crate(directory, bundled=()):
    source = directory / "src"
    groups = {"private": [], "host_contracts": [], "public_contracts": [], "fixtures": [], "inline": [], "upstream": []}
    issues = []
    for path in sorted(source.rglob("*.rs")) if source.exists() else []:
        relative = path.relative_to(directory).as_posix()
        parts = path.relative_to(source).parts
        if any(path.is_relative_to(parent) for parent in bundled):
            groups["upstream"].append(relative)
            continue
        if "tests" in parts:
            groups["private"].append(relative)
        elif "crate_tests" in parts:
            groups["host_contracts"].append(relative)
        else:
            text = path.read_text(encoding="utf-8")
            if re.search(r"(?m)^\s*#\[test\]", text):
                groups["inline"].append(relative)
                issues.append({"path": relative, "reason": "private_test_outside_owner_test_directory"})
            if path.name.endswith("_tests.rs") or path.name == "tests.rs":
                issues.append({"path": relative, "reason": "loose_test_source"})
    public = directory / "tests"
    for path in sorted(public.rglob("*")) if public.exists() else []:
        if not path.is_file():
            continue
        relative = path.relative_to(directory).as_posix()
        if path.parent == public and path.suffix == ".rs":
            groups["public_contracts"].append(relative)
        else:
            groups["fixtures"].append(relative)
    return {"crate": directory.name, "groups": groups, "issues": issues}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    provenance = json.loads((root / "vendor/RELEASE_FORKS.json").read_text(encoding="utf-8"))
    bundled = [root / source["directory"] for source in provenance.get("bundled_sources", []) if source.get("directory")]
    forks = provenance["forks"]
    fork_directories = {root / fork["directory"] for fork in forks}
    own = [inspect_crate(path.parent, bundled) for path in sorted((root / "crates").glob("*/Cargo.toml")) if path.parent not in fork_directories]
    maintained = []
    for fork in forks:
        directory = root / fork["directory"]
        manifest = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))
        maintained.append({"crate": manifest["package"]["name"], "directory": fork["directory"], "policy": "preserve_upstream_test_layout", "rust_test_files": sum(1 for path in directory.rglob("*.rs") if "tests" in path.parts or path.name.endswith("_tests.rs"))})
    issues = [{"crate": record["crate"], **issue} for record in own for issue in record["issues"]]
    result = {"own_crates": own, "maintained_forks": maintained, "issues": issues, "passed": not issues,
              "scope": "physical ownership and discovery layout; actual test identities and runtime results require Cargo logs"}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"own_crates": len(own), "maintained_forks": len(maintained), "private_test_files": sum(len(record["groups"]["private"]) for record in own), "issues": issues, "passed": result["passed"]}, ensure_ascii=False))
    raise SystemExit(0 if result["passed"] else 1)


if __name__ == "__main__":
    main()
