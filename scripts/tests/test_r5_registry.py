import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


specification = importlib.util.spec_from_file_location("registry", Path(__file__).resolve().parents[1] / "release/check-r5-registry.py")
registry = importlib.util.module_from_spec(specification)
specification.loader.exec_module(registry)


class RegistryPreflight(unittest.TestCase):
    def inspect(self, responses, credentials=None):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            closure = root / "closure.json"
            output = root / "report.json"
            closure.write_text(json.dumps({"packages": [{"id": "fixture", "name": "jadix_fixture", "version": "0.1.0", "manifest": "Cargo.toml"}], "publication_order": ["fixture"]}), encoding="utf-8")
            if credentials is not None:
                (root / "credentials.toml").write_text(credentials, encoding="utf-8")
            captured = io.StringIO()
            with mock.patch.dict(os.environ, {"CARGO_HOME": str(root)}, clear=True), mock.patch.object(sys, "argv", ["check-r5-registry.py", "--closure", str(closure), "--output", str(output)]), mock.patch.object(registry, "windows_credential_present", return_value=False), mock.patch.object(registry, "request", side_effect=responses) as request, contextlib.redirect_stdout(captured):
                status = registry.main()
            report = json.loads(output.read_text(encoding="utf-8"))
            return status, report, captured.getvalue(), request.call_args_list

    def test_missing_credential_blocks_without_claiming_permission(self):
        status, report, _, calls = self.inspect([{"crates": [], "meta": {"total": 0}}])
        self.assertEqual(status, 1)
        self.assertFalse(report["credential_present"])
        self.assertFalse(report["publication_order"][0]["existing"])
        self.assertIn("crates_io_publish_credential_not_found", report["blockers"])
        self.assertEqual(report["uploads_performed"], 0)
        self.assertEqual(len(calls), 1)

    def test_existing_version_and_owner_are_observed_without_overwrite(self):
        status, report, _, _ = self.inspect([{"crates": [{"id": "jadix_fixture"}], "meta": {"total": 1}}, {"versions": [{"num": "0.1.0"}]}, {"users": [{"login": "fixture-owner"}]}])
        self.assertEqual(status, 1)
        package = report["publication_order"][0]
        self.assertTrue(package["version_already_published"])
        self.assertEqual(package["owners"], ["fixture-owner"])
        self.assertFalse(package["permission_verified"])
        self.assertIn("planned_version_already_published_requires_content_identity_review", report["blockers"])

    def test_credential_presence_never_leaks_token_or_proves_upload_rights(self):
        token = "fixture-secret-never-log"
        _, report, output, _ = self.inspect([{"crates": [], "meta": {"total": 0}}], '[registry]\ntoken = "' + token + '"\n')
        self.assertTrue(report["credential_present"])
        self.assertNotIn(token, json.dumps(report) + output)
        self.assertFalse(report["ready_to_publish"])
        self.assertIn("publish_permission_not_authenticated", report["blockers"])

    def test_network_failure_is_unknown_not_name_availability(self):
        status, report, _, _ = self.inspect([OSError("offline fixture")])
        self.assertEqual(status, 1)
        self.assertIsNone(report["publication_order"][0]["existing"])
        self.assertIn("registry_query_failed", report["blockers"])


if __name__ == "__main__":
    unittest.main()
