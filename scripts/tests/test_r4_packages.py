import importlib.util
import hashlib
import tempfile
import unittest
from pathlib import Path


def load(name):
    specification = importlib.util.spec_from_file_location(name, Path(__file__).resolve().parents[1] / ("release/" + name + ".py" if name == "check-r4-packages" else "acceptance/" + name + ".py"))
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


packages = load("check-r4-packages")
identities = load("check-r2-test-identities")


class CandidateContracts(unittest.TestCase):
    def test_fork_provenance_must_be_present_and_exact(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            path = directory / "JADIX_PATCH.md"
            path.write_text("# Maintained patch\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                packages.validate_provenance(directory, {})
            packages.validate_provenance(directory, {path.name: hashlib.sha256(path.read_bytes()).hexdigest()})
            with self.assertRaises(ValueError):
                packages.validate_provenance(directory, {path.name: "different"})

    def test_upload_limit_is_inclusive_and_rejects_oversize(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "fixture.crate"
            path.write_bytes(b"1234")
            self.assertEqual(packages.validate_upload_size(path, limit=4), 4)
            with self.assertRaises(ValueError):
                packages.validate_upload_size(path, limit=3)

    def test_registry_paths(self):
        self.assertEqual([packages.index_path(name) for name in ("a", "ab", "abc", "jadix")],
                         ["1/a", "2/ab", "3/a/abc", "ja/di/jadix"])

    def test_alias_target_and_feature_v2_survive_packaging(self):
        manifest = {"package": {"name": "candidate", "version": "0.1.0"},
                    "features": {"syntax": ["dep:parser"]},
                    "target": {"cfg(windows)": {"build-dependencies": {
                        "parser": {"package": "syn", "version": "2", "optional": True,
                                   "default-features": False, "features": ["full"]}}}}}
        record = packages.index_record(manifest, "sha256")
        self.assertEqual(record["v"], 2)
        self.assertEqual(record["features2"], manifest["features"])
        self.assertEqual(record["deps"][0], {"name": "parser", "package": "syn", "req": "2",
            "optional": True, "default_features": False, "features": ["full"],
            "kind": "build", "target": "cfg(windows)", "registry": None})

    def test_packaged_path_and_git_bypasses_are_rejected(self):
        for bypass in ("path", "git", "workspace"):
            with self.subTest(bypass=bypass), self.assertRaises(ValueError):
                packages.index_record({"package": {"name": "bad", "version": "0.1.0"},
                    "dependencies": {"runtime": {"version": "0.1", bypass: "value"}}}, "sha256")

    def test_qualified_identity_keeps_package_target_and_features(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "tests.log"
            path.write_text("Running unittests src/lib.rs (target/debug/deps/jadix_core-012abc.exe)\ntest reactivity::tests::signal ... ok\n", encoding="utf-8")
            record, = identities.qualified_tests(path, None, "lib", ["test-hooks", "default"])
            self.assertEqual(record["package"], "jadix_core")
            self.assertEqual(record["target"], "lib")
            self.assertEqual(record["features"], ["default", "test-hooks"])

    def test_qualified_identity_rejects_duplicates_and_zero_matches(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "tests.log"
            for text in ("test first ... ok\ntest first ... ok\n", "test result: ok. 0 passed; 0 failed\n"):
                path.write_text(text, encoding="utf-8")
                with self.assertRaises(ValueError):
                    identities.qualified_tests(path, "jadix", "lib", [])

    def test_binary_alias_keeps_the_explicit_package_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/"cli.log"
            path.write_text("Running unittests src/main.rs (target/debug/deps/jadix_hot_reload-012abc.exe)\ntest supervisor::repair ... ok\n",encoding="utf-8")
            record,=identities.qualified_tests(path,"jadix_hot_reload_cli","bin:jadix-hot-reload",["default"])
            self.assertEqual(record["package"],"jadix_hot_reload_cli")
            self.assertEqual(record["target"],"bin:jadix-hot-reload")

    def test_historical_utf16_and_current_utf8_keep_test_identities(self):
        text = "test first::case ... ok\r\ntest second::case ... ignored, native GPU\r\n"
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "tests.log"
            for encoding in ("utf-8-sig", "utf-16"):
                path.write_bytes(text.encode(encoding))
                self.assertEqual(identities.read_tests(path), [("first::case", "ok"), ("second::case", "ignored")])


if __name__ == "__main__":
    unittest.main()
