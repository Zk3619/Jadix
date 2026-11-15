"""Offline checks for fail-closed upstream evidence classification."""
import importlib.util
import csv
from pathlib import Path
import unittest
from unittest.mock import patch
import gzip
import io
import json
import hashlib
import tempfile
import zipfile
import subprocess

spec = importlib.util.spec_from_file_location("tracker", Path(__file__).parents[1] / "checks/track-nsh-upstream.py")
tracker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tracker)


class TrackerTests(unittest.TestCase):
    def test_local_reference_comparison_is_complete_and_requires_ancestor(self):
        with tempfile.TemporaryDirectory() as directory:
            repository = Path(directory) / "nsh-upstream"
            repository.mkdir()
            def run(*arguments):
                return subprocess.run(["git", "-C", str(repository), *arguments],
                                      check=True, capture_output=True, text=True).stdout.strip()
            run("init", "-q")
            run("config", "user.name", "Tracker Test")
            run("config", "user.email", "tracker@example.invalid")
            (repository / "crates").mkdir()
            (repository / "crates" / "runtime.rs").write_text("before\n")
            (repository / "crates" / "unchanged.rs").write_text("old capability\n")
            run("add", ".")
            run("commit", "-qm", "baseline")
            base = run("rev-parse", "HEAD")
            (repository / "crates" / "runtime.rs").write_text("after\n")
            (repository / "crates" / "added.rs").write_text("new\n")
            run("add", ".")
            run("commit", "-qm", "shared runtime update")
            run("branch", "-M", "main")
            run("update-ref", "refs/remotes/origin/main", "HEAD")

            report = tracker.local_reference_report(repository, base)
            self.assertEqual(report["total_commits"], 1)
            self.assertTrue(report["files_complete"])
            self.assertEqual(report["insertions"], 2)
            self.assertEqual(report["deletions"], 1)
            self.assertEqual({item["status"] for item in report["files"]}, {"added", "modified"})
            self.assertTrue(all(len(item["blob_sha"]) == 40 for item in report["files"]))
            inventory = tracker.local_source_inventory(repository, report["head"])
            self.assertEqual(inventory["entries_count"], 3)
            self.assertEqual({item["path"] for item in inventory["entries"]},
                             {"crates/runtime.rs", "crates/added.rs", "crates/unchanged.rs"})
            self.assertEqual(inventory["remote_refs_count"], 1)
            self.assertEqual(inventory["remote_refs"][0]["sha"], report["head"])
            snapshot = Path(directory) / "inventory.csv"
            tracker.write_source_inventory({"repositories": {"nsh": dict(report, source_inventory=inventory)}}, snapshot)
            with snapshot.open(newline="", encoding="utf-8") as stream:
                rows = list(csv.DictReader(stream))
            self.assertEqual(len(rows), 3)
            self.assertIn("crates/unchanged.rs", {row["source_path"] for row in rows})
            self.assertTrue(all(row["head"] == report["head"] for row in rows))
            self.assertEqual(tracker.impact("crates/nsh-base/src/menu.rs"), "shared-runtime")
            self.assertEqual(tracker.impact("crates/base/src/input/base/state.rs"), "shared-runtime")
            self.assertEqual(tracker.impact("crates/component/src/table/state.rs"), "shared-runtime")
            self.assertEqual(tracker.impact("crates/gpui/src/window.rs"), "shared-runtime")
            with self.assertRaises(FileExistsError):
                tracker.write_source_inventory({"repositories": {}}, snapshot)
            with self.assertRaises(subprocess.CalledProcessError):
                tracker.local_reference_report(repository, "HEAD", base)

    def test_recursive_trees_cover_paths_beyond_compare_cap_and_reject_truncation(self):
        before = {"tree": [{"path": "gone", "type": "blob", "sha": "a"}]}
        after = {"tree": [{"path": f"file{index}", "type": "blob", "sha": "b"} for index in range(350)]}
        changes = tracker.tree_changes(before, after)
        self.assertEqual(len(changes), 351)
        self.assertEqual(sum(item['status'] == 'removed' for item in changes), 1)
        with self.assertRaises(ValueError):
            tracker.tree_changes(before, dict(after, truncated=True))
    def test_truncated_transport_retries_without_parsing_partial_bytes(self):
        from http.client import IncompleteRead
        class Response(io.BytesIO):
            headers = {}
        with patch.object(tracker, 'urlopen', side_effect=[IncompleteRead(b'{'), Response(b'{"ok":true}')]) as request:
            self.assertEqual(tracker.get('/test'), {"ok": True})
            self.assertEqual(request.call_args.args[0].get_header('Accept-encoding'), 'identity')
        with patch.object(tracker, 'urlopen', side_effect=IncompleteRead(b'{')) as request:
            with self.assertRaises(IncompleteRead):
                tracker.get('/test')
            self.assertEqual(request.call_count, 3)
    def test_compressed_complete_response_is_decoded_without_accepting_partial_json(self):
        value = {"content": "large" * 120000}
        class Response(io.BytesIO):
            headers = {"Content-Encoding": "gzip"}
        with patch.object(tracker, 'urlopen', return_value=Response(gzip.compress(json.dumps(value).encode()))):
            self.assertEqual(tracker.get('/test'), value)
        with patch.object(tracker, 'urlopen', return_value=Response(b'partial')):
            with self.assertRaises(gzip.BadGzipFile):
                tracker.get('/test')
    def comparison(self, files):
        return {"url": f"{tracker.API}/compare/{'a'*40}...{'b'*40}",
                "base_commit": {"sha": "a"*40}, "status": "ahead", "total_commits": 2,
                "files": [{"filename": p, "status": "modified", "sha": "c"*40} for p in files]}

    def test_shared_changes_are_visible_without_terminal_files(self):
        result = tracker.summarize("a"*40, "b"*40, self.comparison(["crates/nsh-js/src/signal.rs", "website/index.md"]))
        self.assertEqual([f["impact"] for f in result["files"]], ["shared-runtime", "docs-ci-examples"])
        self.assertEqual(result["files_count"], 2)

    def test_file_cap_is_explicitly_partial_while_invalid_identity_is_rejected(self):
        capped = tracker.summarize("a"*40, "b"*40, self.comparison(["file"] * 300))
        self.assertFalse(capped["files_complete"])
        self.assertEqual(capped["files_count"], 300)
        self.assertTrue(any("omitted paths are unknown" in limit for limit in capped["limits"]))
        cases = [dict(self.comparison([]), status="diverged"),
                 dict(self.comparison([]), url="wrong"),
                 dict(self.comparison([]), base_commit={"sha": "wrong"})]
        for case in cases:
            with self.subTest(case=case["status"]), self.assertRaises(ValueError):
                tracker.summarize("a"*40, "b"*40, case)

    def test_platform_capture_checks_complete_source_blobs_and_preserves_closed_unmerged_prs(self):
        source = b"verified source\n"
        blob = hashlib.sha1(f"blob {len(source)}\0".encode() + source).hexdigest()
        tree = {"tree": [{"type": "blob", "path": "src/lib.rs", "sha": blob}]}

        def response(path, repository=tracker.REPOSITORY):
            if path == "": return {"default_branch": "main"}
            if path == "/commits/main": return {"sha": "d" * 40}
            if path.startswith("/git/trees/"): return tree
            if "?per_page" in path: return []
            return {"state": "closed", "merged": False}

        for content, path, accepted in [(source, "src/lib.rs", True),
                                        (b"tampered", "src/lib.rs", False),
                                        (source, "../escape.rs", False)]:
            archive = io.BytesIO()
            with zipfile.ZipFile(archive, "w") as bundle:
                bundle.writestr("repository-head/" + path, content)
            with tempfile.TemporaryDirectory() as directory:
                output = Path(directory)
                with patch.object(tracker, "get", side_effect=response), \
                     patch.object(tracker, "urlopen", return_value=io.BytesIO(archive.getvalue())):
                    if accepted:
                        tracker.capture_platforms(output, None)
                        manifest = json.loads((output / "platforms-manifest.json").read_text())
                        self.assertEqual(manifest["blobs"]["src/lib.rs"]["git_blob"], blob)
                        self.assertEqual((output / "miniprogram-sources/src/lib.rs").read_bytes(), source)
                        self.assertFalse(json.loads((output / "pr-474.json").read_text())["merged"])
                    else:
                        with self.assertRaises(ValueError):
                            tracker.capture_platforms(output, None)
                        self.assertFalse((output / "platforms-manifest.json").exists())

    def test_external_platform_compare_paginates_and_rejects_missing_or_duplicate_commits(self):
        repository = "huacnlee/nsh-platform-miniprogram"
        first = dict(self.comparison([]), total_commits=102,
                     url=f"https://api.github.com/repos/{repository}/compare/{'a'*40}...{'b'*40}",
                     commits=[{"sha": f"{index:040x}"} for index in range(100)])
        second = [{"sha": f"{index:040x}"} for index in range(100, 102)]
        with patch.object(tracker, "get", side_effect=[first, {"commits": second}]) as request:
            result = tracker.complete_comparison("a"*40, "b"*40, repository)
            self.assertEqual(len(result["commits"]), 102)
            self.assertEqual(request.call_args.args[1], repository)
            self.assertIn("page=2", request.call_args.args[0])
        for missing in [[], first["commits"][:2]]:
            with patch.object(tracker, "get", side_effect=[first, {"commits": missing}]):
                with self.assertRaises(ValueError):
                    tracker.complete_comparison("a"*40, "b"*40, repository)


if __name__ == "__main__":
    unittest.main()
