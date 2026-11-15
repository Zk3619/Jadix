"""Compare matched development APKs on one owned Android test device; preserve package and document data."""

import argparse
from datetime import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess
import time
import zipfile

from native_android_metrics import SETTLING_SECONDS, cpu_observation, process_stat, resident_observation, total_pss_kib

PACKAGES = {
    "controls": ("dev.jadix.nativeui", "dev.jadix.nativeui.MainActivity"),
    "notes": ("dev.jadix.notebook", "dev.jadix.nativeui.NotesActivity"),
}
ABIS = {"x86_64-linux-android": "x86_64", "aarch64-linux-android": "arm64-v8a"}
CACHE_NAME = re.compile(r"native-state-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.json(?:\.new|\.bak)?")
POLICY = {"cold_total_median_ms": 300, "resume_wait_p95_ms": 30,
          "sampled_pss_p95_kib": 8192, "reported_rss_hwm_p95_kib": 8192, "idle_ticks_p95": 2}


def digest(payload):
    return hashlib.sha256(payload).hexdigest()


def artifact(apk):
    apk = apk.resolve()
    report_path = apk.parent / "build.json"
    report = json.loads(report_path.read_text(encoding="utf8"))
    if report["rust_profile"] != "release" or report.get("debuggable") is not True:
        raise ValueError("requires release-profile debuggable test packages")
    if report.get("application") not in PACKAGES or report["target"] not in ABIS:
        raise ValueError("unsupported sample application or ABI")
    if type(report.get("extended")) is not bool or type(report.get("retained_rows")) is not int or not re.fullmatch(r"[0-9a-f]{64}", str(report.get("cargo_lock_sha256", ""))):
        raise ValueError("missing or invalid feature, row or Cargo.lock identity")
    if report["retained_rows"] not in ((0,) if report["application"] == "notes" else (100, 1000)):
        raise ValueError("unsupported retained row count")
    payload = apk.read_bytes()
    if len(payload) != report["apk_bytes"] or digest(payload) != report["apk_sha256"]:
        raise ValueError("APK differs from build report")
    member = f"lib/{ABIS[report['target']]}/libjadix_native_controls.so"
    with zipfile.ZipFile(apk) as archive:
        if archive.namelist().count(member) != 1:
            raise ValueError("missing or duplicate native library")
        library = archive.read(member)
    if len(library) != report["library_bytes"] or digest(library) != report["library_sha256"]:
        raise ValueError("packaged native library differs from build report")
    return {**report, "path": str(apk), "report_sha256": digest(report_path.read_bytes())}


def match_artifacts(baseline, candidate):
    keys = ("target", "rust_profile", "application", "extended", "retained_rows", "cargo_lock_sha256", "debuggable")
    if any(baseline.get(key) != candidate.get(key) for key in keys):
        raise ValueError("comparison requires matching ABI, application, features, rows, lock and profile")


def percentile(samples):
    if not samples or any(isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0 for value in samples):
        raise ValueError("metrics require finite nonnegative observations")
    return sorted(samples)[math.ceil(len(samples) * 0.95) - 1]


def summarize(samples, rounds, resumes):
    if len(samples) != rounds or any(len(sample["resume"]) != resumes for sample in samples):
        raise ValueError("incomplete comparison samples")
    cold = [sample["cold"]["total_time_ms"] for sample in samples]
    percentile(cold)
    return {"cold_total_median_ms": statistics.median(cold),
            "resume_wait_p95_ms": percentile([value["wait_time_ms"] for sample in samples for value in sample["resume"]]),
            "sampled_pss_p95_kib": percentile([sample["pss_kib"] for sample in samples]),
            "reported_rss_hwm_p95_kib": percentile([sample["resident_memory"]["reported_rss_hwm_kib"] for sample in samples]),
            "idle_ticks_p95": percentile([sample["idle"]["cpu_ticks"] for sample in samples])}


def evaluate(baseline, candidate):
    if set(baseline) != set(POLICY) or set(candidate) != set(POLICY):
        raise ValueError("incomplete gate metrics")
    percentile(list(baseline.values()) + list(candidate.values()))
    return {name: {"baseline": baseline[name], "candidate": candidate[name],
                   "limit": baseline[name] * 1.5 + floor,
                   "passed": candidate[name] <= baseline[name] * 1.5 + floor}
            for name, floor in POLICY.items()}


def timing(text, cold):
    if not re.search(r"^Status: ok\s*$", text, re.MULTILINE):
        raise ValueError("activity launch failed")
    total = re.search(r"^TotalTime:\s*(\d+)\s*$", text, re.MULTILINE)
    wait = re.search(r"^WaitTime:\s*(\d+)\s*$", text, re.MULTILINE)
    if not wait or (cold and not total):
        raise ValueError("required activity timing missing")
    return {"total_time_ms": int(total[1]) if total else None, "wait_time_ms": int(wait[1])}


def signer_certificates(text):
    values = re.findall(r"^Signer #\d+ certificate SHA-256 digest: ([0-9a-fA-F]{64})\s*$", text, re.MULTILINE)
    if not values:
        raise ValueError("no verified APK signer certificates")
    return sorted(value.lower() for value in values)


def manifest_identity(text):
    observed = [re.search(pattern, text, re.MULTILINE) for pattern in
                (r"^package: name='([^']+)'", r"^launchable-activity: name='([^']+)'", r"^(?:minSdkVersion|sdkVersion):'([^']+)'", r"^targetSdkVersion:'([^']+)'")]
    if any(value is None for value in observed):
        raise ValueError("missing package, activity or SDK metadata")
    return [value[1] for value in observed]


def validate_output(output):
    if any((parent / "snapshot.json").exists() for parent in (output, *output.parents)):
        raise ValueError("output contains an archived snapshot")
    if output.exists() and any(output.iterdir()):
        raise ValueError("comparison output must be empty")


def protected_names(text):
    names = text.splitlines()
    if any(any(character.isspace() for character in name) for name in names):
        raise ValueError("expected one filename per listing line")
    return [name for name in names if CACHE_NAME.fullmatch(name) or name in
            ("jadix-notes-v1.json", "jadix-notes-v1.json.new", "jadix-notes-v1.json.bak")]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--baseline-apk", type=Path, required=True)
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=6, choices=range(5, 31))
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    try:
        validate_output(output)
        baseline, candidate = artifact(arguments.baseline_apk), artifact(arguments.apk)
        match_artifacts(baseline, candidate)
    except (ValueError, OSError, KeyError, zipfile.BadZipFile) as error:
        parser.error(str(error))
    sdk = Path(os.environ.get("ANDROID_HOME", ""))
    java = Path(os.environ.get("JAVA_HOME", "")) / "bin" / ("java.exe" if os.name == "nt" else "java")
    builds = sorted((sdk / "build-tools").glob("*"), key=lambda path: tuple(int(part) for part in path.name.split(".")) if re.fullmatch(r"[0-9]+(?:\.[0-9]+)*", path.name) else ())
    if not builds or not java.is_file():
        parser.error("ANDROID_HOME build tools and JAVA_HOME are required")
    tools = builds[-1]
    output.mkdir(parents=True, exist_ok=True)
    sequence = 0
    report = {"observed_at": datetime.now().astimezone().isoformat(), "serial": arguments.serial,
              "runner_sha256": digest(Path(__file__).read_bytes()),
              "metrics_module_sha256": digest(Path(__file__).with_name("native_android_metrics.py").read_bytes()),
              "measurement_protocol": "v2: reported RSS high-water, separately observed five-second post-resume settling, then idle",
              "baseline": baseline, "candidate": candidate, "rounds": arguments.rounds,
              "warmups_per_artifact": 1, "resumes_per_round": 3, "order": [],
              "samples": {"baseline": [], "candidate": []}, "passed": False, "complete": False,
              "limits": ["Owned debuggable test sample; force-stop and clear-task affect its task, never clear application data.",
                         "One environment; AM times are not frame presentation or input-to-paint latency.",
                         "Sampled PSS and one-second process ticks do not prove peak memory, power or leak freedom.",
                         "VmHWM is the kernel-reported process RSS high-water mark, not peak PSS or system/GPU memory. Linux documents RSS accounting as approximate; values cover each fresh measured process through the post-resume observation.",
                         "Idle follows a fixed five-second settling interval, retained separately with its CPU ticks. It is not guaranteed quiescent or interchangeable with earlier immediate-idle reports.",
                         "Relative gate is baseline x 1.5 plus fixed noise floors; nearest-rank p95. No product signature, physical/minimum OS or thermal acceptance."]}
    package, activity = PACKAGES[baseline["application"]]
    adb = [str(sdk / "platform-tools" / ("adb.exe" if os.name == "nt" else "adb")), "-s", arguments.serial]

    def run(command, binary=False):
        nonlocal sequence
        result = subprocess.run([str(item) for item in command], capture_output=True, timeout=90)
        sequence += 1
        (output / f"command-{sequence:04d}.json").write_text(json.dumps({"command": [str(item) for item in command], "exit": result.returncode,
            "stdout": None if binary else result.stdout.decode("utf8", errors="replace"),
            "stdout_sha256": digest(result.stdout), "stdout_bytes": len(result.stdout),
            "stderr": result.stderr.decode("utf8", errors="replace")}, indent=2), encoding="utf8")
        if result.returncode:
            raise RuntimeError(f"command failed: {command}: {result.stderr.decode('utf8', errors='replace')}")
        return result.stdout if binary else result.stdout.decode("utf8", errors="strict")

    def device(*command, binary=False):
        return run(adb + list(command), binary=binary)

    def certificates(apk):
        result = run([java, "-jar", tools / "lib/apksigner.jar", "verify", "--print-certs", apk])
        return signer_certificates(result)

    def installed():
        paths = device("shell", "pm", "path", package).strip().splitlines()
        if len(paths) != 1 or not paths[0].startswith("package:/data/app/"):
            raise ValueError("expected an existing single installed test base APK")
        remote = paths[0][8:]
        return remote, device("shell", "sha256sum", remote).split()[0]

    def state_files():
        names = protected_names(device("shell", "-T", "run-as", package, "ls", "-1", "files"))
        return {name: device("shell", "-T", "run-as", package, "cat", f"files/{name}", binary=True) for name in names}

    def identity():
        properties = ["ro.build.fingerprint", "ro.build.version.sdk", "ro.product.cpu.abi"]
        return {**{name: device("shell", "getprop", name).strip() for name in properties},
                "size": device("shell", "wm", "size").strip(), "density": device("shell", "wm", "density").strip(),
                "font_scale": device("shell", "settings", "get", "system", "font_scale").strip()}

    original, protected, changed = None, None, False
    try:
        signer = certificates(baseline["path"])
        if certificates(candidate["path"]) != signer:
            raise ValueError("baseline and candidate signers differ")
        aapt = tools / ("aapt2.exe" if os.name == "nt" else "aapt2")
        badging = [run([aapt, "dump", "badging", item["path"]]) for item in (baseline, candidate)]
        signatures = [manifest_identity(text) for text in badging]
        if signatures[0] != signatures[1] or signatures[0][:2] != [package, activity]:
            raise ValueError("APK manifest comparison mismatch")
        report.update(signers=signer, manifest=signatures[0], device_before=identity())
        if report["device_before"]["ro.product.cpu.abi"] != ABIS[baseline["target"]]:
            raise ValueError("test device ABI differs from artifact")
        remote, original_hash = installed()
        original = output / "original.apk"
        original.write_bytes(device("exec-out", "cat", remote, binary=True))
        if digest(original.read_bytes()) != original_hash or certificates(original) != signer:
            raise ValueError("installed package backup or signer mismatch")
        report["original_apk_sha256"] = original_hash
        device("shell", "am", "force-stop", package)
        protected = state_files()
        caches = [name for name in protected if CACHE_NAME.fullmatch(name)]
        identities = {name[:54] for name in caches}
        if len(identities) > 15 or sum(len(protected[name]) for name in caches) > 14 * 1024 * 1024:
            raise ValueError("test recovery cache needs one free identity and two MiB headroom")
        now = int(device("shell", "date", "+%s").strip())
        for name in caches:
            modified = int(device("shell", "-T", "run-as", package, "stat", "-c", "%Y", f"files/{name}").strip())
            if now - modified > 6 * 86400:
                raise ValueError("existing recovery files approach expiry; select a clean owned test installation")
        report["protected_files_before"] = {name: digest(payload) for name, payload in protected.items()}

        def cleanup():
            device("shell", "am", "force-stop", package)
            current = state_files()
            if any(current.get(name) != payload for name, payload in protected.items()):
                raise RuntimeError("preexisting test state changed")
            for name in current.keys() - protected.keys():
                if not CACHE_NAME.fullmatch(name):
                    raise RuntimeError("unexpected durable document change")
                device("shell", "-T", "run-as", package, "rm", f"files/{name}")
            if state_files() != protected:
                raise RuntimeError("comparison cleanup did not preserve original state")

        def measure(label, warmup):
            nonlocal changed
            selected = baseline if label == "baseline" else candidate
            changed = True
            device("install", "-r", selected["path"])
            if installed()[1] != selected["apk_sha256"]:
                raise RuntimeError("installed APK verification failed")
            cold = timing(device("shell", "am", "start", "-W", "-S", "-f", "0x10008000", "-n", f"{package}/{activity}"), True)
            pid = device("shell", "pidof", package).strip()
            if not pid.isdigit():
                raise RuntimeError("expected one owned sample process")
            def stat():
                return process_stat(device("shell", "-T", "run-as", package, "cat", f"/proc/{pid}/stat"))
            initial = stat()
            resume = []
            for _ in range(3):
                device("shell", "input", "keyevent", "KEYCODE_HOME")
                resume.append(timing(device("shell", "am", "start", "-W", "-n", f"{package}/{activity}"), False))
                if device("shell", "pidof", package).strip() != pid or stat()["start_ticks"] != initial["start_ticks"]:
                    raise RuntimeError("resume replaced the measured process")
            memory = total_pss_kib(device("shell", "dumpsys", "meminfo", package))
            def observe_cpu(seconds):
                return cpu_observation(stat, lambda duration: device("shell", "sleep", str(duration)), initial["start_ticks"], seconds)
            settling = observe_cpu(SETTLING_SECONDS)
            idle = observe_cpu(1)
            resident = resident_observation(lambda name: device("shell", "-T", "run-as", package, "cat", f"/proc/{pid}/{name}"), initial["start_ticks"])
            sample = {"cold": cold, "resume": resume, "pss_kib": memory, "resident_memory": resident, "pid": pid,
                      "start_ticks": initial["start_ticks"], "post_resume_settling": settling, "idle": idle}
            cleanup()
            report["order"].append({"label": label, "warmup": warmup})
            if not warmup:
                report["samples"][label].append(sample)
            (output / "progress.json").write_text(json.dumps(report, indent=2), encoding="utf8")

        for label in ("baseline", "candidate"):
            measure(label, True)
        for index in range(arguments.rounds):
            for label in (("baseline", "candidate") if index % 2 == 0 else ("candidate", "baseline")):
                measure(label, False)
        report["device_after"] = identity()
        if report["device_before"] != report["device_after"]:
            raise RuntimeError("device configuration changed during comparison")
        report["metrics"] = {label: summarize(samples, arguments.rounds, 3) for label, samples in report["samples"].items()}
        report["gate"] = evaluate(report["metrics"]["baseline"], report["metrics"]["candidate"])
        report["complete"] = True
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
    finally:
        try:
            if changed:
                try:
                    cleanup()
                except Exception as error:
                    report["state_cleanup_error"] = f"{type(error).__name__}: {error}"
                device("install", "-r", original)
                if installed()[1] != report["original_apk_sha256"]:
                    raise RuntimeError("original APK restoration failed")
                report["original_apk_restored"] = True
            if protected is not None:
                report["protected_files_unchanged"] = state_files() == protected
                if not report["protected_files_unchanged"]:
                    raise RuntimeError("protected sample data differs")
        except Exception as error:
            report["cleanup_error"] = f"{type(error).__name__}: {error}"
        report["passed"] = bool(report["complete"] and not report.get("error") and not report.get("cleanup_error") and not report.get("state_cleanup_error") and all(item["passed"] for item in report.get("gate", {}).values()))
        (output / "comparison.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    if not report["passed"]:
        raise SystemExit("Android comparison failed; inspect comparison.json")
    print("PASS Android same-run comparison and original package/data restoration")


if __name__ == "__main__":
    main()
