import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import uuid
from datetime import datetime
import xml.etree.ElementTree as ElementTree

from native_browser import validate_output


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


COMPARISON = module("android_comparison", "native-android-comparison.py")
SCALE = module("notes_scale", "native-notes-scale.py")
PACKAGE = "dev.jadix.notebook"
COMPONENT = PACKAGE + "/dev.jadix.nativeui.NotesActivity"
DOCUMENT = "jadix-notes-v1.json"


def data_names(text):
    names = text.splitlines()
    if any(any(character.isspace() for character in name) for name in names):
        raise ValueError("expected one filename per listing line")
    return [name for name in names if COMPARISON.CACHE_NAME.fullmatch(name) or name in [DOCUMENT, DOCUMENT + ".new", DOCUMENT + ".bak"]]


def recovery_matches(payload, fixture, title, body):
    try:
        state = json.loads(payload)
        return state["archive"] == fixture and state["selected"] is None and state["dirty"] is True and state["draft"] == {"title": title, "body": body}
    except (ValueError, KeyError, TypeError):
        return False


def tap_center(node):
    match = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", node.get("bounds", ""))
    if match is None:
        raise ValueError("invalid node bounds")
    left, top, right, bottom = map(int, match.groups())
    if right <= left or bottom <= top:
        raise ValueError("node has no visible bounds")
    return str((left + right) // 2), str((top + bottom) // 2)


def control_matches(node, label, field=False):
    if field:
        return node.get("class") == "android.widget.EditText" and node.get("hint", node.get("text")) == label
    return node.get("class") == "android.widget.Button" and (node.get("text", "") or node.get("content-desc", "")).casefold() == label.casefold()


def field_value_matches(node, label, value):
    return node.get("class") == "android.widget.EditText" and node.get("text") == value and (not node.get("hint") or node.get("hint") == label)


def scale_metrics(instrument):
    if "JADIX_NOTES_SCALE_PASS" not in instrument or "INSTRUMENTATION_CODE: -1" not in instrument or "failure=" in instrument:
        raise ValueError("capacity instrumentation failed")
    match = re.search(r"^INSTRUMENTATION_RESULT: metrics=(.*)$", instrument, re.MULTILINE)
    if match is None:
        raise ValueError("capacity metrics missing")
    metrics = json.loads(match[1])
    if metrics.get("notes") != 128 or metrics.get("fixture_bytes") != 522240:
        raise ValueError("capacity metrics differ from fixture")
    for name in ["input_dispatch_ms", "input_to_draw_callback_ms"]:
        samples = metrics.get(name)
        if not isinstance(samples, list) or len(samples) != 30 or any(type(sample) not in (int, float) or not math.isfinite(sample) or sample < 0 for sample in samples):
            raise ValueError("expected 30 finite nonnegative samples: " + name)
    return metrics


def main():
    parser = argparse.ArgumentParser(description="Run real Android notebook capacity and saved-task process recovery on an owned test installation.")
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    try:
        validate_output(output)
        build = COMPARISON.artifact(args.apk)
        if build["application"] != "notes":
            raise ValueError("requires the notebook application")
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))
    output.mkdir(parents=True, exist_ok=True)
    sdk = Path(os.environ["ANDROID_HOME"])
    java = Path(os.environ["JAVA_HOME"]) / "bin" / ("java.exe" if os.name == "nt" else "java")
    tools = sdk / "build-tools/36.1.0"
    adb = [str(sdk / "platform-tools" / ("adb.exe" if os.name == "nt" else "adb")), "-s", args.serial]
    report = {"passed": False, "complete": False, "observed_at": datetime.now().astimezone().isoformat(),
              "serial": args.serial, "build": build, "runner_sha256": COMPARISON.digest(Path(__file__).read_bytes()),
              "limits": ["Owned debuggable sample; instrumentation and clear-task/force-stop change its tasks. Original package and tracked data are restored and verified.",
                         "Instrumentation InputConnection and UIAutomator/adb are controlled input, not physical IME or screen-reader proof.",
                         "Native OnDraw callback is not compositor/display presentation. This runner does not measure peak memory or leak freedom.",
                         "Home/am kill covers saved-task process loss; not power loss, force-stop persistence, minimum OS or physical-device acceptance."]}
    sequence, changed = 0, False
    originals, backup = None, None
    scratch = "jadix-notes-scale-" + uuid.uuid4().hex + ".json"

    def run(command, binary=False, payload=None, allowed=(0,)):
        nonlocal sequence
        result = subprocess.run([str(item) for item in command], input=payload, capture_output=True, timeout=120)
        sequence += 1
        value = {"command": [str(item) for item in command], "exit": result.returncode,
                 "stdout": None if binary else result.stdout.decode("utf8", errors="replace"),
                 "stdout_sha256": COMPARISON.digest(result.stdout), "stdout_bytes": len(result.stdout),
                 "stdin_sha256": COMPARISON.digest(payload) if payload is not None else None,
                 "stderr": result.stderr.decode("utf8", errors="replace")}
        (output / f"command-{sequence:04d}.json").write_text(json.dumps(value, indent=2), encoding="utf8")
        if result.returncode not in allowed:
            raise RuntimeError(f"command failed ({result.returncode}): {command}: {value['stderr']}")
        return result.stdout if binary else result.stdout.decode("utf8", errors="strict")

    def device(*command, **options):
        return run(adb + list(command), **options)

    def private(*command, **options):
        return device("shell", "-T", "run-as", PACKAGE, *command, **options)

    def installed():
        paths = device("shell", "pm", "path", PACKAGE).strip().splitlines()
        if len(paths) != 1 or not paths[0].startswith("package:/data/app/"):
            raise ValueError("expected one installed base APK")
        path = paths[0][8:]
        return path, device("shell", "sha256sum", path).split()[0]

    def files():
        return {name: private("cat", "files/" + name, binary=True) for name in data_names(private("ls", "-1", "files"))}

    def write(path, payload):
        if private("tee", path, payload=payload, binary=True) != payload or private("cat", path, binary=True) != payload:
            raise RuntimeError("private file write/readback failed")

    def tree(label):
        remote = "/sdcard/" + scratch + ".xml"
        device("shell", "uiautomator", "dump", remote)
        payload = device("shell", "cat", remote, binary=True)
        (output / (label + ".xml")).write_bytes(payload)
        device("shell", "rm", "-f", remote)
        return list(ElementTree.fromstring(payload).iter("node"))

    def locate(label, field=False):
        for attempt in range(8):
            nodes = tree("locate-" + str(sequence))
            candidates = [node for node in nodes if control_matches(node, label, field)]
            for node in candidates:
                try:
                    tap_center(node)
                    return node
                except ValueError:
                    pass
            scroll = next(node for node in nodes if node.get("scrollable") == "true")
            coordinates = list(map(int, re.findall(r"\d+", scroll.get("bounds"))))
            left, top, right, bottom = coordinates
            device("shell", "input", "swipe", str((left + right) // 2), str(top + (bottom - top) * 3 // 4), str(top + (bottom - top) // 4), "250")
        raise RuntimeError("could not locate visible control: " + label)

    def click(label, field=False):
        device("shell", "input", "tap", *tap_center(locate(label, field)))

    try:
        signer = COMPARISON.signer_certificates(run([java, "-jar", tools / "lib/apksigner.jar", "verify", "--print-certs", args.apk]))
        identity = COMPARISON.manifest_identity(run([tools / ("aapt2.exe" if os.name == "nt" else "aapt2"), "dump", "badging", args.apk]))
        if identity[:2] != [PACKAGE, "dev.jadix.nativeui.NotesActivity"]:
            raise ValueError("unexpected notebook manifest")
        report["device"] = {name: device("shell", "getprop", name).strip() for name in ["ro.build.fingerprint", "ro.build.version.sdk", "ro.product.cpu.abi"]}
        if report["device"]["ro.product.cpu.abi"] != COMPARISON.ABIS[build["target"]]:
            raise ValueError("device ABI differs from artifact")
        path, original_hash = installed()
        original_apk = output / "original.apk"
        original_apk.write_bytes(device("exec-out", "cat", path, binary=True))
        if COMPARISON.digest(original_apk.read_bytes()) != original_hash or COMPARISON.signer_certificates(run([java, "-jar", tools / "lib/apksigner.jar", "verify", "--print-certs", original_apk])) != signer:
            raise ValueError("original APK backup or signer mismatch")
        report["original_apk_sha256"] = original_hash
        device("shell", "am", "force-stop", PACKAGE)
        originals = files()
        now = int(device("shell", "date", "+%s").strip())
        metadata = {}
        for name in originals:
            if private("stat", "-c", "%F", "files/" + name).strip() != "regular file":
                raise ValueError("protected test data must be regular files")
            modified = int(private("stat", "-c", "%Y", "files/" + name).strip())
            mode = private("stat", "-c", "%a", "files/" + name).strip()
            if not re.fullmatch(r"[0-7]{3,4}", mode):
                raise ValueError("unexpected file mode")
            if COMPARISON.CACHE_NAME.fullmatch(name) and now - modified > 6 * 86400:
                raise ValueError("recovery data approaches expiry; select an owned clean installation")
            metadata[name] = {"mtime": modified, "mode": mode}
        caches = [name for name in originals if COMPARISON.CACHE_NAME.fullmatch(name)]
        if len({name[:54] for name in caches}) > 14 or sum(len(originals[name]) for name in caches) > 12 * 1024 * 1024:
            raise ValueError("capacity test requires two free recovery identities and four MiB headroom")
        backup = Path(tempfile.mkdtemp(prefix="jadix-notes-recovery-")).resolve()
        entries = []
        for index, (name, payload) in enumerate(originals.items()):
            filename = f"backup-{index:03d}.bin"
            (backup / filename).write_bytes(payload)
            entries.append({"name": name, "backup": filename, "sha256": COMPARISON.digest(payload), **metadata[name]})
        (backup / "manifest.json").write_text(json.dumps(entries, indent=2), encoding="utf8")
        report["original_data_hashes"] = {name: COMPARISON.digest(payload) for name, payload in originals.items()}
        changed = True
        device("install", "-r", str(args.apk.resolve()))
        if installed()[1] != build["apk_sha256"]:
            raise RuntimeError("installed candidate APK mismatch")
        workflow = device("shell", "am", "instrument", "-w", PACKAGE + "/dev.jadix.nativeui.NotesChecks")
        (output / "workflow-instrumentation.log").write_text(workflow, encoding="utf8")
        if "JADIX_NOTES_CHECKS_PASS" not in workflow or "INSTRUMENTATION_CODE: -1" not in workflow or "failure=" in workflow:
            raise RuntimeError("original notebook workflow instrumentation failed")
        report["original_workflow_passed"] = True
        fixture = SCALE.notebook_fixture()
        fixture_bytes = SCALE.encoded(fixture).encode("utf8")
        (output / "fixture.json").write_bytes(fixture_bytes)
        private("mkdir", "-p", "cache")
        write("cache/" + scratch, fixture_bytes)
        instrument = device("shell", "am", "instrument", "-w", "-e", "fixture", scratch, PACKAGE + "/dev.jadix.nativeui.NotesScaleChecks")
        (output / "instrumentation.log").write_text(instrument, encoding="utf8")
        report["instrumentation_metrics"] = scale_metrics(instrument)
        device("shell", "am", "force-stop", PACKAGE)
        for name in [DOCUMENT + ".new", DOCUMENT + ".bak"]:
            private("rm", "-f", "files/" + name)
        write("files/" + DOCUMENT, fixture_bytes)
        device("shell", "am", "start", "-W", "-S", "-f", "0x10008000", "-n", COMPONENT)
        title = "process_title_" + uuid.uuid4().hex[:12]
        body = "process_body_" + uuid.uuid4().hex[:12]
        click("Title / 标题", True); device("shell", "input", "text", title)
        click("Write a note / 正文", True); device("shell", "input", "text", body)
        device("shell", "input", "keyevent", "KEYCODE_BACK")
        device("shell", "am", "start", "-W", "-n", COMPONENT)
        before_nodes = tree("process-before")
        if not all(any(field_value_matches(node, hint, value) for node in before_nodes) for hint, value in [("Title / 标题", title), ("Write a note / 正文", body)]):
            raise RuntimeError("system input did not create the expected draft")
        before_pid = device("shell", "pidof", PACKAGE).strip()
        if not before_pid.isdigit():
            raise RuntimeError("expected one foreground notebook process")
        device("shell", "input", "keyevent", "KEYCODE_HOME")
        deadline = time.monotonic() + 15
        while True:
            saved = files()
            matching = [name for name, payload in saved.items() if COMPARISON.CACHE_NAME.fullmatch(name) and recovery_matches(payload, fixture, title, body)]
            if matching or time.monotonic() >= deadline:
                break
            time.sleep(0.1)
        report["recovery_observation"] = [{"name": name, "bytes": len(payload), "matches_draft_and_archive": name in matching} for name, payload in saved.items() if COMPARISON.CACHE_NAME.fullmatch(name)]
        if len(matching) != 1 or private("cat", "files/" + DOCUMENT, binary=True) != fixture_bytes:
            raise RuntimeError("near-limit archive and unsaved Rust draft were not saved separately")
        report["recovery_cache"] = {"name": matching[0], "bytes": len(saved[matching[0]]), "sha256": COMPARISON.digest(saved[matching[0]])}
        deadline = time.monotonic() + 15
        while True:
            private("kill", "-9", before_pid)
            value = device("shell", "pidof", PACKAGE, allowed=(0, 1)).strip()
            if not value:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError("old background process remains alive; refusing a force-stop substitute")
            time.sleep(0.1)
        device("shell", "am", "start", "-W", "-n", COMPONENT)
        after_pid = device("shell", "pidof", PACKAGE).strip()
        if not after_pid.isdigit() or after_pid == before_pid:
            raise RuntimeError("saved-task launch did not create a new process")
        after_nodes = tree("process-after")
        if not all(any(field_value_matches(node, hint, value) for node in after_nodes) for hint, value in [("Title / 标题", title), ("Write a note / 正文", body)]):
            raise RuntimeError("new process did not restore native editor values")
        click("New note")
        guarded = tree("process-rust-dirty-guard")
        if not any(field_value_matches(node, "Write a note / 正文", body) for node in guarded):
            raise RuntimeError("restored draft did not reach Rust dirty-navigation guard")
        if private("cat", "files/" + DOCUMENT, binary=True) != fixture_bytes:
            raise RuntimeError("unsaved draft changed durable documents")
        (output / "process.png").write_bytes(device("exec-out", "screencap", "-p", binary=True))
        report["process_recovery"] = {"before_pid": before_pid, "after_pid": after_pid, "termination": "same-UID SIGKILL after Home; absence verified before saved-task restart", "rust_dirty_guard_verified": True, "fixture_bytes": len(fixture_bytes)}
        report["complete"] = True
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
    finally:
        if changed:
            try:
                device("shell", "am", "force-stop", PACKAGE)
                device("install", "-r", str(original_apk))
                if installed()[1] != report["original_apk_sha256"]:
                    raise RuntimeError("original package was not restored")
                report["original_apk_restored"] = True
            except Exception as error:
                report["apk_cleanup_error"] = f"{type(error).__name__}: {error}"
            try:
                device("shell", "am", "force-stop", PACKAGE)
                for name in files().keys() - originals.keys():
                    private("rm", "files/" + name)
                for name, payload in originals.items():
                    write("files/" + name, payload)
                    private("chmod", metadata[name]["mode"], "files/" + name)
                private("rm", "-f", "cache/" + scratch)
                restore_name = "jadix-notes-restore-" + uuid.uuid4().hex + ".json"
                restore_entries = [{"name": name, **metadata[name]} for name in originals]
                write("cache/" + restore_name, json.dumps(restore_entries, separators=(",", ":")).encode("utf8"))
                restore = device("shell", "am", "instrument", "-w", "-e", "restore_metadata", restore_name,
                                 PACKAGE + "/dev.jadix.nativeui.NotesScaleChecks")
                (output / "restore-instrumentation.log").write_text(restore, encoding="utf8")
                if "JADIX_NOTES_RESTORE_PASS" not in restore or "INSTRUMENTATION_CODE: -1" not in restore or "failure=" in restore:
                    raise RuntimeError("original file metadata instrumentation failed")
                private("rm", "-f", "cache/" + restore_name)
                if files() != originals:
                    raise RuntimeError("original private file bytes were not restored")
                for name in originals:
                    if int(private("stat", "-c", "%Y", "files/" + name).strip()) != metadata[name]["mtime"] or private("stat", "-c", "%a", "files/" + name).strip() != metadata[name]["mode"]:
                        raise RuntimeError("original file metadata was not restored")
                report["original_data_restored"] = True
            except Exception as error:
                report["data_cleanup_error"] = f"{type(error).__name__}: {error}"
        if backup is not None:
            if report.get("original_data_restored") and report.get("original_apk_restored"):
                try:
                    if backup.parent != Path(tempfile.gettempdir()).resolve() or not backup.name.startswith("jadix-notes-recovery-"):
                        raise RuntimeError("unexpected temporary backup directory")
                    for entry in entries:
                        (backup / entry["backup"]).unlink()
                    (backup / "manifest.json").unlink(); backup.rmdir()
                    report["temporary_backup_removed_after_verified_restore"] = True
                except Exception as error:
                    report["backup_cleanup_error"] = str(error)
            else:
                report["retained_private_backup"] = str(backup)
        report["passed"] = bool(report["complete"] and report.get("original_data_restored") and report.get("original_apk_restored") and not any(name.endswith("_error") for name in report))
        (output / "scale.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    if not report["passed"]:
        raise SystemExit("Android notebook capacity/process recovery failed; inspect scale.json")
    print("PASS Android near-limit notebook, actual process recovery and original package/data restoration")


if __name__ == "__main__":
    main()
