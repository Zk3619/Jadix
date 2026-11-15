import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1] / "acceptance"))
from native_android_metrics import cpu_observation, resident_memory_kib, resident_observation

SCRIPT = Path(__file__).parents[1] / "acceptance" / "native-android-performance.py"
SPEC = importlib.util.spec_from_file_location("native_android_performance", SCRIPT)
PERFORMANCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PERFORMANCE)


class NativeAndroidPerformanceTests(unittest.TestCase):
    def test_legacy_and_current_meminfo_summary_units_match(self):
        self.assertEqual(PERFORMANCE.total_pss_kib("        TOTAL    28642    18700\n               TOTAL:    28642       TOTAL SWAP PSS:        0\n"), 28642)
        self.assertEqual(PERFORMANCE.total_pss_kib("               TOTAL PSS:    40021       TOTAL RSS:    82644\n"), 40021)
        self.assertEqual(PERFORMANCE.total_pss_kib("TOTAL PSS: 0\n"), 0)

    def test_missing_or_malformed_totals_are_not_replaced_by_zero(self):
        for text in ["", "TOTAL SWAP PSS: 9", "TOTAL RSS: 9", "TOTAL 28642", "TOTAL: -1", "TOTAL: 12x", "NOT TOTAL PSS: 12", "TOTAL:\n12"]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                PERFORMANCE.total_pss_kib(text)

    def test_multiple_process_totals_are_not_silently_selected(self):
        for text in ["TOTAL: 12\nTOTAL: 12\n", "TOTAL PSS: 12\nTOTAL: 13\n"]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                PERFORMANCE.total_pss_kib(text)

    def test_resident_high_water_and_current_rss_require_unique_explicit_kib(self):
        for ending in ("\n", "\r\n"):
            text = ending.join(["Name: owned", "VmPeak: 900 kB", "VmRSS:\t123 kB", "VmHWM:\t456 kB", ""])
            self.assertEqual(resident_memory_kib(text), {"rss_kib": 123, "reported_rss_hwm_kib": 456})
        for text in ("", "VmRSS: 12 kB\n", "VmRSS: 12 kB\nVmHWM: -1 kB\n", "VmRSS: 12 MB\nVmHWM: 13 kB\n", "VmRSS: 12 kB\nVmHWM: 13\n", "VmRSS: 12 kB\nVmHWM: 13 kB\nVmHWM: 14 kB\n", "VmRSS: 12 kB\nVmHWM: 13x kB\n"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                resident_memory_kib(text)
        with self.assertRaises(ValueError):
            resident_memory_kib("VmRSS: 12 kB\nVmHWM: 13 kB\nVmHWM: invalid\n")

    def test_resident_observations_reject_process_replacement_on_either_side(self):
        def stat(start):
            return "123 (owned ) process) " + " ".join(["S"] + ["0"] * 18 + [str(start)])
        for before, after in ((99, 99), (98, 99), (99, 100)):
            calls = []
            stats = iter((stat(before), stat(after)))
            def read(name):
                calls.append(name)
                return next(stats) if name == "stat" else "VmRSS: 12 kB\nVmHWM: 13 kB\n"
            if before == after == 99:
                self.assertEqual(resident_observation(read, 99), {"rss_kib": 12, "reported_rss_hwm_kib": 13, "start_ticks": 99})
            else:
                with self.assertRaisesRegex(ValueError, "identity changed"):
                    resident_observation(read, 99)
            self.assertEqual(calls, ["stat", "status", "stat"])

    def test_cpu_intervals_preserve_work_and_reject_replacement_or_counter_rewind(self):
        before = {"start_ticks": 99, "cpu_ticks": 10}
        for after in ({"start_ticks": 99, "cpu_ticks": 12}, {"start_ticks": 100, "cpu_ticks": 12}, {"start_ticks": 99, "cpu_ticks": 9}):
            snapshots = iter((before, after))
            sleeps = []
            with patch("native_android_metrics.time.monotonic", side_effect=[100, 105.2]):
                if after["start_ticks"] != 99 or after["cpu_ticks"] < 10:
                    with self.assertRaisesRegex(ValueError, "identity or CPU"):
                        cpu_observation(lambda: next(snapshots), sleeps.append, 99, 5)
                else:
                    result = cpu_observation(lambda: next(snapshots), sleeps.append, 99, 5)
                    self.assertAlmostEqual(result["seconds"], 5.2)
                    self.assertEqual(result["requested_seconds"], 5)
                    self.assertEqual(result["cpu_ticks"], 2)
            self.assertEqual(sleeps, [5])


if __name__ == "__main__":
    unittest.main()
