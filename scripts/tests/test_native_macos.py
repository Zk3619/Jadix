import argparse
import json
import os
from pathlib import Path
import plistlib
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_macos as macos


class MacOSBuildTests(unittest.TestCase):
    def fixture(self, root):
        project = root / "project"
        project.mkdir()
        (project / "App.swift").write_text("// application entry", encoding="utf-8")
        source = root / "runtime/crates/jadix_native/platforms/macos"
        source.mkdir(parents=True)
        for name in ["JadixNativeView.swift", "JadixUpdater.swift", "NewCapability.swift", "JadixTextField.m", "JadixNative.h"]:
            (source / name).write_text("// fixture", encoding="utf-8")
        config = {"schema": 1, "name": "demo", "crate": "demo-app", "bundle_identifier": "example.demo",
                  "repository": str(root / "runtime"), "url_schemes": ["demo", "demo-auth"], "single_instance": False}
        (project / "jadix-macos.json").write_text(json.dumps(config), encoding="utf-8")
        args = argparse.Namespace(project=project, release=True, offline=True, features="native-host")
        metadata = {"packages": [{"name": "demo-app", "version": "2.3.4-beta.1+build.9"}], "target_directory": str(root / "target")}
        return args, source, config, metadata

    def test_bundle_build_includes_all_runtime_sources_and_one_platform_contract(self):
        for machine, architecture, target in [("arm64", "arm64", "aarch64-apple-darwin"), ("x86_64", "x86_64", "x86_64-apple-darwin")]:
            with self.subTest(machine=machine), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                args, source, config, metadata = self.fixture(root)
                calls = []
                def capture(command, cwd, log):
                    command = [str(part) for part in command]
                    calls.append((command, cwd, os.environ.get("MACOSX_DEPLOYMENT_TARGET")))
                    log.write_text("captured", encoding="utf-8")
                    if "swiftc" in command:
                        Path(command[command.index("-o") + 1]).write_bytes(b"executable fixture")
                with patch.object(macos.platform, "system", return_value="Darwin"), patch.object(macos.platform, "machine", return_value=machine), \
                        patch.object(macos.subprocess, "run") as run, patch.dict(os.environ, {"MACOSX_DEPLOYMENT_TARGET": "13.0"}):
                    run.return_value.stdout = json.dumps(metadata)
                    result = macos.build(args, root / "build.json", capture)
                    self.assertEqual(os.environ["MACOSX_DEPLOYMENT_TARGET"], "13.0")
                cargo, cwd, deployment = calls[0]
                self.assertEqual(cwd, args.project)
                self.assertEqual(deployment, "14.0")
                self.assertIn(target, cargo)
                self.assertTrue(all(flag in cargo for flag in ["--locked", "--release", "--offline", "--features"]))
                swift = next(command for command, _, _ in calls if "swiftc" in command)
                self.assertIn(architecture + "-apple-macosx14.0", swift)
                self.assertTrue(all(str(path) in swift for path in source.glob("*.swift")))
                self.assertIn(str(root / "target" / target / "release/libdemo_app.a"), swift)
                self.assertIn("-mmacosx-version-min=14.0", calls[1][0])
                self.assertEqual(calls[-1][0][:4], ["codesign", "--verify", "--strict", "--deep"])
                info = plistlib.loads((Path(result["bundle"]) / "Contents/Info.plist").read_bytes())
                self.assertEqual(info["LSMinimumSystemVersion"], "14.0")
                self.assertEqual(info["CFBundleVersion"], "2.3.4")
                self.assertEqual(info["CFBundleShortVersionString"], metadata["packages"][0]["version"])
                self.assertEqual(info["CFBundleURLTypes"][0]["CFBundleURLSchemes"], config["url_schemes"])
                self.assertFalse(info["LSMultipleInstancesProhibited"])
                self.assertEqual(macos.package_files(result), result["files"])
                self.assertEqual(set(result["files"]), {"demo.app/Contents/MacOS/demo", "demo.app/Contents/Info.plist"})

    def test_failed_rust_build_restores_environment(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args, _, _, metadata = self.fixture(root)
            def fail(*args):
                self.assertEqual(os.environ["MACOSX_DEPLOYMENT_TARGET"], "14.0")
                raise RuntimeError("fixture build failure")
            with patch.object(macos.platform, "system", return_value="Darwin"), patch.object(macos.subprocess, "run") as run, patch.dict(os.environ):
                os.environ.pop("MACOSX_DEPLOYMENT_TARGET", None)
                run.return_value.stdout = json.dumps(metadata)
                with self.assertRaisesRegex(RuntimeError, "fixture build failure"):
                    macos.build(args, root / "build.json", fail)
                self.assertNotIn("MACOSX_DEPLOYMENT_TARGET", os.environ)

    def test_packaging_rejects_path_aliases_escape_and_wrong_executable(self):
        identity = {"sha256": "abc", "size": 3}
        valid = {"bundle": "demo.app", "executable": "demo.app/Contents/MacOS/demo", "identity": identity,
                 "files": {"demo.app/Contents/MacOS/demo": identity, "demo.app/Contents/Info.plist": identity}}
        for invalid in ["demo.app/Contents/../outside", "demo.app/Contents/./alias", "demo.app//Contents/alias",
                        "other.app/Contents/alias", "/demo.app/Contents/alias", "demo.app/Contents/C:escape"]:
            with self.subTest(path=invalid), self.assertRaises(ValueError):
                macos.package_files({**valid, "files": {**valid["files"], invalid: identity}})
        with self.assertRaises(ValueError):
            macos.package_files({**valid, "identity": {"sha256": "wrong"}})


if __name__ == "__main__":
    unittest.main()
