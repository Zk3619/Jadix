#!/usr/bin/env python3
"""Repeat pinned nsh/Jadix terminal baselines in ABBA order; preserve raw data.

Run inside the same Linux host as both release executables. This deliberately
does not call unequal application/layout pipelines behaviorally equivalent.
"""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import re
import statistics
import subprocess
import time


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nsh", type=Path, required=True)
    parser.add_argument("--jadix", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--blocks", type=int, default=2, choices=range(1, 6))
    parser.add_argument("--jadix-colors", choices=("rgb", "256", "16"), default="rgb")
    parser.add_argument("--jadix-auto-layout", action="store_true")
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=False)
    result = {"platform": platform.uname()._asdict(), "nsh_sha256": digest(args.nsh),
              "jadix_sha256": digest(args.jadix), "order": "ABBA" * args.blocks,
              "equivalent_applications": False, "jadix_colors": args.jadix_colors,"jadix_auto_layout":args.jadix_auto_layout,
              "limitations": ["Jadix optional shared Taffy initial LongList layout, explicit virtual table frames; nsh JS/Taffy/engine pipeline",
                              "DataTable adapters and paint output differ",
                              "nsh apply+draw excludes ANSI flush; Jadix scope includes ANSI flush",
                              "memory output; no PTY/SSH/scanout timing"], "runs": []}
    for index, kind in enumerate(result["order"]):
        command = ([str(args.nsh.resolve()), "--exact", "performance::bench_terminal_scroll",
                    "--ignored", "--nocapture", "--test-threads=1"] if kind == "A" else [str(args.jadix.resolve()), "--colors", args.jadix_colors])
        memory = args.directory / f"{index:02}-{kind}-resources.txt"
        if kind=="B" and args.jadix_auto_layout: command.append("--auto-layout")
        measured_command = (["/usr/bin/time", "-v", "-o", str(memory), *command]
                            if Path("/usr/bin/time").exists() else command)
        start = time.monotonic()
        run = subprocess.run(measured_command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
        raw = args.directory / f"{index:02}-{kind}.log"
        raw.write_bytes(run.stdout)
        assert run.returncode == 0, (command, run.returncode, raw)
        output = run.stdout.decode()
        if kind == "A":
            values = [{"workload": name, "median_ns": round(float(median) * 1e6),
                       "p95_ns": round(float(p95) * 1e6), "bytes_per_frame": float(byte),
                       "diff_cells_per_frame": float(cell)}
                      for name, median, p95, byte, cell in re.findall(
                          r"(LongList|DataTable): .*?median ([\d.]+) ms p95 ([\d.]+) ms;.*? ([\d.]+) B/frame ([\d.]+) Cells/frame", output)]
            assert len(values) == 2, output
        else:
            payload = json.loads(output)
            assert payload["profile"] == "release", payload
            values = payload["results"]
        result["runs"].append({"index": index, "project": "nsh" if kind == "A" else "jadix",
                               "elapsed_s": time.monotonic() - start, "command": command,
                               "max_rss_kib": (int(re.search(r"Maximum resident set size \(kbytes\): (\d+)", memory.read_text()).group(1)) if memory.exists() else None),
                               "raw_sha256": digest(raw), "results": values})
        print(index, kind, [(v["workload"], v["p95_ns"]) for v in values], flush=True)
        (args.directory / "comparison.json").write_text(json.dumps(result, indent=2))
    result["summary"] = [{"project": project, "workload": workload,
        "median_of_run_medians_ns": statistics.median(values["median_ns"] for run in result["runs"] if run["project"] == project for values in run["results"] if values["workload"] == workload),
        "median_of_run_p95_ns": statistics.median(values["p95_ns"] for run in result["runs"] if run["project"] == project for values in run["results"] if values["workload"] == workload)}
        for project in ("nsh", "jadix") for workload in ("LongList", "DataTable")]
    result["status"] = "measured; different application pipelines"
    (args.directory / "comparison.json").write_text(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
