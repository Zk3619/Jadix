import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest

DIRECTORY = Path(__file__).parents[1] / "acceptance"
sys.path.insert(0, str(DIRECTORY))
SPEC = importlib.util.spec_from_file_location("native_popover_dom", DIRECTORY / "native-popover-dom-check.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class NativePopoverDomTests(unittest.TestCase):
    def test_requires_the_exact_isolated_application_mode(self):
        valid = {"popover": True, "selection": False, "extended": False, "retained_rows": 0}
        CHECK.require_popover_build(valid)
        for name, value in [("popover", False), ("popover", 1), ("selection", True), ("extended", True), ("retained_rows", 100), ("retained_rows", False)]:
            with self.subTest(name=name, value=value):
                with self.assertRaises(ValueError):
                    CHECK.require_popover_build({**valid, name: value})

    def test_missing_mode_fields_are_not_inferred_from_other_artifacts(self):
        valid = {"popover": True, "selection": False, "extended": False, "retained_rows": 0}
        for name in valid:
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    CHECK.require_popover_build({key: value for key, value in valid.items() if key != name})

    def test_mixed_sample_modes_fail_before_building_or_writing_an_artifact(self):
        builder = Path(__file__).parents[2] / "examples" / "native-controls" / "build-web.py"
        for mode in ("--notes", "--selection", "--extended", "--large-list"):
            with self.subTest(mode=mode):
                result = subprocess.run([sys.executable, str(builder), "--popover", mode], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn("--popover requires its own sample", result.stderr)


if __name__ == "__main__":
    unittest.main()
