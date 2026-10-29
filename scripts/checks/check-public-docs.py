import argparse
import importlib.util
import json
import re
import subprocess
from pathlib import Path
from urllib.parse import unquote, urlsplit

STAGE = re.compile(r"(?<![A-Za-z0-9])(?:HR\d+|R[0-5]|TG\d+|DG\d+)(?![A-Za-z0-9])")


def documentation_issues(name, text):
    issues = []
    for number, line in enumerate(text.splitlines(), 1):
        if "development-docs" in line:
            issues.append({"path": name, "line": number, "reason": "private_development_document_reference"})
        if STAGE.search(line):
            issues.append({"path": name, "line": number, "reason": "internal_execution_stage"})
    return issues


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    specification = importlib.util.spec_from_file_location("package_documents", root / "scripts/release/check-r4-documents.py")
    documents = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(documents)
    names = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root).decode("utf-8").split("\0")
    names += ["README.zh.md", *(path.relative_to(root).as_posix() for path in (root / "docs").rglob("*.md"))]
    issues, links, inspected = [], [], []
    for name in sorted(set(names)):
        path = root / name
        if path.suffix.lower() != ".md" or not path.is_file():
            continue
        own = "/" not in name or name.startswith(("docs/", "crates/", "examples/", "scripts/", ".github/"))
        maintained = name.startswith("vendor/") and path.name.startswith("JADIX_")
        if not (own or maintained):
            continue
        text = path.read_text(encoding="utf-8-sig")
        inspected.append(name)
        issues.extend(documentation_issues(name, text))
        for entry in re.findall(r"(?<![/A-Za-z0-9_-])scripts/[A-Za-z0-9_./-]+\.(?:py|ps1|sh|mjs)\b", text):
            if not (root / entry).is_file():
                issues.append({"path": name, "target": entry, "reason": "missing_tool_entry"})
        for target in documents.local_links(text):
            resolved = (path.parent / unquote(urlsplit(target).path)).resolve()
            exists = resolved.exists()
            links.append({"path": name, "target": target, "exists": exists})
            if not exists:
                issues.append({"path": name, "target": target, "reason": "missing_local_file"})
    ignored = subprocess.run(["git", "check-ignore", "--no-index", "-z", "--stdin"], cwd=root, input="\0".join(inspected) + "\0", text=True, encoding="utf-8", stdout=subprocess.PIPE, check=False)
    if ignored.returncode not in (0, 1):
        raise RuntimeError("cannot verify public document distribution")
    issues.extend({"path": name, "reason": "public_document_ignored_by_git"} for name in ignored.stdout.split("\0") if name)
    non_markdown = [str(path.relative_to(root)) for path in (root / "docs").rglob("*") if path.is_file() and path.suffix.lower() != ".md"]
    issues.extend({"path": name, "reason": "non_document_in_public_docs"} for name in non_markdown)
    result = {"documents": inspected, "relative_links": links, "non_markdown_files": non_markdown, "issues": issues, "passed": not issues,
              "scope": "first-party public Markdown and maintained fork provenance; upstream history/licenses remain intact; archive contents are checked separately"}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"documents": len(inspected), "relative_links": len(links), "issues": issues, "passed": result["passed"]}, ensure_ascii=False))
    raise SystemExit(0 if result["passed"] else 1)


if __name__ == "__main__":
    main()
