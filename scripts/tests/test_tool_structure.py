import unittest
from pathlib import Path


class ToolStructureContracts(unittest.TestCase):
    def test_executable_tools_have_an_explicit_category(self):
        scripts = Path(__file__).resolve().parents[1]
        loose = [path.name for path in scripts.iterdir() if path.is_file() and path.suffix in {".py", ".ps1", ".sh", ".mjs", ".json"}]
        self.assertEqual(loose, [])
        for category in ("assets", "checks", "acceptance", "release", "benchmarks", "tests"):
            with self.subTest(category=category):
                self.assertTrue((scripts / category).is_dir())

    def test_tool_fixtures_and_policies_stay_with_their_owner(self):
        scripts = Path(__file__).resolve().parents[1]
        for name in ("checks/fixtures/extracted-kernel-contracts.rs", "checks/fixtures/product-distributions.rs", "checks/data/color-audit-allowlist.txt", "benchmarks/gpui-compare/Cargo.toml", "benchmarks/glyphon-bench/main.rs"):
            with self.subTest(path=name):
                self.assertTrue((scripts / name).is_file())


if __name__ == "__main__":
    unittest.main()
