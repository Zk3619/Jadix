import importlib.util
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("ci_workflow", ROOT / "scripts/checks/check-ci-workflow.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class WorkflowContracts(unittest.TestCase):
    def inspect(self, command):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text("[workspace]\nmembers=[]\n", encoding="utf-8")
            package = root / "crates/fixture"
            package.mkdir(parents=True)
            (package / "Cargo.toml").write_text('[package]\nname="jadix_fixture"\nversion="0.0.0"\n[features]\nknown=[]\n', encoding="utf-8")
            workflow = root / "ci.yml"
            workflow.write_text("name: fixture\non: push\njobs:\n  test:\n    steps:\n      - run: " + command + "\n", encoding="utf-8")
            return MODULE.inspect_workflow(root, workflow)

    def test_renamed_packages_fail_before_any_build(self):
        self.assertEqual(self.inspect("cargo test -p old-fixture")["issues"][0]["kind"], "unknown-package")
        self.assertTrue(self.inspect("cargo test -p jadix_fixture --features known")["passed"])

    def test_unknown_features_and_missing_targets_fail(self):
        self.assertEqual(self.inspect("cargo test -p jadix_fixture --features absent")["issues"][0]["kind"], "unknown-feature")
        self.assertEqual(self.inspect("cargo test -p jadix_fixture --test vanished")["issues"][0]["kind"], "missing-target")

    def test_missing_checkout_script_fails(self):
        self.assertEqual(self.inspect("python scripts/checks/vanished.py")["issues"][0]["kind"], "script-unavailable-after-checkout")

    def test_existing_untracked_script_is_rejected_in_checkout_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text("[workspace]\nmembers=[]\n", encoding="utf-8")
            script = root / "scripts/checks/local.py"
            script.parent.mkdir(parents=True)
            script.write_text("", encoding="utf-8")
            workflow = root / "ci.yml"
            workflow.write_text("jobs:\n  test:\n    steps:\n      - run: python scripts/checks/local.py\n", encoding="utf-8")
            self.assertTrue(MODULE.inspect_workflow(root, workflow)["passed"])
            self.assertFalse(MODULE.inspect_workflow(root, workflow, set())["passed"])
            self.assertTrue(MODULE.inspect_workflow(root, workflow, {"scripts/checks/local.py"})["passed"])


if __name__ == "__main__":
    unittest.main()
