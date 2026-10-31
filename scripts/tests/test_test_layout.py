import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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

    def test_maintained_fork_under_crates_preserves_upstream_test_layout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("jadix_core", "jadix_fork"):
                directory = root / "crates" / name
                (directory / "src").mkdir(parents=True)
                (directory / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\n', encoding="utf-8")
            (root / "crates/jadix_fork/src/lib.rs").write_text("#[test]\nfn upstream_contract() {}\n", encoding="utf-8")
            (root / "vendor").mkdir()
            (root / "vendor/RELEASE_FORKS.json").write_text(json.dumps({"forks": [{"directory": "crates/jadix_fork"}]}), encoding="utf-8")
            output = root / "layout.json"
            with mock.patch.object(layout, "__file__", str(root / "scripts/checks/check-test-layout.py")), mock.patch("sys.argv", ["check-test-layout.py", "--output", str(output)]), contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit) as exit_status:
                layout.main()
            self.assertEqual(exit_status.exception.code, 0)
            result = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual([record["crate"] for record in result["own_crates"]], ["jadix_core"])
            self.assertEqual([record["crate"] for record in result["maintained_forks"]], ["jadix_fork"])
            self.assertEqual(result["maintained_forks"][0]["policy"], "preserve_upstream_test_layout")
            self.assertEqual(result["issues"], [])


if __name__ == "__main__":
    unittest.main()
