import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native

SPEC = importlib.util.spec_from_file_location("android_owned_install", Path(__file__).with_name("native-android-check.py"))
OWNED = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OWNED)


def receipt(log, token, api):
    lines = log.splitlines()
    prefix = "INSTRUMENTATION_RESULT: jadix_shell="
    values = [line[len(prefix):] for line in lines if line.startswith(prefix)]
    if (len(values) != 1 or lines.count("INSTRUMENTATION_RESULT: stream=JADIX_NATIVE_SHELL_PASS") != 1
            or "JADIX_NATIVE_SHELL_FAIL" in log or lines.count("INSTRUMENTATION_CODE: -1") != 1):
        raise ValueError("missing, repeated or failed Shell receipt")
    value = json.loads(values[0])
    expected = {"schema": 1, "token": token, "passed": True, "api": api, "activations": 20,
                "unicode": "中文😀 العربية", "keyed_identity_retained": True, "activity_checkpoint_restored": True}
    if not isinstance(value, dict) or any(type(value.get(key)) is not type(item) or value[key] != item for key, item in expected.items()):
        raise ValueError("Shell receipt identity or checks mismatch")
    return value


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--adb", type=Path, required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    apk = args.apk.resolve()
    build = json.loads(apk.with_name("build.json").read_text(encoding="utf-8"))
    if (build.get("application") != "shell" or build.get("package_name") != OWNED.PACKAGE
            or build.get("min_sdk") != 26 or build.get("apk_identity") != native.file_identity(apk)):
        raise ValueError("identified native Shell APK required")
    adb = [str(args.adb.resolve()), "-s", args.serial]
    report = {"schema": 1, "passed": False, "token": str(uuid.uuid4()), "serial": args.serial,
              "artifact": native.file_identity(apk), "runner": native.file_identity(Path(__file__)), "commands": []}

    def run(command):
        result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=180)
        report["commands"].append({"command": command, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return result

    try:
        api = int(run(adb + ["shell", "getprop", "ro.build.version.sdk"]).stdout.strip())
        if api < 26:
            raise ValueError("Android API 26 or later required")
        with OWNED.owned_install(run, adb, apk, report):
            result = run(adb + ["shell", "am", "instrument", "-r", "-w", "-e", "jadix.token", report["token"], OWNED.PACKAGE + "/.NativeShellChecks"])
            log = result.stdout + result.stderr
            (args.output / "instrumentation.log").write_text(log, encoding="utf-8")
            report["shell"] = receipt(log, report["token"], api)
        report["passed"] = True
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (args.output / "checks.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
