#!/usr/bin/env python3
"""Check stable capability IDs and explicit media coverage, optionally with an upstream tree."""
import argparse
import csv
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
LEDGER = ROOT / "development-docs/comparison/jadix-nsh-capabilities-20261106.md"
TERMINAL = ROOT / "development-docs/ops/terminal-competitive-20261107.md"
PATH_DISPOSITIONS = ROOT / "development-docs/comparison/nsh-followup71-path-dispositions.csv"
GROUPS = [("P",12),("R",15),("T",14),("A",18),("I",13),
          ("S",10),("D",14),("V",12),("E",12),("C",17)]
PATH_COLUMNS = {"source_path", "upstream_impact_heuristic", "review_status",
                "jadix_capability_ids", "evidence", "disposition_reason"}
PATH_STATUSES = {"unreviewed", "mapped", "partial", "test-only", "docs-only", "build-only",
                 "not-applicable"}
PATH_IMPACTS = {"shared-runtime", "platform-or-tooling", "docs-ci-examples", "terminal"}


def check_path_dispositions(path, nsh_receipt=None):
    with path.open(encoding="utf-8-sig", newline="") as stream:
        reader = csv.DictReader(stream)
        if set(reader.fieldnames or []) != PATH_COLUMNS:
            raise ValueError("path disposition table has an invalid column set")
        rows = list(reader)
    paths = [row["source_path"] for row in rows]
    if any(not source_path for source_path in paths) or len(paths) != len(set(paths)):
        raise ValueError("path disposition table contains an empty or duplicate source path")
    for row in rows:
        status = row["review_status"]
        if status not in PATH_STATUSES:
            raise ValueError(f"unknown path disposition status: {status}")
        if row["upstream_impact_heuristic"] not in PATH_IMPACTS:
            raise ValueError(f"unknown upstream impact heuristic for {row['source_path']}")
        has_ids = bool(row["jadix_capability_ids"].strip())
        has_evidence = bool(row["evidence"].strip())
        has_reason = bool(row["disposition_reason"].strip())
        if status == "unreviewed" and (has_ids or has_evidence or has_reason):
            raise ValueError(f"unreviewed path has a recorded conclusion: {row['source_path']}")
        if status in {"mapped", "partial"} and not (has_ids and has_evidence):
            raise ValueError(f"{status} path lacks capability IDs or evidence: {row['source_path']}")
        if status == "partial" and not has_reason:
            raise ValueError(f"partial path lacks a residual-difference reason: {row['source_path']}")
        if status in PATH_STATUSES - {"unreviewed", "mapped", "partial"} and not (has_evidence and has_reason):
            raise ValueError(f"disposed path lacks evidence or a reason: {row['source_path']}")
    if nsh_receipt is not None:
        receipt = json.loads(nsh_receipt.read_text(encoding="utf-8"))
        expected = {item["path"]: item["impact"]
                    for item in receipt["repositories"]["nsh"]["files"]}
        actual = {row["source_path"]: row["upstream_impact_heuristic"] for row in rows}
        if actual != expected:
            missing = sorted(expected.keys() - actual.keys())
            extra = sorted(actual.keys() - expected.keys())
            changed = sorted(path for path in expected.keys() & actual.keys()
                             if expected[path] != actual[path])
            raise ValueError(f"path table differs from nsh receipt: missing={missing}, extra={extra}, impact_changed={changed}")
    impacts = {impact: sum(row["upstream_impact_heuristic"] == impact for row in rows)
               for impact in sorted(PATH_IMPACTS)}
    statuses = {status: sum(row["review_status"] == status for row in rows)
                for status in sorted(PATH_STATUSES)}
    return {"paths": len(rows), "unique_paths": len(set(paths)),
            "impacts": impacts, "review_statuses": statuses}


def check(ledger, terminal, tree=None, path_dispositions=None, nsh_receipt=None):
    expected = [f"{group}{index:02}" for group, count in GROUPS for index in range(1,count+1)]
    ids = re.findall(r"^\| ([A-Z]\d{2}) \|", ledger, re.M)
    if ids != expected:
        raise ValueError("137 main capability IDs changed, disappeared or moved")
    terminal_ids = re.findall(r"^\| (T\d{2}) \|", terminal, re.M)
    if terminal_ids != [f"T{i:02}" for i in range(1,43)]:
        raise ValueError("42 terminal capability IDs changed, disappeared or moved")
    rows = {line.split("|")[1].strip(): line for line in ledger.splitlines()
            if re.match(r"^\| [A-Z]\d{2} \|",line)}
    # Check the owning rows, not a historical changelog mentioning the names.
    for identity, names in {"E10": ["Audio", "Sound", "AudioPlayer", "Speech"],
                            "C16": ["Video", "audioSession"]}.items():
        for name in names:
            if name not in rows[identity]:
                raise ValueError(f"{identity} lost independent media capability {name}")
    if '<a id="media-detail"></a>' not in ledger:
        raise ValueError("missing media detail matrix")
    if tree is not None:
        if tree.get("truncated") or not isinstance(tree.get("tree"),list):
            raise ValueError("upstream tree is incomplete")
        paths = {item["path"] for item in tree["tree"] if item.get("type")=="blob"}
        for source, owner in [("crates/nsh-core/src/audio.rs","E10"),
                              ("website/zh-CN/modules/nsh-video.md","C16")]:
            if source in paths and "#media-detail" not in rows[owner]:
                raise ValueError(f"upstream {source} has no explicit {owner} detail link")
    path_audit = check_path_dispositions(path_dispositions, nsh_receipt) if path_dispositions else None
    return {"status":"pass","main_ids":len(ids),"terminal_ids":len(terminal_ids),
            "path_audit":path_audit,
            "scope":"stable IDs and named media coverage; not semantic completeness or runtime parity"}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream-tree",type=Path)
    parser.add_argument("--path-dispositions",type=Path,default=PATH_DISPOSITIONS)
    parser.add_argument("--nsh-receipt",type=Path,
                        help="compare the path table with a complete local Git receipt")
    parser.add_argument("--output",type=Path)
    args=parser.parse_args()
    tree=json.loads(args.upstream_tree.read_text(encoding="utf-8")) if args.upstream_tree else None
    result=check(LEDGER.read_text(encoding="utf-8"),TERMINAL.read_text(encoding="utf-8"),tree,
                 args.path_dispositions,args.nsh_receipt)
    if args.output:
        args.output.write_text(json.dumps(result,indent=2),encoding="utf-8")
    print(json.dumps(result))


if __name__=="__main__":
    main()
