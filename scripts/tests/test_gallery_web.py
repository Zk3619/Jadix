import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("gallery_web_build", ROOT / "examples/gallery_web/build.py")
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class GalleryWebBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.target = Path(self.temporary.name)
        self.metadata = json.dumps({"target_directory": str(self.target)})

    def test_missing_cli_has_actionable_error(self):
        with patch("sys.argv", ["build.py"]), patch.object(BUILD.subprocess, "check_output", return_value=self.metadata), patch.object(BUILD.shutil, "which", return_value=None):
            with self.assertRaisesRegex(SystemExit, "wasm-bindgen-cli 0.2.114"):
                BUILD.main()

    def test_cli_schema_mismatch_stops_before_compilation(self):
        with patch("sys.argv", ["build.py"]), patch.object(BUILD.subprocess, "check_output", side_effect=[self.metadata, "wasm-bindgen 0.2.113"]), patch.object(BUILD.shutil, "which", return_value="wasm-bindgen"), patch.object(BUILD.subprocess, "run") as run:
            with self.assertRaisesRegex(SystemExit, "Expected wasm-bindgen 0.2.114"):
                BUILD.main()
            run.assert_not_called()

    def test_build_preserves_flags_and_generates_static_bundle(self):
        def generate(command, **kwargs):
            if "--out-dir" in command:
                output = Path(command[command.index("--out-dir") + 1])
                output.mkdir(parents=True)
                (output / "gallery_web_bg.wasm").write_bytes(b"\0asm")

        with patch("sys.argv", ["build.py"]), patch.object(BUILD.subprocess, "check_output", side_effect=[self.metadata, "wasm-bindgen 0.2.114"]), patch.object(BUILD.shutil, "which", return_value="wasm-bindgen"), patch.object(BUILD.subprocess, "run", side_effect=generate) as run, patch.dict(os.environ, {"RUSTFLAGS": "-C opt-level=2"}):
            BUILD.main()
            cargo = run.call_args_list[0]
            self.assertIn("--locked", cargo.args[0])
            self.assertIn("wasm32-unknown-unknown", cargo.args[0])
            self.assertEqual(cargo.kwargs["env"]["RUSTFLAGS"], "-C opt-level=2 --cfg web_sys_unstable_apis")
            self.assertIn("web_sys_unstable_apis", cargo.kwargs["env"]["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"])
            self.assertTrue((self.target / "gallery-web/index.html").is_file())

    def test_serve_reuse_requires_existing_wasm(self):
        with patch("sys.argv", ["build.py", "--no-build"]), patch.object(BUILD.subprocess, "check_output", return_value=self.metadata):
            with self.assertRaisesRegex(SystemExit, "No browser bundle found"):
                BUILD.main()


if __name__ == "__main__":
    unittest.main()
