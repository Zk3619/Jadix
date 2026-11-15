import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


DIRECTORY = Path(__file__).parents[1] / "acceptance"
sys.path.insert(0, str(DIRECTORY))
from native_browser import artifact, launch_options, validate_output

SPEC = importlib.util.spec_from_file_location("notes_scale", DIRECTORY / "native-notes-scale.py")
SCALE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCALE)


class NativeBrowserTests(unittest.TestCase):
    def test_select_build_rejects_incompatible_pages_before_creating_outputs(self):
        builder = DIRECTORY.parents[1] / "examples/native-controls/build-web.py"
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "must-not-be-created"
            for flag in ["--extended", "--large-list", "--notes"]:
                with self.subTest(flag=flag):
                    result = subprocess.run([sys.executable, str(builder), "--selection", flag, "--output", str(output)],
                                            capture_output=True, text=True, encoding="utf8")
                    self.assertEqual(result.returncode, 2)
                    self.assertIn("cannot be combined", result.stderr)
                    self.assertFalse(output.exists())

    def bundle(self, root):
        rows = []
        for name in ["index.html", "pkg/jadix_native_controls.js", "pkg/jadix_native_controls_bg.wasm"]:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture bytes")
            rows.append({"path": name, "bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
        return {"application": "notes", "rust_profile": "release", "files": rows}

    def test_engine_options_preserve_edge_default_and_reject_ignored_channel(self):
        self.assertEqual(launch_options("chromium", None), {"headless": True, "channel": "msedge"})
        self.assertEqual(launch_options("chromium", "chromium")["channel"], "chromium")
        for engine in ["firefox", "webkit"]:
            self.assertEqual(launch_options(engine, None), {"headless": True})
            with self.assertRaises(ValueError):
                launch_options(engine, "msedge")
        with self.assertRaises(ValueError):
            launch_options("unknown", None)

    def test_release_bundle_hashes_and_required_members(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            valid = self.bundle(root)
            report = root / "build.json"
            report.write_text(json.dumps(valid))
            self.assertEqual(artifact(root)["application"], "notes")
            for key, value in [("application", "controls"), ("rust_profile", "debug"), ("files", valid["files"][:-1]), ("files", valid["files"] * 2)]:
                report.write_text(json.dumps({**valid, key: value}))
                with self.assertRaises(ValueError):
                    artifact(root)
            report.write_text(json.dumps(valid))
            (root / "index.html").write_bytes(b"modified")
            with self.assertRaisesRegex(ValueError, "does not match"):
                artifact(root)

    def test_explicit_chromium_executable_requires_an_existing_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            browser = Path(temporary) / "fixture-browser-not-executable"
            browser.write_bytes(b"fixture")
            options = launch_options("chromium", "msedge", browser)
            self.assertEqual(options, {"headless": True, "channel": "msedge", "executable_path": str(browser.resolve())})
            for engine, path in (("firefox", browser), ("webkit", browser), ("chromium", browser.parent), ("chromium", browser.with_name("missing"))):
                with self.subTest(engine=engine, path=path), self.assertRaises(ValueError):
                    launch_options(engine, None, path)

    def test_escaping_bundle_path_is_rejected_before_file_read(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = self.bundle(root)
            report["files"][0]["path"] = "../outside.html"
            (root / "build.json").write_text(json.dumps(report))
            with self.assertRaisesRegex(ValueError, "escaped"):
                artifact(root)

    def test_controls_require_explicit_application_and_profile_policy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            valid = {**self.bundle(root), "application": "controls", "rust_profile": "debug"}
            report = root / "build.json"
            report.write_text(json.dumps(valid))
            with self.assertRaises(ValueError):
                artifact(root)
            with self.assertRaises(ValueError):
                artifact(root, "controls")
            self.assertEqual(artifact(root, "controls", release=False)["rust_profile"], "debug")
            for profile in ["", "unknown", None]:
                report.write_text(json.dumps({**valid, "rust_profile": profile}))
                with self.assertRaises(ValueError):
                    artifact(root, "controls", release=False)

    def test_archived_nested_and_used_outputs_are_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            validate_output(root)
            (root / "snapshot.json").write_text("{}")
            for path in [root, root / "nested"]:
                with self.assertRaisesRegex(ValueError, "archived"):
                    validate_output(path)
            self.assertFalse((root / "nested").exists())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "keep").write_text("unchanged")
            with self.assertRaisesRegex(ValueError, "empty"):
                validate_output(root)
            self.assertEqual((root / "keep").read_text(), "unchanged")

    def test_large_fixture_respects_count_utf8_and_document_budgets(self):
        notebook = SCALE.notebook_fixture()
        self.assertEqual(len(notebook["notes"]), 128)
        self.assertEqual(notebook["next_id"], "129")
        self.assertEqual(len(SCALE.encoded(notebook).encode("utf8")), 512 * 1024 - 2048)
        self.assertEqual([note["id"] for note in notebook["notes"]], [str(index) for index in range(1, 129)])
        for note in notebook["notes"]:
            self.assertLessEqual(len(note["title"].encode("utf8")), 256)
            self.assertLessEqual(len(note["body"].encode("utf8")), 32768)
            self.assertIn("אבג", note["body"])
        self.assertLess(len(notebook["notes"][0]["body"].encode("utf8")), 8192)


if __name__ == "__main__":
    unittest.main()
