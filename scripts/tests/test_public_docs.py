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

    def test_maintainer_commands_and_development_records_are_reported(self):
        text = "cargo publish --registry crates-io\ncargo login\ncargo owner --list\ncargo yank --version 0.1.0\ncargo test -p jadix\npython scripts/release/check-r5-registry.py\n./scripts/checks/check-features.ps1\n.theme-evidence/run/results.json\n发布候选与待发布版本\n"
        issues = documents.documentation_issues("manual.md", text)
        self.assertEqual([issue["line"] for issue in issues if issue["reason"] == "maintainer_operation_in_public_docs"], list(range(1, 8)))
        self.assertEqual([issue["line"] for issue in issues if issue["reason"] == "development_record_in_public_docs"], [8, 9])

    def test_application_build_install_and_runtime_instructions_remain_valid(self):
        text = "cargo build -p gallery --release\ncargo run -p gallery\ncargo install jadix_hot_reload --features cli\ncargo add jadix\nUpstream source: https://crates.io/crates/wgpu\nUse Debug hot reload while developing your application.\n"
        self.assertEqual(documents.documentation_issues("manual.md", text), [])

    def test_upstream_history_is_preserved_and_jadix_notes_follow_public_policy(self):
        self.assertEqual(documents.documentation_issues("crates/jadix_wgpu/upstream/naga/GUIDE.md", "cargo test\n"), [])
        self.assertEqual(documents.documentation_issues("crates/jadix_winit/src/changelog/v0.30.md", "cargo test\n"), [])
        issues = documents.documentation_issues("crates/jadix_winit/upstream/backend/JADIX_PATCH.md", "cargo test\n")
        self.assertEqual(issues[0]["reason"], "maintainer_operation_in_public_docs")

    def test_package_readme_maintainer_boilerplate_is_reported(self):
        issues = documents.documentation_issues("crates/library/README.md", "Documentation covers testing and releases.\n项目文档说明测试和发布。保留测试发现入口。\n")
        self.assertEqual([issue["line"] for issue in issues], [1, 2])


if __name__ == "__main__":
    unittest.main()
