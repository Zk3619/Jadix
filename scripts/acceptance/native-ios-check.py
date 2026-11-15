"""Run opt-in UIKit probes on a fresh, owned simulator; never reuse a user's device."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import plistlib
import re
import subprocess
import sys
import time
import uuid

from native_browser import validate_output


def artifact(directory):
    directory = directory.resolve()
    source = (directory / "build.json").read_bytes()
    build = json.loads(source)
    application = build.get("application")
    if application not in ("controls", "notes", "selection", "popover"):
        raise ValueError("unknown application")
    if application == "selection" and build.get("features") != "uikit,selection":
        raise ValueError("selection requires the isolated uikit,selection feature set")
    if application == "popover" and build.get("features") != "uikit,popover":
        raise ValueError("popover requires the isolated uikit,popover feature set")
    if build.get("target") != "aarch64-apple-ios-sim" or build.get("sdk") != "iphonesimulator" or build.get("checks_enabled") is not True or build.get("rust_profile") != "release":
        raise ValueError("requires a release simulator bundle built with --checks")
    name = {"controls": "JadixNative.app", "notes": "JadixNotes.app", "selection": "JadixSelect.app", "popover": "JadixPopover.app"}[application]
    bundle = directory / name
    expected_identifier = {"controls": "dev.jadix.nativeui", "notes": "dev.jadix.notebook", "selection": "dev.jadix.selection", "popover": "dev.jadix.popover"}[application]
    if build.get("bundle_identifier") != expected_identifier:
        raise ValueError("unexpected bundle identifier")
    seen = set()
    for item in build["files"]:
        relative = PurePosixPath(item["path"])
        path = directory.joinpath(*relative.parts)
        if not relative.parts or relative.is_absolute() or ".." in relative.parts or "\\" in item["path"] or relative.parts[0] != name or path.is_symlink() or path.resolve() != path.absolute() or path in seen:
            raise ValueError("unsafe or duplicate bundle member")
        seen.add(path)
        data = path.read_bytes()
        if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise ValueError("bundle does not match its build report")
    actual = {path for path in bundle.rglob("*") if path.is_file()}
    if seen != actual or not {bundle / "Info.plist", bundle / "JadixNative"} <= seen or any(path.is_symlink() for path in bundle.rglob("*")):
        raise ValueError("bundle inventory is incomplete or contains symlinks")
    with (bundle / "Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info.get("CFBundleIdentifier") != expected_identifier or info.get("CFBundleExecutable") != "JadixNative" or info.get("CFBundleSupportedPlatforms") != ["iPhoneSimulator"]:
        raise ValueError("bundle metadata disagrees with build report")
    return bundle, {**build, "build_report_sha256": hashlib.sha256(source).hexdigest()}


def version(text):
    if not isinstance(text, str) or not re.fullmatch(r"\d+(?:\.\d+){0,2}", text):
        raise ValueError("invalid simulator runtime version")
    return tuple(int(part) for part in text.split(".")) + (0,) * (3 - len(text.split(".")))


def simulator_pair(inventory):
    runtimes = [runtime for runtime in inventory["runtimes"] if runtime.get("isAvailable") is True and runtime["identifier"].startswith("com.apple.CoreSimulator.SimRuntime.iOS-") and version(runtime["version"]) >= (14, 0, 0)]
    for runtime in sorted(runtimes, key=lambda item: version(item["version"]), reverse=True):
        devices = [device for device in inventory["devicetypes"] if device["identifier"].startswith("com.apple.CoreSimulator.SimDeviceType.iPhone-") and version(device.get("minRuntimeVersionString", "0")) <= version(runtime["version"]) <= version(device.get("maxRuntimeVersionString", "65535.255.255"))]
        if devices:
            return sorted(devices, key=lambda item: item["identifier"])[0]["identifier"], runtime["identifier"]
    raise ValueError("no available iOS runtime and compatible iPhone device type")


def validate_receipt(receipt, token, build):
    required = {"mounted-platform-controls", "native-edit-to-rust", "native-multiline-edit-to-rust", "native-textarea-scroll-isolated", "dispose-rejects-state", "fresh-root-restore", "stale-native-actions-isolated", "protected-document-save" if build["application"] == "notes" else "native-activate-to-rust"}
    required.add("native-dynamic-type-traits")
    required.add("native-keyboard-viewport")
    if build["application"] == "controls":
        required.add("native-password-entry")
        required.add("native-switch-to-rust")
    if build["application"] == "selection":
        required = {"mounted-native-select-menu", "programmatic-select-to-rust", "disabled-option-rejected", "disabled-select-rejected", "native-select-identity-retained", "controlled-select-reset", "invalid-select-restore-atomic", "native-select-dynamic-type", "select-dispose-and-stale-callbacks", "fresh-select-root-restore"}
        if receipt.get("event_authority") != "programmatic Select callback and UIKit menu inspection; not menu touch, keyboard or VoiceOver" or type(receipt.get("controls")) is not int or receipt["controls"] != 2 or type(receipt.get("options")) is not int or receipt["options"] != 3:
            raise ValueError("missing Select probe authority or control inventory")
    if build["application"] == "popover":
        required = {"mounted-native-popover-page", "programmatic-popover-open", "native-popover-anchor-geometry",
                    "native-popover-input-to-rust", "native-popover-preedit-publication", "native-popover-nested-context",
                    "native-popover-controlled-reset-retained", "native-popover-scroll-anchor", "native-popover-opening-order",
                    "native-popover-edge-flip", "native-popover-dispose-stale-actions", "native-popover-fresh-root-restore"}
    if receipt.get("token") != token or type(receipt.get("schema")) is not int or receipt.get("schema") != 1 or receipt.get("bundle_identifier") != build["bundle_identifier"] or receipt.get("application") != build["application"]:
        raise ValueError("stale or mismatched UIKit receipt")
    if receipt.get("passed") is not True or receipt.get("native_errors") != [] or set(receipt.get("checks", [])) != required or len(receipt["checks"]) != len(required):
        raise ValueError("UIKit probes failed or are incomplete: " + str(receipt.get("error", "missing checks")))
    if build["application"] == "popover":
        if receipt.get("event_authority") != "programmatic UIKit controls, text input, scroll and dismissal; not touch, hardware keys, physical IME or VoiceOver":
            raise ValueError("missing Popover probe authority")
        geometry = receipt.get("geometry")
        if not isinstance(geometry, dict) or geometry.get("inside_viewport") is not True:
            raise ValueError("missing Popover viewport proof")
        for field in ("anchor_left_error", "anchor_gap_error"):
            value = geometry.get(field)
            if type(value) not in (int, float) or not 0 <= value < 1:
                raise ValueError("invalid Popover anchor geometry")
        return
    metrics = receipt.get("dynamic_type")
    if not isinstance(metrics, dict) or metrics.get("categories") != ["large", "accessibilityExtraExtraExtraLarge", "large"]:
        raise ValueError("missing Dynamic Type trait phases")
    count = {"controls": 6, "notes": 4, "selection": 2}[build["application"]]
    arrays = [metrics.get(name) for name in ("standard_fonts", "accessibility_fonts", "restored_fonts", "measurement_heights")]
    for values, size in zip(arrays, (count, count, count, 3)):
        if not isinstance(values, list) or len(values) != size or any(type(value) not in (int, float) or not 0 < value <= sys.float_info.max for value in values):
            raise ValueError("invalid Dynamic Type measurements")
    standard, accessibility, restored, heights = arrays
    if any(large <= base or abs(final - base) >= 0.1 for base, large, final in zip(standard, accessibility, restored)) or heights[1] <= heights[0] or abs(heights[2] - heights[0]) >= 0.1:
        raise ValueError("Dynamic Type did not scale and restore")
    if build["application"] == "selection":
        return
    viewport = receipt.get("keyboard_viewport")
    if not isinstance(viewport, dict) or viewport.get("authority") != "programmatic UIKit notifications and additional safe area; not OS background or physical keyboard" or viewport.get("focused_control_visible") is not True:
        raise ValueError("missing keyboard viewport probe authority or visible focus")
    heights = viewport.get("heights")
    covered = viewport.get("covered_height")
    safe = viewport.get("safe_area_reduction")
    if not isinstance(heights, list) or len(heights) != 7 or any(type(value) not in (int, float) or not 0 < value <= sys.float_info.max for value in [*heights, covered, safe]):
        raise ValueError("invalid keyboard viewport measurements")
    baseline = heights[0]
    if baseline <= 120 or abs(covered - min(200, baseline / 3)) >= 0.5 or safe != 28 or abs(heights[1] - baseline + covered) >= 0.5 or abs(heights[5] - baseline + safe) >= 0.5 or any(abs(heights[index] - baseline) >= 0.5 for index in (2, 3, 4, 6)):
        raise ValueError("keyboard viewport did not reduce and restore")
    if build["application"] == "notes":
        for name in ("initial_archive", "saved_archive"):
            if not isinstance(receipt.get(name), dict):
                raise ValueError("missing notebook archive in UIKit receipt")


def execute(bundle, build, output, timeout):
    report = {"passed": False, "scope": "Owned simulator, programmatic UIKit events, root disposal and notebook process restart; not physical IME, VoiceOver, minimum iOS or distribution acceptance", "build": build, "commands": [], "receipts": [], "cleanup": {}, "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    owned = None

    def command(arguments, *, environment=None, seconds=120):
        invocation = ["xcrun", "simctl", *arguments]
        entry = {"command": invocation}
        report["commands"].append(entry)
        try:
            result = subprocess.run(invocation, capture_output=True, text=True, encoding="utf8", errors="replace", timeout=seconds, env=environment)
            entry.update(exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr)
            if result.returncode:
                raise RuntimeError(f"simctl {arguments[0]} failed: {result.stderr.strip()}")
            return result.stdout.strip()
        except Exception as error:
            entry["error"] = str(error)
            raise

    try:
        report["simctl_help"] = command(["help"])
        inventory = json.loads(command(["list", "--json"]))
        device, runtime = simulator_pair(inventory)
        report.update(device_type=device, runtime=runtime)
        created = command(["create", "Jadix acceptance " + str(uuid.uuid4()), device, runtime])
        try:
            owned = str(uuid.UUID(created))
        except ValueError as error:
            report["cleanup"]["uncertain_create"] = created
            raise ValueError("simctl create returned no valid owned UUID; refusing arbitrary cleanup") from error
        report["owned_simulator"] = owned
        command(["boot", owned])
        command(["bootstatus", owned, "-b"], seconds=timeout)
        command(["install", owned, str(bundle)])
        container = Path(command(["get_app_container", owned, build["bundle_identifier"], "data"]))
        if not container.is_absolute() or not container.is_dir():
            raise ValueError("simctl did not return an existing absolute app container")
        receipt_path = container / "Library/Application Support/native-checks.json"
        for phase in ("initial", "relaunch"):
            token = str(uuid.uuid4())
            environment = {**os.environ, "SIMCTL_CHILD_JADIX_NATIVE_CHECK_TOKEN": token}
            command(["launch", owned, build["bundle_identifier"]], environment=environment)
            deadline = time.monotonic() + timeout
            while True:
                receipt = None
                if receipt_path.is_file():
                    receipt = json.loads(receipt_path.read_text(encoding="utf8"))
                if isinstance(receipt, dict) and receipt.get("token") == token:
                    (output / f"{phase}-receipt.json").write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf8")
                    validate_receipt(receipt, token, build)
                    report["receipts"].append(receipt)
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"no fresh UIKit receipt for {phase}")
                time.sleep(0.25)
            command(["io", owned, "screenshot", str(output / f"{phase}.png")])
            if not (output / f"{phase}.png").read_bytes().startswith(b"\x89PNG\r\n\x1a\n"):
                raise ValueError("simulator screenshot is not PNG")
            command(["terminate", owned, build["bundle_identifier"]])
        if build["application"] == "notes" and report["receipts"][0]["saved_archive"] != report["receipts"][1]["initial_archive"]:
            raise ValueError("notebook archive did not survive process termination and relaunch")
        report["passed"] = True
    except Exception as error:
        report["error"] = str(error)
    finally:
        if owned is not None:
            for operation in ("shutdown", "delete"):
                try:
                    command([operation, owned])
                    report["cleanup"][operation] = True
                except Exception as error:
                    report["cleanup"][operation] = str(error)
                    report["passed"] = False
        (output / "checks.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf8")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=180)
    args = parser.parse_args()
    output = args.output.resolve()
    try:
        validate_output(output)
        if not 30 <= args.timeout <= 600:
            raise ValueError("--timeout must be between 30 and 600 seconds")
        if sys.platform != "darwin":
            raise ValueError("UIKit runtime checks require macOS and Xcode")
        bundle, build = artifact(args.build)
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
    output.mkdir(parents=True, exist_ok=True)
    report = execute(bundle, build, output, args.timeout)
    print(f"{'PASS' if report['passed'] else 'FAIL'}: UIKit simulator probes; {output / 'checks.json'}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
