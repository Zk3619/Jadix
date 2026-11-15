import copy
import importlib.util
from pathlib import Path
import sys
import unittest


DIRECTORY = Path(__file__).parents[1] / "acceptance"
sys.path.insert(0, str(DIRECTORY))
SPEC = importlib.util.spec_from_file_location("android_notes_scale", DIRECTORY / "native-notes-android-scale.py")
SCALE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCALE)


class NotesAndroidScaleTests(unittest.TestCase):
    def test_restoration_timestamp_is_explicitly_utc_and_old_touch_compatible(self):
        self.assertEqual(SCALE.mtime_stamp(0), "197001010000.00")
        self.assertEqual(SCALE.mtime_stamp(86400), "197001020000.00")
        self.assertEqual(SCALE.mtime_stamp(1793585077), "202611020204.37")

    def test_protected_names_are_exact_and_single_column(self):
        cache = "native-state-53c25b1d-0fca-4a09-aa56-06220a3ce484.json"
        self.assertEqual(SCALE.data_names("jadix-notes-v1.json\r\n" + cache + "\r\njadix-notes-v1.json.other\r\n"), ["jadix-notes-v1.json", cache])
        with self.assertRaises(ValueError):
            SCALE.data_names("jadix-notes-v1.json  " + cache)
        self.assertEqual(SCALE.data_names("../jadix-notes-v1.json\n"), [])

    def test_recovery_requires_full_archive_and_rust_unsaved_state(self):
        fixture = {"schema": 1, "next_id": "1", "notes": []}
        state = {"archive": fixture, "selected": None, "dirty": True, "draft": {"title": "title", "body": "body"}}
        self.assertTrue(SCALE.recovery_matches(SCALE.json.dumps(state).encode(), fixture, "title", "body"))
        for key, value in [("selected", "1"), ("dirty", False), ("draft", {"title": "title", "body": "stale"}), ("archive", {})]:
            candidate = copy.deepcopy(state)
            candidate[key] = value
            self.assertFalse(SCALE.recovery_matches(SCALE.json.dumps(candidate), fixture, "title", "body"))
        for source in ["{}", "not json", "null"]:
            self.assertFalse(SCALE.recovery_matches(source, fixture, "title", "body"))

    def test_only_visible_valid_bounds_can_be_tapped(self):
        self.assertEqual(SCALE.tap_center({"bounds": "[10,20][30,40]"}), ("20", "30"))
        for bounds in ["", "[0,0][0,0]", "[30,20][10,40]", "[-1,0][5,10]", "[0,0][2,2]junk"]:
            with self.subTest(bounds=bounds), self.assertRaises(ValueError):
                SCALE.tap_center({"bounds": bounds})

    def test_android_button_transform_preserves_control_identity(self):
        self.assertTrue(SCALE.control_matches({"class": "android.widget.Button", "text": "NEW NOTE"}, "New note"))
        self.assertFalse(SCALE.control_matches({"class": "android.widget.TextView", "text": "NEW NOTE"}, "New note"))
        self.assertFalse(SCALE.control_matches({"class": "android.widget.Button", "text": "NEW NOTEBOOK"}, "New note"))
        self.assertTrue(SCALE.control_matches({"class": "android.widget.EditText", "hint": "Title / 标题"}, "Title / 标题", True))

    def test_scale_metrics_require_complete_finite_samples(self):
        metrics = {"notes": 128, "fixture_bytes": 522240, "input_dispatch_ms": [1.0] * 30, "input_to_draw_callback_ms": [2.0] * 30}
        def output(value):
            return "INSTRUMENTATION_RESULT: metrics=" + SCALE.json.dumps(value) + "\nJADIX_NOTES_SCALE_PASS\nINSTRUMENTATION_CODE: -1"
        self.assertEqual(SCALE.scale_metrics(output(metrics)), metrics)
        for key, value in [("notes", 127), ("fixture_bytes", 0), ("input_dispatch_ms", [1.0] * 29), ("input_to_draw_callback_ms", [float("nan")] * 30), ("input_dispatch_ms", [True] * 30), ("input_dispatch_ms", [-1.0] * 30)]:
            candidate = copy.deepcopy(metrics)
            candidate[key] = value
            with self.subTest(key=key, value=str(value)[:20]), self.assertRaises(ValueError):
                SCALE.scale_metrics(output(candidate))
        for source in ["JADIX_NOTES_SCALE_PASS\nINSTRUMENTATION_CODE: -1", output(metrics) + "\nfailure=broken"]:
            with self.assertRaises(ValueError):
                SCALE.scale_metrics(source)

    def test_old_android_xml_uses_exact_placeholder_and_typed_field_values(self):
        field = {"class": "android.widget.EditText", "text": "Title / 标题"}
        self.assertTrue(SCALE.control_matches(field, "Title / 标题", True))
        self.assertFalse(SCALE.control_matches(field, "Write a note / 正文", True))
        self.assertTrue(SCALE.field_value_matches({"class": "android.widget.EditText", "text": "draft"}, "Title / 标题", "draft"))
        for node in [{"class": "android.widget.TextView", "text": "draft"}, {"class": "android.widget.EditText", "text": "stale"}, {"class": "android.widget.EditText", "text": "draft", "hint": "other"}]:
            self.assertFalse(SCALE.field_value_matches(node, "Title / 标题", "draft"))


if __name__ == "__main__":
    unittest.main()
