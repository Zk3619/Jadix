import importlib.util
from pathlib import Path
import unittest


path = Path(__file__).resolve().parents[1] / "benchmarks" / "startup-bench.py"
spec = importlib.util.spec_from_file_location("startup_bench", path)
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class StartupBenchTests(unittest.TestCase):
    def test_splash_feedback_is_not_business_content(self):
        observed = {}
        bench.observe_startup("startup system=cpu-buffer; startup startup_frame epoch=1 scene=1 generation=1", 0.2, observed)
        self.assertNotIn("content_frame", observed)
        self.assertEqual(observed["startup_frame"], 0.2)
        bench.observe_startup("startup content_frame epoch=1 scene=1 generation=1", 0.6, observed)
        self.assertEqual(observed["content_frame"], 0.6)

    def test_business_title_containing_startup_is_not_excluded(self):
        self.assertFalse(bench.is_startup_window("Jadix Startup Example"))
        self.assertTrue(bench.is_startup_window("Jadix Startup Splash"))
        self.assertTrue(bench.is_startup_window("Jadix Gallery Startup"))

    def test_first_observation_is_stable(self):
        observed = {}
        bench.observe_startup("startup window_frame epoch=1", 0.1, observed)
        bench.observe_startup("startup window_frame epoch=1", 0.7, observed)
        self.assertEqual(observed["window_frame"], 0.1)

    def test_ui_duration_requires_real_finite_millisecond_markers(self):
        marker = "startup bootstrap_poll_ms="
        self.assertIsNone(bench.maximum_logged_duration_ms("other event", marker))
        text = chr(10).join([marker + value for value in ["1.2", "7.8", "NaN", "inf", "bad"]])
        self.assertEqual(bench.maximum_logged_duration_ms(text, marker), 7.8)

    def test_percentiles_use_actual_samples(self):
        self.assertIsNone(bench.sample_percentile([], 0.95))
        self.assertEqual(bench.sample_percentile([0.4, 0.1, 0.2], 0.95), 0.4)
        self.assertEqual(bench.sample_percentile([0.4, 0.1, 0.2], 0.5), 0.2)
