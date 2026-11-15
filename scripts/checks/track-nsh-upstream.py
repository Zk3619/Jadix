#!/usr/bin/env python3
"""Capture an immutable NSH comparison. Read-only GitHub access; no checkout/build."""
import argparse
import csv
from datetime import datetime, timezone
import hashlib
import gzip
import json
from pathlib import Path
import re
import subprocess
from http.client import IncompleteRead
from urllib.request import Request, urlopen

REPOSITORY = "longbridge/nsh"
API = f"https://api.github.com/repos/{REPOSITORY}"


def get(path, repository=REPOSITORY):
    for attempt in range(3):
        request = Request(f"https://api.github.com/repos/{repository}" + path, headers={"User-Agent": "jadix-upstream-tracker",
                                              "Accept": "application/vnd.github+json",
                                              "Accept-Encoding": "gzip" if attempt == 0 else "identity"})
        try:
            with urlopen(request, timeout=60) as response:
                payload = response.read()
                if response.headers.get("Content-Encoding") == "gzip":
                    payload = gzip.decompress(payload)
                return json.loads(payload)
        except IncompleteRead:
            if attempt == 2:
                raise


def capture_platforms(output, base):
    import io
    import zipfile
    from pathlib import PurePosixPath

    artifacts = {}

    def save(name, value):
        payload = (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()
        (output / name).write_bytes(payload)
        artifacts[name] = hashlib.sha256(payload).hexdigest()

    def pages(path):
        values = []
        page = 1
        while True:
            chunk = get(f"{path}?per_page=100&page={page}")
            values.extend(chunk)
            if len(chunk) < 100:
                return values
            page += 1

    for number in (473, 474, 475, 1393):
        save(f"pr-{number}.json", get(f"/pulls/{number}"))
        save(f"comments-{number}.json", pages(f"/issues/{number}/comments"))
        save(f"reviews-{number}.json", pages(f"/pulls/{number}/reviews"))
    repository = "huacnlee/nsh-platform-miniprogram"
    metadata = get("", repository)
    head = get(f"/commits/{metadata['default_branch']}", repository)
    sha = head["sha"]
    tree = get(f"/git/trees/{sha}?recursive=1", repository)
    if tree.get("truncated"):
        raise ValueError("miniprogram tree is incomplete")
    save("miniprogram-repository.json", metadata)
    save("miniprogram-head.json", head)
    save("miniprogram-tree.json", tree)
    if base:
        comparison = complete_comparison(base, sha, repository)
        save("miniprogram-compare.json", comparison)
        save("miniprogram-report.json", summarize(base, sha, comparison, repository))
    request = Request(f"https://api.github.com/repos/{repository}/zipball/{sha}",
                      headers={"User-Agent": "jadix-upstream-tracker"})
    with urlopen(request, timeout=60) as response:
        archive = response.read(32 * 1024 * 1024 + 1)
    if len(archive) > 32 * 1024 * 1024:
        raise ValueError("miniprogram archive exceeds capture limit")
    (output / "miniprogram-source.zip").write_bytes(archive)
    artifacts["miniprogram-source.zip"] = hashlib.sha256(archive).hexdigest()
    blobs = {item["path"]: item for item in tree["tree"] if item["type"] == "blob"}
    verified = {}
    with zipfile.ZipFile(io.BytesIO(archive)) as bundle:
        total = 0
        for entry in bundle.infolist():
            if entry.is_dir():
                continue
            parts = PurePosixPath(entry.filename).parts[1:]
            if not parts or any(part in (".", "..") or ":" in part or "\\" in part for part in parts):
                raise ValueError("unsafe miniprogram archive path")
            path = "/".join(parts)
            total += entry.file_size
            if total > 128 * 1024 * 1024 or path not in blobs or path in verified:
                raise ValueError("unexpected miniprogram archive entry")
            content = bundle.read(entry)
            digest = hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest()
            if digest != blobs[path]["sha"]:
                raise ValueError(f"miniprogram blob identity mismatch: {path}")
            verified[path] = {"git_blob": digest, "sha256": hashlib.sha256(content).hexdigest()}
            target = output / "miniprogram-sources" / Path(*parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
    if set(verified) != set(blobs):
        raise ValueError("miniprogram archive omitted tree blobs")
    save("platforms-manifest.json", {"observed_at": datetime.now(timezone.utc).isoformat(),
         "repository": repository, "head": sha, "blobs": verified, "artifacts": dict(artifacts),
         "limits": ["Static source and PR observations; platform runtime remains unverified."]})


def impact(path):
    """Shared code changes can affect terminals even without TUI file changes."""
    if path.startswith(("crates/nsh-platform-tui/", "crates/nsh-tui-host/")):
        return "terminal"
    if path.startswith(("crates/nsh-base/", "crates/nsh-core/", "crates/nsh-js/", "crates/nsh-engine/",
                        "crates/nsh-runtime/", "crates/nsh-layout/",
                        "crates/base/", "crates/component/", "crates/gpui/")):
        return "shared-runtime"
    if path.startswith("crates/"):
        return "platform-or-tooling"
    return "docs-ci-examples"


def summarize(base, head, comparison, repository=REPOSITORY):
    if comparison["base_commit"]["sha"] != base or comparison["status"] not in ("ahead", "identical"):
        raise ValueError("base diverged or mismatched; choose an ancestor explicitly")
    if comparison["url"] != f"https://api.github.com/repos/{repository}/compare/{base}...{head}":
        raise ValueError("head identity mismatch")
    files = comparison.get("files", [])
    # GitHub compare returns at most 300 files. Preserve the useful commit
    # receipt, while making the changed-path inventory's incomplete coverage explicit.
    files_complete = len(files) < 300
    limits = ["Static upstream observations; no runtime or parity validation.",
              "Impact labels are path heuristics; inspect shared dependencies separately.",
              "A missing patch is not proof of no code change."]
    if not files_complete:
        limits.append("GitHub compare returned its 300-file cap; changed-path inventory is incomplete and omitted paths are unknown.")
    return {"base": base, "head": head, "status": comparison["status"],
            "total_commits": comparison["total_commits"], "files_count": len(files),
            "files_complete": files_complete, "file_cap": 300,
            "files": [{"path": f["filename"], "status": f["status"],
                       "blob_sha": f["sha"], "impact": impact(f["filename"]),
                       **({"previous_path": f["previous_filename"]} if "previous_filename" in f else {})}
                      for f in files],
            "compare_url": f"https://github.com/{repository}/compare/{base}...{head}",
            "limits": limits}


def complete_comparison(base, head, repository=REPOSITORY):
    comparison = get(f"/compare/{base}...{head}?per_page=100", repository)
    summarize(base, head, comparison, repository)
    commits = list(comparison["commits"])
    page = 2
    while len(commits) < comparison["total_commits"]:
        extra = get(f"/compare/{base}...{head}?per_page=100&page={page}", repository)["commits"]
        if not extra:
            raise ValueError("incomplete commit pagination")
        commits.extend(extra)
        page += 1
    if len(commits) != comparison["total_commits"] or len({commit["sha"] for commit in commits}) != len(commits):
        raise ValueError("inconsistent commit pagination")
    return dict(comparison, commits=commits)


def tree_changes(before, after):
    if any(tree.get("truncated") or not isinstance(tree.get("tree"), list) for tree in [before, after]):
        raise ValueError("recursive tree inventory is incomplete")
    left, right = [{item["path"]: item for item in tree["tree"] if item["type"] != "tree"} for tree in [before, after]]
    return [{"path": path, "status": "added" if path not in left else "removed" if path not in right else "modified",
             "blob_sha": (right.get(path) or left[path])["sha"], "impact": impact(path)}
            for path in sorted(left.keys() | right.keys())
            if left.get(path) != right.get(path)]


def git(repository, *arguments):
    result = subprocess.run(["git", "-C", str(repository), *arguments],
                            check=True, capture_output=True, text=True, encoding="utf-8")
    return result.stdout


def local_reference_report(repository, base, head="origin/main", label=REPOSITORY):
    """Build a complete source delta from an existing local Git mirror."""
    base_sha = git(repository, "rev-parse", "--verify", f"{base}^{{commit}}").strip()
    head_sha = git(repository, "rev-parse", "--verify", f"{head}^{{commit}}").strip()
    if not re.fullmatch(r"[0-9a-f]{40}", base_sha) or not re.fullmatch(r"[0-9a-f]{40}", head_sha):
        raise ValueError("local refs must resolve to full commit identities")
    subprocess.run(["git", "-C", str(repository), "merge-base", "--is-ancestor", base_sha, head_sha],
                   check=True, capture_output=True)

    commits = []
    for line in git(repository, "log", "--format=%H%x09%cI%x09%s", f"{base_sha}..{head_sha}").splitlines():
        sha, date, message = line.split("\t", 2)
        commits.append({"sha": sha, "date": date, "message": message})

    raw_paths = git(repository, "diff", "--name-status", "-M", "-z", base_sha, head_sha).split("\0")
    files = []
    index = 0
    while index < len(raw_paths) and raw_paths[index]:
        status = raw_paths[index]
        index += 1
        if status.startswith("R") or status.startswith("C"):
            previous_path, path = raw_paths[index:index + 2]
            index += 2
        else:
            previous_path = None
            path = raw_paths[index]
            index += 1
        revision = base_sha if status.startswith("D") else head_sha
        blob = git(repository, "rev-parse", f"{revision}:{path}").strip()
        files.append({"path": path, "status": "renamed" if status.startswith("R") else
                      "copied" if status.startswith("C") else
                      "added" if status.startswith("A") else
                      "removed" if status.startswith("D") else "modified",
                      "blob_sha": blob, "impact": impact(path),
                      **({"previous_path": previous_path} if previous_path else {})})
    shortstat = git(repository, "diff", "--shortstat", base_sha, head_sha).strip()
    numbers = re.search(r"(\d+) files? changed(?:, (\d+) insertions?\(\+\))?(?:, (\d+) deletions?\(-\))?", shortstat)
    return {"repository": label, "base": base_sha, "head": head_sha,
            "status": "ahead" if commits else "identical", "total_commits": len(commits),
            "files_count": len(files), "files_complete": True, "files": files,
            "commits": commits, "shortstat": shortstat,
            "insertions": int(numbers.group(2) or 0) if numbers else 0,
            "deletions": int(numbers.group(3) or 0) if numbers else 0,
            "inventory_source": "complete local Git commit and tree diff",
            "limits": ["PR discussion, reviews, CI and external platform manifests are not included.",
                       "Impact labels are path heuristics; inspect shared dependencies separately."]}


def local_source_inventory(repository, head):
    """Record the whole current tree separately from the reviewed delta ledger."""
    entries = []
    for entry in git(repository, "ls-tree", "-r", "-z", head).split("\0"):
        if not entry:
            continue
        metadata, path = entry.split("\t", 1)
        mode, kind, sha = metadata.split(" ")
        entries.append({"path": path, "mode": mode, "kind": kind, "object_sha": sha})
    refs = []
    for line in git(repository, "for-each-ref", "--format=%(refname)%09%(objectname)",
                    "refs/remotes/origin/").splitlines():
        ref, sha = line.split("\t", 1)
        if ref != "refs/remotes/origin/HEAD":
            refs.append({"ref": ref, "sha": sha})
    return {"head": head, "entries": entries, "entries_count": len(entries),
            "remote_refs": refs, "remote_refs_count": len(refs),
            "limits": ["Tree membership is not a semantic difference or parity claim.",
                       "Remote refs reflect the local fetch snapshot; branch-exclusive trees are not inventoried."]}


def write_source_inventory(payload, path):
    """Export complete tree membership; deliberately make no semantic review claim."""
    with path.open("x", newline="", encoding="utf-8") as stream:
        writer = csv.writer(stream)
        writer.writerow(["repository", "head", "source_path", "mode", "kind", "object_sha"])
        for name, report in sorted(payload["repositories"].items()):
            for entry in report.get("source_inventory", {}).get("entries", []):
                writer.writerow([name, report["head"], entry["path"], entry["mode"],
                                 entry["kind"], entry["object_sha"]])


def capture_local_references(reference_root, output, nsh_base, miniprogram_base=None,
                             gpui_kit_base=None, gpui_fast_base=None, source_inventory=False):
    reports = {"nsh": local_reference_report(reference_root / "nsh", nsh_base)}
    for name, base in (("gpui-kit", gpui_kit_base), ("gpui-fast", gpui_fast_base)):
        if base:
            reports[name] = local_reference_report(reference_root / name, base,
                                                   label=f"longbridge/{name}")
    if miniprogram_base:
        reports["miniprogram"] = local_reference_report(
            reference_root / "nsh-platform-miniprogram", miniprogram_base,
            label="huacnlee/nsh-platform-miniprogram")
    if source_inventory:
        for name, report in reports.items():
            directory = "nsh-platform-miniprogram" if name == "miniprogram" else name
            report["source_inventory"] = local_source_inventory(reference_root / directory,
                                                                report["head"])
    payload = {"observed_at": datetime.now(timezone.utc).isoformat(),
               "source": "local-git-mirrors", "repositories": reports,
               "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    output.mkdir(parents=True)
    (output / "local-report.json").write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
    if source_inventory:
        write_source_inventory(payload, output / "source-inventory.csv")
    return payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="previous full 40-character commit SHA")
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory")
    parser.add_argument("--platforms", action="store_true", help="also capture OHOS PRs and verified miniprogram sources")
    parser.add_argument("--miniprogram-base", help="previous external platform commit SHA")
    parser.add_argument("--gpui-kit-base", help="previous GPUI Kit commit SHA (local mode)")
    parser.add_argument("--gpui-fast-base", help="previous GPUI Fast commit SHA (local mode)")
    parser.add_argument("--source-inventory", action="store_true",
                        help="include whole current trees and local remote-ref identities (local mode)")
    parser.add_argument("--local-reference-root", type=Path,
                        help="compare existing ignored Reference/nsh and miniprogram Git mirrors without GitHub API")
    args = parser.parse_args()
    if args.local_reference_root:
        if not args.base:
            parser.error("--base is required with --local-reference-root")
        if args.platforms:
            parser.error("--platforms is API-only; use --miniprogram-base for local source comparison")
        if not re.fullmatch(r"[0-9a-f]{40}", args.base):
            parser.error("base must be a full lowercase SHA")
        if args.miniprogram_base and not re.fullmatch(r"[0-9a-f]{40}", args.miniprogram_base):
            parser.error("miniprogram base must be a full lowercase SHA")
        for name, base in (("gpui-kit", args.gpui_kit_base), ("gpui-fast", args.gpui_fast_base)):
            if base and not re.fullmatch(r"[0-9a-f]{40}", base):
                parser.error(f"{name} base must be a full lowercase SHA")
        if args.output.exists():
            parser.error("output already exists; retain prior evidence and choose a new directory")
        try:
            reports = capture_local_references(args.local_reference_root, args.output,
                                               args.base, args.miniprogram_base,
                                               args.gpui_kit_base, args.gpui_fast_base,
                                               args.source_inventory)
        except (subprocess.CalledProcessError, ValueError) as error:
            parser.error(f"local Git comparison failed: {error}")
        for name, report in reports["repositories"].items():
            print(f"{name}: {report['base'][:8]} -> {report['head'][:8]}: "
                  f"{report['total_commits']} commits, {report['files_count']} files")
        print(args.output / "local-report.json")
        return
    if args.gpui_kit_base or args.gpui_fast_base or args.source_inventory:
        parser.error("GPUI bases and source inventory require --local-reference-root")
    if not args.base:
        parser.error("--base is required for GitHub API mode")
    if not re.fullmatch(r"[0-9a-f]{40}", args.base):
        parser.error("base must be a full lowercase SHA")
    if args.miniprogram_base and not re.fullmatch(r"[0-9a-f]{40}", args.miniprogram_base):
        parser.error("miniprogram base must be a full lowercase SHA")
    if args.output.exists():
        parser.error("output already exists; retain prior evidence and choose a new directory")
    head = get("/commits/main")
    sha = head["sha"]
    comparison = complete_comparison(args.base, sha)
    report = summarize(args.base, sha, comparison)
    commits = list(comparison["commits"])
    report.update(repository=REPOSITORY, observed_at=datetime.now(timezone.utc).isoformat(),
                  commits=[{"sha": c["sha"], "date": c["commit"]["committer"]["date"],
                            "message": c["commit"]["message"].splitlines()[0]} for c in commits],
                  runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    args.output.mkdir(parents=True)
    if not report["files_complete"]:
        trees = [get(f"/git/trees/{revision}?recursive=1") for revision in [args.base, sha]]
        report["files"] = tree_changes(*trees)
        report["files_count"] = len(report["files"])
        report["files_complete"] = True
        report["inventory_source"] = "complete recursive Git trees; renames represented as removal/addition"
        report["limits"] = [limit for limit in report["limits"] if "300-file cap" not in limit]
        for name, tree in zip(["base-tree.json", "head-tree.json"], trees):
            payload = (json.dumps(tree, ensure_ascii=False, indent=2) + "\n").encode()
            (args.output / name).write_bytes(payload)
            report.setdefault("artifacts", {})[name] = hashlib.sha256(payload).hexdigest()
    for name, value in (("head.json", head), ("compare.json", comparison), ("commits.json", commits)):
        payload = (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()
        (args.output / name).write_bytes(payload)
        report.setdefault("artifacts", {})[name] = hashlib.sha256(payload).hexdigest()
    (args.output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
    if args.platforms:
        capture_platforms(args.output, args.miniprogram_base)
    print(f"{args.base[:8]} -> {sha[:8]}: {len(commits)} commits, {len(report['files'])} files; {args.output}")


if __name__ == "__main__":
    main()
