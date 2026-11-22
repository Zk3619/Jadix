import json
import hashlib
import zipfile
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native
import native_linux as linux
import native_workflow as workflow


class LinuxBundleTests(unittest.TestCase):
    def test_doctor_requires_curl_even_when_native_libraries_are_present(self):
        with mock.patch.object(linux.sys, "platform", "linux"), \
                mock.patch.object(linux.shutil, "which", side_effect=lambda name: "/usr/bin/pkg-config" if name == "pkg-config" else None), \
                mock.patch.object(linux.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, stdout="99.0\n")):
            observation = linux.doctor()
        self.assertTrue(all(package["available"] for package in observation["packages"].values()))
        self.assertIsNone(observation["curl"])
        self.assertFalse(observation["available"])

    def config(self):
        return linux.configuration({"id": "org.jadix.Test", "name": "Test", "version": "0.1.0",
                                    "executable": "bin/test", "schemes": ["jadix-test"]})

    def artifact(self, directory):
        root = Path(directory)
        config = self.config()
        executable = root / config["executable"]
        executable.parent.mkdir(parents=True)
        header = bytearray(20)
        header[:6] = b"\x7fELF\x02\x01"
        header[16:18] = (3).to_bytes(2, "little")
        header[18:20] = (62).to_bytes(2, "little")
        executable.write_bytes(header)
        executable.chmod(0o755)
        (root / "jadix-native-linux.json").write_text(json.dumps(config), encoding="utf-8")
        (root / "install-desktop.py").write_text(linux.INSTALLER, encoding="utf-8")
        desktop = root / "share/applications/org.jadix.Test.desktop.in"
        desktop.parent.mkdir(parents=True)
        desktop.write_text(linux.desktop_entry(config), encoding="utf-8")
        return {"kind": "linux-native-build", "bundle": str(root), "executable": str(executable),
                "identity": native.file_identity(executable), "application": config,
                "files": linux.bundle_files(root)}

    def test_zip_verifier_rejects_symlinks_even_with_matching_content_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            observation=self.artifact(root / "bundle")
            build=json.dumps({"schema":1,"operation":"build","observations":observation}).encode()
            manifest={"schema":1,"kind":"linux-native-package","signed":False,
                      "files":observation["files"],
                      "build_report":{"bytes":len(build),"sha256":hashlib.sha256(build).hexdigest()}}
            for malicious in [False,True]:
                package=root / ("bad.zip" if malicious else "good.zip")
                with zipfile.ZipFile(package,"w") as archive:
                    archive.writestr("manifest.json",json.dumps(manifest))
                    archive.writestr("build.json",build)
                    for name in observation["files"]:
                        entry=zipfile.ZipInfo(name)
                        entry.create_system=3
                        mode=0o100755 if name==observation["application"]["executable"] else 0o100644
                        if malicious and name=="install-desktop.py":mode=0o120777
                        entry.external_attr=mode << 16
                        archive.writestr(entry,(root / "bundle" / name).read_bytes())
                if malicious:
                    with self.assertRaisesRegex(ValueError,"regular file modes"):workflow.verify_package(package)
                else:self.assertTrue(workflow.verify_package(package)["integrity_verified"])

    def test_rejects_escaping_paths_and_desktop_injection(self):
        for path in ["", "/bin/test", "../bin/test", "a/../test", "a//test", "a/./test", "C:evil", "a\\test", "a/"]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                linux.relative(path)
        for key, value in [("id", "org.jadix.Test\nExec=evil"), ("name", "Test\nExec=evil"),
                           ("schemes", ["bad\nscheme"]), ("schemes", ["x", "x"]),
                           ("assets", ["bin/test"]), ("assets", ["../secret"])]:
            config = self.config()
            config[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                linux.configuration(config)

    def test_checks_inventory_identity_elf_and_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            artifact = self.artifact(directory)
            executable = Path(artifact["executable"])
            self.assertEqual(linux.validate_artifact(artifact), executable)
            (Path(directory) / "undeclared").write_bytes(b"secret")
            with self.assertRaises(ValueError):
                linux.validate_artifact(artifact)
            (Path(directory) / "undeclared").unlink()
            config = dict(artifact["application"], version="0.2.0")
            metadata = Path(directory) / "jadix-native-linux.json"
            metadata.write_text(json.dumps(config), encoding="utf-8")
            artifact["files"] = linux.bundle_files(directory)
            with self.assertRaises(ValueError):
                linux.validate_artifact(artifact)
            metadata.write_text(json.dumps(artifact["application"]), encoding="utf-8")
            executable.write_bytes(b"not a Linux executable")
            artifact["identity"] = native.file_identity(executable)
            artifact["files"] = linux.bundle_files(directory)
            with self.assertRaises(ValueError):
                linux.validate_artifact(artifact)

    @unittest.skipUnless(sys.platform == "linux", "Unix file permissions and symlinks")
    def test_rejects_symlinks_and_missing_execute_permission(self):
        with tempfile.TemporaryDirectory() as directory:
            artifact = self.artifact(directory)
            executable = Path(artifact["executable"])
            executable.chmod(0o644)
            with self.assertRaises(ValueError):
                linux.validate_artifact(artifact)
            executable.chmod(0o755)
            (Path(directory) / "escape").symlink_to("/etc/passwd")
            with self.assertRaises(ValueError):
                linux.validate_artifact(artifact)

    @unittest.skipUnless(sys.platform == "linux", "Linux desktop installer")
    def test_installer_quotes_paths_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as data:
            bundle = Path(directory) / 'bundle % dollar$ quote" tick`'
            artifact = self.artifact(bundle)
            environment = dict(os.environ, XDG_DATA_HOME=data, PATH="/missing")
            installer = str(bundle / "install-desktop.py")
            first = subprocess.run([sys.executable, installer], env=environment, capture_output=True, text=True)
            self.assertEqual(first.returncode, 0, first.stderr)
            desktop = Path(data) / "applications/org.jadix.Test.desktop"
            content = desktop.read_text(encoding="utf-8")
            self.assertIn("%%", content)
            self.assertIn("\\$", content)
            self.assertIn('\\"', content)
            self.assertNotIn("@EXECUTABLE@", content)
            second = subprocess.run([sys.executable, installer], env=environment, capture_output=True)
            self.assertNotEqual(second.returncode, 0)
            self.assertEqual(desktop.read_text(encoding="utf-8"), content)


if __name__ == "__main__":
    unittest.main()
