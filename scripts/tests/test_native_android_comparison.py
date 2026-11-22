import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(Path(__file__).parents[1] / "acceptance"))

SPEC = importlib.util.spec_from_file_location("android_comparison", Path(__file__).parents[1] / "acceptance/native-android-comparison.py")
COMPARISON = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPARISON)


class AndroidComparisonTests(unittest.TestCase):
    def test_data_listing_rejects_columns_and_keeps_separate_exact_paths(self):
        cache = "native-state-53c25b1d-0fca-4a09-aa56-06220a3ce484.json"
        text = "jadix-notes-v1.json\n" + cache + "\ndialog-ime.png\njadix-notes-v1.json.unowned\n"
        self.assertEqual(COMPARISON.protected_names(text), ["jadix-notes-v1.json", cache])
        for invalid in ("jadix-notes-v1.json  " + cache, "jadix-notes-v1.json\t" + cache):
            with self.assertRaisesRegex(ValueError, "one filename"):
                COMPARISON.protected_names(invalid)

    def test_old_and_new_aapt_minimum_sdk_fields(self):
        for field in ("sdkVersion", "minSdkVersion"):
            text = "package: name='dev.jadix.nativeui' versionCode=''\r\nlaunchable-activity: name='dev.jadix.nativeui.MainActivity'\r\n" + field + ":'26'\r\ntargetSdkVersion:'35'\r\n"
            self.assertEqual(COMPARISON.manifest_identity(text), ["dev.jadix.nativeui", "dev.jadix.nativeui.MainActivity", "26", "35"])
        with self.assertRaises(ValueError):
            COMPARISON.manifest_identity("package: name='dev.jadix.nativeui'")

    def test_apksigner_lf_and_windows_crlf_are_parsed(self):
        fingerprint = "AB" * 32
        for ending in ("\n", "\r\n"):
            self.assertEqual(COMPARISON.signer_certificates("Signer #1 certificate SHA-256 digest: " + fingerprint + ending), [fingerprint.lower()])
        with self.assertRaises(ValueError):
            COMPARISON.signer_certificates("certificate SHA-256 digest: " + fingerprint)

    def test_release_apk_and_embedded_library_are_verified(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            apk = root / "sample.apk"
            library = b"native library fixture"
            with zipfile.ZipFile(apk, "w") as archive:
                archive.writestr("lib/x86_64/libjadix_native_controls.so", library)
            report = {"target": "x86_64-linux-android", "application": "controls", "rust_profile": "release", "debuggable": True,
                      "extended": True, "retained_rows": 1000, "cargo_lock_sha256": "a" * 64,
                      "apk_bytes": apk.stat().st_size, "apk_sha256": hashlib.sha256(apk.read_bytes()).hexdigest(),
                      "library_bytes": len(library), "library_sha256": hashlib.sha256(library).hexdigest()}
            path = root / "build.json"
            path.write_text(json.dumps(report), encoding="utf8")
            self.assertEqual(COMPARISON.artifact(apk)["path"], str(apk.resolve()))
            for key, value in (("rust_profile", "debug"), ("debuggable", False), ("library_sha256", "0" * 64), ("apk_bytes", 0), ("apk_sha256", "0" * 64), ("extended", None), ("retained_rows", True), ("retained_rows", 0), ("cargo_lock_sha256", None)):
                with self.subTest(key=key):
                    path.write_text(json.dumps({**report, key: value}), encoding="utf8")
                    with self.assertRaises(ValueError):
                        COMPARISON.artifact(apk)

    def test_comparison_inputs_must_match(self):
        baseline = {"target": "x86_64-linux-android", "application": "controls", "rust_profile": "release", "extended": True,
                    "retained_rows": 1000, "cargo_lock_sha256": "lock", "debuggable": True}
        COMPARISON.match_artifacts(baseline, dict(baseline))
        for key in baseline:
            with self.subTest(key=key), self.assertRaises(ValueError):
                COMPARISON.match_artifacts(baseline, {**baseline, key: None})

    def test_timing_retains_missing_resume_total_and_rejects_cold_missing(self):
        text = "Status: ok\nWaitTime: 42\n"
        self.assertEqual(COMPARISON.timing(text, False), {"total_time_ms": None, "wait_time_ms": 42})
        for invalid in (text, "Status: error\nTotalTime: 1\nWaitTime: 2\n", "Status: ok\nTotalTime: 1\n"):
            with self.assertRaises(ValueError):
                COMPARISON.timing(invalid, True)
        self.assertEqual(COMPARISON.timing(text + "TotalTime: 12\n", True)["total_time_ms"], 12)

    def test_process_stat_handles_spaces_and_parentheses_in_name(self):
        fields = ["S"] + ["0"] * 19
        fields[11], fields[12], fields[19] = "7", "9", "1234"
        self.assertEqual(COMPARISON.process_stat("123 (app ) name) " + " ".join(fields)), {"start_ticks": 1234, "cpu_ticks": 16})
        with self.assertRaises(ValueError):
            COMPARISON.process_stat("123 (name) S 0")

    def test_complete_samples_and_relative_gate_reject_regressions(self):
        sample = {"cold": {"total_time_ms": 200}, "resume": [{"wait_time_ms": 20}] * 3, "pss_kib": 40000, "resident_memory": {"rss_kib": 60000, "reported_rss_hwm_kib": 70000, "start_ticks": 123}, "idle": {"cpu_ticks": 0}}
        samples = [copy.deepcopy(sample) for _ in range(6)]
        baseline = COMPARISON.summarize(samples, 6, 3)
        self.assertEqual(baseline["reported_rss_hwm_p95_kib"], 70000)
        self.assertTrue(all(item["passed"] for item in COMPARISON.evaluate(baseline, baseline).values()))
        for name, floor in COMPARISON.POLICY.items():
            candidate = {**baseline, name: baseline[name] * 1.5 + floor + 1}
            self.assertFalse(COMPARISON.evaluate(baseline, candidate)[name]["passed"])
        with self.assertRaises(ValueError):
            COMPARISON.summarize(samples[:-1], 6, 3)
        samples[0]["resume"].pop()
        with self.assertRaises(ValueError):
            COMPARISON.summarize(samples, 6, 3)
        for invalid in ([], [float("nan")], [float("inf")], [-1], [True], [None]):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                COMPARISON.percentile(invalid)
        self.assertEqual(COMPARISON.percentile(list(range(20))), 18)
        with self.assertRaises(ValueError):
            COMPARISON.evaluate(baseline, {})
        for invalid in (-1, None, True, float("nan")):
            damaged = [copy.deepcopy(sample) for _ in range(6)]
            damaged[0]["resident_memory"]["reported_rss_hwm_kib"] = invalid
            with self.assertRaises(ValueError):
                COMPARISON.summarize(damaged, 6, 3)

    def test_archives_and_nonempty_output_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            COMPARISON.validate_output(root)
            (root / "snapshot.json").write_text("{}")
            for output in (root, root / "nested"):
                with self.assertRaisesRegex(ValueError, "archived"):
                    COMPARISON.validate_output(output)
            self.assertFalse((root / "nested").exists())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "keep").write_text("untouched")
            with self.assertRaisesRegex(ValueError, "empty"):
                COMPARISON.validate_output(root)
            self.assertEqual((root / "keep").read_text(), "untouched")


if __name__ == "__main__":
    unittest.main()
