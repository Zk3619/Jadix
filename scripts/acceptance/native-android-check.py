import argparse
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import subprocess
import re
import time
import uuid
import xml.etree.ElementTree as ElementTree


def name_editor(node, value=None):
    if node.get("class") != "android.widget.EditText":
        return False
    if value is not None:
        return node.get("text") == value and (not node.get("hint") or node.get("hint") == "Name / 中文")
    return node.get("hint", node.get("text")) == "Name / 中文"


def instrumentation_passed(log, extended):
    receipts = re.findall(r"JADIX_NATIVE_CHECKS_PASS: ([^\r\n]*)", log)
    if len(receipts) != 1 or "JADIX_NATIVE_CHECKS_FAIL" in log:
        return False
    fields = {}
    for item in receipts[0].split(";"):
        if "=" not in item:
            continue
        name, value = (part.strip() for part in item.split("=", 1))
        if name in fields:
            return False
        fields[name] = value
    required = {"extensions": str(extended).lower(), "retained_editing_checked": "true",
                "retained_editors": str(3 if extended else 1)}
    if extended:
        required.update(dialog_ime_checked="true", switch_checked="true")
    return all(fields.get(name) == value for name, value in required.items())


def keyboard_receipt(log, token, rows):
    prefix = "INSTRUMENTATION_RESULT: jadix_keyboard="
    receipts = [line[len(prefix):] for line in log.splitlines() if line.startswith(prefix)]
    if len(receipts) != 1 or log.splitlines().count("INSTRUMENTATION_RESULT: stream=JADIX_NATIVE_KEYBOARD_PASS") != 1 or "JADIX_NATIVE_KEYBOARD_FAIL" in log or log.splitlines().count("INSTRUMENTATION_CODE: -1") != 1:
        raise ValueError("missing, repeated or failed keyboard receipt")
    value = json.loads(receipts[0])
    if not isinstance(value, dict):
        raise ValueError("keyboard receipt must be an object")
    required = {"schema": 1, "retained_rows": rows, "controls": 8, "key_events": 84, "initial_focus_key_events": 6, "remounts": 25}
    if any(type(value.get(name)) is not int or value[name] != expected for name, expected in required.items()):
        raise ValueError("keyboard count/schema mismatch")
    if value.get("passed") is not True or value.get("token") != token or value.get("event_authority") != "Instrumentation-injected Tab/Shift+Tab; programmatic initial focus, viewport and remounts; soft keyboard suppressed":
        raise ValueError("keyboard token, result or authority mismatch")
    phases = value.get("phases")
    if not isinstance(phases, list) or len(phases) != 3:
        raise ValueError("three keyboard phases required")
    labels = ["Accept", "Airplane mode", "Password / 密码", "Multiline / 多行", "Open editor", "Continue", "Name / 中文", "Reset"]
    for phase, name in zip(phases, ("initial", "compact-viewport", "after-25-remounts")):
        if not isinstance(phase, dict) or phase.get("name") != name:
            raise ValueError("keyboard phase order mismatch")
        for direction, expected in (("forward", labels), ("backward", labels[::-1])):
            observations = phase.get(direction)
            if not isinstance(observations, list) or len(observations) != 8:
                raise ValueError("all eight keyboard controls required")
            for observation, label in zip(observations, expected):
                if not isinstance(observation, dict) or observation.get("control") != label or observation.get("fully_visible") is not True:
                    raise ValueError("keyboard order or visibility mismatch")
                bounds = observation.get("bounds")
                if not isinstance(bounds, list) or len(bounds) != 4 or any(type(number) is not int for number in bounds) or not bounds[0] < bounds[2] or not bounds[1] < bounds[3]:
                    raise ValueError("invalid keyboard control bounds")
    return value


def editing_receipt(log, token, rows):
    prefix = "INSTRUMENTATION_RESULT: jadix_editing="
    receipts = [line[len(prefix):] for line in log.splitlines() if line.startswith(prefix)]
    if len(receipts) != 1 or log.splitlines().count("INSTRUMENTATION_RESULT: stream=JADIX_NATIVE_EDITING_PASS") != 1 or "JADIX_NATIVE_EDITING_FAIL" in log or log.splitlines().count("INSTRUMENTATION_CODE: -1") != 1:
        raise ValueError("missing, repeated or failed editing receipt")
    value = json.loads(receipts[0])
    if not isinstance(value, dict):
        raise ValueError("editing receipt must be an object")
    for name, expected in {"schema": 1, "retained_rows": rows, "key_events": 42}.items():
        if type(value.get(name)) is not int or value[name] != expected:
            raise ValueError("editing schema or counts mismatch")
    authority = "Instrumentation-injected clipboard/undo shortcuts; programmatic InputConnection seed, selection, controlled reset and sibling perturbation; soft keyboard suppressed"
    if value.get("passed") is not True or value.get("token") != token or value.get("clipboard_restored") is not True or value.get("event_authority") != authority:
        raise ValueError("editing token, authority or clipboard restoration mismatch")
    editors = value.get("editors")
    checks = ["unicode-commit", "copy", "cut", "paste", "undo", "redo", "history-after-republish-reorder", "controlled-reset-clears-history"]
    if not isinstance(editors, list) or len(editors) != 2:
        raise ValueError("both native editors required")
    for editor, label in zip(editors, ("Name / 中文", "Multiline / 多行")):
        if not isinstance(editor, dict) or editor.get("control") != label or editor.get("checks") != checks or editor.get("native_identity_retained") is not True or editor.get("business_roundtrip") is not True:
            raise ValueError("editing checks or native identity incomplete")
    if value.get("password_checks") != ["unicode-paste", "copy-blocked", "cut-blocked", "public-state-isolation"]:
        raise ValueError("password privacy checks incomplete")
    return value


PACKAGE = "dev.jadix.nativeui"


def artifact(apk, extended, rows):
    source = (apk.parent / "build.json").read_bytes()
    build = json.loads(source)
    data = apk.read_bytes()
    if (build.get("package_name") != PACKAGE or build.get("application") != "controls"
            or build.get("extended") is not extended or type(build.get("retained_rows")) is not int
            or build["retained_rows"] != rows or build.get("apk_bytes") != len(data)
            or build.get("apk_sha256") != hashlib.sha256(data).hexdigest()):
        raise ValueError("APK does not match the controls build report and requested scenario")
    return {"build_report_sha256": hashlib.sha256(source).hexdigest(),
            "apk_sha256": build["apk_sha256"], "package_name": PACKAGE}


def packages(run, adb):
    lines = [line.strip() for line in run(adb + ["shell", "pm", "list", "packages", "-u"]).stdout.splitlines() if line.strip()]
    if not lines or len(lines) != len(set(lines)) or any(not re.fullmatch(r"package:[A-Za-z0-9_.]+", line) for line in lines):
        raise RuntimeError("complete package registry is empty, malformed or repeated")
    return sorted(lines)


@contextmanager
def owned_install(run, adb, apk, report, keep_installed=False, package=PACKAGE):
    cleanup = {"installed_by_this_run": False, "kept_installed": False, "completed": False}
    report["cleanup"] = cleanup
    baseline = packages(run, adb)
    report["baseline_packages"] = baseline
    if "package:" + package in baseline:
        raise RuntimeError("controls package already registered (including uninstalled entries); refusing replacement")
    try:
        result = run(adb + ["install", str(apk.resolve())])
        if result.stdout.splitlines().count("Success") != 1:
            raise RuntimeError("install did not report exactly one Success")
        cleanup["installed_by_this_run"] = True
        yield
        if keep_installed:
            after = packages(run, adb)
            if after != sorted([*baseline, "package:" + package]):
                raise RuntimeError("owned installation changed the package registry unexpectedly")
            cleanup.update(kept_installed=True, completed=True, after_packages=after,
                           package_registry_restored=False)
    finally:
        if cleanup["installed_by_this_run"] and not cleanup["kept_installed"]:
            try:
                if run(adb + ["uninstall", package]).stdout.strip() != "Success":
                    raise RuntimeError("owned controls uninstall did not report Success")
                after = packages(run, adb)
                cleanup.update(after_packages=after, package_registry_restored=after == baseline)
                if after != baseline:
                    raise RuntimeError("package registry did not return to baseline")
                cleanup["completed"] = True
            except Exception as error:
                cleanup["error"] = str(error)
                raise


def dump_tree(run, adb, destination):
    remote = "/data/local/tmp/jadix-native-" + str(uuid.uuid4()) + ".xml"
    try:
        for attempt in range(3):
            result = run(adb + ["shell", "uiautomator", "dump", remote])
            if any(line.strip() in ("UI hierchary dumped to: " + remote, "UI hierarchy dumped to: " + remote) for line in result.stdout.splitlines()):
                break
            if "ERROR: null root node returned by UiTestAutomationBridge." not in result.stdout + result.stderr or attempt == 2:
                raise RuntimeError("UI tree export was not confirmed: " + result.stdout + result.stderr)
            time.sleep(0.2)
        run(adb + ["pull", remote, str(destination)])
    finally:
        run(adb + ["shell", "rm", "-f", remote])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", type=Path, help="Explicit adb executable; otherwise resolve adb from PATH")
    parser.add_argument("--nonce", default=uuid.uuid4().hex)
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/native-controls/android-checks"))
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--keep-installed", action="store_true", help="Retain only this run's successful install for a following performance check; failures are still cleaned")
    parser.add_argument("--large-list", action="store_true")
    parser.add_argument("--process-recovery", action="store_true", help="Kill the background sample process and verify saved-task business state recovery")
    arguments = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{32}", arguments.nonce):
        parser.error("nonce must contain exactly 32 lowercase hexadecimal digits")
    arguments.output = arguments.output.resolve()
    if any((directory / "snapshot.json").exists() for directory in (arguments.output, *arguments.output.parents)):
        parser.error("output contains an archived snapshot; select a new output directory")
    arguments.output.mkdir(parents=True, exist_ok=True)
    adb = [str(arguments.adb.resolve()) if arguments.adb else "adb", "-s", arguments.serial]
    commands = []

    def run(command):
        result = subprocess.run(command, capture_output=True, text=True, encoding="utf8", errors="replace", timeout=120)
        commands.append({"command": command, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        (arguments.output / "commands.json").write_text(json.dumps(commands, indent=2) + "\n", encoding="utf8")
        result.check_returncode()
        return result

    report = {"passed": False, "nonce": arguments.nonce, "serial": arguments.serial,
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    try:
        rows = 1000 if arguments.large_list else 100
        report["artifact"] = artifact(arguments.apk.resolve(), arguments.extended, rows)
        with owned_install(run, adb, arguments.apk, report, arguments.keep_installed):
            rows = 1000 if arguments.large_list else 100
            result = run(adb + ["shell", "am", "instrument", "-w", "-e", "jadix.rows", str(rows), "dev.jadix.nativeui/.NativeChecks"])
            log = result.stdout + result.stderr
            (arguments.output / "instrumentation.log").write_text(log, encoding="utf8")
            if not instrumentation_passed(log, arguments.extended):
                raise RuntimeError(log)
            keyboard = None
            editing = None
            if arguments.extended:
                token = str(uuid.uuid4())
                keyboard_run = run(adb + ["shell", "am", "instrument", "-r", "-w", "-e", "jadix.token", token, "-e", "jadix.rows", str(rows), "dev.jadix.nativeui/.NativeKeyboardChecks"])
                keyboard_log = keyboard_run.stdout + keyboard_run.stderr
                (arguments.output / "keyboard-instrumentation.log").write_text(keyboard_log, encoding="utf8")
                keyboard = keyboard_receipt(keyboard_log, token, rows)
                (arguments.output / "keyboard.json").write_text(json.dumps(keyboard, ensure_ascii=False, indent=2) + chr(10), encoding="utf8")
                keyboard_image = subprocess.run(adb + ["exec-out", "run-as", "dev.jadix.nativeui", "cat", "files/keyboard-navigation.png"], capture_output=True, check=True, timeout=120).stdout
                if not keyboard_image.startswith(b"\x89PNG\r\n\x1a\n"):
                    raise ValueError("keyboard screenshot is not PNG")
                (arguments.output / "keyboard-navigation.png").write_bytes(keyboard_image)
                editing_token = str(uuid.uuid4())
                editing_run = run(adb + ["shell", "am", "instrument", "-r", "-w", "-e", "jadix.token", editing_token, "-e", "jadix.rows", str(rows), "dev.jadix.nativeui/.NativeEditingChecks"])
                editing_log = editing_run.stdout + editing_run.stderr
                (arguments.output / "editing-instrumentation.log").write_text(editing_log, encoding="utf8")
                editing = editing_receipt(editing_log, editing_token, rows)
                (arguments.output / "editing.json").write_text(json.dumps(editing, ensure_ascii=False, indent=2) + chr(10), encoding="utf8")
                editing_image = subprocess.run(adb + ["exec-out", "run-as", "dev.jadix.nativeui", "cat", "files/editing.png"], capture_output=True, check=True, timeout=120).stdout
                if not editing_image.startswith(b"\x89PNG\r\n\x1a\n"):
                    raise ValueError("editing screenshot is not PNG")
                (arguments.output / "editing.png").write_bytes(editing_image)
            sdk = int(run(adb + ["shell", "getprop", "ro.build.version.sdk"]).stdout.strip())
            if arguments.extended:
                screenshot = subprocess.run(adb + ["exec-out", "run-as", "dev.jadix.nativeui", "cat", "files/dialog-ime.png"], capture_output=True, check=True).stdout
                (arguments.output / "native-android-dialog-ime.png").write_bytes(screenshot)
            identity = run(adb + ["shell", "getprop"]).stdout
            (arguments.output / "system-properties.log").write_text(identity, encoding="utf8")
            run(adb + ["shell", "am", "start", "-W", "-n", "dev.jadix.nativeui/.MainActivity"])
            memory = run(adb + ["shell", "dumpsys", "meminfo", "dev.jadix.nativeui"]).stdout
            (arguments.output / "memory.log").write_text(memory, encoding="utf8")
            dump_tree(run, adb, arguments.output / "accessibility.xml")
            screenshot = subprocess.run(adb + ["exec-out", "screencap", "-p"], capture_output=True, check=True).stdout
            (arguments.output / "native-android.png").write_bytes(screenshot)
            if arguments.extended:
                tree = ElementTree.parse(arguments.output / "accessibility.xml")
                trigger = next(node for node in tree.iter("node") if node.get("text", "").lower() == "open editor")
                left, top, right, bottom = map(int, re.findall(r"\d+", trigger.attrib["bounds"]))
                run(adb + ["shell", "input", "tap", str((left+right)//2), str((top+bottom)//2)])
                dump_tree(run, adb, arguments.output / "dialog-accessibility.xml")
                modal_tree = ElementTree.parse(arguments.output / "dialog-accessibility.xml")
                assert any(node.get("text") == "Edit details" for node in modal_tree.iter("node")), "native modal title missing"
                screenshot = subprocess.run(adb + ["exec-out", "screencap", "-p"], capture_output=True, check=True).stdout
                (arguments.output / "native-android-dialog.png").write_bytes(screenshot)
                run(adb + ["shell", "input", "keyevent", "KEYCODE_BACK"])
            recovery = None
            if arguments.process_recovery:
                observations = []

                def observe(*command):
                    result = run(adb + list(command))
                    observations.append({"command": list(command), "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
                    (arguments.output / "process-recovery-commands.json").write_text(json.dumps(observations, indent=2) + "\n", encoding="utf8")
                    return result.stdout

                def tree(name):
                    path = arguments.output / name
                    dump_tree(run, adb, path)
                    return list(ElementTree.parse(path).iter("node"))

                def tap(node):
                    left, top, right, bottom = map(int, re.findall(r"\d+", node.attrib["bounds"]))
                    observe("shell", "input", "tap", str((left + right) // 2), str((top + bottom) // 2))

                package = "dev.jadix.nativeui"
                nodes = tree("process-initial.xml")
                counter = next(node for node in nodes if node.get("text", "").startswith("Count: "))
                expected_count = int(re.match(r"Count: (\d+) /", counter.attrib["text"]).group(1)) + 1
                tap(next(node for node in nodes if node.get("class") == "android.widget.Button" and node.get("text", "").lower() == "continue"))
                tap(next(node for node in nodes if name_editor(node)))
                value = f"process_recovery_{int(time.time())}"
                observe("shell", "input", "text", value)
                nodes = tree("process-before.xml")
                expected_summary = f"Count: {expected_count} / Name: {value}"
                assert any(node.get("text") == expected_summary for node in nodes), "system input did not reach Rust business summary"
                before_pid = observe("shell", "pidof", package).strip()
                assert before_pid.isdigit(), before_pid
                observe("shell", "input", "keyevent", "KEYCODE_HOME")
                deadline = time.monotonic() + 10
                while True:
                    observe("shell", "am", "kill", package)
                    result = subprocess.run(adb + ["shell", "pidof", package], capture_output=True, text=True, encoding="utf8", check=False)
                    observations.append({"command": ["shell", "pidof", package], "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
                    if result.returncode == 1 and not result.stdout.strip():
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError("background process was not killed; refusing to substitute force-stop or relaunch a live process")
                    time.sleep(0.1)
                observe("shell", "am", "start", "-W", "-n", package + "/.MainActivity")
                after_pid = observe("shell", "pidof", package).strip()
                assert after_pid.isdigit() and after_pid != before_pid, (before_pid, after_pid)
                nodes = tree("process-after.xml")
                assert any(node.get("text") == expected_summary for node in nodes), "new process did not restore Rust business state"
                assert any(name_editor(node, value) for node in nodes), "new process did not restore native editor value"
                recovery = {"passed": True, "before_pid": before_pid, "after_pid": after_pid, "counter": expected_count,
                            "value": value, "termination": "am kill after Home; old process absence verified before restart",
                            "limits": ["One owned AVD and bounded form state; this is not power-loss, force-stop, physical-device or long-term document persistence coverage."]}
                (arguments.output / "process-recovery.json").write_text(json.dumps(recovery, indent=2) + "\n", encoding="utf8")
            report.update({"passed": True, "serial": arguments.serial, "extended": arguments.extended, "sdk": sdk, "dialog_ime_checked": arguments.extended,
                      "switch_checked": arguments.extended,
                      "keyboard_navigation": keyboard,
                      "native_editing": editing,
                      "retained_editing_checked": True, "retained_editors": 3 if arguments.extended else 1,
                      "process_recovery": recovery,
                      "retained_rows": rows,
                      "apk_bytes": arguments.apk.stat().st_size,
                      "apk_sha256": hashlib.sha256(arguments.apk.read_bytes()).hexdigest(),
                      "instrumentation_sha256": hashlib.sha256(log.encode("utf8")).hexdigest(),
                      "limits": ["Device kind must be identified from system-properties; emulator results do not establish physical-device or TalkBack coverage"]})
            print(log.strip())
    except Exception as error:
        report.update(passed=False, error=str(error))
        raise
    finally:
        (arguments.output / "checks.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")


if __name__ == "__main__":
    main()
