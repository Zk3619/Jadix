import importlib.util
import subprocess
import unittest
from pathlib import Path

specification = importlib.util.spec_from_file_location("public_documents", Path(__file__).resolve().parents[1] / "checks/check-public-docs.py")
documents = importlib.util.module_from_spec(specification)
specification.loader.exec_module(documents)


class PublicDocumentationContracts(unittest.TestCase):
    def test_bilingual_entry_and_authoritative_docs_are_distributable(self):
        root = Path(__file__).resolve().parents[2]
        paths = [root / "README.zh.md", *(root / "docs").glob("*.md")]
        self.assertTrue(all(path.is_file() for path in paths))
        result = subprocess.run(["git", "check-ignore", "--no-index", "--", *(str(path.relative_to(root)) for path in paths)], cwd=root, stdout=subprocess.PIPE, check=False)
        self.assertEqual(result.returncode, 1)

    def test_private_references_and_internal_stages_are_reported(self):
        issues = documents.documentation_issues("manual.md", "# API\nSee development-docs/plan.md\nR5 verification pending\n")
        self.assertEqual([(issue["line"], issue["reason"]) for issue in issues], [(2, "private_development_document_reference"), (3, "internal_execution_stage")])

    def test_public_debug_features_and_source_provenance_remain_valid(self):
        text = "# Debug hot reload\nTrusted local development only.\nSource revision: c9ee2bff.\nGPU register R32Uint; Apache-2.0.\n"
        self.assertEqual(documents.documentation_issues("manual.md", text), [])


if __name__ == "__main__":
    unittest.main()
