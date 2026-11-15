import argparse
import os
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1]))
import native_validation
import native_workflow
import test_native_workflow as fixtures


class FieldToolTests(unittest.TestCase):
    def test_field_profiles_remain_separate_from_controls_and_uikit_runtime(self):
        for suite, features in [("field", "windows,field-workflow"), ("uikit-field", "uikit,field-workflow"), ("wasm-field", "field-workflow")]:
            if suite == "field" and os.name != "nt":
                continue
            [(name, command)] = native_validation.commands([suite])
            self.assertEqual(name, suite)
            self.assertEqual(command[command.index("--features") + 1], features)
        for builder in ["build-web.py", "build-android.py", "build-ios.py"]:
            result = subprocess.run([sys.executable, str(Path(__file__).parents[2] / "examples/native-controls" / builder),
                                     "--field", "--notes"], capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn(b"own application", result.stderr)

    def test_android_field_build_retains_requested_application_or_fails_closed(self):
        for application in ["field", "controls"]:
            with self.subTest(application=application), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                args = argparse.Namespace(platform="android", features="field-workflow", android_sdk=directory,
                    ndk=directory, java_home=directory, android_target="x86_64-linux-android", project=None,
                    offline=True, release=True, extended=False, large_list=False)
                def build(command, **kwargs):
                    self.assertIn("--field", command)
                    destination = Path(command[command.index("--output") + 1]); destination.mkdir()
                    report = fixtures.NativeWorkflowTests().android_bundle(destination)
                    report.update(application=application, package_name="dev.jadix.field" if application == "field" else "dev.jadix.nativeui")
                    (destination / "build.json").write_text(json.dumps(report), encoding="utf-8")
                    return subprocess.CompletedProcess(command, 0, "", "")
                with patch.object(native_workflow.subprocess, "run", side_effect=build):
                    if application == "field":
                        self.assertFalse(native_workflow.build(args, directory / "field.json")["runtime_acceptance"])
                    else:
                        with self.assertRaisesRegex(ValueError, "requested field"):
                            native_workflow.build(args, directory / "field.json")


if __name__ == "__main__":
    unittest.main()
