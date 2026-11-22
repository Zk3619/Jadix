import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import time
from xml.etree import ElementTree


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import native

SPEC = importlib.util.spec_from_file_location("android_owned_install", Path(__file__).with_name("native-android-check.py"))
OWNED = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OWNED)


def validate_receipt(log, nonce, package):
    lines = log.splitlines()
    expected = {"jadix.native.project": "PASS:" + nonce, "jadix.package": package}
    for name, value in expected.items():
        prefix = "INSTRUMENTATION_RESULT: " + name + "="
        if [line[len(prefix):] for line in lines if line.startswith(prefix)] != [value]:
            raise ValueError("missing, duplicated or mismatched project receipt: " + name)
    if lines.count("INSTRUMENTATION_CODE: -1") != 1 or any("INSTRUMENTATION_FAILED" in line for line in lines):
        raise ValueError("project instrumentation did not finish successfully")


def process_recovery(run, adb, package, nonce, output):
    def observe(*command):
        return run(adb + list(command)).stdout.strip()

    def tree(name):
        path = output / name
        OWNED.dump_tree(run, adb, path)
        return list(ElementTree.parse(path).iter("node"))

    def tap(node):
        left, top, right, bottom = map(int, re.findall(r"\d+", node.attrib["bounds"]))
        observe("shell", "input", "tap", str((left + right) // 2), str((top + bottom) // 2))

    component = package + "/dev.jadix.nativeui.MainActivity"
    observe("shell", "am", "start", "-W", "-f", "0x10008000", "-n", component)
    nodes = tree("process-initial.xml")
    counter = next(node for node in nodes if re.fullmatch(r"Count: \d+", node.get("text", "")))
    expected_count = int(counter.attrib["text"].split(": ")[1]) + 1
    tap(next(node for node in nodes if node.get("class") == "android.widget.Button" and (node.get("text") or node.get("content-desc", "")).lower() == "continue"))
    tap(next(node for node in nodes if node.get("class") == "android.widget.EditText"))
    value = "recovery_" + nonce
    observe("shell", "input", "text", value)
    observe("shell", "input", "keyevent", "KEYCODE_BACK")
    nodes = tree("process-edit.xml")
    preference = next(node for node in nodes if node.get("class") == "android.widget.CheckBox")
    if preference.get("checked") != "true":
        tap(preference)

    def require_state(nodes):
        expected = [("android.widget.EditText", "text", value),
                    ("android.widget.CheckBox", "checked", "true")]
        if not any(node.get("text") == f"Count: {expected_count}" for node in nodes):
            raise ValueError("new process did not preserve the Rust counter")
        for kind, field, expected_value in expected:
            if not any(node.get("class") == kind and node.get(field) == expected_value for node in nodes):
                raise ValueError("native business state differs: " + kind)

    require_state(tree("process-before.xml"))
    before_pid = observe("shell", "pidof", package)
    if not before_pid.isdecimal():
        raise ValueError("one identified application process required")
    observe("shell", "input", "keyevent", "KEYCODE_HOME")
    OWNED.wait_for_saved_background(run, adb, package, output)
    observe("shell", "am", "kill", package)
    result = run(adb + ["shell", "pidof", package], allow_failure=True)
    if result.stdout.strip():
        activities = observe("shell", "dumpsys", "activity", "activities")
        if not OWNED.resumed_elsewhere(activities, package):
            raise RuntimeError("Home did not move the test activity to the background")
        observe("shell", "run-as", package, "kill", "-9", before_pid)
    deadline = time.monotonic() + 10
    while True:
        result = run(adb + ["shell", "pidof", package], allow_failure=True)
        if result.returncode == 1 and not result.stdout.strip():
            break
        if time.monotonic() >= deadline:
            observe("shell", "dumpsys", "activity", "processes")
            observe("shell", "dumpsys", "window", "windows")
            raise RuntimeError("background process survived the verified termination request")
        time.sleep(0.1)
    observe("shell", "am", "start", "-W", "-n", component)
    after_pid = observe("shell", "pidof", package)
    if not after_pid.isdecimal() or before_pid == after_pid:
        raise ValueError("a distinct restored application process required")
    require_state(tree("process-after.xml"))
    receipt = {"passed": True, "before_pid": before_pid, "after_pid": after_pid,
               "nonce": nonce, "package_name": package, "counter": expected_count,
               "value": value, "checked": True,
               "termination": "am kill after Home, then run-as SIGKILL if the OS retained the background process; absence verified before restart",
               "limits": ["Activity saved-task checkpoint only; no power-loss, force-stop, physical-device or document-journal acceptance."]}
    (output / "process-recovery.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return receipt


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--adb", type=Path, required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--nonce", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--process-recovery", action="store_true")
    arguments = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{32}", arguments.nonce):
        parser.error("a fresh 32-digit hexadecimal nonce is required")
    arguments.output.mkdir(parents=True, exist_ok=False)
    apk = arguments.apk.resolve()
    build_path = apk.with_name("build.json")
    build = json.loads(build_path.read_text(encoding="utf-8"))
    package = build.get("package_name", "")
    if (build.get("application") != "project" or not isinstance(package, str)
            or not re.fullmatch(r"dev\.jadix\.created\.[a-z][a-z0-9_]{0,63}", package)
            or build.get("source_unchanged") is not True or build.get("apk_identity") != native.file_identity(apk)):
        raise ValueError("identified independent-project build required")
    report = {"schema": 1, "passed": False, "nonce": arguments.nonce, "serial": arguments.serial,
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "ownership_runner_identity": native.file_identity(Path(OWNED.__file__)),
              "artifact": {"build_report_sha256": native.file_identity(build_path)["sha256"],
                           "apk_sha256": build["apk_sha256"], "package_name": package},
              "apk_sha256": build["apk_sha256"], "apk_bytes": build["apk_bytes"],
              "runtime_acceptance": False}
    adb = [str(arguments.adb.resolve()), "-s", arguments.serial]
    commands = []
    report["commands"] = commands

    def run(command, allow_failure=False):
        result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=180,
                                creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0)
        commands.append({"command": command, "exit_code": result.returncode,
                         "stdout": result.stdout, "stderr": result.stderr})
        if result.returncode and not allow_failure:
            raise RuntimeError("Android project command failed: " + result.stdout + result.stderr)
        return result

    try:
        sdk = run(adb + ["shell", "getprop", "ro.build.version.sdk"]).stdout.strip()
        if not sdk.isdecimal() or int(sdk) < 26:
            raise ValueError("project requires Android API 26 or later")
        report["sdk"] = int(sdk)
        with OWNED.owned_install(run, adb, apk, report, package=package):
            result = run(adb + ["shell", "am", "instrument", "-r", "-w", "-e", "jadix.nonce", arguments.nonce,
                                package + "/dev.jadix.nativeui.NativeAppChecks"])
            log = result.stdout + result.stderr
            (arguments.output / "instrumentation.log").write_text(log, encoding="utf-8")
            try:
                validate_receipt(log, arguments.nonce, package)
                if arguments.process_recovery:
                    report["process_recovery"] = process_recovery(run, adb, package, arguments.nonce, arguments.output)
            finally:
                diagnostics = run(adb + ["logcat", "-d", "-s", "JadixNativeProject"])
                (arguments.output / "application.log").write_text(diagnostics.stdout + diagnostics.stderr, encoding="utf-8")
        report["passed"] = True
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (arguments.output / "checks.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
