import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

DIRECTORY = Path(__file__).parents[1] / "acceptance"
sys.path.insert(0, str(DIRECTORY))
SPEC = importlib.util.spec_from_file_location("native_select_android", DIRECTORY / "native-select-android-check.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class NativeSelectAndroidTests(unittest.TestCase):
    def test_receipt_requires_one_complete_unambiguous_current_scenario_set(self):
        valid = "JADIX_SELECT_CHECKS_PASS: scenarios=10; native_select_checked=true; popup_checked=true; recreate_checked=true; disposal_checked=true;"
        self.assertTrue(CHECK.instrumentation_passed(valid))
        self.assertTrue(CHECK.instrumentation_passed("INSTRUMENTATION_RESULT: stream=" + valid + "\nINSTRUMENTATION_CODE: -1"))
        for invalid in [valid + "\n" + valid, valid + " JADIX_SELECT_CHECKS_FAIL", valid.replace("scenarios=10", "scenarios=9"),
                        valid.replace("popup_checked=true", "popup_checked=trueish"), valid.replace("recreate_checked=true;", ""),
                        valid + " popup_checked=false;", valid.replace("disposal_checked=true;", "\nother log disposal_checked=true;")]:
            with self.subTest(receipt=invalid):
                self.assertFalse(CHECK.instrumentation_passed(invalid))

    def bundle(self, root):
        apk = root / "native-controls.apk"
        library = b"fixture native library, never executed"
        with zipfile.ZipFile(apk, "w") as archive:
            archive.writestr("lib/x86_64/libjadix_native_controls.so", library)
        report = {"application": "controls", "package_name": "dev.jadix.selection", "selection": True, "extended": False,
                  "retained_rows": 0, "rust_profile": "release", "target": "x86_64-linux-android",
                  "apk_bytes": apk.stat().st_size, "apk_sha256": hashlib.sha256(apk.read_bytes()).hexdigest(),
                  "library_bytes": len(library), "library_sha256": hashlib.sha256(library).hexdigest()}
        (root / "build.json").write_text(json.dumps(report), encoding="utf8")
        return apk, report

    def test_isolated_selection_profile_target_and_both_binary_hashes_are_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            apk, valid = self.bundle(root)
            self.assertEqual(CHECK.artifact(apk)["package_name"], "dev.jadix.selection")
            for name, value in [("selection", False), ("extended", True), ("retained_rows", 100), ("rust_profile", "debug"),
                                ("application", "notes"), ("package_name", "dev.jadix.nativeui"), ("target", "unknown"),
                                ("library_sha256", "0" * 64), ("apk_bytes", apk.stat().st_size + 1)]:
                with self.subTest(field=name):
                    (root / "build.json").write_text(json.dumps({**valid, name: value}), encoding="utf8")
                    with self.assertRaises(ValueError):
                        CHECK.artifact(apk)
            (root / "build.json").write_text(json.dumps(valid), encoding="utf8")
            apk.write_bytes(apk.read_bytes() + b"tampered")
            with self.assertRaisesRegex(ValueError, "APK does not match"):
                CHECK.artifact(apk)

    def test_missing_reported_library_fails_even_when_outer_apk_hash_matches(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            apk, report = self.bundle(root)
            with zipfile.ZipFile(apk, "w") as archive:
                archive.writestr("lib/other/libjadix_native_controls.so", b"fixture")
            report.update(apk_bytes=apk.stat().st_size, apk_sha256=hashlib.sha256(apk.read_bytes()).hexdigest())
            (root / "build.json").write_text(json.dumps(report), encoding="utf8")
            with self.assertRaisesRegex(ValueError, "exactly one"):
                CHECK.artifact(apk)

    def test_android_selection_build_rejects_other_pages_before_mutation(self):
        builder = DIRECTORY.parents[1] / "examples/native-controls/build-android.py"
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "must-not-exist"
            for flag in ["--extended", "--large-list", "--notes"]:
                with self.subTest(flag=flag):
                    result = subprocess.run([sys.executable, str(builder), "--selection", flag, "--output", str(output)], capture_output=True, text=True)
                    self.assertEqual(result.returncode, 2)
                    self.assertIn("cannot be combined", result.stderr)
                    self.assertFalse(output.exists())

    def test_registered_baseline_and_failed_install_are_never_uninstalled(self):
        for mode in ["registered", "install-failed", "instrumentation-failed"]:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                apk, _ = self.bundle(root)
                aapt = root / "aapt2"
                aapt.write_bytes(b"fixture, never executed")
                output = root / "evidence"
                calls = []

                def run(command, **_):
                    calls.append(command)
                    text = ""
                    if "badging" in command:
                        text = "package: name='dev.jadix.selection'\n"
                    elif "xmltree" in command:
                        text = 'A: targetPackage(0x01)="dev.jadix.selection"\n'
                    elif command[-1] == "get-state":
                        text = "device\n"
                    elif "packages" in command:
                        text = "package:unrelated.application\n" + ("package:dev.jadix.selection\n" if mode == "registered" else "")
                    elif "install" in command:
                        text = "Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE]\n" if mode == "install-failed" else "Success\n"
                    elif "instrument" in command:
                        text = "JADIX_SELECT_CHECKS_FAIL: fixture\n"
                    elif "uninstall" in command:
                        text = "Success\n"
                    return subprocess.CompletedProcess(command, 0, text.encode(), b"")

                argv = ["native-select-android-check.py", "--apk", str(apk), "--serial", "fixture-device",
                        "--output", str(output), "--aapt2", str(aapt)]
                with patch.object(sys, "argv", argv), patch.object(CHECK.subprocess, "run", side_effect=run):
                    with self.assertRaises(RuntimeError):
                        CHECK.main()
                self.assertEqual(any("uninstall" in command for command in calls), mode == "instrumentation-failed")
                self.assertEqual(any("install" in command for command in calls), mode != "registered")
                self.assertEqual(any("instrument" in command for command in calls), mode == "instrumentation-failed")
                report = json.loads((output / "checks.json").read_text(encoding="utf8"))
                self.assertFalse(report["passed"])
                if mode == "instrumentation-failed":
                    self.assertTrue(report["cleanup"]["package_registry_restored"])


if __name__ == "__main__":
    unittest.main()
