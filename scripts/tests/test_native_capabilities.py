import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_capabilities as capabilities


class ReferenceContractTests(unittest.TestCase):
    def test_foundation_inventory_rejects_a_reference_changed_during_the_scan(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "README.md").write_text("# Before\n", encoding="utf-8")
            frozen = capabilities.native.tree_identity(root)
            changed = {**frozen, "manifest_sha256": "changed"}
            with patch.object(capabilities.native, "tree_identity", side_effect=[frozen, changed]):
                with self.assertRaisesRegex(ValueError, "reference sources changed"):
                    capabilities.foundation_ledger(root)

    def test_foundations_do_not_require_component_exports_and_retain_features_and_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            guide = root / "website/zh-CN/docs/reactivity.md"
            guide.parent.mkdir(parents=True)
            guide.write_text("# Reactivity\n## Owner\n```md\n## Fake\n```\n", encoding="utf-8")
            manifest = root / "crates/nsh-core/Cargo.toml"
            manifest.parent.mkdir(parents=True)
            manifest.write_text('[package]\nname = "nsh-core"\nversion = "0.4.0"\n'
                                '[features]\ndefault = []\ntrace = []\n'
                                '[target.\'cfg(windows)\'.dependencies]\nwindows = "0.61"\n', encoding="utf-8")
            with patch.object(capabilities.native, "ROOT", root):
                report = capabilities.foundation_ledger(root)
                self.assertEqual(report["document_count"], 1)
                self.assertEqual([row["title"] for row in report["documents"][0]["headings"]],
                                 ["Reactivity", "Owner"])
                package = report["reference_packages"][0]
                self.assertEqual(package["features"]["trace"], [])
                self.assertIn("cfg(windows)", package["target_dependencies"])
                self.assertFalse(package["runtime_observed"])
                self.assertEqual(report["reviewed_count"], 0)
                self.assertEqual(report["documents"][0]["parity"], "unknown")
                before = report["documents"][0]["identity"]
                guide.write_text("# Changed\n", encoding="utf-8")
                self.assertNotEqual(before, capabilities.foundation_ledger(root)["documents"][0]["identity"])

    def test_runtime_conditions_remain_unknown_and_unrecognized_mutations_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "host.rs"
            body = """fn capabilities(&self) -> BackendCapabilities {
                let mut node_kinds = vec![NodeKind::Button, NodeKind::Input];
                if unsafe { jadix_ohos_dialog_supported() } {
                    node_kinds.push(NodeKind::Dialog);
                    node_kinds.push(NodeKind::Popover);
                    node_kinds.push(NodeKind::Select);
                }
                BackendCapabilities { node_kinds, focus: true }
            }
            fn measure() {}
            """
            source.write_text(body, encoding="utf-8")
            self.assertEqual(capabilities.declared_kinds(source), {
                "unconditional": {"button", "input"},
                "conditional": {kind: "unsafe { jadix_ohos_dialog_supported() }"
                                for kind in ["dialog", "popover", "select"]},
            })
            source.write_text(body.replace("if unsafe", "if !unsafe"), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "unsupported native capability condition"):
                capabilities.declared_kinds(source)
            source.write_text(body.replace("NodeKind::Select", "NodeKind::Dialog"), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "ambiguous native capability condition"):
                capabilities.declared_kinds(source)
            source.write_text(body.replace("node_kinds.push(NodeKind::Dialog);", "node_kinds.clear();"), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "unsupported native capability condition"):
                capabilities.declared_kinds(source)

    def test_pipes_inside_code_and_escaped_pipes_preserve_cells(self):
        self.assertEqual(capabilities.table_cells(r"| style | `A | B` | `a\|b` |"),
                         ["style", "`A | B`", r"`a\|b`" ])
        self.assertEqual(capabilities.table_cells("| a | ``x ` | y`` |"),
                         ["a", "``x ` | y``"])
        self.assertEqual(capabilities.table_cells(r"| a\|b | value |"), [r"a\|b", "value"])
        self.assertEqual(capabilities.table_cells("| `cmd+shift+\\` | `ctrl+shift+\\` | pair |"),
                         ["`cmd+shift+\\`", "`ctrl+shift+\\`", "pair"])
        self.assertIsNone(capabilities.table_cells("ordinary prose"))

    def test_contract_retains_lines_and_hash_and_ignores_fenced_examples(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "Button.md"
            text = "---\ntitle: Button\n---\n# Button\n## 属性\n| Name | Type |\n| --- | --- |\n| value | `A | B` |\n```md\n## misleading\n| Fake | API |\n| --- | --- |\n| wrong | data |\n```\n## 事件\n| Name | Rust |\n| --- | --- |\n| onPress | .on_press |\n"
            source.write_text(text, encoding="utf-8")
            contract = capabilities.documented_contract(source, root)
            self.assertEqual(contract["title"], "Button")
            self.assertEqual([table["line"] for table in contract["tables"]], [6, 16])
            self.assertEqual(contract["tables"][0]["rows"],
                             [{"line": 8, "cells": ["value", "`A | B`"]}])
            self.assertEqual(contract["tables"][1]["headings"][-1]["title"], "事件")
            self.assertFalse(contract["runtime_observed"])
            self.assertFalse(contract["reviewed"])
            source.write_text(text + "changed", encoding="utf-8")
            self.assertNotEqual(contract["identity"], capabilities.documented_contract(source, root)["identity"])

    def test_ambiguous_table_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "broken.md"
            source.write_text("---\ntitle: Broken\n---\n| Name | Type |\n| --- | --- |\n| wrong | extra | cell |\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "ambiguous contract table"):
                capabilities.documented_contract(source, root)

    def test_exact_title_matching_does_not_claim_compound_part_parity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference = root / "reference"
            source = reference / "crates/nsh-base/src/lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                'assert_eq!(components, ["Button", "ButtonPart"]);\n'
                'assert_eq!(modules, ["storage"]);\n'
                'assert_eq!(plot, ["Line"]);\n'
                'assert_eq!(bare.native_value_names(), ["rgb"]);\n', encoding="utf-8")
            documents = reference / "website/zh-CN/elements"
            documents.mkdir(parents=True)
            (documents / "button.md").write_text(
                "---\ntitle: Button\n---\n## Props\n| TSX | Rust |\n| --- | --- |\n| label | .label |\n", encoding="utf-8")
            modules = reference / "website/zh-CN/modules"
            modules.mkdir()
            (modules / "nsh.md").write_text("", encoding="utf-8")
            schemas = root / "crates/jadix_components/schema"
            schemas.mkdir(parents=True)
            (schemas / "portable-v1.json").write_text(
                json.dumps([{"name": "Button", "kind": "button"}]), encoding="utf-8")
            (schemas / "extended-v1.json").write_text("[]", encoding="utf-8")
            platforms = root / "crates/jadix_native/src"
            platforms.mkdir(parents=True)
            for platform in ["windows", "ohos"]:
                (platforms / f"{platform}.rs").write_text(
                    "fn capabilities() { node_kinds: vec![NodeKind::Button] }", encoding="utf-8")
            (schemas / "extended-v1.json").write_text(
                json.dumps([{"name": "ButtonPart", "kind": "dialog"}]), encoding="utf-8")
            (platforms / "ohos.rs").write_text(
                "fn capabilities() { let mut node_kinds = vec![NodeKind::Button]; "
                "if unsafe { jadix_ohos_dialog_supported() } { node_kinds.push(NodeKind::Dialog); } "
                "BackendCapabilities { node_kinds, focus: true } }", encoding="utf-8")
            with patch.object(capabilities.native, "ROOT", root):
                report = capabilities.ledger(reference)
            self.assertEqual(report["source_contract_record_count"], 1)
            self.assertEqual(report["source_contract_document_count"], 1)
            self.assertEqual(report["reviewed_count"], 0)
            rows = {record["name"]: record for record in report["records"]}
            self.assertTrue(rows["Button"]["reference_props_slots_events_styles"])
            self.assertIsNone(rows["ButtonPart"]["reference_props_slots_events_styles"])
            self.assertIsNone(rows["ButtonPart"]["platform_node_mapping_declared"]["ohos"])
            self.assertFalse(rows["ButtonPart"]["platform_node_mapping_declared"]["windows"])
            self.assertEqual(rows["ButtonPart"]["platform_node_mapping_conditions"],
                             {"ohos": "unsafe { jadix_ohos_dialog_supported() }"})
            for row in rows.values():
                self.assertEqual(row["status"], "unreviewed")
                self.assertEqual(row["parity"], "unknown")
                self.assertEqual(row["comparison_evidence"], [])


if __name__ == "__main__":
    unittest.main()
