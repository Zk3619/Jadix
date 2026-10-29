import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import time
import urllib.request
from pathlib import Path


def matches_requirement(version, requirement):
    value = tuple(map(int, version.split("+")[0].split(".")))
    for clause in requirement.split(","):
        match = re.fullmatch(r"\s*(\^|~|>=|<=|>|<|=)?\s*(\d+(?:\.\d+){0,2})(\.\*)?\s*", clause)
        if not match:
            raise ValueError(f"unsupported requirement; use an explicit version: {requirement}")
        operator, numbers, wildcard = match.groups()
        parts = tuple(map(int, numbers.split(".")))
        lower = parts + (0,) * (3 - len(parts))
        if operator in (">=", "<=", ">", "<", "="):
            accepted = {">=": value >= lower, "<=": value <= lower, ">": value > lower,
                        "<": value < lower, "=": value[:len(parts)] == parts}[operator]
        else:
            position = len(parts) - 1 if wildcard else (0 if len(parts) == 1 else 1) if operator == "~" else next((index for index, number in enumerate(parts) if number), len(parts) - 1)
            upper = list(lower)
            upper[position] += 1
            upper[position + 1:] = [0] * (2 - position)
            accepted = lower <= value < tuple(upper)
        if not accepted:
            return False
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--fetch", nargs="*", default=[])
    parser.add_argument("--repair-from", type=Path)
    parser.add_argument("--reuse", type=Path)
    arguments = parser.parse_args()
    registry = arguments.registry.resolve(strict=True)
    specification = importlib.util.spec_from_file_location("packages", Path(__file__).with_name("check-r4-packages.py"))
    packages = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(packages)
    if arguments.repair_from:
        for row in json.loads(arguments.repair_from.read_text(encoding="utf-8")):
            if not row["exit_code"]:
                continue
            log = arguments.repair_from.parent / (row["package"] + ".log")
            text = log.read_text(encoding="utf-8")
            missing = re.search(r"no matching package named `([^`]+)` found", text) or re.search(r"failed to select a version for the requirement `([^ =`]+) =", text)
            parent = re.search(r"required by package `(\S+) v([^` (]+)", text)
            if not missing or not parent:
                raise ValueError(f"unrecognized Cargo failure: {log}")
            name = missing.group(1)
            path = registry / "index" / packages.index_path(parent.group(1))
            entries = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
            entry = next(item for item in entries if item["vers"] == parent.group(2))
            dependency = next(item for item in entry["deps"] if (item.get("package") or item["name"]) == name)
            selector = name + "@req:" + dependency["req"]
            if selector not in arguments.fetch:
                arguments.fetch.append(selector)
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    records = []
    if arguments.reuse:
        local = {row["name"] for row in json.loads((registry.parent / "archives.json").read_text(encoding="utf-8")) if row["local"]}
        for source in sorted((arguments.reuse / "index").rglob("*")):
            if not source.is_file() or source.name == "config.json":
                continue
            entries = [json.loads(line) for line in source.read_text(encoding="utf-8").splitlines()]
            if not entries or entries[0]["name"] in local:
                continue
            target = registry / "index" / source.relative_to(arguments.reuse / "index")
            present = [json.loads(line) for line in target.read_text(encoding="utf-8").splitlines()] if target.exists() else []
            versions = {entry["vers"] for entry in present}
            for entry in entries:
                archive = arguments.reuse / f"{entry['name']}-{entry['vers']}.crate"
                if packages.digest(archive) != entry["cksum"]:
                    raise ValueError(f"reused archive checksum mismatch: {archive}")
                if entry["vers"] in versions:
                    continue
                shutil.copy2(archive, registry / archive.name)
                present.append(entry)
                versions.add(entry["vers"])
                records.append({"name": entry["name"], "version": entry["vers"], "source": str(archive), "archive_sha256": entry["cksum"], "yanked": entry["yanked"]})
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("".join(json.dumps(entry) + "\n" for entry in present), encoding="utf-8")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def download(url):
        if url.startswith("https://static.crates.io/crates/"):
            name, archive = url.rsplit("/", 2)[-2:]
            version = archive[len(name) + 1:-6]
            url = f"https://rsproxy.cn/api/v1/crates/{name}/{version}/download"
        elif url.startswith("https://index.crates.io/"):
            url = url.replace("https://index.crates.io/", "https://rsproxy.cn/index/", 1)
        for attempt in range(3):
            try:
                with opener.open(url, timeout=20) as response:
                    return response.geturl(), response.read()
            except OSError:
                if attempt == 2:
                    raise
                time.sleep(1)
    for selector in arguments.fetch:
        name, version = selector.rsplit("@", 1)
        url = "https://index.crates.io/" + packages.index_path(name)
        cached_indexes = list((cargo_home / "registry/index").glob("*/.cache/" + packages.index_path(name)))
        if cached_indexes:
            index = b"\n".join(part for part in cached_indexes[0].read_bytes().split(b"\0") if part.startswith(b'{"name":'))
            url = str(cached_indexes[0])
        else:
            url, index = download(url)
        entries = [json.loads(line) for line in index.splitlines()]
        if version.startswith("req:"):
            matches = [item for item in entries if not item["yanked"] and "-" not in item["vers"].split("+")[0] and matches_requirement(item["vers"], version[4:])]
            entry = max(matches, key=lambda item: tuple(map(int, item["vers"].split("+")[0].split("."))))
            version = entry["vers"]
        elif version.count(".") < 2:
            matches = [item for item in entries if item["vers"].startswith(version + ".") and not item["yanked"] and "-" not in item["vers"]]
            entry = max(matches, key=lambda item: tuple(map(int, item["vers"].split("."))))
            version = entry["vers"]
        else:
            entry = next(item for item in entries if item["vers"].split("+")[0] == version)
            version = entry["vers"]
        if entry["yanked"]:
            raise ValueError(f"requested version is yanked: {selector}")
        archive_url, archive = download(f"https://static.crates.io/crates/{name}/{name}-{version}.crate")
        checksum = hashlib.sha256(archive).hexdigest()
        if checksum != entry["cksum"]:
            raise ValueError(f"upstream checksum mismatch: {selector}")
        destination = registry / f"{name}-{version}.crate"
        destination.write_bytes(archive)
        path = registry / "index" / packages.index_path(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        present = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
        merged = {item["vers"]: item for item in present + [entry]}
        path.write_text("".join(json.dumps(item) + "\n" for item in merged.values()), encoding="utf-8")
        snapshot = arguments.evidence.parent / f"{arguments.evidence.stem}-upstream-{name}-{version}.jsonl"
        snapshot.write_bytes(index)
        records.append({"name": name, "version": version, "source": url, "archive_sha256": checksum,
                        "archive_url": archive_url,
                        "index_sha256": hashlib.sha256(index).hexdigest(), "yanked": entry["yanked"]})
        print(f"fetched {selector}", flush=True)
    for archive in sorted((cargo_home / "registry/cache").glob("*/*.crate")):
        filename = archive.name
        name, version = re.fullmatch(r"(.+?)-(\d+\.\d+\.\d+.*)\.crate", filename).groups()
        index = registry / "index" / packages.index_path(name)
        present = [json.loads(line) for line in index.read_text().splitlines()] if index.exists() else []
        if any(item["vers"] == version for item in present):
            continue
        entries = []
        for cache in (cargo_home / "registry/index").glob("*/.cache/" + packages.index_path(name)):
            entries.extend(json.loads(part) for part in cache.read_bytes().split(b"\0") if part.startswith(b'{"name":'))
        entry = next((item for item in entries if item["vers"] == version), None)
        checksum = packages.digest(archive)
        if entry is None or checksum != entry["cksum"]:
            raise ValueError(f"cached archive lacks authenticated index: {filename}")
        index.parent.mkdir(parents=True, exist_ok=True)
        index.write_text("".join(json.dumps(item) + "\n" for item in present + [entry]), encoding="utf-8")
        shutil.copy2(archive, registry / filename)
        records.append({"name": name, "version": version, "source": str(archive), "archive_sha256": checksum,
                        "yanked": entry["yanked"]})
    arguments.evidence.write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"added {len(records)} authenticated archives")


if __name__ == "__main__":
    main()
