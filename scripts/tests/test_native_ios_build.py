import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import sys
import tempfile
import unittest
from unittest.mock import patch


root = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("native_ios_build", root / "examples/native-controls/build-ios.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
sys.path.insert(0, str(root / "scripts/acceptance"))
check_spec = importlib.util.spec_from_file_location("native_ios_check", root / "scripts/acceptance/native-ios-check.py")
ios_check = importlib.util.module_from_spec(check_spec)
check_spec.loader.exec_module(ios_check)


class NativeIOSBuildTests(unittest.TestCase):
    def test_frozen_exact_and_nested_outputs_are_rejected(self):
        with tempfile.TemporaryDirectory(prefix="jadix-ios-test-") as temporary:
            archive = Path(temporary) / "archive"
            archive.mkdir()
            (archive / "snapshot.json").write_text("{}", encoding="utf8")
            for output in [archive, archive / "nested"]:
                with self.assertRaisesRegex(ValueError, "archived snapshot"):
                    builder.validate_output(output)
            self.assertFalse((archive / "nested").exists())

    def test_existing_control_or_notes_bundle_is_not_reused(self):
        for name in ["JadixNative.app", "JadixNotes.app", "JadixSelect.app"]:
            with tempfile.TemporaryDirectory(prefix="jadix-ios-test-") as temporary:
                output = Path(temporary)
                (output / name).mkdir()
                sentinel = output / name / "preserved"
                sentinel.write_bytes(b"unchanged")
                with self.assertRaisesRegex(ValueError, "already exists"):
                    builder.validate_output(output)
                self.assertEqual(sentinel.read_bytes(), b"unchanged")

    def test_non_apple_host_and_missing_profile_fail_before_output_or_commands(self):
        with tempfile.TemporaryDirectory(prefix="jadix-ios-test-") as temporary:
            output = Path(temporary) / "must-not-create"
            for extra in [[], ["--provisioning-profile", str(Path(temporary) / "absent.mobileprovision")]]:
                with patch.object(builder.sys, "argv", ["build-ios.py", "--output", str(output), *extra]), patch.object(builder.sys, "platform", "win32"), patch.object(builder.subprocess, "run") as run, patch.object(builder.subprocess, "check_output") as capture, contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as failure:
                        builder.main()
                    self.assertEqual(failure.exception.code, 2)
                    run.assert_not_called()
                    capture.assert_not_called()
                    self.assertFalse(output.exists())

    def test_swift_commands_keep_target_header_and_notes_feature_in_sync(self):
        for target, (sdk, triple) in builder.TARGETS.items():
            for notes in [False, True]:
                command = builder.swift_command(root, Path("app"), Path("external-target/lib.a"), target, "/fixture/sdk", notes)
                self.assertEqual(command[:3], ["xcrun", "--sdk", sdk])
                self.assertIn(triple, command)
                self.assertIn(str(root / "crates/jadix_native/platforms/ios/JadixNative.h"), command)
                self.assertEqual("JADIX_NOTES" in command, notes)
                self.assertEqual(str(root / "examples/native-controls/ios/NativeNotes.swift") in command, notes)
                self.assertIn("external-target/lib.a", [item.replace(chr(92), "/") for item in command])

    def test_ios_targets_match_the_supported_deployment_baseline(self):
        self.assertEqual(builder.TARGETS["aarch64-apple-ios-sim"][1], "arm64-apple-ios15.0-simulator")
        self.assertEqual(builder.TARGETS["aarch64-apple-ios"][1], "arm64-apple-ios15.0")
        with (root / "examples/native-controls/ios/Info.plist").open("rb") as stream:
            self.assertEqual(plistlib.load(stream)["MinimumOSVersion"], "15.0")

    def test_simulator_environment_forwards_only_declared_safe_variables(self):
        forwarded = ios_check.simulator_child_environment(
            ["APP_MODE", "APP_MODE", "HOME", "TMPDIR", "PATH", "CFFIXED_USER_HOME", "DYLD_INSERT_LIBRARIES", "SIMULATOR_DEVICE_NAME", "MISSING", "JADIX_NATIVE_CHECK_TOKEN"],
            {"APP_MODE": "value with spaces=kept", "HOME": "/host", "DYLD_INSERT_LIBRARIES": "/host/inject", "SIMULATOR_DEVICE_NAME": "Host Simulator"},
            "probe-token",
        )
        self.assertEqual(forwarded, {
            "SIMCTL_CHILD_JADIX_NATIVE_CHECK_TOKEN": "probe-token",
            "SIMCTL_CHILD_APP_MODE": "value with spaces=kept",
        })
        with self.assertRaisesRegex(ValueError, "invalid declared"):
            ios_check.simulator_child_environment(["APP-MODE"], {"APP-MODE": "value"}, "probe-token")
        command_environment = ios_check.simctl_environment(
            ["APP_MODE"],
            {"PATH": "host-path", "SIMCTL_CHILD_UNDECLARED": "inherited", "APP_MODE": "declared"},
            "probe-token",
        )
        self.assertEqual(command_environment, {
            "PATH": "host-path",
            "APP_MODE": "declared",
            "SIMCTL_CHILD_JADIX_NATIVE_CHECK_TOKEN": "probe-token",
            "SIMCTL_CHILD_APP_MODE": "declared",
        })

    def test_simulator_selection_skips_ios_14_runtime(self):
        inventory = {
            "runtimes": [
                {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-14-5", "version": "14.5", "isAvailable": True},
                {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-15-0", "version": "15.0", "isAvailable": True},
            ],
            "devicetypes": [
                {"identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-13", "minRuntimeVersionString": "15.0", "maxRuntimeVersionString": "99.0"},
            ],
        }
        self.assertEqual(ios_check.simulator_pair(inventory), (
            "com.apple.CoreSimulator.SimDeviceType.iPhone-13",
            "com.apple.CoreSimulator.SimRuntime.iOS-15-0",
        ))

    def test_checks_are_opt_in_and_device_checks_fail_before_commands(self):
        for enabled in [False, True]:
            command = builder.swift_command(root, Path("app"), Path("lib.a"), "aarch64-apple-ios-sim", "/sdk", False, enabled)
            self.assertEqual("JADIX_NATIVE_CHECKS" in command, enabled)
            self.assertEqual(str(root / "examples/native-controls/ios/NativeChecks.swift") in command, enabled)
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "absent"
            with patch.object(builder.sys, "argv", ["build-ios.py", "--checks", "--target", "aarch64-apple-ios", "--output", str(output)]), patch.object(builder.sys, "platform", "darwin"), patch.object(builder.subprocess, "run") as run, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    builder.main()
                run.assert_not_called()
                self.assertFalse(output.exists())

    def simulated_build(self, target, notes, sign=False, profile=False, checks=False, selection=False, popover=False):
        with tempfile.TemporaryDirectory(prefix="jadix-ios-test-") as temporary:
            directory = Path(temporary)
            output = directory / "output"
            target_directory = directory / "external-target"
            library = target_directory / target / "release/libjadix_native_controls.a"
            library.parent.mkdir(parents=True)
            library.write_bytes(b"simulated-library-not-Apple-compiled")
            args = ["build-ios.py", "--target", target, "--output", str(output)]
            if notes:
                args.append("--notes")
            if selection:
                args.append("--selection")
            if popover:
                args.append("--popover")
            if checks:
                args.append("--checks")
            provisioning = directory / "fixture.mobileprovision"
            if profile:
                provisioning.write_bytes(b"simulated-profile-not-valid-for-installation")
                args += ["--provisioning-profile", str(provisioning)]
            commands = []

            def capture(command, **_):
                if command[:2] == ["cargo", "metadata"]:
                    return json.dumps({"target_directory": str(target_directory)})
                if "--show-sdk-path" in command:
                    return "/fixture/sdk"
                if "--show-sdk-version" in command:
                    return "fixture-sdk"
                return "fixture-swift"

            def run(command, **_):
                commands.append(command)
                if "swiftc" in command:
                    Path(command[-1]).write_bytes(b"simulated-executable-not-Apple-compiled")

            with patch.object(builder.sys, "argv", args), patch.object(builder.sys, "platform", "darwin"), patch.dict(builder.os.environ, {"CODE_SIGN_IDENTITY": "Fixture Identity" if sign else ""}), patch.object(builder.subprocess, "check_output", side_effect=capture), patch.object(builder.subprocess, "run", side_effect=run), contextlib.redirect_stdout(io.StringIO()):
                builder.main()
            report = json.loads((output / "build.json").read_text())
            app = output / ("JadixPopover.app" if popover else "JadixSelect.app" if selection else "JadixNotes.app" if notes else "JadixNative.app")
            with (app / "Info.plist").open("rb") as stream:
                info = plistlib.load(stream)
            self.assertEqual(info["CFBundleIdentifier"], "dev.jadix.popover" if popover else "dev.jadix.selection" if selection else "dev.jadix.notebook" if notes else "dev.jadix.nativeui")
            self.assertEqual(report["application"], "popover" if popover else "selection" if selection else "notes" if notes else "controls")
            self.assertEqual(report["selection_authoring"], "compound-primary-options-disabled" if selection else None)
            self.assertEqual(report["library_sha256"], hashlib.sha256(library.read_bytes()).hexdigest())
            self.assertEqual(report["provisioning_profile_copied"], profile)
            self.assertFalse(report["distribution_acceptance"])
            self.assertEqual(report["checks_enabled"], checks)
            self.assertEqual(any("JADIX_NATIVE_CHECKS" in command for command in commands), checks)
            for item in report["files"]:
                self.assertEqual(hashlib.sha256((output / item["path"]).read_bytes()).hexdigest(), item["sha256"])
            self.assertEqual(any(command[0] == "codesign" for command in commands), target.endswith("-sim") or sign)
            if target.endswith("-sim"):
                self.assertEqual(report["signing"], "ad-hoc simulator")
            elif not sign:
                self.assertEqual(report["signing"], "unsigned device bundle")
            self.assertIn("uikit,popover" if popover else "uikit,selection" if selection else "uikit,notes" if notes else "uikit,extended", commands[0])
            self.assertTrue(any(str(library) in command for command in commands))
            if profile:
                self.assertEqual((app / "embedded.mobileprovision").read_bytes(), provisioning.read_bytes())

    def test_simulated_control_and_notes_simulator_packaging(self):
        for notes in [False, True]:
            self.simulated_build("aarch64-apple-ios-sim", notes)
            self.simulated_build("aarch64-apple-ios-sim", notes, checks=True)

    def test_selection_build_uses_its_own_probe_and_bundle(self):
        self.simulated_build("aarch64-apple-ios-sim", False, checks=True, selection=True)
        self.simulated_build("aarch64-apple-ios", False, selection=True)
        for enabled in (False, True):
            command = builder.swift_command(root, Path("app"), Path("lib.a"), "aarch64-apple-ios-sim", "/sdk", False, enabled, True)
            self.assertIn("JADIX_SELECT", command)
            self.assertEqual(str(root / "examples/native-controls/ios/NativeSelectChecks.swift") in command, enabled)
            self.assertNotIn(str(root / "examples/native-controls/ios/NativeChecks.swift"), command)
            self.assertNotIn(str(root / "examples/native-controls/ios/NativeNotes.swift"), command)

    def test_selection_and_notes_conflict_before_commands_or_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "absent"
            with patch.object(builder.sys, "argv", ["build-ios.py", "--selection", "--notes", "--output", str(output)]), patch.object(builder.sys, "platform", "darwin"), patch.object(builder.subprocess, "run") as run, patch.object(builder.subprocess, "check_output") as capture, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    builder.main()
                run.assert_not_called()
                capture.assert_not_called()
                self.assertFalse(output.exists())

    def test_popover_build_uses_separate_probe_bundle_and_native_source(self):
        self.simulated_build("aarch64-apple-ios-sim", False, checks=True, popover=True)
        self.simulated_build("aarch64-apple-ios", False, popover=True)
        for enabled in (False, True):
            command = builder.swift_command(root, Path("app"), Path("lib.a"), "aarch64-apple-ios-sim", "/sdk", False, enabled, popover=True)
            self.assertIn("JADIX_POPOVER", command)
            self.assertIn(str(root / "crates/jadix_native/platforms/ios/NativePopover.swift"), command)
            self.assertEqual(str(root / "examples/native-controls/ios/NativePopoverChecks.swift") in command, enabled)
            self.assertNotIn(str(root / "examples/native-controls/ios/NativeChecks.swift"), command)
            self.assertNotIn(str(root / "examples/native-controls/ios/NativeSelectChecks.swift"), command)

    def test_popover_rejects_mixed_pages_and_existing_bundle_before_commands(self):
        for flag in ("--selection", "--notes"):
            with self.subTest(flag=flag), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "absent"
                with patch.object(builder.sys, "argv", ["build-ios.py", "--popover", flag, "--output", str(output)]), patch.object(builder.sys, "platform", "darwin"), patch.object(builder.subprocess, "run") as run, patch.object(builder.subprocess, "check_output") as capture, contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit):
                        builder.main()
                    run.assert_not_called()
                    capture.assert_not_called()
                    self.assertFalse(output.exists())
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            (output / "JadixPopover.app").mkdir()
            with self.assertRaisesRegex(ValueError, "already exists"):
                builder.validate_output(output)

    def test_simulated_unsigned_and_configured_device_packaging(self):
        self.simulated_build("aarch64-apple-ios", True)
        self.simulated_build("aarch64-apple-ios", True, sign=True, profile=True)


if __name__ == "__main__":
    unittest.main()
