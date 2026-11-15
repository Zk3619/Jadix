import importlib.util
from pathlib import Path
import sys
import unittest


DIRECTORY = Path(__file__).parents[1] / "acceptance"
sys.path.insert(0, str(DIRECTORY))
SPEC = importlib.util.spec_from_file_location("native_dom_resources", DIRECTORY / "native-dom-resources.py")
RESOURCES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RESOURCES)


class NativeDomResourceTests(unittest.TestCase):
    def samples(self):
        return [{"cycle": cycle, "phase": phase, "documents": 1,
                 "nodes": 2000 if phase == "mounted" else 20,
                 "listeners": 46 if phase == "mounted" else 15,
                 "js_heap_bytes": 1600000, "wasm_memory_bytes": 5898240}
                for cycle in range(0, 101, 20) for phase in ("mounted", "unmounted")]

    def test_stable_counts_and_bounded_jit_heap_growth_pass(self):
        samples = self.samples()
        samples[-1]["js_heap_bytes"] += RESOURCES.HEAP_GROWTH_BUDGET
        RESOURCES.validate_samples(samples)

    def test_retained_document_node_listener_or_wasm_growth_fails(self):
        for name in ("documents", "nodes", "listeners", "wasm_memory_bytes"):
            for index in (-2, -1):
                with self.subTest(name=name, phase=index), self.assertRaises(ValueError):
                    samples = self.samples()
                    samples[index][name] += 1
                    RESOURCES.validate_samples(samples)

    def test_heap_growth_beyond_budget_fails_in_each_phase(self):
        for index in (-2, -1):
            with self.subTest(phase=index), self.assertRaisesRegex(ValueError, "JS heap"):
                samples = self.samples()
                samples[index]["js_heap_bytes"] += RESOURCES.HEAP_GROWTH_BUDGET + 1
                RESOURCES.validate_samples(samples)

    def test_missing_nonfinite_negative_and_boolean_metrics_fail(self):
        for name in ("documents", "nodes", "listeners", "js_heap_bytes", "wasm_memory_bytes"):
            for value in (None, float("inf"), float("nan"), -1, True, 1.5):
                with self.subTest(name=name, value=value), self.assertRaises(ValueError):
                    samples = self.samples()
                    if value is None:
                        del samples[-1][name]
                    else:
                        samples[-1][name] = value
                    RESOURCES.validate_samples(samples)

    def test_partial_duplicate_and_reordered_checkpoints_fail(self):
        samples = self.samples()
        for invalid in ([], samples[:-1], samples + [samples[-1]], list(reversed(samples))):
            with self.subTest(length=len(invalid)), self.assertRaises(ValueError):
                RESOURCES.validate_samples(invalid)


if __name__ == "__main__":
    unittest.main()
