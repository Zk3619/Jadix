import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    if any((directory / "snapshot.json").exists() for directory in (output, *output.parents)):
        parser.error("output contains an archived snapshot; select a new directory")
    output.mkdir(parents=True, exist_ok=True)
    apk = arguments.apk.resolve()
    build = json.loads((apk.parent / "build.json").read_text(encoding="utf8"))
    assert build["application"] == "notes" and build["rust_profile"] == "release"
    assert build["apk_sha256"] == hashlib.sha256(apk.read_bytes()).hexdigest()
    assert build["apk_bytes"] == apk.stat().st_size
    adb = ["adb", "-s", arguments.serial]
    observations = []

    def run(*command):
        result = subprocess.run(adb + list(command), capture_output=True, text=True, encoding="utf8", errors="replace")
        observations.append({"command": list(command), "returncode": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        (output / "commands.json").write_text(json.dumps(observations, indent=2, ensure_ascii=False), encoding="utf8")
        result.check_returncode()
        return result.stdout

    run("install", "-r", str(apk))
    log = run("shell", "am", "instrument", "-w", "dev.jadix.notebook/dev.jadix.nativeui.NotesChecks")
    (output / "instrumentation.log").write_text(log, encoding="utf8")
    assert "JADIX_NOTES_CHECKS_PASS" in log and "INSTRUMENTATION_CODE: -1" in log and "failure=" not in log, log
    identity = run("shell", "getprop")
    (output / "system-properties.log").write_text(identity, encoding="utf8")
    run("shell", "am", "start", "-W", "-n", "dev.jadix.notebook/dev.jadix.nativeui.NotesActivity")
    run("shell", "uiautomator", "dump", "/sdcard/jadix-notes.xml")
    run("pull", "/sdcard/jadix-notes.xml", str(output / "accessibility.xml"))
    screenshot = subprocess.run(adb + ["exec-out", "screencap", "-p"], capture_output=True, check=True).stdout
    (output / "notes.png").write_bytes(screenshot)
    report = {"passed": True, "build": build, "serial": arguments.serial,
              "scope": "Owned target: InputConnection edits, normal finish/relaunch durable documents, sequential stale-writer rejection, malformed UTF-8 protection and explicit replacement; no power-loss/readers/physical-device proof"}
    (output / "notes.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
