"""Fail-closed checks for the cross-framework comparison's admissibility."""
import json
import importlib.util
from pathlib import Path
import tempfile
import unittest

from summarize import ENGINES, SCENES, summarize


class SummaryTest(unittest.TestCase):
    def test_blank_window_cannot_satisfy_visual_gate(self):
        from PIL import Image, ImageDraw
        spec = importlib.util.spec_from_file_location("cpu_bench", Path(__file__).resolve().parents[1] / "cpu-bench.py")
        bench = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bench)
        image = Image.new("RGB", (1280, 800), "black")
        self.assertFalse(bench.calibration_content_visible(image))
        draw = ImageDraw.Draw(image)
        draw.rectangle((0, 0, 1279, 799), outline="white", width=2)
        self.assertFalse(bench.calibration_content_visible(image), "window border is not content")
        for y in range(40, 760, 24):
            draw.text((40, y), "Frame comparison - fixed content / no virtualization", fill="white")
        self.assertTrue(bench.calibration_content_visible(image))
        self.assertFalse(bench.calibration_palette_matches(image))
        image = Image.new("RGB", (1280, 800), (24, 24, 27))
        self.assertTrue(bench.calibration_palette_matches(image))
        image = Image.new("RGB", (1280, 800), (2, 2, 3))
        self.assertFalse(bench.calibration_palette_matches(image))

    def test_capture_validation(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for engine in ENGINES:
                for index in range(1, 4):
                    rows = [dict(section=scene, phase=phase, valid=True, foreground_visible=True, visual_nonblank=True, visual_palette=True,
                        wall_seconds=10, client_size=[1280, 800], dpi=144,
                        single_core_percent=12, cpu_ms_per_observed_frame=2,
                        frame_counter_kind="presented_frames" if engine == "jadix" else "paint_callbacks",
                        fps=60 if phase == "scroll" else 0, scroll_range_px=1500 if phase == "scroll" else 0,
                        redraws=600 if phase == "scroll" else 0, passed=False)
                        for scene in SCENES for phase in ("scroll", "settled")]
                    for row in rows:
                        row["renderer"] = dict(backend="Dx11", implementation="native", device_type="Hardware",
                            dxgi=dict(adapter_luid=[0, 123], vendor_id=0x8086, device_id=0xa788))
                    (directory / f"{engine}-{index}.json").write_text(json.dumps(dict(calibration=True,
                        physical_client_size=True, diagnostic_profiling=False,
                        executable_sha256=engine, results=rows)), encoding="utf-8")
            result = summarize(directory, 3)
            self.assertEqual(result["status"], "calibration-only")
            self.assertFalse(any(r["numeric_cpu_target_passed"] for r in result["results"]))
            path = directory / "gpui-fast-1.json"
            original = json.loads(path.read_text())
            for field, value in (("valid", False), ("visual_nonblank", False), ("visual_palette", False), ("dpi", 96), ("wall_seconds", 6)):
                changed = json.loads(json.dumps(original))
                changed["results"][0][field] = value
                path.write_text(json.dumps(changed))
                with self.assertRaises(AssertionError):
                    summarize(directory, 3)
            for renderer in (None, dict(backend="Dx12", implementation="native")):
                changed = json.loads(json.dumps(original))
                for row in changed["results"]:
                    row["renderer"] = renderer
                for index in range(1, 4):
                    (directory / f"gpui-fast-{index}.json").write_text(json.dumps(changed))
                self.assertEqual(summarize(directory, 3)["status"], "incomparable-renderer")
            for index in range(1, 4):
                (directory / f"gpui-fast-{index}.json").write_text(json.dumps(original))
            changed = json.loads(json.dumps(original))
            for row in changed["results"]:
                row["renderer"]["dxgi"]["adapter_luid"] = [0, 999]
            for index in range(1, 4):
                (directory / f"gpui-fast-{index}.json").write_text(json.dumps(changed))
            self.assertEqual(summarize(directory, 3)["status"], "incomparable-renderer")
            for index in range(1, 4):
                (directory / f"gpui-fast-{index}.json").write_text(json.dumps(original))
            translated = json.loads(json.dumps(original))
            for row in translated["results"]:
                row["renderer"].update(backend="Gl", implementation="angle-d3d11")
            for index in range(1, 4):
                (directory / f"jadix-{index}.json").write_text(json.dumps(translated))
            self.assertEqual(summarize(directory, 3)["renderer_status"], "verified-d3d11")
            changed = json.loads(json.dumps(original))
            for row in changed["results"]:
                if row["phase"] == "scroll":
                    row["scroll_range_px"] = 3000
            # Change all Fast rounds so the median cannot hide a travel mismatch.
            for index in range(1, 4):
                (directory / f"gpui-fast-{index}.json").write_text(json.dumps(changed))
            self.assertEqual(summarize(directory, 3)["status"], "incomparable-scroll-travel")


if __name__ == "__main__":
    unittest.main()
