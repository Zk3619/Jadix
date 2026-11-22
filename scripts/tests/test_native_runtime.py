import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_runtime as runtime


class AndroidSourceRuntimeTests(unittest.TestCase):
    @contextmanager
    def fixture(self, **overrides):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            project = root / "source project 中文"
            nested = project / "src" / "nested"
            nested.mkdir(parents=True)
            (project / "Cargo.toml").write_text("[package]\nname='fixture'\nversion='0.1.0'\n")
            (project / "native-android.json").write_text("{}")
            sdk, java, ndk = root / "sdk", root / "java", root / "ndk"
            for directory in (sdk / "platforms/android-36", sdk / "build-tools/36.0.0", java / "bin", ndk):
                directory.mkdir(parents=True)
            (sdk / "platforms/android-36/android.jar").touch()
            adb = sdk / "platform-tools" / ("adb.exe" if os.name == "nt" else "adb")
            adb.parent.mkdir()
            adb.touch()
            (java / "bin" / ("javac.exe" if os.name == "nt" else "javac")).touch()
            (ndk / "source.properties").touch()
            arguments = argparse.Namespace(project=nested, android_sdk=sdk, java_home=java, ndk=ndk,
                adb=None, serial="emulator-5554", compile_sdk=36, build_tools="36.0.0", target=None,
                output=root / "result", release=True, offline=True)
            for key, value in overrides.items():
                setattr(arguments, key, value)
            apk = root / "application.apk"
            apk.write_bytes(b"retained APK")
            report = root / "build-report.json"
            report.write_text(json.dumps({"package_name": "dev.example.fixture", "rust_profile": "release"}))
            build = {"apk": str(apk), "apk_identity": runtime.native.file_identity(apk), "build_report": str(report)}
            responses = {("get-state",): "device",
                ("shell", "getprop", "ro.build.version.sdk"): "36",
                ("shell", "getprop", "ro.product.cpu.abilist"): "x86_64,arm64-v8a",
                ("install", "-r", str(apk)): "Performing Streamed Install\nSuccess",
                ("shell", "am", "start", "-W", "-n", "dev.example.fixture/dev.jadix.nativeui.MainActivity"): "Status: ok\nComplete"}
            with patch.dict(os.environ, JADIX_DISTRIBUTION_HOME=str(root / "home")), \
                 patch.object(runtime.native_android, "development_keystore", return_value=root / "debug.jks") as key, \
                 patch.object(runtime.native_workflow, "build", return_value=build) as builder, \
                 patch.object(runtime, "adb_command", side_effect=lambda adb, serial, *command: responses[command]) as device:
                yield arguments, project, build, responses, builder, device, key

    def test_nested_source_run_preserves_profile_offline_and_device_identity(self):
        with self.fixture() as (arguments, project, build, responses, builder, device, key):
            receipt = runtime.run(arguments)
            options = builder.call_args.args[0]
            self.assertEqual(options.project, project.resolve())
            self.assertEqual(options.android_target, "x86_64-linux-android")
            self.assertTrue(options.release)
            self.assertTrue(options.offline)
            self.assertEqual(options.compile_sdk, 36)
            self.assertEqual(options.build_tools, "36.0.0")
            self.assertEqual(receipt["serial"], arguments.serial)
            self.assertFalse(receipt["runtime_acceptance"])
            self.assertTrue(receipt["installed"] and receipt["launched"])
            self.assertEqual(json.loads((arguments.output / "run.json").read_bytes()), receipt)
            self.assertTrue(all(call.args[1] == arguments.serial for call in device.call_args_list))
            self.assertEqual(runtime.project_root(project / "Cargo.toml"), project)

    def test_device_api_and_abi_fail_before_build_or_key_creation(self):
        for sdk_value, abis, target in (("25", "x86_64", None), ("invalid", "x86_64", None),
                                        ("36", "armeabi-v7a", None), ("36", "x86_64", "aarch64-linux-android")):
            with self.subTest(api=sdk_value, abis=abis, target=target), self.fixture(target=target) as fixture:
                arguments, _, _, responses, builder, _, key = fixture
                responses[("shell", "getprop", "ro.build.version.sdk")] = sdk_value
                responses[("shell", "getprop", "ro.product.cpu.abilist")] = abis
                with self.assertRaises(ValueError):
                    runtime.run(arguments)
                builder.assert_not_called()
                key.assert_not_called()

    def test_changed_apk_and_wrong_release_profile_are_not_installed(self):
        for problem in ("artifact", "profile"):
            with self.subTest(problem=problem), self.fixture() as fixture:
                arguments, _, build, _, _, device, _ = fixture
                if problem == "artifact":
                    Path(build["apk"]).write_bytes(b"replaced APK")
                else:
                    Path(build["build_report"]).write_text(json.dumps({"package_name": "dev.example.fixture", "rust_profile": "debug"}))
                with self.assertRaises(ValueError):
                    runtime.run(arguments)
                self.assertFalse(any(call.args[2] == "install" for call in device.call_args_list))
                self.assertFalse((arguments.output / "run.json").exists())

    def test_install_and_launch_failures_never_publish_success_receipt(self):
        for failure in ("install", "launch"):
            with self.subTest(failure=failure), self.fixture() as fixture:
                arguments, _, build, responses, _, device, _ = fixture
                if failure == "install":
                    responses[("install", "-r", build["apk"])] = "Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE]"
                else:
                    responses[("shell", "am", "start", "-W", "-n", "dev.example.fixture/dev.jadix.nativeui.MainActivity")] = "Status: ok\nError: Activity not started"
                with self.assertRaises(ValueError):
                    runtime.run(arguments)
                self.assertFalse((arguments.output / "run.json").exists())
                if failure == "install":
                    self.assertFalse(any(call.args[2:5] == ("shell", "am", "start") for call in device.call_args_list))


if __name__ == "__main__":
    unittest.main()
