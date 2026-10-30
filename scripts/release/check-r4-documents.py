import argparse
import hashlib
import json
import posixpath
import re
import tarfile
from pathlib import Path
from urllib.parse import unquote, urlsplit


def local_links(text):
    text = re.sub(r"^```[^\n]*\n.*?^```[^\n]*$", "", text, flags=re.M | re.S)
    targets = re.findall(r"\]\(<?([^\s)>]+)>?(?:\s+[^)]*)?\)", text)
    targets += re.findall(r"^\s*\[[^\]]+\]:\s*<?([^\s>]+)", text, flags=re.M)
    targets += re.findall(r"(?:src|href)=[\"']([^\"']+)[\"']", text)
    return [target for target in targets if not urlsplit(target).scheme and not target.startswith(("#", "//"))]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--archives", type=Path, required=True)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    packages = [row for row in json.loads(arguments.archives.read_text(encoding="utf-8")) if row["local"]]
    if not packages or len({package["name"] for package in packages}) != len(packages):
        raise ValueError("expected a nonempty publication closure with unique package names")
    links = []
    manuals = {}
    archives = []
    bilingual_readmes = {}
    for package in packages:
        archive = arguments.registry / f"{package['name']}-{package['version']}.crate"
        prefix = f"{package['name']}-{package['version']}/"
        with tarfile.open(archive, "r:gz") as source:
            files = {member.name[len(prefix):]: source.extractfile(member).read() for member in source.getmembers() if member.isfile()}
        bilingual_readmes[package["name"]] = {name: name in files for name in ("README.md", "README.zh.md")}
        for path, contents in files.items():
            if not path.lower().endswith(".md"):
                continue
            for target in local_links(contents.decode("utf-8-sig")):
                resolved = posixpath.normpath(posixpath.join(posixpath.dirname(path), unquote(urlsplit(target).path)))
                present = resolved in files or any(name.startswith(resolved.rstrip("/") + "/") for name in files)
                links.append({"package": package["name"], "document": path, "target": target, "resolved": resolved, "exists": present})
        if package["name"] == "jadix":
            manuals = {name: name in files for name in ("README.md", "MOTION.md", "SCALE_PREVIEW.md", "THEMES.md", "DOCUMENT_EDITING.md", "TEXT_DOCUMENT_VIEWS.md")}
        archives.append({"package": package["name"], "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest()})
    missing = [row for row in links if not row["exists"]]
    result = {"archives": archives, "relative_links": links, "missing": missing, "manuals": manuals,
              "scope": "actual archive Markdown inline/reference and HTML local file targets; remote reachability, anchors and public API semantic completeness are separate checks",
              "bilingual_readmes": bilingual_readmes,
              "passed": not missing and bool(manuals) and all(manuals.values()) and all(all(pair.values()) for pair in bilingual_readmes.values())}
    arguments.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"archives": len(archives), "relative_links": len(links), "missing": missing, "manuals": manuals, "bilingual_pairs": sum(all(pair.values()) for pair in bilingual_readmes.values())}, ensure_ascii=False, indent=2))
    raise SystemExit(0 if result["passed"] else 1)


if __name__ == "__main__":
    main()
