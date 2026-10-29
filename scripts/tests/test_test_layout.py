import importlib.util
import tempfile
import unittest
from pathlib import Path

specification = importlib.util.spec_from_file_location("test_layout", Path(__file__).resolve().parents[1] / "checks/check-test-layout.py")
layout = importlib.util.module_from_spec(specification)
specification.loader.exec_module(layout)


class TestLayoutContracts(unittest.TestCase):
    def test_private_host_and_public_contracts_have_distinct_owners(self):
        with tempfile.TemporaryDirectory() as temporary:
            crate = Path(temporary)
            paths = ["src/runtime/tests/mod.rs", "src/crate_tests/layout.rs", "tests/contracts.rs", "tests/support/helpers.rs", "tests/golden/image.png"]
            for name in paths:
                path = crate / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("#[test]\nfn contract() {}\n", encoding="utf-8")
            record = layout.inspect_crate(crate)
            self.assertEqual(record["issues"], [])
            self.assertEqual(record["groups"]["private"], ["src/runtime/tests/mod.rs"])
            self.assertEqual(record["groups"]["public_contracts"], ["tests/contracts.rs"])
            self.assertEqual(len(record["groups"]["fixtures"]), 2)

    def test_loose_and_inline_private_tests_are_reported(self):
        with tempfile.TemporaryDirectory() as temporary:
            crate = Path(temporary)
            (crate / "src").mkdir()
            (crate / "src/runtime_tests.rs").write_text("#[test]\nfn regression() {}\n", encoding="utf-8")
            record = layout.inspect_crate(crate)
            self.assertEqual({issue["reason"] for issue in record["issues"]}, {"private_test_outside_owner_test_directory", "loose_test_source"})


if __name__ == "__main__":
    unittest.main()
