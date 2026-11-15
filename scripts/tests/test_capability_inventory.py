"""Guard against the historical Audio/Video omission and ID drift."""
import importlib.util
import csv
from pathlib import Path
import tempfile
import unittest

SOURCE=Path(__file__).resolve().parents[1]/"checks/check-capability-inventory.py"
SPEC=importlib.util.spec_from_file_location("capability_inventory",SOURCE)
MODULE=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

class InventoryTests(unittest.TestCase):
    def test_current_and_incomplete_observations(self):
        ledger=MODULE.LEDGER.read_text(encoding="utf-8")
        terminal=MODULE.TERMINAL.read_text(encoding="utf-8")
        self.assertEqual(MODULE.check(ledger,terminal)["main_ids"],137)
        for broken in [ledger.replace("AudioPlayer", "Streaming"),
                       ledger.replace("| P01 |", "| P99 |", 1),
                       ledger.replace('<a id="media-detail"></a>', "")]:
            with self.assertRaises(ValueError):
                MODULE.check(broken,terminal)
        with self.assertRaises(ValueError):
            MODULE.check(ledger,terminal,{"tree":[],"truncated":True})
        with self.assertRaises(ValueError):
            MODULE.check(ledger,terminal.replace("| T42 |", "| T43 |"))

    def test_current_followup71_path_dispositions_match_complete_receipt(self):
        receipt=MODULE.ROOT/"Reference/reports/followup71/local-report.json"
        audit=MODULE.check_path_dispositions(MODULE.PATH_DISPOSITIONS,receipt)
        self.assertEqual(audit["paths"],222)
        self.assertEqual(audit["unique_paths"],222)
        self.assertEqual(audit["review_statuses"]["mapped"],4)
        self.assertEqual(audit["review_statuses"]["partial"],53)
        self.assertEqual(audit["review_statuses"]["not-applicable"],24)
        self.assertEqual(audit["review_statuses"]["test-only"],14)
        self.assertEqual(audit["review_statuses"]["unreviewed"],76)
        self.assertEqual(audit["review_statuses"]["docs-only"],50)
        self.assertEqual(audit["review_statuses"]["build-only"],1)
        self.assertEqual(audit["impacts"]["shared-runtime"],30)
        ledger=MODULE.LEDGER.read_text(encoding="utf-8")
        self.assertIn("## 140. 2026-11-13 Tooltip/HoverCard 悬停打开延迟合同复验",ledger)
        self.assertIn("配置 `openDelay`",ledger)
        self.assertIn("Rust Builder 对应 `.open_delay_ms(ms)`",ledger)
        self.assertIn("## 139. 2026-11-13 followup71 Tooltip 路径差异与存量复核",ledger)
        self.assertIn("仍有 76 条 `unreviewed`",ledger)

    def test_live_nsh_path_dispositions_match_latest_local_git_receipt(self):
        receipt=MODULE.ROOT/"Reference/reports/nsh-live-20261113-followup73/local-report.json"
        dispositions=MODULE.ROOT/"development-docs/comparison/nsh-live-20261113-path-dispositions.csv"
        audit=MODULE.check_path_dispositions(dispositions,receipt)
        self.assertEqual(audit["paths"],419)
        self.assertEqual(audit["review_statuses"]["unreviewed"],263)
        self.assertEqual(audit["review_statuses"]["partial"],62)
        self.assertEqual(audit["review_statuses"]["mapped"],4)
        ledger=MODULE.LEDGER.read_text(encoding="utf-8")
        self.assertIn("## 147. 2026-11-13 NSH live main 全路径增量与未登记差异",ledger)
        self.assertIn("新增 32 条路径",ledger)

    def test_followup63_remains_a_historical_snapshot(self):
        historical=MODULE.ROOT/"development-docs/comparison/nsh-followup63-path-dispositions.csv"
        receipt=MODULE.ROOT/"Reference/reports/followup63/local-report.json"
        audit=MODULE.check_path_dispositions(historical,receipt)
        self.assertEqual(audit["paths"],202)
        self.assertEqual(audit["review_statuses"]["unreviewed"],78)
        ledger=MODULE.LEDGER.read_text(encoding="utf-8")
        self.assertIn("## 115. 2026-11-12 followup63 全路径与未入册存量差异审计",ledger)
        self.assertIn("仍有 98 条 `unreviewed`",ledger)
        self.assertIn("Mini Program 主线 `7bb46ef32dbb96bec0ad6f09d29e69b4a9140a3c` 本轮为 0 commits／0 paths",ledger)
        branch=MODULE.ROOT/"development-docs/comparison/nsh-branch-candidates-20261112.csv"
        with branch.open(encoding="utf-8-sig",newline="") as stream:
            candidates=list(csv.DictReader(stream))
        self.assertEqual(len(candidates),25)
        self.assertEqual(len({row["source_path"] for row in candidates}),25)
        self.assertTrue(all(row["branch"]=="origin/agent/T-9164-ios-native-controls" for row in candidates))
        self.assertTrue(all(row["base_head"]=="c1a73bfaef4c88d50488a679ac6bf19840977ebe" for row in candidates))
        self.assertTrue(all(row["source_head"]=="1489deb35c0d82059db5060f200b0ad2dc9133d7" for row in candidates))
        self.assertIn("## 116. 2026-11-12 NSH 候选分支与存量差异续审",ledger)

    def test_mapped_path_requires_capability_and_evidence(self):
        with MODULE.PATH_DISPOSITIONS.open(encoding="utf-8-sig",newline="") as stream:
            reader=csv.DictReader(stream)
            rows=list(reader)
            columns=reader.fieldnames
        rows[0]["review_status"]="mapped"
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"path-dispositions.csv"
            with path.open("w",encoding="utf-8",newline="") as stream:
                writer=csv.DictWriter(stream,fieldnames=columns)
                writer.writeheader()
                writer.writerows(rows)
            with self.assertRaisesRegex(ValueError,"lacks capability IDs or evidence"):
                MODULE.check_path_dispositions(path)

    def test_partial_path_requires_a_residual_difference_reason(self):
        with MODULE.PATH_DISPOSITIONS.open(encoding="utf-8-sig",newline="") as stream:
            reader=csv.DictReader(stream)
            rows=list(reader)
            columns=reader.fieldnames
        row=next(row for row in rows if row["review_status"]=="partial")
        row["disposition_reason"]=""
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"path-dispositions.csv"
            with path.open("w",encoding="utf-8",newline="") as stream:
                writer=csv.DictWriter(stream,fieldnames=columns)
                writer.writeheader()
                writer.writerows(rows)
            with self.assertRaisesRegex(ValueError,"partial path lacks a residual-difference reason"):
                MODULE.check_path_dispositions(path)

if __name__=="__main__":
    unittest.main()
