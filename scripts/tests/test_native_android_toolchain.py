import importlib.util
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import native_android
SPEC = importlib.util.spec_from_file_location("android_builder", SCRIPTS.parent / "examples/native-controls/build-android.py")
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)


class AndroidToolchainTests(unittest.TestCase):
    def test_installed_sdk_selection_and_explicit_pin(self):
        with tempfile.TemporaryDirectory() as temporary:
            sdk = Path(temporary)
            for api in [35, 36, 37]:
                directory = sdk / "platforms" / f"android-{api}"
                directory.mkdir(parents=True)
                (directory / "android.jar").touch()
            for version in ["35.0.0", "36.0.0", "36.1.0", "37.0.0-rc1"]:
                (sdk / "build-tools" / version).mkdir(parents=True)
            self.assertEqual(native_android.select_sdk(sdk), (37, "36.1.0"))
            self.assertEqual(native_android.select_sdk(sdk, 36, "36.0.0"), (36, "36.0.0"))
            for api in [35, 38]:
                with self.assertRaises(ValueError):
                    native_android.select_sdk(sdk, api)

    def test_publisher_key_and_password_references(self):
        with tempfile.TemporaryDirectory() as temporary:
            key = Path(temporary) / "publisher.jks"
            key.touch()
            with self.assertRaises(ValueError):
                native_android.signing_options(None, None, None, None, True, {})
            with self.assertRaises(ValueError):
                native_android.signing_options(None, "publisher", None, None, False, {})
            with self.assertRaises(ValueError):
                native_android.signing_options(key, "publisher", "STORE_SECRET", None, True, {})
            options = native_android.signing_options(key, "publisher", "STORE_SECRET", "KEY_SECRET", True,
                                                     {"STORE_SECRET": "sensitive", "KEY_SECRET": "private"})
            self.assertEqual(options, ["--ks-pass", "env:STORE_SECRET", "--ks-key-alias", "publisher", "--key-pass", "env:KEY_SECRET"])

    def test_production_manifest_preserves_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "source.xml"
            original = '<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.example.app"><application android:debuggable="true"><activity android:name=".Main"><intent-filter><category android:name="android.intent.category.LAUNCHER"/></intent-filter></activity></application></manifest>'
            source.write_text(original, encoding="utf-8")
            result = BUILDER.configured_manifest(source, Path(temporary) / "production.xml",
                                                 {"version_code": 42, "version_name": "2.1", "url_schemes": ["example"]}, True)
            tree = ET.parse(result).getroot()
            android = "{http://schemas.android.com/apk/res/android}"
            self.assertEqual(tree.find("application").get(android + "debuggable"), "false")
            self.assertEqual(tree.get(android + "versionCode"), "42")
            self.assertEqual(tree.find("application/activity/intent-filter/data").get(android + "scheme"), "example")
            self.assertEqual(source.read_text(encoding="utf-8"), original)

    def test_certificate_preflight_with_real_keytool(self):
        java = Path(os.environ.get("JAVA_HOME", "missing"))
        tool = java / "bin" / ("keytool.exe" if os.name == "nt" else "keytool")
        if not tool.is_file():
            self.skipTest("JDK not installed")
        with tempfile.TemporaryDirectory() as temporary:
            key = Path(temporary) / "publisher.jks"
            environment = dict(os.environ, JADIX_TEST_STORE_PASSWORD="temporary-fixture-password")
            from unittest.mock import patch
            with patch.dict(os.environ, environment):
                subprocess.run([str(tool), "-genkeypair", "-keystore", str(key), "-alias", "publisher",
                                "-storepass:env", "JADIX_TEST_STORE_PASSWORD", "-dname", "CN=Fixture",
                                "-keyalg", "RSA", "-validity", "1"], check=True, capture_output=True,
                               creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                identity = native_android.signing_identity(java, key, "publisher", "JADIX_TEST_STORE_PASSWORD")
                self.assertEqual(identity["alias"], "publisher")
                self.assertEqual(len(identity["certificate_sha256"]), 64)
                with self.assertRaises(ValueError):
                    native_android.signing_identity(java, key, "missing", "JADIX_TEST_STORE_PASSWORD")
                os.environ["JADIX_TEST_STORE_PASSWORD"] = "wrong-password"
                with self.assertRaises(ValueError) as error:
                    native_android.signing_identity(java, key, "publisher", "JADIX_TEST_STORE_PASSWORD")
                self.assertNotIn("wrong-password", str(error.exception))
            home = Path(temporary) / "distribution"
            development = native_android.development_keystore(java, home)
            original = development.read_bytes()
            self.assertEqual(native_android.development_keystore(java, home), development)
            self.assertEqual(development.read_bytes(), original)
            self.assertEqual(native_android.signing_identity(java, development, "jadix-debug")["alias"], "jadix-debug")


if __name__ == "__main__":
    unittest.main()
