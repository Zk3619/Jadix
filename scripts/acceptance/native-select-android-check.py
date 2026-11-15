"""Verify the isolated native Select application and remove only this run's install."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile

from native_browser import validate_output

PACKAGE = "dev.jadix.selection"


def artifact(apk):
    build = json.loads((apk.parent / "build.json").read_text(encoding="utf8"))
    if (build.get("application") != "controls" or build.get("package_name") != PACKAGE
            or build.get("selection") is not True or build.get("extended") is not False
            or build.get("retained_rows") != 0 or build.get("rust_profile") != "release"):
        raise ValueError("requires the isolated --selection release APK with zero retained rows")
    abi = {"x86_64-linux-android": "x86_64", "aarch64-linux-android": "arm64-v8a"}.get(build.get("target"))
    if abi is None:
        raise ValueError("unsupported Android target")
    data = apk.read_bytes()
    if len(data) != build.get("apk_bytes") or hashlib.sha256(data).hexdigest() != build.get("apk_sha256"):
        raise ValueError("APK does not match build report")
    with zipfile.ZipFile(apk) as archive:
        library = f"lib/{abi}/libjadix_native_controls.so"
        if archive.namelist().count(library) != 1:
            raise ValueError("APK must contain exactly one reported native library")
        data = archive.read(library)
    if len(data) != build.get("library_bytes") or hashlib.sha256(data).hexdigest() != build.get("library_sha256"):
        raise ValueError("native library does not match build report")
    return {**build, "build_report_sha256": hashlib.sha256((apk.parent / "build.json").read_bytes()).hexdigest()}


def instrumentation_passed(log):
    if "JADIX_SELECT_CHECKS_FAIL" in log:
        return False
    receipts = re.findall(r"JADIX_SELECT_CHECKS_PASS: ([^\r\n]*)", log)
    if len(receipts) != 1:
        return False
    fields = {}
    for part in receipts[0].split(";"):
        if not part.strip():
            continue
        if part.count("=") != 1:
            return False
        name, value = (item.strip() for item in part.split("=", 1))
        if name in fields:
            return False
        fields[name] = value
    return fields == {"scenarios": "10", "native_select_checked": "true", "popup_checked": "true",
                      "recreate_checked": "true", "disposal_checked": "true"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--aapt2", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    apk = args.apk.resolve()
    try:
        validate_output(output)
        build = artifact(apk)
        aapt = args.aapt2 or Path(os.environ["ANDROID_HOME"]) / "build-tools/36.1.0" / ("aapt2.exe" if os.name == "nt" else "aapt2")
        if not aapt.is_file():
            raise ValueError("requires the Android build-tools aapt2 executable")
    except (ValueError, KeyError, OSError, zipfile.BadZipFile) as error:
        parser.error(str(error))
    output.mkdir(parents=True, exist_ok=True)
    commands = []
    report = {"passed": False, "build": build, "serial": args.serial, "package": PACKAGE,
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "limits": ["Instrumented native popup/accessibility actions and injected keys on the selected device; not physical input or TalkBack.",
                         "Only the absent-baseline isolated sample is installed/removed; no upgrade or general private-data preservation claim.",
                         "No Apple, production release signing, other OS versions or full N1-N5 acceptance."]}
    installed = False
    baseline = None
    adb = ["adb", "-s", args.serial]

    def run(command, *, binary=False):
        result = subprocess.run([str(item) for item in command], capture_output=True, timeout=120)
        record = {"command": [str(item) for item in command], "exit_code": result.returncode,
                  "stderr": result.stderr.decode("utf8", errors="replace")}
        if binary:
            record.update(stdout_bytes=len(result.stdout), stdout_sha256=hashlib.sha256(result.stdout).hexdigest())
        else:
            record["stdout"] = result.stdout.decode("utf8", errors="replace")
        commands.append(record)
        (output / "commands.json").write_text(json.dumps(commands, indent=2) + "\n", encoding="utf8")
        result.check_returncode()
        return result.stdout if binary else record["stdout"]

    def packages():
        return sorted(line.strip() for line in run(adb + ["shell", "pm", "list", "packages", "-u"]).splitlines() if line.strip())

    try:
        badging = run([aapt, "dump", "badging", apk])
        if not re.search(r"^package: name='" + re.escape(PACKAGE) + "'", badging, re.MULTILINE):
            raise RuntimeError("APK manifest package is not the isolated Select identity")
        manifest = run([aapt, "dump", "xmltree", "--file", "AndroidManifest.xml", apk])
        targets = re.findall(r'targetPackage\([^)]*\)="([^"]+)"', manifest)
        if targets != [PACKAGE]:
            raise RuntimeError("instrumentation must target only the isolated Select package")
        (output / "manifest.log").write_text(manifest, encoding="utf8")
        run(adb + ["get-state"])
        baseline = packages()
        report["baseline_packages"] = baseline
        if "package:" + PACKAGE in baseline:
            raise RuntimeError("Select package already registered (including uninstalled entries); refusing replacement")
        result = run(adb + ["install", str(apk)])
        if "Success" not in result.splitlines():
            raise RuntimeError("install did not report Success")
        installed = True
        log = run(adb + ["shell", "am", "instrument", "-w", PACKAGE + "/dev.jadix.nativeui.SelectChecks"])
        (output / "instrumentation.log").write_text(log, encoding="utf8")
        if not instrumentation_passed(log):
            (output / "failure-screen.png").write_bytes(run(adb + ["exec-out", "screencap", "-p"], binary=True))
            for name in ["select-popup.png", "select-native.png"]:
                try:
                    (output / name).write_bytes(run(adb + ["exec-out", "run-as", PACKAGE, "cat", "files/" + name], binary=True))
                except subprocess.CalledProcessError:
                    pass
            raise RuntimeError("native Select instrumentation receipt is missing, failed or inconsistent")
        for name in ["select-checks.json", "select-native.png", "select-popup.png"]:
            data = run(adb + ["exec-out", "run-as", PACKAGE, "cat", "files/" + name], binary=True)
            (output / name).write_bytes(data)
        checks = json.loads((output / "select-checks.json").read_text(encoding="utf8"))
        if checks.get("passed") is not True or len(checks.get("checks", [])) != 10:
            raise RuntimeError("native Select scenario report is incomplete")
        report.update(sdk=int(run(adb + ["shell", "getprop", "ro.build.version.sdk"]).strip()),
                      fingerprint=run(adb + ["shell", "getprop", "ro.build.fingerprint"]).strip(), checks=checks["checks"])
        report["runtime_passed"] = True
    except Exception as error:
        report["error"] = str(error)
    finally:
        report["cleanup"] = {"installed_by_this_run": installed, "completed": False}
        if installed:
            try:
                run(adb + ["shell", "am", "force-stop", PACKAGE])
                if run(adb + ["uninstall", PACKAGE]).strip() != "Success":
                    raise RuntimeError("owned Select uninstall did not report Success")
                after = packages()
                report["cleanup"].update(completed=True, after_packages=after, package_registry_restored=after == baseline)
            except Exception as error:
                report["cleanup"]["error"] = str(error)
        report["passed"] = (report.get("runtime_passed") is True and report["cleanup"].get("package_registry_restored") is True)
        (output / "checks.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    if not report["passed"]:
        raise RuntimeError(report.get("error") or report["cleanup"].get("error") or "Select package registry did not return to baseline")
    print("PASS native Android Select: 10 scenarios; isolated package removed and package registry restored")


if __name__ == "__main__":
    main()
