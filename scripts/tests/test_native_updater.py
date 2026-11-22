import argparse
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_updater as updater


class UpdaterReleaseTests(unittest.TestCase):
    def test_packaged_configuration_uses_shared_validation_and_binds_application_version(self):
        with patch.object(updater, "tool", return_value={"config": {"validated": True}}) as tool:
            self.assertEqual(updater.configuration({"github": "example/app"}, "example.app", "2.0.0"), {"validated": True})
            self.assertEqual(tool.call_args.args[0]["config"]["version"], "2.0.0")
            self.assertEqual(tool.call_args.args[0]["config"]["id"], "example.app")
        with self.assertRaises(ValueError):
            updater.configuration({"id": "script.override"}, "example.app", "2.0.0")
        self.assertIsNone(updater.configuration(None, "example.app", "2.0.0"))

    def test_real_rust_signing_pipeline_preserves_exact_manifest_and_refuses_bad_platforms(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            key = root / "private-key"
            key_report = updater.keygen(argparse.Namespace(key=key), root / "key.json")
            self.assertNotIn(key.read_text().strip(), json.dumps(key_report))
            with self.assertRaises(ValueError):
                updater.keygen(argparse.Namespace(key=key), root / "key2.json")
            asset = root / "app.zip"
            asset.write_bytes(b"release archive")
            notes = root / "zh.md"
            notes.write_text("修复🦀", encoding="utf-8")
            arguments = argparse.Namespace(id="example.app", version="2.0.0", manifest_url="https://example.test/preview/latest.json",
                asset=[["macos-aarch64", str(asset), "./app.zip"]], localized_notes=[["zh", str(notes), "./zh.md"]], notes=None, key=key)
            result = updater.release(arguments, root / "release.json")
            manifest = Path(result["manifest"])
            self.assertTrue(result["signed"])
            self.assertEqual(result["publicKey"], key_report["publicKey"])
            value = json.loads(manifest.read_bytes())
            self.assertEqual(value["platforms"]["macos-aarch64"]["size"], len(b"release archive"))
            self.assertEqual(value["localizedNotes"]["zh"]["sha256"], updater.native.file_identity(notes)["sha256"])
            regenerated = updater.tool({"action": "release", "seed": key.read_text().strip(), "manifest_url": arguments.manifest_url, "manifest": value})
            self.assertEqual(manifest.read_bytes(), regenerated["manifest"].encode("utf-8"))
            self.assertEqual(manifest.with_suffix(".json.sig").read_text(), regenerated["signature"])
            arguments.asset.append(["macos-x86_64", str(asset), "http://untrusted.test/app.zip"])
            with self.assertRaises(ValueError):
                updater.release(arguments, root / "invalid.json")
            self.assertFalse((root / "invalid-files").exists())


if __name__ == "__main__":
    unittest.main()
