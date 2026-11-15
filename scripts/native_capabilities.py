"""Inventory source-declared reference exports without treating matching names as parity."""

import json
from pathlib import Path
import re
import tomllib

import native


def foundation_ledger(reference):
    """Inventory foundations independently of the component export test layout."""
    reference = reference.resolve()
    # Only inspect files present in the source manifest, never traversed symlink directories.
    identity = native.tree_identity(reference)
    declared_files = {row["path"] for row in identity["files"]}
    documents = []
    paths = {reference / "README.md"}
    for directory in ["docs", "website/docs", "website/zh-CN/docs",
                      "website/modules", "website/zh-CN/modules"]:
        paths.update((reference / directory).rglob("*.md"))
    for path in sorted(paths):
        if path.relative_to(reference).as_posix() not in declared_files:
            continue
        contract = documented_contract(path, reference)
        text = path.read_text(encoding="utf-8")
        # Headings inside fenced examples are not API declarations.
        headings = []
        fence = None
        for line, value in enumerate(text.splitlines(), 1):
            marker = re.match(r"^\s{0,3}(`{3,}|~{3,})(.*)$", value)
            if marker:
                run = marker[1]
                if fence is None:
                    fence = run
                elif run[0] == fence[0] and len(run) >= len(fence) and not marker[2].strip():
                    fence = None
                continue
            heading = re.match(r"^(#{1,6})\s+(.+?)\s*#*\s*$", value)
            if fence is None and heading:
                headings.append({"level": len(heading[1]), "title": heading[2], "line": line})
        documents.append({"path": path.relative_to(reference).as_posix(),
                          "identity": native.file_identity(path), "headings": headings,
                          "contract": contract, "status": "unreviewed", "parity": "unknown",
                          "runtime_observed": False})

    def packages(root):
        records = []
        for path in sorted((root / "crates").glob("*/Cargo.toml")):
            if root == reference and path.relative_to(root).as_posix() not in declared_files:
                continue
            manifest = tomllib.loads(path.read_text(encoding="utf-8"))
            package = manifest.get("package")
            if not package:
                continue
            records.append({"path": path.relative_to(root).as_posix(),
                            "identity": native.file_identity(path), "package": package,
                            "features": manifest.get("features", {}),
                            "dependencies": manifest.get("dependencies", {}),
                            "build_dependencies": manifest.get("build-dependencies", {}),
                            "target_dependencies": manifest.get("target", {}),
                            "status": "unreviewed", "runtime_observed": False})
        return records

    reference_packages = packages(reference)
    candidate_packages = packages(native.ROOT)
    if native.tree_identity(reference) != identity:
        raise ValueError("reference sources changed during foundation inventory")
    return {"reference_tree": identity, "documents": documents,
            "generator": native.file_identity(Path(__file__)),
            "reference_packages": reference_packages, "candidate_packages": candidate_packages,
            "document_count": len(documents), "reviewed_count": 0,
            "scope": "Foundation guides, developer documents and first-party crate manifests; excludes component export counting.",
            "limits": ["Documentation and Cargo declarations are source observations, not runtime or feature parity.",
                       "Workspace-inherited versions and dependency settings remain unresolved declarations.",
                       "English and Chinese documents are separate source records, not separate capabilities.",
                       "No network refresh is performed; choose an independently identified reference checkout."]}


def table_cells(line):
    text = line.strip()
    if not text.startswith("|"):
        return None
    cells = []
    current = []
    code = 0
    position = 1
    while position < len(text):
        character = text[position]
        if character == "\\" and code == 0 and position + 1 < len(text):
            current.append(text[position:position + 2])
            position += 2
            continue
        if character == "`":
            end = position + 1
            while end < len(text) and text[end] == "`":
                end += 1
            count = end - position
            if code == 0:
                code = count
            elif code == count:
                code = 0
            current.append(text[position:end])
            position = end
            continue
        if character == "|" and code == 0:
            cells.append("".join(current).strip())
            current = []
        else:
            current.append(character)
        position += 1
    if current or not text.endswith("|"):
        cells.append("".join(current).strip())
    return cells


def documented_contract(path, reference):
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    frontmatter = re.match(r"\A---\n(.*?)\n---(?:\n|$)", text, re.S)
    title = re.search(r"^title:\s*(.+)$", frontmatter[1], re.M) if frontmatter else None
    if not title:
        return None
    headings = []
    tables = []
    fence = None
    position = 0
    while position < len(lines):
        line = lines[position]
        marker = re.match(r"^\s{0,3}(`{3,}|~{3,})(.*)$", line)
        if marker:
            run = marker[1]
            if fence is None:
                fence = run
            elif run[0] == fence[0] and len(run) >= len(fence) and not marker[2].strip():
                fence = None
            position += 1
            continue
        if fence is not None:
            position += 1
            continue
        heading = re.match(r"^(#{1,6})\s+(.+?)\s*#*\s*$", line)
        if heading:
            level = len(heading[1])
            headings = [entry for entry in headings if entry["level"] < level]
            headings.append({"level": level, "title": heading[2], "line": position + 1})
        header = table_cells(line)
        separator = table_cells(lines[position + 1]) if position + 1 < len(lines) else None
        if (not header or not separator or len(header) != len(separator)
                or not all(re.fullmatch(r":?-{3,}:?", cell) for cell in separator)):
            position += 1
            continue
        table = {"line": position + 1, "headings": list(headings), "columns": header, "rows": []}
        position += 2
        while position < len(lines):
            cells = table_cells(lines[position])
            if cells is None:
                break
            if len(cells) != len(header):
                raise ValueError(f"ambiguous contract table: {path}:{position + 1}")
            table["rows"].append({"line": position + 1, "cells": cells})
            position += 1
        tables.append(table)
    return {"title": title[1].strip().strip("\"'"),
            "path": path.relative_to(reference).as_posix(), "identity": native.file_identity(path),
            "authority": "source-declared documentation tables", "tables": tables,
            "runtime_observed": False, "reviewed": False}


def contract_documents(reference):
    documents = {}
    for directory in ["elements", "modules"]:
        for path in sorted((reference / "website/zh-CN" / directory).glob("*.md")):
            contract = documented_contract(path, reference)
            if contract:
                documents.setdefault(contract["title"], []).append(contract)
    return documents


def pinned_exports(text, variable):
    match = re.search(r"assert_eq!\(\s*" + re.escape(variable) + r"\s*,\s*\[([^\]]+)\]", text)
    if not match:
        raise ValueError(f"reference export inventory changed: no pinned {variable} array")
    names = re.findall(r'"([A-Za-z][A-Za-z0-9]*)"', match[1])
    if not names or len(names) != len(set(names)):
        raise ValueError(f"reference {variable} array is empty or ambiguous")
    return names, text[:match.start()].count("\n") + 1


def declared_kinds(path):
    text = path.read_text(encoding="utf-8")
    method = re.search(r"fn capabilities\([^)]*\)[^{]*\{(.*?)(?=\n\s*fn\s|\Z)", text, re.S)
    if not method:
        raise ValueError(f"native capability declaration was not found: {path}")
    body = method[1]
    inline = re.match(r"\s*(?:BackendCapabilities\s*\{\s*)?node_kinds:\s*vec!\[([^\]]+)\]", body, re.S)
    local = re.fullmatch(
        r"\s*let mut node_kinds\s*=\s*vec!\[([^\]]+)\];(.*?)"
        r"BackendCapabilities\s*\{\s*node_kinds\s*,[^{}]*\}\s*\}\s*", body, re.S)
    if not inline and not local:
        raise ValueError(f"unsupported native capability declaration: {path}")
    names = (inline or local)[1]
    if not re.fullmatch(r"\s*NodeKind::\w+(?:\s*,\s*NodeKind::\w+)*\s*,?\s*", names):
        raise ValueError(f"ambiguous native capability kinds: {path}")
    snake_case = lambda name: re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()
    unconditional = {snake_case(name) for name in re.findall(r"NodeKind::(\w+)", names)}
    conditional = {}
    if local:
        additions = local[2].strip()
        while additions:
            addition = re.match(
                r"if\s+(unsafe\s*\{\s*[A-Za-z_]\w*\(\)\s*\})\s*\{\s*"
                r"((?:node_kinds\.push\(NodeKind::\w+\);\s*)+)\}\s*", additions)
            if not addition:
                raise ValueError(f"unsupported native capability condition: {path}")
            for name in re.findall(r"NodeKind::(\w+)", addition[2]):
                kind = snake_case(name)
                if kind in unconditional or kind in conditional:
                    raise ValueError(f"ambiguous native capability condition: {path}")
                conditional[kind] = addition[1]
            additions = additions[addition.end():]
    return {"unconditional": unconditional, "conditional": conditional}


def ledger(reference):
    source = reference / "crates/nsh-base/src/lib.rs"
    text = source.read_text(encoding="utf-8")
    contracts = contract_documents(reference)
    schemas = {}
    for name in ["portable-v1.json", "extended-v1.json"]:
        path = native.ROOT / "crates/jadix_components/schema" / name
        for schema in json.loads(path.read_text(encoding="utf-8")):
            schemas[schema["name"]] = {"source": path.relative_to(native.ROOT).as_posix(), **schema}
    platforms = {name: declared_kinds(native.ROOT / f"crates/jadix_native/src/{name}.rs")
                 for name in ["windows", "ohos"]}
    advanced = {"Tree", "DataTable", "Table", "DockArea", "Editor", "RichEditor", "DiffView", "TextView"}
    rows = []
    identity = native.file_identity(source)
    for variable, namespace in [("components", "nsh/base"), ("modules", "nsh/base/modules"),
                                ("plot", "nsh/base/plot"), ("bare.native_value_names()", "nsh/base/native-values")]:
        names, line = pinned_exports(text, variable)
        for name in names:
            schema = schemas.get(name)
            rows.append({"id": f"{namespace}/{name}", "name": name,
                         "reference": {"path": "crates/nsh-base/src/lib.rs", "line": line,
                                       "identity": identity, "authority": "source-declared export inventory"},
                         "stage": "N14" if name in advanced or namespace.endswith("/plot") or name.startswith("Table") else "N13",
                         "jadix_schema": schema,
                         "platform_node_mapping_declared": {
                             platform: (None if schema["kind"] in kinds["conditional"]
                                        else schema["kind"] in kinds["unconditional"]) if schema else None
                             for platform, kinds in platforms.items()},
                         "platform_node_mapping_conditions": {
                             platform: kinds["conditional"][schema["kind"]]
                             for platform, kinds in platforms.items()
                             if schema and schema["kind"] in kinds["conditional"]},
                         "reference_props_slots_events_styles": contracts.get(name),
                         "scenario": None, "comparison_evidence": [], "performance_workload": None,
                         "status": "unreviewed", "parity": "unknown"})
    documents = [reference / "website/zh-CN/modules/nsh.md", *sorted((reference / "website/zh-CN/modules").glob("nsh-*.md"))]
    for source in documents:
        identity = native.file_identity(source)
        for line, heading in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
            if heading.startswith("## "):
                name = heading[3:].strip()
                rows.append({"id": f"reference-doc/{source.stem}/{line}", "name": name, "stage": "N6-review",
                             "reference": {"path": source.relative_to(reference).as_posix(), "line": line,
                                           "identity": identity, "authority": "source-declared documentation heading"},
                             "status": "unreviewed", "parity": "unknown", "comparison_evidence": []})
    documented = [row for row in rows if row.get("reference_props_slots_events_styles")]
    return {"reference_tree": native.tree_identity(reference), "records": rows,
            "record_count": len(rows), "reviewed_count": 0,
            "source_contract_record_count": len(documented),
            "source_contract_document_count": sum(len(documents) for documents in contracts.values()),
            "source_contract_scope": "Exact documentation titles only; unmatched compound parts remain unknown.",
            "limits": ["Export inventories and documentation headings are source observations, not executed reference behavior.",
                       "Documentation tables preserve source cells and line numbers; prose, inheritance and implementation behavior require review.",
                       "A matching Jadix schema or node kind does not establish props, slots, events, styling or platform parity.",
                       "Conditional native node mappings remain unknown until their runtime prerequisite is observed.",
                       "N6 still requires per-record review, workload selection and supporting evidence."]}
