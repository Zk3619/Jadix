import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tarfile
import time
import tomllib
from pathlib import Path


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")


def index_path(name):
    if len(name) < 3:
        return f"{len(name)}/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"


def index_record(manifest, checksum):
    dependencies = []
    groups = [(None, manifest)] + list(manifest.get("target", {}).items())
    for target, group in groups:
        for section, kind in [("dependencies", "normal"), ("build-dependencies", "build"), ("dev-dependencies", "dev")]:
            for name, value in group.get(section, {}).items():
                specification = {"version": value} if isinstance(value, str) else value
                if any(key in specification for key in ("path", "git", "workspace")):
                    raise ValueError(f"unresolved dependency in packaged manifest: {name}")
                registry = specification.get("registry-index")
                if registry and registry != "https://github.com/rust-lang/crates.io-index":
                    raise ValueError(f"unexpected registry: {registry}")
                dependencies.append({"name": name, "req": specification["version"],
                                     "features": specification.get("features", []),
                                     "optional": specification.get("optional", False),
                                     "default_features": specification.get("default-features", True),
                                     "target": target, "kind": kind, "registry": None,
                                     "package": specification.get("package")})
    package = manifest["package"]
    return {"name": package["name"], "vers": package["version"], "deps": dependencies,
            "cksum": checksum, "features": {}, "features2": manifest.get("features", {}),
            "v": 2, "yanked": False, "links": package.get("links"),
            "rust_version": package.get("rust-version")}


def run(arguments, directory, log):
    started = time.monotonic()
    with log.open("wb") as output:
        result = subprocess.run(arguments, cwd=directory, stdout=output, stderr=subprocess.STDOUT)
    record = {"arguments": list(map(str, arguments)), "cwd": str(directory),
              "exit_code": result.returncode, "elapsed_seconds": time.monotonic() - started,
              "log": str(log), "sha256": digest(log)}
    print(f"{log.stem}: exit={result.returncode}", flush=True)
    return record


def validate_provenance(directory, files):
    for path in directory.glob("JADIX_*.md"):
        if files.get(path.name) != digest(path):
            raise ValueError(f"provenance absent or changed in archive: {path}")


def validate_upload_size(path, limit=10 * 1024 * 1024):
    size = path.stat().st_size
    if size > limit:
        raise ValueError(f"archive exceeds default crates.io upload limit: {path.name}: {size} > {limit}")
    return size


def stage(root, closure, evidence):
    registry = evidence / "registry"
    (registry / "index").mkdir(parents=True)
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    packages = {package["id"]: package for package in closure["packages"]}
    records = []
    save(evidence / "archives.json", records)
    ordered = [packages[identity] for identity in closure["publication_order"]]
    ordered += [package for package in closure["packages"] if package["source"]]
    for package in ordered:
        name, version = package["name"], package["version"]
        filename = f"{name}-{version}.crate"
        entry = registry / "index" / index_path(name)
        entry.parent.mkdir(parents=True, exist_ok=True)
        if package["source"]:
            sources = list((cargo_home / "registry/cache").glob(f"*/{filename}"))
            if not sources:
                raise ValueError(f"missing cached archive: {filename}")
            source = sources[0]
            caches = list((cargo_home / "registry/index").glob(f"*/.cache/{index_path(name)}"))
            candidates = []
            for cache in caches:
                for part in cache.read_bytes().split(b"\0"):
                    if part.startswith(b'{"name":'):
                        candidates.append(json.loads(part))
            expected = next((item for item in candidates if item["vers"] == version), None)
            if not expected or expected["cksum"] != digest(source):
                raise ValueError(f"upstream index/archive checksum mismatch: {filename}")
            selected = {item["version"] for item in closure["packages"] if item["name"] == name and item["source"]}
            entries = {item["vers"]: item for item in candidates if item["vers"] in selected}
            entry.write_text("".join(json.dumps(item) + "\n" for item in entries.values()), encoding="utf-8")
        else:
            command = ["cargo", "package", "--manifest-path", package["manifest"],
                       "--allow-dirty", "--no-verify", "--exclude-lockfile", "--offline",
                       "--target-dir", str(root / "target")]
            record = run(command, root, evidence / f"package-{name}.log")
            if record["exit_code"]:
                raise ValueError(f"packaging failed: {name}")
            source = root / "target/package" / filename
            validate_upload_size(source)
            with tarfile.open(source, "r:gz") as archive:
                members = archive.getmembers()
                prefix = f"{name}-{version}/"
                files = {member.name[len(prefix):]: hashlib.sha256(archive.extractfile(member).read()).hexdigest()
                         for member in members if member.isfile() and member.name.startswith(prefix)}
                manifest = tomllib.loads(archive.extractfile(prefix + "Cargo.toml").read().decode("utf-8"))
                for notice in package["notices"]:
                    if notice.replace("\\", "/") not in files:
                        raise ValueError(f"notice absent from archive: {name}/{notice}")
                validate_provenance(Path(package["manifest"]).parent, files)
            candidate = index_record(manifest, digest(source))
            entry.write_text(json.dumps(candidate) + "\n", encoding="utf-8")
            record.update({"archive_files_sha256": files, "index": candidate})
        shutil.copy2(source, registry / filename)
        record = {"name": name, "version": version, "local": not bool(package["source"]),
                  "archive_sha256": digest(registry / filename), "source": str(source),
                  "index_sha256": digest(entry), **(record if not package["source"] else {})}
        records.append(record)
        save(evidence / "archives.json", records)
    return registry


def consumers(root, destination, registry, evidence, target_directory=None):
    if destination == root or root in destination.parents:
        raise ValueError("consumer directory must be outside the repository")
    destination.mkdir(exist_ok=False)
    configuration = f'[source.crates-io]\nreplace-with = "r4-candidate"\n[source.r4-candidate]\nlocal-registry = {json.dumps(registry.as_posix())}\n'
    checks = []
    for product in (False, True):
        directory = destination / ("components" if product else "framework")
        (directory / "tests").mkdir(parents=True)
        (directory / ".cargo").mkdir()
        (directory / ".cargo/config.toml").write_text(configuration, encoding="utf-8")
        fixture = root / ("scripts/checks/fixtures/product-distributions.rs" if product
                          else "crates/jadix/tests/fixtures/optional_macros/consumer.rs")
        shutil.copy2(fixture, directory / "tests/contracts.rs")
        manifest = '[package]\nname = "r4-' + directory.name + '"\nversion = "0.0.0"\nedition = "2021"\npublish = false\n[workspace]\n'
        manifest += '[features]\nmacros = ["ui_runtime/macros"' + (', "widgets/macros", "icons/macros", "drag_product/macros"' if product else '') + ']\ndevtools = ["ui_runtime/devtools"]\nhot-reload = ["ui_runtime/hot-reload"]\n'
        if product:
            manifest += 'animation-path = ["icons/animation-path"]\n'
        manifest += '[dependencies]\nui_runtime = { package = "jadix", version = "=0.1.0", default-features = false' + (', features = ["svg", "animation-base"]' if product else '') + ' }\n'
        if product:
            manifest += 'widgets = { package = "jadix_components", version = "=0.1.0", default-features = false }\nicons = { package = "jadix_lucide", version = "=0.1.0", default-features = false, features = ["animated", "svg"] }\ndrag_product = { package = "jadix_dnd", version = "=0.1.0", default-features = false }\n'
        (directory / "Cargo.toml").write_text(manifest, encoding="utf-8")
        modes = ("builder", "macros", "macros,animation-path") if product else ("builder", "devtools", "hot-reload", "devtools,hot-reload", "macros,devtools,hot-reload")
        for mode in modes:
            features = [] if mode == "builder" else ["--features", mode]
            label = directory.name + "-" + mode.replace(",", "-")
            tree = run(["cargo", "tree", "--offline", "--edges", "normal,build", "--prefix", "none", "--format", "{p}", *features], directory, evidence / f"{label}-tree.log")
            checks.append(tree)
            if tree["exit_code"]:
                save(evidence / "consumer-results.json", checks)
                return False
            text = Path(tree["log"]).read_text(encoding="utf-8")
            assert bool(re.search(r"^jadix_macros v", text, re.M)) == ("macros" in mode)
            if not product:
                assert not re.search(r"^jadix_(?:components|lucide|dnd) v", text, re.M)
            check = run(["cargo", "test", "--offline", "--target-dir", str(target_directory or destination / "target"), *features, "--test", "contracts", "--", "--test-threads=1"], directory, evidence / f"{label}-test.log")
            checks.append(check)
            save(evidence / "consumer-results.json", checks)
            text = Path(check["log"]).read_text(encoding="utf-8")
            if check["exit_code"] or not re.search(r"test result: ok\. [1-9]\d* passed; 0 failed", text):
                return False
            host = next(line.removeprefix("host: ") for line in subprocess.check_output(["rustc","-vV"],text=True).splitlines() if line.startswith("host: "))
            metadata = run(["cargo", "metadata", "--offline", "--locked", "--format-version", "1", "--filter-platform", host, *features], directory, evidence / f"{label}-metadata.json")
            checks.append(metadata)
            if metadata["exit_code"]:
                save(evidence / "consumer-results.json", checks)
                return False
            raw = Path(metadata["log"]).read_text(encoding="utf-8")
            data = json.loads(raw[raw.index('{'):])
            assert all(package["source"] and package["source"].startswith("registry+") for package in data["packages"] if package["name"] != "r4-" + directory.name)
            gpu_tree = run(["cargo", "tree", "--offline", "--edges", "features", "--invert", "jadix_wgpu", "--target", host, "--prefix", "none", "--format", "{p}", *features], directory, evidence / f"{label}-gpu-features.log")
            checks.append(gpu_tree)
            if gpu_tree["exit_code"]:
                save(evidence / "consumer-results.json", checks)
                return False
            gpu_features = sorted(set(re.findall(r'^jadix_wgpu feature "([^"]+)"', Path(gpu_tree["log"]).read_text(encoding="utf-8"), re.M)))
            assert gpu_features and not set(gpu_features).intersection({"default","gles","metal","vulkan","web","webgpu"}), "backend-free consumers must not regain GPU default backends"
            save(evidence / f"{label}-gpu-features.json", {"host":host,"resolved":gpu_features,"authority":"target-specific cargo tree; cargo metadata feature sets can unify other targets"})
            save(evidence / "consumer-results.json", checks)
        shutil.copy2(directory / "Cargo.lock", evidence / f"{directory.name}-Cargo.lock")
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--closure", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--consumers", type=Path, required=True)
    parser.add_argument("--resume-consumers", action="store_true")
    parser.add_argument("--target-dir", type=Path)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    evidence = arguments.evidence.resolve()
    if arguments.resume_consumers:
        registry = evidence / "registry"
    else:
        evidence.mkdir(parents=True, exist_ok=False)
        closure = json.loads(arguments.closure.read_text(encoding="utf-8-sig"))
        if not closure["ready_for_r3_acceptance"]:
            raise ValueError("R3 closure contains blockers")
        record = run(["cargo", "metadata", "--format-version", "1", "--all-features", "--locked", "--offline"], root, evidence / "registry-resolution.json")
        if record["exit_code"]:
            raise ValueError("workspace test/development resolution failed")
        raw = (evidence / "registry-resolution.json").read_text(encoding="utf-8")
        metadata = json.loads(raw[raw.index('{'):])
        known = {package["id"] for package in closure["packages"]}
        closure["packages"].extend(package for package in metadata["packages"] if package["source"] and package["id"] not in known)
        registry = stage(root, closure, evidence)
    passed = consumers(root, arguments.consumers.resolve(), registry, evidence, arguments.target_dir)
    save(evidence / "package-consumption.json", {"registry": str(registry), "consumers": str(arguments.consumers.resolve()),
         "passed": passed, "scope": "real Cargo archives and offline controlled local registry; version-only external consumers; not target-registry publication"})
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
