import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class FontDistributionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if shutil.which("rustc") is None:
            raise unittest.SkipTest("rustc is required for the build-script contract")
        cls.temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.root = Path(cls.temporary.name)
        cls.executable = cls.root / ("font-build.exe" if os.name == "nt" else "font-build")
        subprocess.run(["rustc", "--edition=2021", str(ROOT / "crates/jadix/build.rs"), "-o", str(cls.executable)], check=True, capture_output=True)

    def run_build(self, features=(), directory=None):
        environment = {key: value for key, value in os.environ.items() if not key.startswith("CARGO_FEATURE_") and key != "JADIX_BUNDLED_FONT_DIR"}
        environment["CARGO_MANIFEST_DIR"] = str(self.root / "package-without-assets")
        if directory is not None:
            environment["JADIX_BUNDLED_FONT_DIR"] = str(directory)
        for feature in features:
            environment["CARGO_FEATURE_" + feature] = "1"
        return subprocess.run([str(self.executable)], env=environment, capture_output=True, text=True, encoding="utf-8")

    def test_default_build_requires_no_packaged_fonts(self):
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cargo:rustc-env=JADIX_BUNDLED_FONT_DIR=", result.stdout)

    def test_requested_missing_fonts_fail_with_configuration_guidance(self):
        result = self.run_build(["BUNDLED_CJK"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("NotoSansCJK.ttc", result.stderr)
        self.assertIn("JADIX_BUNDLED_FONT_DIR", result.stderr)

    def test_explicit_directory_covers_every_embedding_group(self):
        names = ["NotoSansCJK.ttc", "NotoSerifCJK.ttc", "NotoColorEmoji_WindowsCompatible.ttf", "NotoColorEmoji.ttf", "NotoSansSymbols-wght.ttf", "NotoSansSymbols2-Regular.ttf", "NotoSansMath-Regular.ttf", "NotoMusic-Regular.ttf"]
        directory = self.root / "fixture-fonts"
        directory.mkdir(exist_ok=True)
        for name in names:
            (directory / name).write_bytes(b"build-script path fixture; not font-rendering evidence")
        result = self.run_build(["BUNDLED_CJK", "BUNDLED_CJK_SERIF", "BUNDLED_EMOJI", "BUNDLED_SYMBOLS"], directory)
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in names:
            self.assertIn(str(directory / name), result.stdout)


if __name__ == "__main__":
    unittest.main()
