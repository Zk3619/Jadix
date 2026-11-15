import argparse
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_validation


class ValidationTests(unittest.TestCase):
    def arguments(self):
        return argparse.Namespace(suite=["python"], timeout=30)

    def execute(self, directory, result=0, identities=None):
        source = identities or [{"source": "unchanged"}] * 2
        with patch.object(native_validation, "source_identity", side_effect=source), \
                patch.object(native_validation.subprocess, "run", return_value=argparse.Namespace(returncode=result)):
            return native_validation.run(self.arguments(), directory / "checks.json")

    def report(self, directory, observations):
        path = directory / "checks.json"
        path.write_text(json.dumps({"schema": 1, "operation": "validate", "runtime_acceptance": False,
                                    "observations": observations}), encoding="utf-8")
        return path

    def test_batch_retains_logs_and_only_accepts_selected_engineering_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            result = self.execute(directory)
            self.assertTrue(result["checks_passed"])
            self.assertFalse(result["engineering_signoff"])
            self.assertFalse(result["runtime_acceptance"])
            self.assertEqual([check["suite"] for check in result["checks"]], ["python"])
            path = self.report(directory, result)
            with patch.object(native_validation, "source_identity", return_value={"source": "unchanged"}):
                self.assertTrue(native_validation.verify(path)["integrity_verified"])
            (directory / "checks-files/python.log").write_text("changed", encoding="utf-8")
            with patch.object(native_validation, "source_identity", return_value={"source": "unchanged"}):
                with self.assertRaisesRegex(ValueError, "log differs"):
                    native_validation.verify(path)

    def test_failure_source_drift_and_overclaim_cannot_be_accepted(self):
        for status, sources in [(7, None), (0, [{"source": "before"}, {"source": "after"}])]:
            with self.subTest(status=status), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                result = self.execute(directory, status, sources)
                self.assertFalse(result["checks_passed"])
                with self.assertRaisesRegex(ValueError, "failed or overclaimed"):
                    native_validation.verify(self.report(directory, result))
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            result = self.execute(directory)
            result["engineering_signoff"] = True
            with self.assertRaisesRegex(ValueError, "overclaimed"):
                native_validation.verify(self.report(directory, result))

    def test_launch_failure_is_retained_and_later_suites_still_run(self):
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(native_validation, "source_identity", return_value={"source": "same"}), \
                patch.object(native_validation.subprocess, "run", side_effect=OSError("missing tool")):
            arguments = argparse.Namespace(suite=["python", "portable"], timeout=30)
            result = native_validation.run(arguments, Path(temporary) / "checks.json")
            self.assertFalse(result["checks_passed"])
            self.assertEqual(len(result["checks"]), 2)
            self.assertTrue(all("missing tool" in check["error"] for check in result["checks"]))

    def test_uikit_profiles_are_distinct_bounded_mock_abi_checks(self):
        for application, expected in [("controls", "uikit,extended"), ("notes", "uikit,notes"),
                                      ("selection", "uikit,selection"), ("popover", "uikit,popover")]:
            suite = "uikit-" + application
            [(name, command)] = native_validation.commands([suite])
            self.assertEqual(name, suite)
            self.assertEqual(command[command.index("--features") + 1], expected)
            self.assertEqual(command[-1], "--lib")

    def test_refuses_duplicate_suites_and_nested_source_evidence(self):
        with self.assertRaisesRegex(ValueError, "only once"):
            native_validation.commands(["python", "python"])
        with self.assertRaisesRegex(ValueError, "source directories"):
            native_validation.run(self.arguments(), native_validation.native.ROOT / "scripts" / "checks.json")
        if os.name == "nt":
            with self.assertRaisesRegex(ValueError, "already includes"):
                native_validation.commands(["portable", "windows"])


if __name__ == "__main__":
    unittest.main()
