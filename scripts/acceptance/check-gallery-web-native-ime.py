"""Check real Microsoft Pinyin candidate positioning in a dedicated Windows browser."""

import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import sys

from PIL import ImageGrab
from playwright.sync_api import sync_playwright


def main():
    if sys.platform != "win32":
        raise SystemExit("This check requires Windows and Microsoft Pinyin in Chinese mode")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:8765/")
    parser.add_argument("--browser", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    user = ctypes.WinDLL("user32", use_last_error=True)
    user.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    user.GetForegroundWindow.restype = wintypes.HWND
    user.SetForegroundWindow.argtypes = [wintypes.HWND]
    user.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    user.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    previous = user.GetForegroundWindow()
    results = {"passed": False, "samples": []}
    title = "Jadix native IME acceptance"

    def browser_window():
        selected = []
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

        @callback_type
        def visit(hwnd, _):
            buffer = ctypes.create_unicode_buffer(512)
            user.GetWindowTextW(hwnd, buffer, len(buffer))
            if title in buffer.value and user.IsWindowVisible(hwnd):
                selected.append(hwnd)
                return False
            return True

        user.EnumWindows(visit, 0)
        assert selected, "Dedicated test browser window not found"
        return selected[0]

    def capture(hwnd):
        bounds = wintypes.RECT()
        assert user.GetWindowRect(hwnd, ctypes.byref(bounds))
        return ImageGrab.grab(bbox=(bounds.left, bounds.top, bounds.right, bounds.bottom)).convert("RGB")

    def key(hwnd, virtual_key, scan_code):
        assert user.GetForegroundWindow() == hwnd, "Test browser lost foreground; refusing native key input"
        user.keybd_event(virtual_key, scan_code, 0, 0)
        user.keybd_event(virtual_key, scan_code, 2, 0)

    def sample(page, hwnd, baseline, name):
        image = capture(hwnd)
        image.save(args.output / f"{name}.png")
        points = []
        for index, (color, old) in enumerate(zip(image.get_flattened_data(), baseline.get_flattened_data())):
            red, green, blue = color
            if blue > 175 and green > 110 and red < 110 and blue > red + 60 and green > red + 30 and color != old:
                points.append((index % image.width, index // image.width))
        assert points, "Microsoft Pinyin candidate indicator not found; check IME mode/theme and screenshots"
        indicator = [min(point[0] for point in points), min(point[1] for point in points),
                     max(point[0] for point in points), max(point[1] for point in points)]
        assert indicator[2] - indicator[0] < 12 and 10 <= indicator[3] - indicator[1] <= 50, indicator
        state = page.locator("[data-jadix-ime]").evaluate("""input => ({
            left: input.getBoundingClientRect().left, top: input.getBoundingClientRect().top,
            width: input.getBoundingClientRect().width, height: input.getBoundingClientRect().height,
            scrollLeft: input.scrollLeft, scrollTop: input.scrollTop, value: input.value,
            dpr: devicePixelRatio, border: (outerWidth - innerWidth) / 2, chrome: outerHeight - innerHeight
        })""")
        row = {"name": name, "native_indicator": indicator, "input": state}
        results["samples"].append(row)
        expected_x = (state["border"] + state["left"]) * state["dpr"]
        expected_y = (state["chrome"] + state["top"] + state["height"]) * state["dpr"]
        assert abs(indicator[0] - expected_x) < 25 * state["dpr"], f"Native candidate lost caret X: {row}"
        assert abs(indicator[1] - expected_y) < 55 * state["dpr"], f"Native candidate lost caret Y: {row}"
        return row

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=str(args.browser), headless=False,
            args=["--window-position=60,60", "--window-size=1100,850"])
        try:
            for section in ("forms", "basics"):
                page = browser.new_page(no_viewport=True, color_scheme="light")
                page.goto(f"{args.url.rstrip('/')}/#{section}", wait_until="networkidle")
                page.wait_for_function("document.body.dataset.state === 'started'")
                page.evaluate("title => document.title = title", title)
                page.wait_for_timeout(800)
                hwnd = browser_window()
                user.SetForegroundWindow(hwnd)
                if section == "basics":
                    page.mouse.move(1000, 600)
                    page.mouse.wheel(0, 400)
                    page.wait_for_timeout(350)
                page.mouse.click(400, 219 if section == "forms" else 645)
                page.wait_for_timeout(300)
                assert page.locator("[data-jadix-ime]").evaluate("input => !input.disabled && input === document.activeElement"), section
                baseline = capture(hwnd)
                key(hwnd, 0x41, 0x1E)
                page.wait_for_timeout(500)
                first = sample(page, hwnd, baseline, f"{section}-single")
                for _ in range(12):
                    key(hwnd, 0x41, 0x1E)
                    page.wait_for_timeout(50)
                page.wait_for_timeout(250)
                longer = sample(page, hwnd, baseline, f"{section}-long")
                assert longer["input"]["value"].count("a") >= 10, "Native keys did not enter a real composition"
                assert max(abs(current - initial) for current, initial in zip(longer["native_indicator"], first["native_indicator"])) <= 2, "Native candidate moved as preedit grew"
                for _ in range(5):
                    key(hwnd, 0x08, 0x0E)
                    page.wait_for_timeout(50)
                page.wait_for_timeout(250)
                shorter = sample(page, hwnd, baseline, f"{section}-shortened")
                assert max(abs(current - initial) for current, initial in zip(shorter["native_indicator"], first["native_indicator"])) <= 2, "Native candidate moved as preedit shrank"
                if section == "basics":
                    page.mouse.move(1000, 500)
                    page.mouse.wheel(0, 150)
                    page.wait_for_timeout(500)
                    moved = sample(page, hwnd, baseline, "basics-composing-scroll")
                    assert moved["input"]["top"] < shorter["input"]["top"] - 100, "Focused field did not scroll"
                    expected_delta = (moved["input"]["top"] - shorter["input"]["top"]) * moved["input"]["dpr"]
                    actual_delta = moved["native_indicator"][1] - shorter["native_indicator"][1]
                    assert abs(actual_delta - expected_delta) <= 3, "Native candidate did not follow scrolling field"
                key(hwnd, 0x1B, 1)
                page.wait_for_timeout(250)
                page.close()
            results["passed"] = True
        finally:
            browser.close()
            user.SetForegroundWindow(previous)
            (args.output / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(results, ensure_ascii=True, indent=2))


if __name__ == "__main__":
    main()
