"""Measure the installed development sample; retain raw observations without inferring device release readiness."""

import argparse
from datetime import datetime
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import subprocess
import time

from native_android_metrics import SETTLING_SECONDS, cpu_observation, process_stat, resident_observation, total_pss_kib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--apk", type=Path, help="Verify the installed base APK against this package and its build report")
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    if any((directory / "snapshot.json").exists() for directory in (output, *output.parents)):
        parser.error("output contains an archived snapshot; select a new output directory")
    output.mkdir(parents=True, exist_ok=True)
    adb = ["adb", "-s", arguments.serial]
    package = "dev.jadix.nativeui"
    component = package + "/.MainActivity"
    sequence = 0

    def run(*command):
        nonlocal sequence
        result = subprocess.run(adb + list(command), capture_output=True, text=True, encoding="utf8", errors="replace", timeout=30)
        sequence += 1
        (output / f"command-{sequence:03d}.log").write_text(result.stdout + result.stderr, encoding="utf8")
        if result.returncode:
            raise RuntimeError(f"{command}: {result.stdout}{result.stderr}")
        return result.stdout

    def launch(cold):
        command = ["shell", "am", "start", "-W"]
        if cold:
            command.append("-S")
        result = run(*command, "-n", component)
        if not re.search(r"^Status: ok\s*$", result, re.MULTILINE):
            raise RuntimeError(result)
        total = re.search(r"^TotalTime:\s*(\d+)", result, re.MULTILINE)
        wait = re.search(r"^WaitTime:\s*(\d+)", result, re.MULTILINE)
        if not wait or (cold and not total):
            raise RuntimeError("Activity manager did not report required timing fields")
        return {"total_time_ms": int(total.group(1)) if total else None, "wait_time_ms": int(wait.group(1))}

    def memory():
        return total_pss_kib(run("shell", "dumpsys", "meminfo", package))

    def process_ticks(pid):
        result = run("exec-out", "run-as", package, "cat", f"/proc/{pid}/stat")
        return process_stat(result)

    artifact = None
    if arguments.apk:
        apk = arguments.apk.resolve()
        digest = hashlib.sha256(apk.read_bytes()).hexdigest()
        paths = run("shell", "pm", "path", package).strip().splitlines()
        if len(paths) != 1 or not paths[0].startswith("package:"):
            raise RuntimeError("expected one installed base APK")
        installed_digest = run("shell", "sha256sum", paths[0][len("package:"):]).split()[0]
        if installed_digest != digest:
            raise RuntimeError("installed APK does not match requested artifact")
        build = json.loads((apk.parent / "build.json").read_text(encoding="utf8"))
        if build["apk_sha256"] != digest:
            raise RuntimeError("build report does not match requested artifact")
        artifact = {"apk_sha256": digest, "installed_apk_matches": True,
                    "rust_profile_from_build_report": build["rust_profile"], "retained_rows_from_build_report": build.get("retained_rows")}
    cold = [launch(True) for _ in range(5)]
    initial_pss = memory()
    resume, pss = [], [initial_pss]
    for iteration in range(20):
        run("shell", "input", "keyevent", "KEYCODE_HOME")
        resume.append(launch(False))
        if (iteration + 1) % 5 == 0:
            pss.append(memory())
    pid = run("shell", "pidof", package).strip()
    if not pid.isdigit():
        raise RuntimeError("expected one sample process")
    start_ticks = process_ticks(pid)["start_ticks"]
    def observe_cpu(seconds):
        return cpu_observation(lambda: process_ticks(pid), lambda duration: run("shell", "sleep", str(duration)), start_ticks, seconds)
    settling = observe_cpu(SETTLING_SECONDS)
    idle = [observe_cpu(1) for _ in range(5)]
    resident = resident_observation(lambda name: run("exec-out", "run-as", package, "cat", f"/proc/{pid}/{name}"), start_ticks)

    def distribution(samples):
        return {"samples_ms": samples, "median_ms": statistics.median(samples),
                "p95_ms": sorted(samples)[math.ceil(len(samples) * 0.95) - 1]}

    report = {"observed_at": datetime.now().astimezone().isoformat(), "serial": arguments.serial,
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "metrics_module_sha256": hashlib.sha256(Path(__file__).with_name("native_android_metrics.py").read_bytes()).hexdigest(),
              "measurement_protocol": "v2: reported RSS high-water, separately observed five-second post-resume settling, then idle",
              "artifact": artifact,
              "percentile_method": "nearest rank: ceil(sample_count * 0.95) - 1",
              "sdk": run("shell", "getprop", "ro.build.version.sdk").strip(),
              "abi": run("shell", "getprop", "ro.product.cpu.abi").strip(),
              "cold_activity": {"samples": cold, "total_time": distribution([sample["total_time_ms"] for sample in cold]),
                                "wait_time": distribution([sample["wait_time_ms"] for sample in cold])},
              "background_resume": {"samples": resume, "wait_time": distribution([sample["wait_time_ms"] for sample in resume])},
              "pss_kib_at_0_5_10_15_20_resumes": pss, "resident_memory_after_idle": resident,
              "post_resume_settling": settling, "idle_process_samples": idle,
              "limits": ["Installed development APK; without --apk its build profile remains unknown. Product signing is not established.",
                         "Activity-manager TotalTime and WaitTime include platform scheduling; neither measures frame presentation or input latency.",
                         "Foreground task resumes may omit TotalTime; missing values are retained as null and are never replaced by zero.",
                         "PSS and process CPU ticks are finite observations; they do not prove absence of leaks or OS wakeups.",
                         "VmHWM is the kernel-reported resident high-water mark for the identified final process; Linux documents RSS accounting as approximate. It is not peak PSS, total system/GPU memory or power.",
                         "Five cold starts, twenty background/resume cycles and five one-second idle intervals on this one environment.",
                         "Idle follows a fixed five-second settling interval whose CPU work is retained separately. No claim of quiescence or equivalence to earlier immediate-idle reports.",
                         "Row count follows the matched build report when supplied. No historical baseline, real-device, thermal, screen-reader or minimum-OS acceptance is inferred."]}
    (output / "performance.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print(f"Measured {len(cold)} cold starts, {len(resume)} resumes and {len(idle)} idle intervals")


if __name__ == "__main__":
    main()
