import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


TOOL = Path(__file__).parents[1] / "native.py"
SPEC = importlib.util.spec_from_file_location("native_toolchain", TOOL)
NATIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NATIVE)


class NativeToolchainTests(unittest.TestCase):
    def test_manifest_changes_when_source_changes_and_ignores_build_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "source.rs").write_text("first", encoding="utf-8")
            first = NATIVE.tree_identity(root)
            (root / "target").mkdir()
            (root / "target/cache").write_bytes(b"not source")
            self.assertEqual(first, NATIVE.tree_identity(root))
            (root / "source.rs").write_text("second", encoding="utf-8")
            second = NATIVE.tree_identity(root)
            self.assertNotEqual(first["manifest_sha256"], second["manifest_sha256"])
            self.assertEqual(second["file_count"], 1)

    def test_empty_reference_cannot_be_frozen_as_a_valid_baseline(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "empty or unavailable"):
                NATIVE.tree_identity(Path(temporary))

    def test_output_refuses_existing_files_and_archived_directories(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "report.json"
            output.write_text("preserved", encoding="utf-8")
            for protected in [output, root / "archive/new.json"]:
                if protected != output:
                    protected.parent.mkdir()
                    (protected.parent / "snapshot.json").write_text("{}", encoding="utf-8")
                result = subprocess.run([sys.executable, str(TOOL), "doctor", "--output", str(protected)],
                                        capture_output=True, text=True, encoding="utf-8")
                self.assertEqual(result.returncode, 2)
            self.assertEqual(output.read_text(encoding="utf-8"), "preserved")
            self.assertFalse((root / "archive/new.json").exists())

    def test_sdk_metadata_is_observed_instead_of_inferred_from_directory_name(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "20"
            native = root / "native"
            native.mkdir(parents=True)
            metadata = {"apiVersion": "22", "version": "fixture"}
            (native / "oh-uni-package.json").write_text(json.dumps(metadata), encoding="utf-8")
            observed = NATIVE.sdk_identity(root)
            self.assertEqual(observed["metadata"]["apiVersion"], "22")
            self.assertFalse(observed["available"])


if __name__ == "__main__":
    unittest.main()
