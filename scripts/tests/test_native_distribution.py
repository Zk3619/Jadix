import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_distribution as distribution


class DistributionTests(unittest.TestCase):
    def release(self, root, version, names=("cli", "android"), program=None):
        release = root / version
        release.mkdir()
        components = {}
        for name in names:
            source = release / name
            source.mkdir()
            (source / "main.py").write_text(program or "import sys\nfrom pathlib import Path\nPath(sys.argv[1]).write_text(" + repr(version + ":" + name) + ")\n", encoding="utf-8")
            result = distribution.pack(argparse.Namespace(source=source, name=name, version=version,
                commit="a" * 40, entry="main.py"), release / (name + ".json"))
            components[name] = result["component"]
        manifest = release / "release.json"
        manifest.write_bytes(distribution.encoded({"schema": 1, "channel": "stable", "version": version,
            "commit": "a" * 40, "components": components}))
        return manifest

    def test_real_install_launch_offline_and_transactional_self_update(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            home = root / "installed"
            first = self.release(root, "1.0")
            manifest, _ = distribution.catalog(first, "stable")
            distribution.install(home, manifest, ["cli", "android"])
            receipt = root / "run.txt"
            self.assertEqual(distribution.launch(home, "android", [str(receipt)])["exit_code"], 0)
            self.assertEqual(receipt.read_text(), "1.0:android")
            # The cache is sufficient even after the release archive disappears.
            (first.parent / "android.zip").unlink()
            distribution.install(home, manifest, ["android"], offline=True)
            before = (home / "active.json").read_bytes()
            broken = self.release(root, "2.0", ("cli",))
            second, _ = distribution.catalog(broken, "stable")
            with self.assertRaisesRegex(ValueError, "unavailable"):
                distribution.install(home, second, None)
            self.assertEqual((home / "active.json").read_bytes(), before)
            self.assertFalse((home / ".update-lock").exists())
            good = self.release(root, "3.0")
            third, _ = distribution.catalog(good, "stable")
            distribution.install(home, third, None)
            self.assertEqual({item["version"] for item in distribution.state(home)["components"].values()}, {"3.0"})
            distribution.launch(home, "cli", [str(receipt)])
            self.assertEqual(receipt.read_text(), "3.0:cli")

    def test_bad_download_and_activation_failure_preserve_installed_generation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _ = distribution.catalog(self.release(root, "1.0"), "stable")
            home = root / "installed"
            distribution.install(home, manifest, ["cli"])
            before = (home / "active.json").read_bytes()
            generations = set((home / "versions").iterdir())
            with patch.object(distribution, "activate", side_effect=OSError("disk full")):
                with self.assertRaises(OSError):
                    distribution.install(home, manifest, ["cli"], offline=True)
            self.assertEqual(set((home / "versions").iterdir()), generations)
            self.assertEqual((home / "active.json").read_bytes(), before)
            cache = home / "cache" / (manifest["components"]["cli"]["sha256"] + ".zip")
            cache.write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "SHA-256"):
                distribution.install(home, manifest, ["cli"], offline=True)
            self.assertEqual((home / "active.json").read_bytes(), before)
            with distribution.lock(home):
                with self.assertRaisesRegex(ValueError, "locked"):
                    distribution.install(home, manifest, ["cli"])

    def test_archive_escape_alias_symlink_unrecorded_and_wrong_commit_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = self.release(root, "1.0", ("cli",))
            manifest, _ = distribution.catalog(path, "stable")
            original = Path(manifest["components"]["cli"]["url"])
            for index, extra in enumerate(("../escape", "nested/../../escape", "main.py.", "NUL.txt", "MAIN.PY", "extra.txt", "link")):
                archive = root / (str(index) + ".zip")
                with zipfile.ZipFile(original) as source, zipfile.ZipFile(archive, "w") as target:
                    for member in source.infolist():
                        target.writestr(member, source.read(member))
                    member = zipfile.ZipInfo(extra)
                    if extra == "link":
                        member.create_system = 3
                        member.external_attr = 0o120777 << 16
                    target.writestr(member, b"bad")
                manifest["components"]["cli"] = {"url": str(archive), **distribution.native.file_identity(archive)}
                with self.assertRaises((ValueError, OSError)):
                    distribution.install(root / ("home" + str(index)), manifest, ["cli"])
            manifest["components"]["cli"] = {"url": str(original), **distribution.native.file_identity(original)}
            manifest["commit"] = "b" * 40
            with self.assertRaisesRegex(ValueError, "version/commit"):
                distribution.install(root / "mismatch", manifest, ["cli"])
            self.assertFalse((root / "escape").exists())

    def test_cli_reports_components_and_propagates_entry_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            home = root / "home"
            manifest = self.release(root, "1.0", ("cli",))
            cli = Path(distribution.__file__).with_name("native.py")
            result = subprocess.run([sys.executable, str(cli), "component-run", "cli", "--home", str(home),
                "--manifest", str(manifest), "--output", str(root / "run.json")], capture_output=True)
            self.assertNotEqual(result.returncode, 0)  # fixture requires a receipt argument
            self.assertNotEqual(json.loads((root / "run.json").read_bytes())["observations"]["exit_code"], 0)
            result = subprocess.run([sys.executable, str(cli), "component-list", "--home", str(home),
                "--output", str(root / "list.json")], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("cli", json.loads((root / "list.json").read_bytes())["observations"]["components"])

    def test_existing_runtime_is_replaced_only_when_release_identity_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            home = root / "home"
            first, second = self.release(root, "1.0"), self.release(root, "2.0")
            before = distribution.ensure(home, "android", str(first))
            after = distribution.ensure(home, "android", str(second))
            self.assertNotEqual(before["directory"], after["directory"])
            self.assertEqual(after["version"], "2.0")
            self.assertEqual(distribution.ensure(home, "android", str(second)), after)
            with patch.dict(os.environ, JADIX_DISTRIBUTION_VERSION="1.0", JADIX_DISTRIBUTION_COMMIT="a" * 40):
                with self.assertRaisesRegex(ValueError, "running CLI"):
                    distribution.ensure(home, "android", str(second))
                with self.assertRaisesRegex(ValueError, "running CLI"):
                    distribution.ensure(home, "android")
            self.assertEqual(distribution.state(home)["components"]["android"], after)

    def test_remote_catalog_can_be_reopened_offline_with_original_digest_and_url_base(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            local = self.release(root, "1.0")
            home = root / "home"
            remote = "https://releases.example/stable/release.json"
            digest = distribution.native.file_identity(local)["sha256"]
            transfer = distribution.transfer
            with patch.object(distribution, "transfer", side_effect=lambda source, path, maximum: transfer(local if source == remote else source, path, maximum)):
                online, _ = distribution.catalog(remote, "stable", digest, home)
            with patch.object(distribution, "urlopen", side_effect=AssertionError("offline must not contact a server")):
                offline, _ = distribution.catalog(remote, "stable", digest, home, offline=True)
                self.assertEqual(online, offline)
                self.assertEqual(offline["components"]["android"]["url"], "https://releases.example/stable/android.zip")
                with self.assertRaisesRegex(ValueError, "missing from offline"):
                    distribution.catalog(remote + "?new", "stable", digest, home, offline=True)
            cache = next((home / "catalogs").iterdir())
            cache.write_bytes(b"{}")
            with self.assertRaisesRegex(ValueError, "SHA-256"):
                distribution.catalog(remote, "stable", digest, home, offline=True)

    def test_source_run_installs_runtime_and_preserves_paths_arguments_and_release_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            receipt = root / "receipt.json"
            program = ("import os, sys, json\nfrom pathlib import Path\n"
                       "Path(os.environ['JADIX_TEST_RECEIPT']).write_text(json.dumps({'args':sys.argv[1:],"
                       "'version':os.environ['JADIX_DISTRIBUTION_VERSION'],'commit':os.environ['JADIX_DISTRIBUTION_COMMIT']}))\n")
            manifest = self.release(root, "1.0", ("android",), program)
            project = root / "source project 中文"
            project.mkdir()
            (project / "Cargo.toml").write_text("[package]\nname='fixture'\nversion='0.1.0'\n")
            cli = Path(distribution.__file__).with_name("native.py")
            result = subprocess.run([sys.executable, str(cli), "run", "--runtime", "android", "--project", str(project),
                "--release", "--runtime-argument=--example", "--home", str(root / "home"), "--manifest", str(manifest),
                "--output", str(root / "run.json")], capture_output=True, env=dict(os.environ, JADIX_TEST_RECEIPT=str(receipt)))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(receipt.read_bytes()), {"args": ["--project", str(project.resolve()), "--release", "--example"],
                "version": "1.0", "commit": "a" * 40})
            nested = project / "src" / "nested"
            nested.mkdir(parents=True)
            result = subprocess.run([sys.executable, str(cli), "run", "--runtime", "android", "--project", str(nested),
                "--offline", "--home", str(root / "home"), "--manifest", str(manifest),
                "--output", str(root / "offline-run.json")], capture_output=True, env=dict(os.environ, JADIX_TEST_RECEIPT=str(receipt)))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(receipt.read_bytes())["args"], ["--project", str(nested.resolve()), "--offline"])


if __name__ == "__main__":
    unittest.main()
