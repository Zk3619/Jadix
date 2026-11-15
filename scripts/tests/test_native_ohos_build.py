import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


TOOL = Path(__file__).resolve().parents[2] / "examples/native-controls/build-ohos.py"
SPEC = importlib.util.spec_from_file_location("native_ohos_build", TOOL)
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class OhosBuildTests(unittest.TestCase):
    def test_workspace_manifest_and_shared_document_mutation_invalidate_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            product = root / "product"
            product.mkdir()
            (product / "source.rs").write_text("product", encoding="utf-8")
            (product / "build-ohos.py").write_bytes(TOOL.read_bytes())
            for name in ["Cargo.toml", "Cargo.lock"]:
                (root / name).write_text("initial", encoding="utf-8")
            for name in ["jadix_native", "jadix", "jadix_core", "jadix_components", "jadix_macros",
                         "jadix_tasks", "jadix_layout", "jadix_style_tokens", "jadix_taffy", "jadix_theme",
                         "jadix_animation", "jadix_hot_reload", "jadix_document"]:
                crate = root / "crates" / name
                crate.mkdir(parents=True)
                (crate / "source.rs").write_text("initial", encoding="utf-8")
            sdk = {"directory": str(root / "sdk"), "native": str(root / "sdk/native"),
                   "available": True, "metadata": {"apiVersion": "20"}}
            targets = [root / "Cargo.toml", root / "crates/jadix_document/source.rs"]
            for index, target in enumerate(targets):
                output = root / f"build-{index}"

                def mutate(command, **kwargs):
                    target.write_text("changed", encoding="utf-8")
                    return subprocess.CompletedProcess(command, 0)

                arguments = [str(TOOL), "--sdk", sdk["directory"], "--native-only", "--output", str(output)]
                with patch.object(BUILD, "ROOT", root), patch.object(BUILD, "__file__", str(product / "build-ohos.py")), \
                        patch.object(BUILD.NATIVE, "sdk_identity", return_value=sdk), \
                        patch.object(BUILD.subprocess, "run", side_effect=mutate), patch.object(sys, "argv", arguments):
                    self.assertEqual(BUILD.main(), 1)
                report = json.loads((output / "build.json").read_text(encoding="utf-8"))
                self.assertFalse(report["source_unchanged"])
                self.assertFalse(report["native_built"])
                self.assertIn("changed during", report["error"])

    def test_selected_sdk_paths_with_spaces_remain_single_compiler_arguments(self):
        sdk = {"directory": "D:/SDK with spaces/openharmony", "native": "D:/SDK with spaces/openharmony/native"}
        env = BUILD.environment(sdk, "x86_64-unknown-linux-ohos", "x86_64-linux-ohos", Path("D:/output"))
        flags = env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")
        self.assertIn("link-arg=--sysroot=" + str(Path(sdk["native"]) / "sysroot"), flags)
        self.assertEqual(env["CC_SHELL_ESCAPED_FLAGS"], "1")
        self.assertEqual(env["JADIX_OHOS_SDK"], sdk["directory"])
        self.assertNotIn("RUSTFLAGS", env)
        self.assertIn("--target=x86_64-linux-ohos", env["CFLAGS_x86_64_unknown_linux_ohos"])

    def test_existing_and_archived_outputs_are_rejected_before_building(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "snapshot.json").write_text("preserved", encoding="utf-8")
            for output in [root, root / "new-build"]:
                result = subprocess.run([sys.executable, str(TOOL), "--sdk", str(root / "missing-sdk"), "--output", str(output)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
            self.assertEqual((root / "snapshot.json").read_text(encoding="utf-8"), "preserved")
            self.assertFalse((root / "new-build").exists())


if __name__ == "__main__":
    unittest.main()
