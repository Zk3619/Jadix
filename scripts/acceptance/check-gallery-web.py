"""Exercise the served WebAssembly Gallery in Chromium with real WebGPU rendering."""

import argparse
import io
import json
from pathlib import Path
import time

from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright


SECTIONS = (
    "basics", "forms", "feedback", "data", "navigation", "overlays", "events", "drag",
    "layout", "responsive", "style", "transforms", "text", "theme", "draw", "scene",
    "media", "motion", "documents", "code", "desktop", "workspace", "reactivity",
    "rendering", "about",
)


def rendered_image(page, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        image = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
        colors = image.resize((240, 160)).getcolors(38400) or []
        if len(colors) > 80:
            return image
        page.wait_for_timeout(100)
    raise AssertionError("Canvas remains blank or unpainted")


def changed_pixels(before, after, bounds):
    difference = ImageChops.difference(before.crop(bounds), after.crop(bounds)).convert("L")
    return sum(difference.histogram()[13:])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:8765/")
    parser.add_argument("--browser", type=Path)
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/gallery-web"))
    parser.add_argument("--sections", nargs="+", choices=SECTIONS, default=SECTIONS)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    results = {"url": args.url, "pages": [], "errors": []}
    with sync_playwright() as playwright:
        launch = {"headless": True}
        if args.browser:
            launch["executable_path"] = str(args.browser)
        browser = playwright.chromium.launch(**launch)
        try:
            for section in args.sections:
                context = browser.new_context(viewport={"width": 1440, "height": 1000})
                page = context.new_page()
                logs = []
                page.on("console", lambda message: logs.append({"type": message.type, "text": message.text}))
                page.on("pageerror", lambda error: results["errors"].append(str(error)))
                page.on("requestfailed", lambda request: results["errors"].append(f"{request.url}: {request.failure}"))
                try:
                    page.goto(f"{args.url.rstrip('/')}/#{section}", wait_until="networkidle")
                    page.wait_for_function("document.body.dataset.state === 'started'")
                    image = rendered_image(page)
                    image.save(args.output / f"{section}-desktop.png")
                    page.wait_for_timeout(700)
                    errors = [entry["text"] for entry in logs if entry["type"] == "error"]
                    results["errors"].extend(errors)
                    row = {"section": section, "canvas": page.locator("canvas").evaluate("canvas => [canvas.width, canvas.height]"), "errors": errors}
                    assert row["canvas"] == [1440, 1000], row
                    if section == "scene":
                        after = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
                        row["animated_pixels"] = changed_pixels(image, after, (313, 201, 533, 361))
                        assert row["animated_pixels"] > 100, "3D scene is not advancing"
                        after.save(args.output / "scene-animated.png")
                    if section == "basics":
                        page.mouse.click(365, 190)
                        page.mouse.move(1400, 70)
                        page.wait_for_timeout(250)
                        clicked = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
                        row["counter_click_pixels"] = changed_pixels(image, clicked, (340, 181, 398, 200))
                        assert row["counter_click_pixels"] > 10, "Pointer input did not update the counter"
                        page.keyboard.press("Space")
                        page.wait_for_timeout(250)
                        keyed = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
                        row["counter_keyboard_pixels"] = changed_pixels(clicked, keyed, (340, 181, 398, 200))
                        assert row["counter_keyboard_pixels"] > 10, "Keyboard input did not update the focused counter"
                        page.mouse.click(100, 137)
                        page.wait_for_timeout(400)
                        form = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
                        row["navigation_pixels"] = changed_pixels(keyed, form, (329, 14, 520, 61))
                        assert row["navigation_pixels"] > 50, "Sidebar navigation did not change the page header"
                        page.mouse.click(400, 219)
                        page.keyboard.type("Hello Web", delay=40)
                        page.mouse.click(1350, 70)
                        page.wait_for_timeout(250)
                        edited = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
                        row["input_pixels"] = changed_pixels(form, edited, (325, 209, 550, 230))
                        assert row["input_pixels"] > 100, "Text input did not repaint"
                        edited.save(args.output / "forms-input.png")
                        page.mouse.click(100, 101)
                        page.wait_for_timeout(250)
                        page.set_viewport_size({"width": 390, "height": 844})
                        page.wait_for_timeout(3500)
                        rendered_image(page).save(args.output / "basics-mobile.png")
                        assert page.locator("canvas").evaluate("canvas => [canvas.width, canvas.height]") == [390, 844]
                    results["pages"].append(row)
                    print(json.dumps(row), flush=True)
                finally:
                    (args.output / f"{section}-console.json").write_text(json.dumps(logs, ensure_ascii=False, indent=2), encoding="utf-8")
                    context.close()
            mobile_browser = playwright.chromium.launch(**launch, args=["--force-device-scale-factor=2"])
            mobile = mobile_browser.new_context(viewport={"width": 390, "height": 844}, is_mobile=True, has_touch=True, device_scale_factor=2)
            try:
                page = mobile.new_page()
                page.on("pageerror", lambda error: results["errors"].append(str(error)))
                page.on("console", lambda message: results["errors"].append(message.text) if message.type == "error" else None)
                page.goto(f"{args.url.rstrip('/')}/#basics", wait_until="networkidle")
                page.wait_for_timeout(500)
                before = rendered_image(page)
                before.save(args.output / "touch-closed.png")
                assert page.locator("canvas").evaluate("canvas => [canvas.width, canvas.height]") == [780, 1688]
                page.touchscreen.tap(32, 31)
                page.wait_for_timeout(700)
                opened = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
                results["touch_sidebar_pixels"] = changed_pixels(before, opened, (0, 0, 570, 1000))
                assert results["touch_sidebar_pixels"] > 1000, "Touch did not open the mobile sidebar"
                opened.save(args.output / "touch-open.png")
                page.touchscreen.tap(260, 24)
                page.wait_for_timeout(700)
                closed = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
                assert changed_pixels(opened, closed, (0, 0, 570, 1000)) > 1000, "Touch did not close the mobile sidebar"
                closed.save(args.output / "touch-closed-again.png")
            finally:
                mobile.close()
                mobile_browser.close()
        finally:
            browser.close()
            (args.output / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
    assert not results["errors"], results["errors"]


if __name__ == "__main__":
    main()
