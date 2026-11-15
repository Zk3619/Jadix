import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import uuid


root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "scripts/acceptance"))
spec = importlib.util.spec_from_file_location("native_ios_runtime", root / "scripts/acceptance/native-ios-check.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def inventory():
    return {"runtimes": [{"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-18-0", "version": "18.0", "isAvailable": True}], "devicetypes": [{"identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-16", "minRuntimeVersionString": "18.0.0", "maxRuntimeVersionString": "65535.255.255"}]}


def receipt(token, application="controls"):
    if application == "popover":
        checks = ["mounted-native-popover-page", "programmatic-popover-open", "native-popover-anchor-geometry", "native-popover-input-to-rust",
                  "native-popover-preedit-publication", "native-popover-nested-context", "native-popover-controlled-reset-retained", "native-popover-scroll-anchor",
                  "native-popover-opening-order", "native-popover-edge-flip", "native-popover-dispose-stale-actions", "native-popover-fresh-root-restore"]
        return {"schema": 1, "token": token, "bundle_identifier": "dev.jadix.popover", "application": application, "passed": True, "native_errors": [], "checks": checks,
                "event_authority": "programmatic UIKit controls, text input, scroll and dismissal; not touch, hardware keys, physical IME or VoiceOver",
                "geometry": {"inside_viewport": True, "anchor_left_error": 0.0, "anchor_gap_error": 0.0}}
    checks = ["mounted-platform-controls", "native-edit-to-rust", "native-multiline-edit-to-rust", "native-textarea-scroll-isolated", "dispose-rejects-state", "fresh-root-restore", "stale-native-actions-isolated", "protected-document-save" if application == "notes" else "native-activate-to-rust"]
    if application == "controls":
        checks.append("native-password-entry")
        checks.append("native-switch-to-rust")
    checks.append("native-dynamic-type-traits")
    checks.append("native-keyboard-viewport")
    count = 6 if application == "controls" else 4
    dynamic_type = {"categories": ["large", "accessibilityExtraExtraExtraLarge", "large"], "standard_fonts": [17.0] * count, "accessibility_fonts": [35.0] * count, "restored_fonts": [17.0] * count, "measurement_heights": [20.0, 42.0, 20.0]}
    keyboard_viewport = {"authority": "programmatic UIKit notifications and additional safe area; not OS background or physical keyboard", "heights": [600.0, 400.0, 600.0, 600.0, 600.0, 572.0, 600.0], "covered_height": 200.0, "safe_area_reduction": 28.0, "focused_control_visible": True}
    if application == "selection":
        checks = ["mounted-native-select-menu", "programmatic-select-to-rust", "disabled-option-rejected", "disabled-select-rejected", "native-select-identity-retained", "controlled-select-reset", "invalid-select-restore-atomic", "native-select-dynamic-type", "select-dispose-and-stale-callbacks", "fresh-select-root-restore"]
        dynamic_type = {**dynamic_type, "standard_fonts": [17.0] * 2, "accessibility_fonts": [35.0] * 2, "restored_fonts": [17.0] * 2}
        return {"schema": 1, "token": token, "bundle_identifier": "dev.jadix.selection", "application": application, "passed": True, "native_errors": [], "checks": checks, "dynamic_type": dynamic_type, "controls": 2, "options": 3, "event_authority": "programmatic Select callback and UIKit menu inspection; not menu touch, keyboard or VoiceOver"}
    return {"schema": 1, "token": token, "bundle_identifier": "dev.jadix.notebook" if application == "notes" else "dev.jadix.nativeui", "application": application, "passed": True, "native_errors": [], "initial_archive": {"fixture": []}, "saved_archive": {"fixture": ["saved"]}, "checks": checks, "dynamic_type": dynamic_type, "keyboard_viewport": keyboard_viewport}


class NativeIOSRuntimeTests(unittest.TestCase):
    def build_fixture(self, directory, notes=False, selection=False, popover=False):
        name = "JadixPopover.app" if popover else "JadixSelect.app" if selection else "JadixNotes.app" if notes else "JadixNative.app"
        bundle = directory / name
        bundle.mkdir()
        identifier = "dev.jadix.popover" if popover else "dev.jadix.selection" if selection else "dev.jadix.notebook" if notes else "dev.jadix.nativeui"
        (bundle / "Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier": identifier, "CFBundleExecutable": "JadixNative", "CFBundleSupportedPlatforms": ["iPhoneSimulator"]}))
        (bundle / "JadixNative").write_bytes(b"controlled-fixture-not-an-Apple-executable")
        build = {"target": "aarch64-apple-ios-sim", "sdk": "iphonesimulator", "checks_enabled": True, "rust_profile": "release", "application": "popover" if popover else "selection" if selection else "notes" if notes else "controls", "features": "uikit,popover" if popover else "uikit,selection" if selection else "uikit,notes" if notes else "uikit,extended", "bundle_identifier": identifier,
                 "files": [{"path": path.relative_to(directory).as_posix(), "bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in bundle.iterdir()]}
        (directory / "build.json").write_text(json.dumps(build), encoding="utf8")
        return bundle, build

    def test_bundle_validation_checks_hashes_inventory_paths_and_probe_mode(self):
        for mutation in ("none", "tamper", "extra", "escape", "duplicate", "missing", "unchecked", "wrong-id", "device", "empty-path"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary).resolve()
                bundle, build = self.build_fixture(directory)
                if mutation == "tamper":
                    (bundle / "JadixNative").write_bytes(b"tampered")
                elif mutation == "extra":
                    (bundle / "unlisted").write_bytes(b"unlisted")
                elif mutation == "escape":
                    build["files"][0]["path"] = "../outside"
                elif mutation == "duplicate":
                    build["files"].append(build["files"][0])
                elif mutation == "missing":
                    build["files"] = []
                elif mutation == "unchecked":
                    build["checks_enabled"] = False
                elif mutation == "wrong-id":
                    build["bundle_identifier"] = "unrelated.user.application"
                elif mutation == "device":
                    build["target"] = "aarch64-apple-ios"
                elif mutation == "empty-path":
                    build["files"][0]["path"] = ""
                (directory / "build.json").write_text(json.dumps(build), encoding="utf8")
                if mutation == "none":
                    self.assertEqual(runner.artifact(directory)[0], bundle)
                else:
                    with self.assertRaises(ValueError):
                        runner.artifact(directory)

    def test_runtime_selection_ignores_unavailable_and_incompatible_devices(self):
        data = inventory()
        data["runtimes"].append({"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-99-0", "version": "99", "isAvailable": False})
        data["devicetypes"].insert(0, {"identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-17", "minRuntimeVersionString": "26.0"})
        self.assertEqual(runner.simulator_pair(data), (data["devicetypes"][1]["identifier"], data["runtimes"][0]["identifier"]))
        data["runtimes"][0]["isAvailable"] = False
        with self.assertRaises(ValueError):
            runner.simulator_pair(data)
        for invalid in ("18.beta", "", "18.1.0.2", None):
            with self.assertRaises(ValueError):
                runner.version(invalid)

    def test_receipts_reject_stale_failed_incomplete_or_duplicated_checks(self):
        token = str(uuid.uuid4())
        for application in ("controls", "notes", "selection", "popover"):
            build = {"application": application, "bundle_identifier": receipt(token, application)["bundle_identifier"]}
            runner.validate_receipt(receipt(token, application), token, build)
            for mutation in ("token", "passed", "native_errors", "checks", "duplicate", "archive", "application", "schema"):
                with self.subTest(application=application, mutation=mutation):
                    value = receipt(token, application)
                    if mutation == "duplicate":
                        value["checks"].append(value["checks"][0])
                    elif mutation == "archive":
                        if application != "notes":
                            continue
                        del value["saved_archive"]
                    else:
                        value[mutation] = [] if mutation in ("checks", "native_errors") else False
                        if mutation == "native_errors":
                            value[mutation] = ["failure"]
                    with self.assertRaises(ValueError):
                        runner.validate_receipt(value, token, build)

    def test_selection_rejects_wrong_probe_authority_inventory_and_features(self):
        token = str(uuid.uuid4())
        build = {"application": "selection", "bundle_identifier": "dev.jadix.selection"}
        for field, invalid in (("event_authority", "native menu touch"), ("controls", 3), ("controls", True), ("options", 2), ("options", True), ("dynamic_type", {})):
            with self.subTest(field=field, invalid=invalid):
                value = receipt(token, "selection")
                value[field] = invalid
                with self.assertRaises(ValueError):
                    runner.validate_receipt(value, token, build)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary).resolve()
            bundle, report = self.build_fixture(directory, selection=True)
            self.assertEqual(runner.artifact(directory)[0], bundle)
            for features in ("uikit,extended", "uikit,selection,gpu", None):
                report["features"] = features
                (directory / "build.json").write_text(json.dumps(report), encoding="utf8")
                with self.assertRaises(ValueError):
                    runner.artifact(directory)

    def test_popover_receipt_requires_exact_authority_scenarios_and_finite_anchor_geometry(self):
        token = str(uuid.uuid4())
        build = {"application": "popover", "bundle_identifier": "dev.jadix.popover"}
        runner.validate_receipt(receipt(token, "popover"), token, build)
        for mutation in ("authority", "viewport", "missing", "duplicate", "gap", "nan", "infinity", "boolean", "negative", "schema-bool"):
            with self.subTest(mutation=mutation):
                value = receipt(token, "popover")
                if mutation == "authority":
                    value["event_authority"] = "physical input"
                elif mutation == "viewport":
                    value["geometry"]["inside_viewport"] = False
                elif mutation == "missing":
                    value["checks"].pop()
                elif mutation == "duplicate":
                    value["checks"].append(value["checks"][0])
                elif mutation == "schema-bool":
                    value["schema"] = True
                else:
                    value["geometry"]["anchor_gap_error"] = {"gap": 1.0, "nan": float("nan"), "infinity": float("inf"), "boolean": False, "negative": -1}[mutation]
                with self.assertRaises(ValueError):
                    runner.validate_receipt(value, token, build)

    def test_popover_bundle_requires_its_exact_features_identity_and_file_inventory(self):
        for mutation in ("none", "features", "identifier", "tamper", "extra"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary).resolve()
                bundle, build = self.build_fixture(directory, popover=True)
                if mutation == "features":
                    build["features"] = "uikit,extended,popover"
                elif mutation == "identifier":
                    build["bundle_identifier"] = "dev.jadix.nativeui"
                elif mutation == "tamper":
                    (bundle / "JadixNative").write_bytes(b"tampered")
                elif mutation == "extra":
                    (bundle / "unreported").write_bytes(b"unreported")
                (directory / "build.json").write_text(json.dumps(build), encoding="utf8")
                if mutation == "none":
                    self.assertEqual(runner.artifact(directory)[0], bundle)
                else:
                    with self.assertRaises(ValueError):
                        runner.artifact(directory)
    def test_controls_receipt_requires_switch_and_both_boolean_fonts(self):
        token = str(uuid.uuid4())
        build = {"application": "controls", "bundle_identifier": receipt(token)["bundle_identifier"]}
        missing = receipt(token)
        missing["checks"].remove("native-switch-to-rust")
        with self.assertRaises(ValueError):
            runner.validate_receipt(missing, token, build)
        old = receipt(token)
        for name in ("standard_fonts", "accessibility_fonts", "restored_fonts"):
            old["dynamic_type"][name].pop()
        with self.assertRaises(ValueError):
            runner.validate_receipt(old, token, build)

    def test_frozen_and_non_apple_outputs_fail_before_commands_or_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "archive"
            archive.mkdir()
            (archive / "snapshot.json").write_text("{}")
            for output in (archive, archive / "nested", Path(temporary) / "absent"):
                with patch.object(runner.sys, "argv", ["native-ios-check.py", "--build", "missing", "--output", str(output)]), patch.object(runner.sys, "platform", "win32"), patch.object(runner.subprocess, "run") as run, contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit):
                        runner.main()
                    run.assert_not_called()
                    self.assertFalse((output / "checks.json").exists())
                    if output != archive:
                        self.assertFalse(output.exists())

    def test_receipts_require_multiline_edit_and_scroll_isolation(self):
        token = str(uuid.uuid4())
        for application in ("controls", "notes"):
            build = {"application": application, "bundle_identifier": receipt(token, application)["bundle_identifier"]}
            for name in ("native-multiline-edit-to-rust", "native-textarea-scroll-isolated"):
                with self.subTest(application=application, missing=name):
                    value = receipt(token, application)
                    value["checks"].remove(name)
                    with self.assertRaises(ValueError):
                        runner.validate_receipt(value, token, build)

    def test_receipts_require_dynamic_type_for_both_applications(self):
        token = str(uuid.uuid4())
        for application in ("controls", "notes"):
            with self.subTest(application=application):
                value = receipt(token, application)
                value["checks"].remove("native-dynamic-type-traits")
                build = {"application": application, "bundle_identifier": value["bundle_identifier"]}
                with self.assertRaises(ValueError):
                    runner.validate_receipt(value, token, build)

    def test_receipts_require_keyboard_viewport_for_both_applications(self):
        token = str(uuid.uuid4())
        for application in ("controls", "notes"):
            with self.subTest(application=application):
                value = receipt(token, application)
                value["checks"].remove("native-keyboard-viewport")
                build = {"application": application, "bundle_identifier": value["bundle_identifier"]}
                with self.assertRaises(ValueError):
                    runner.validate_receipt(value, token, build)

    def test_dynamic_type_measurements_reject_missing_unscaled_or_invalid_values(self):
        token = str(uuid.uuid4())
        for application in ("controls", "notes", "selection"):
            for mutation in ("missing", "categories", "count", "infinity", "nan", "boolean", "negative", "huge-integer", "not-scaled", "accumulated", "measurement", "measurement-restored"):
                with self.subTest(application=application, mutation=mutation):
                    value = receipt(token, application)
                    metrics = value["dynamic_type"]
                    if mutation == "missing":
                        del value["dynamic_type"]
                    elif mutation == "categories":
                        metrics["categories"] = ["large"] * 3
                    elif mutation == "count":
                        metrics["standard_fonts"].pop()
                    elif mutation in ("infinity", "nan", "boolean", "negative"):
                        metrics["standard_fonts"][0] = {"infinity": float("inf"), "nan": float("nan"), "boolean": True, "negative": -1}[mutation]
                    elif mutation == "huge-integer":
                        metrics["standard_fonts"][0] = 10 ** 1000
                    elif mutation == "not-scaled":
                        metrics["accessibility_fonts"][0] = metrics["standard_fonts"][0]
                    elif mutation == "accumulated":
                        metrics["restored_fonts"][0] += 1
                    elif mutation == "measurement":
                        metrics["measurement_heights"][1] = metrics["measurement_heights"][0]
                    else:
                        metrics["measurement_heights"][2] += 1
                    build = {"application": application, "bundle_identifier": value["bundle_identifier"]}
                    with self.assertRaises(ValueError):
                        runner.validate_receipt(value, token, build)

    def test_keyboard_viewport_rejects_missing_invalid_or_stale_geometry(self):
        token = str(uuid.uuid4())
        for application in ("controls", "notes"):
            for mutation in ("missing", "authority", "focus", "count", "nan", "infinity", "boolean", "negative", "covered", "safe-area", "docked", "hidden", "background", "active", "safe", "restored"):
                with self.subTest(application=application, mutation=mutation):
                    value = receipt(token, application)
                    metrics = value["keyboard_viewport"]
                    if mutation == "missing":
                        del value["keyboard_viewport"]
                    elif mutation == "authority":
                        metrics["authority"] = "OS background"
                    elif mutation == "focus":
                        metrics["focused_control_visible"] = False
                    elif mutation == "count":
                        metrics["heights"].pop()
                    elif mutation in ("nan", "infinity", "boolean", "negative"):
                        metrics["heights"][0] = {"nan": float("nan"), "infinity": float("inf"), "boolean": True, "negative": -1}[mutation]
                    elif mutation == "covered":
                        metrics["covered_height"] = 0
                    elif mutation == "safe-area":
                        metrics["safe_area_reduction"] = 0
                    else:
                        metrics["heights"][{"docked": 1, "hidden": 2, "background": 3, "active": 4, "safe": 5, "restored": 6}[mutation]] -= 10
                    build = {"application": application, "bundle_identifier": value["bundle_identifier"]}
                    with self.assertRaises(ValueError):
                        runner.validate_receipt(value, token, build)

    def simulated_run(self, application="controls", failure=None):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            output = directory / "output"
            output.mkdir()
            container = directory / "container"
            destination = container / "Library/Application Support/native-checks.json"
            destination.parent.mkdir(parents=True)
            owned = str(uuid.uuid4())
            calls = []
            launches = 0
            build = {"application": application, "bundle_identifier": receipt("", application)["bundle_identifier"]}

            def run(command, **options):
                nonlocal launches
                arguments = command[2:]
                calls.append(arguments)
                action = arguments[0]
                stdout = ""
                if action == "list":
                    stdout = json.dumps(inventory())
                elif action == "create":
                    stdout = "not-a-uuid" if failure == "invalid-create" else owned
                elif action == "get_app_container":
                    stdout = str(container.resolve())
                elif action == "launch":
                    launches += 1
                    token = options["env"]["SIMCTL_CHILD_JADIX_NATIVE_CHECK_TOKEN"]
                    value = receipt("stale" if failure == "stale" else token, application)
                    if launches == 2:
                        value["initial_archive"] = {"fixture": ["saved"]}
                    if failure == "archive-mismatch" and launches == 2:
                        value["initial_archive"] = {"fixture": ["lost"]}
                    if failure == "probe":
                        value["passed"] = False
                    if failure != "missing":
                        destination.write_text("{malformed" if failure == "malformed" else json.dumps(value), encoding="utf8")
                elif action == "io":
                    Path(arguments[-1]).write_bytes(b"invalid" if failure == "screenshot" else b"\x89PNG\r\n\x1a\ncontrolled-screenshot-fixture")
                elif action == "bootstatus" and failure == "timeout":
                    raise subprocess.TimeoutExpired(command, options["timeout"])
                elif action == failure:
                    return subprocess.CompletedProcess(command, 1, "", "controlled failure")
                return subprocess.CompletedProcess(command, 0, stdout, "")

            with patch.object(runner.subprocess, "run", side_effect=run), patch.object(runner.time, "monotonic", side_effect=range(0, 10000, 1000)), patch.object(runner.time, "sleep"):
                result = runner.execute(Path("fixture.app"), build, output, 30)
            self.assertEqual(json.loads((output / "checks.json").read_text(encoding="utf8")), result)
            cleanup = [call for call in calls if call[0] in ("shutdown", "delete")]
            if failure == "invalid-create":
                self.assertEqual(cleanup, [])
                self.assertIn("uncertain_create", result["cleanup"])
            else:
                self.assertEqual(cleanup, [["shutdown", owned], ["delete", owned]])
            if failure is None:
                self.assertTrue(result["passed"], result.get("error"))
                self.assertEqual(launches, 2)
                self.assertEqual(sum(call[0] == "terminate" for call in calls), 2)
                self.assertNotEqual(result["receipts"][0]["token"], result["receipts"][1]["token"])
            else:
                self.assertFalse(result["passed"])
            if failure == "probe":
                self.assertTrue((output / "initial-receipt.json").is_file())

    def test_controlled_lifecycle_and_notebook_process_restart(self):
        for application in ("controls", "notes", "selection", "popover"):
            self.simulated_run(application)

    def test_failures_never_pass_and_cleanup_targets_only_owned_uuid(self):
        for failure in ("boot", "install", "probe", "stale", "missing", "malformed", "screenshot", "timeout", "invalid-create", "shutdown", "delete", "archive-mismatch"):
            with self.subTest(failure=failure):
                self.simulated_run("notes", failure)


if __name__ == "__main__":
    unittest.main()
