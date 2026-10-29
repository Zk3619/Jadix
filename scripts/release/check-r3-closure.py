import argparse
import hashlib
import json
import subprocess
import sys
import tomllib
from pathlib import Path


def command(root, *arguments):
    return subprocess.run(arguments, cwd=root, capture_output=True, text=True, encoding="utf-8")


def inspect(root, offline, package_lists=False):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    arguments = ["cargo", "metadata", "--locked", "--format-version", "1", "--all-features"]
    if offline:
        arguments.append("--offline")
    full = command(root, *arguments)
    blockers = []
    complete = full.returncode == 0
    if not complete:
        blockers.append({"kind": "incomplete-resolution", "reason": full.stderr.strip()})
        arguments = ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"]
        if offline:
            arguments.append("--offline")
        full = command(root, *arguments)
    if full.returncode:
        raise RuntimeError(full.stderr)
    metadata = json.loads(full.stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    members = set(metadata["workspace_members"])
    roots = {identity for identity in members if packages[identity].get("publish") != []}
    nodes = {node["id"]: node for node in (metadata.get("resolve") or {}).get("nodes", [])}
    edges = {}
    visited = set()
    pending = list(roots)
    while pending:
        identity = pending.pop()
        if identity in visited:
            continue
        visited.add(identity)
        node = nodes.get(identity)
        dependencies = []
        if node:
            dependencies = [dependency["pkg"] for dependency in node["deps"]
                            if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"])]
        edges[identity] = sorted(set(dependencies))
        pending.extend(dependencies)
    if not complete:
        visited = members
    release = []
    for identity in sorted(visited):
        package = packages[identity]
        local = package.get("source") is None
        record = {"id": identity, "name": package["name"], "version": package["version"],
                  "source": package.get("source"), "license": package.get("license"),
                  "license_file": package.get("license_file"), "publish": package.get("publish"),
                  "features": package["features"], "dependencies": package["dependencies"],
                  "targets": package["targets"], "manifest": package["manifest_path"], "rust_version": package.get("rust_version")}
        if (package.get("source") or "").startswith("git+"):
            blockers.append({"kind": "git-package", "package": identity, "source": package["source"]})
        if local:
            manifest_path = Path(package["manifest_path"])
            record["manifest_sha256"] = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
            record["notices"] = [str(path.relative_to(manifest_path.parent)) for name in ("LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "LICENSE-ZLIB", "LICENSE.MIT", "LICENSE.APACHE", "NOTICE", "THIRD_PARTY_NOTICES.md")
                                 if (path := manifest_path.parent / name).is_file()]
            if package_lists:
                arguments = ["cargo", "package", "--manifest-path", str(manifest_path), "--list", "--allow-dirty"]
                if offline:
                    arguments.append("--offline")
                catalog = command(root, *arguments)
                files = catalog.stdout.splitlines() if catalog.returncode == 0 else []
                file_hashes = {}
                for filename in files:
                    path = manifest_path.parent / filename
                    if path.is_file() and filename != "Cargo.lock":
                        file_hashes[filename] = hashlib.sha256(path.read_bytes()).hexdigest()
                record["package_catalog"] = {"exit_code": catalog.returncode, "files": files, "file_sha256": file_hashes, "diagnostics": catalog.stderr.strip()}
                if catalog.returncode:
                    blockers.append({"kind": "package-catalog-failed", "package": identity, "reason": catalog.stderr.strip()})
                for notice in record["notices"]:
                    if catalog.returncode == 0 and notice.replace("\\", "/") not in files:
                        blockers.append({"kind": "notice-not-packaged", "package": identity, "file": notice})
                readme = package.get("readme")
                if readme and not (manifest_path.parent / readme).is_file():
                    blockers.append({"kind": "readme-missing", "package": identity, "file": readme})
            if package.get("publish") == []:
                blockers.append({"kind": "unpublishable-local-dependency", "package": identity})
            if not package.get("license") and not package.get("license_file"):
                blockers.append({"kind": "missing-license", "package": identity})
            for dependency in package["dependencies"]:
                if dependency.get("kind") == "dev":
                    continue
                if (dependency.get("source") or "").startswith("git+"):
                    blockers.append({"kind": "git-dependency", "package": identity, "dependency": dependency["name"], "source": dependency["source"]})
                if dependency.get("path") and dependency["req"] == "*":
                    blockers.append({"kind": "unversioned-path", "package": identity, "dependency": dependency["name"]})
        release.append(record)
    patches = []
    for registry, entries in manifest.get("patch", {}).items():
        for name, specification in entries.items():
            patches.append({"registry": registry, "name": name, "specification": specification})
            blockers.append({"kind": "root-patch-not-inherited", "name": name, "registry": registry,
                             "required": "Use verified upstream behavior or publish a lawful maintained package and rewrite its complete reference chain."})
    ordered = []
    visiting = set()
    emitted = set()
    local_ids = {identity for identity in visited if packages[identity].get("source") is None}
    def visit(identity):
        if identity in emitted:
            return
        if identity in visiting:
            raise RuntimeError("publication dependency cycle: " + identity)
        visiting.add(identity)
        for dependency in edges.get(identity, []):
            if dependency in local_ids:
                visit(dependency)
        visiting.remove(identity)
        emitted.add(identity)
        ordered.append(identity)
    for identity in sorted(local_ids):
        visit(identity)
    if not complete:
        ordered = []
    identity = command(root, "git", "rev-parse", "HEAD").stdout.strip()
    diff = subprocess.run(["git", "diff", "--binary", "HEAD"], cwd=root, capture_output=True)
    if not package_lists:
        blockers.append({"kind": "package-catalog-not-checked", "required": "Run with --package-lists before R3 acceptance."})
    return {"schema": 1, "source_commit": identity, "working_diff_sha256": hashlib.sha256(diff.stdout).hexdigest(), "lock_sha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(), "edges": edges, "package_catalogs_checked": package_lists, "all_features_all_targets_resolved": complete,
            "scope": "all workspace publishable roots, normal/build edges, all features and targets",
            "publication_order": ordered, "roots": sorted(roots), "packages": release,
            "root_patches": patches, "blockers": blockers, "ready_for_r3_acceptance": not blockers}


def markdown(report):
    packages = {package["id"]: package for package in report["packages"]}
    lines = ["# R3 精确发布闭包", "", "本清单由 scripts/release/check-r3-closure.py 按实际 Cargo 元数据生成。仅表示发布准备；不代表 registry 已上传、名称已占用或 R4/R5 已签收。", "", f"源码提交：`{report['source_commit']}`；Cargo.lock SHA-256：`{report['lock_sha256']}`。", "", f"全 feature / 全 target 元数据：{report['all_features_all_targets_resolved']}；正常和构建依赖 {len(packages)} 包；本地发布项 {len(report['publication_order'])} 包；阻塞 {len(report['blockers'])} 项。", "", "## 发布顺序", "", "| 顺序 | 包 | 版本 | 许可 |", "| --- | --- | --- | --- |"]
    for index, identity in enumerate(report["publication_order"], 1):
        package = packages[identity]
        lines.append(f"| {index} | `{package['name']}` | `{package['version']}` | {package['license'] or package['license_file']} |")
    lines += ["", "## 验证边界", "", "JSON 同时保留各包 feature、target、正常/构建依赖、目标条件、manifest 和实际打包文件哈希。维护叉来源见 vendor/RELEASE_FORKS.json 及每包 JADIX_FORK.md；许可随包保留。", "", "发布时逐包核对 registry 身份与权限并等待依赖可解析。R4 使用受控来源验证包内 Cargo.toml 与仓外消费；R5 才执行真实上传，不能用根 patch 替代下游闭包。", ""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--package-lists", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--markdown", type=Path)
    options = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    report = inspect(root, options.offline, options.package_lists)
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    if options.markdown:
        options.markdown.parent.mkdir(parents=True, exist_ok=True)
        options.markdown.write_text(markdown(report), encoding="utf-8")
    print(f"R3: {len(report['packages'])} packages; {len(report['blockers'])} blockers; report={options.output}")
    for blocker in report["blockers"]:
        print(json.dumps(blocker, ensure_ascii=False))
    return 0 if report["ready_for_r3_acceptance"] else 1


if __name__ == "__main__":
    sys.exit(main())
