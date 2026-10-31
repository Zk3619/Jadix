"""Verify WebGPU colors, modal scrims, typography scrolling and browser IME."""

import argparse
import io
import json
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright


def screenshot(page, path=None):
    image = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
    if path:
        image.save(path)
    return image


def changed(before, after, bounds):
    delta = ImageChops.difference(before.crop(bounds), after.crop(bounds)).convert("L")
    return sum(delta.histogram()[13:])


def assert_scrim(source, actual, color, opacity):
    def linear(channel):
        encoded = channel / 255
        return encoded / 12.92 if encoded <= 0.04045 else ((encoded + 0.055) / 1.055) ** 2.4

    for original, composited, tint in zip(source, actual, color):
        mixed = linear(original) * (1 - opacity) + linear(tint) * opacity
        expected = 255 * (12.92 * mixed if mixed <= 0.0031308 else 1.055 * mixed ** (1 / 2.4) - 0.055)
        assert abs(composited - expected) <= 4, (source, actual, color, opacity, expected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:8765/")
    parser.add_argument("--browser", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/gallery-web-regressions"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    results = {"passed": False, "errors": [], "checks": []}
    logs = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=str(args.browser), headless=True)

        def open_page(section, scheme):
            page = browser.new_page(viewport={"width": 1440, "height": 1000}, color_scheme=scheme)
            page.on("console", lambda message: logs.append({"type": message.type, "text": message.text}))
            page.on("pageerror", lambda error: results["errors"].append(str(error)))
            page.goto(f"{args.url.rstrip('/')}/#{section}", wait_until="networkidle")
            page.wait_for_function("document.body.dataset.state === 'started'")
            page.wait_for_timeout(700)
            return page

        try:
            for scheme, expected in (("dark", 23), ("light", 255)):
                page = open_page("text", scheme)
                top = screenshot(page, args.output / f"text-{scheme}-top.png")
                pixel = top.getpixel((1300, 200))
                assert all(abs(channel - expected) <= 3 for channel in pixel), (scheme, pixel)
                paper = top.getpixel((1020, 880))
                assert all(abs(actual - target) <= 3 for actual, target in zip(paper, (246, 243, 233))), (scheme, paper)
                page.mouse.move(1395, 700)
                for index in range(18):
                    page.mouse.wheel(0, 600)
                    page.wait_for_timeout(250)
                    if index in (5, 6, 8, 12, 17):
                        image = screenshot(page, args.output / f"text-{scheme}-scroll-{index}.png")
                        if index == 6:
                            colors = image.crop((320, 70, 1380, 700)).get_flattened_data()
                            colorful = sum(max(color) - min(color) > 65 for color in colors)
                            assert colorful > 100, "Color emoji not painted in the typography stress section"
                assert page.evaluate("document.body.dataset.state") == "started", "Typography scroll crashed"
                results["checks"].append({"scheme": scheme, "card_rgb": pixel, "paper_rgb": paper, "typography_scroll": "passed"})
                page.close()

            page = open_page("forms", "dark")
            page.mouse.click(400, 219)
            page.wait_for_timeout(250)
            assert page.locator("[data-jadix-ime]").evaluate("input => input === document.activeElement && !input.disabled"), "IME input not focused"
            cdp = page.context.new_cdp_session(page)
            geometry = []
            results["ime_geometry"] = geometry
            for text in ["w", "w'", "w'w", "w'w'w", "w'w", "w'", "w", "ni", "ni'hao", "n", "pinyin" * 80, "w"]:
                cdp.send("Input.imeSetComposition", {"text": text, "selectionStart": len(text), "selectionEnd": len(text)})
                page.wait_for_timeout(100)
                state = page.locator("[data-jadix-ime]").evaluate("""input => ({
                    left: input.getBoundingClientRect().left,
                    top: input.getBoundingClientRect().top,
                    height: input.getBoundingClientRect().height,
                    width: input.getBoundingClientRect().width,
                    fontSize: getComputedStyle(input).fontSize,
                    lineHeight: getComputedStyle(input).lineHeight,
                    scrollHeight: input.scrollHeight, clientHeight: input.clientHeight,
                    scrollLeft: input.scrollLeft, scrollTop: input.scrollTop,
                    value: input.value, pageX: scrollX, pageY: scrollY
                })""")
                state["preedit"] = text
                geometry.append(state)
                if text in ("w", "w'w"):
                    screenshot(page, args.output / f"ime-anchor-{len(text)}.png")
            for state in geometry:
                assert state["scrollHeight"] == state["clientHeight"], f"IME preedit wrapped: {state}"
                assert state["scrollTop"] == state["scrollLeft"] == 0, f"IME host scrolled: {state}"
                assert state["width"] == 1, f"IME host resized with preedit text: {state}"
                assert float(state["fontSize"].removesuffix("px")) >= 1, f"Zero-sized native glyphs invalidate candidate positioning: {state}"
                assert (state["left"], state["top"], state["height"]) == (geometry[0]["left"], geometry[0]["top"], geometry[0]["height"]), f"IME anchor moved: {state}"
                assert abs(float(state["lineHeight"].removesuffix("px")) - state["height"]) < 0.01, f"IME line height differs from caret: {state}"
                assert state["value"] == state["preedit"], f"IME composition lost: {state}"
                assert state["pageX"] == state["pageY"] == 0, f"IME scrolled the page: {state}"
            results["checks"].append({"ime_anchor": "single/double/triple syllables, deletion and long preedit remain single-line and unscrolled"})
            cdp.send("Input.imeSetComposition", {"text": "", "selectionStart": 0, "selectionEnd": 0})
            edge_style = page.add_style_tag(content="[data-jadix-ime] {left: calc(100vw - 2px) !important; top: calc(100vh - 22px) !important;}")
            edges = []
            results["ime_edge_geometry"] = edges
            for text in ["w", "w'w", "w'w'w'w", "pinyin" * 80, "w"]:
                cdp.send("Input.imeSetComposition", {"text": text, "selectionStart": len(text), "selectionEnd": len(text)})
                page.wait_for_timeout(100)
                state = page.locator("[data-jadix-ime]").evaluate("""input => ({
                    left: input.getBoundingClientRect().left, right: input.getBoundingClientRect().right,
                    top: input.getBoundingClientRect().top, bottom: input.getBoundingClientRect().bottom,
                    width: input.getBoundingClientRect().width, viewportWidth: innerWidth, viewportHeight: innerHeight,
                    scrollLeft: input.scrollLeft, scrollTop: input.scrollTop, value: input.value
                })""")
                edges.append(state)
                assert state["width"] == 1 and state["right"] <= state["viewportWidth"], f"IME host grew beyond viewport: {state}"
                assert state["bottom"] <= state["viewportHeight"], f"IME host exceeds viewport height: {state}"
                assert state["scrollLeft"] == state["scrollTop"] == 0, f"IME host scrolled at viewport edge: {state}"
                assert state["value"] == text, "Long preedit was truncated at viewport edge"
            cdp.send("Input.imeSetComposition", {"text": "", "selectionStart": 0, "selectionEnd": 0})
            edge_style.evaluate("style => style.remove()")
            results["checks"].append({"ime_edge": "host stays bounded and does not resize or scroll as composition grows and shrinks"})
            page.wait_for_timeout(200)
            baseline = screenshot(page)
            cdp.send("Input.imeSetComposition", {"text": "nihao", "selectionStart": 5, "selectionEnd": 5})
            page.wait_for_timeout(200)
            preedit = screenshot(page, args.output / "ime-preedit.png")
            assert changed(baseline, preedit, (325, 209, 440, 232)) > 25, "Preedit not painted"
            cdp.send("Input.insertText", {"text": "\u4f60\u597d"})
            page.wait_for_timeout(200)
            committed = screenshot(page, args.output / "ime-committed.png")
            assert changed(preedit, committed, (325, 209, 440, 232)) > 25, "Commit not painted"
            assert page.locator("[data-jadix-ime]").input_value() == "", "DOM input retained committed text"
            font = TTFont(Path(__file__).resolve().parents[2] / "examples/gallery_web/assets/gallery-sans.otf")
            cmap = font.getBestCmap()
            chinese = "\u54c7\u55e1\u545c\u9e23\u7f51\u9f98"
            for character in chinese:
                assert cmap.get(ord(character)) not in (None, ".notdef"), f"Committed Chinese has no glyph: U+{ord(character):04X}"
            cjk_count = sum(0x4E00 <= codepoint <= 0x9FFF for codepoint in cmap)
            assert cjk_count > 20000, f"CJK input still limited to a source-string subset: {cjk_count}"
            font.close()
            page.keyboard.press("Control+a")
            cdp.send("Input.insertText", {"text": chinese})
            page.wait_for_timeout(200)
            screenshot(page, args.output / "ime-chinese-outside-gallery-strings.png")
            results["checks"].append({"chinese_input": "non-example Chinese committed and font cmap verified", "basic_cjk_glyphs": cjk_count})
            page.keyboard.press("Control+a")
            cdp.send("Input.insertText", {"text": "\u4f60\u597d"})
            page.wait_for_timeout(200)
            committed = screenshot(page)
            cdp.send("Input.insertText", {"text": "\u4f60\u597d"})
            page.wait_for_timeout(200)
            repeated = screenshot(page, args.output / "ime-repeated.png")
            assert changed(committed, repeated, (325, 209, 440, 232)) > 25, "Repeated identical commit was dropped"
            page.keyboard.press("Control+a")
            page.keyboard.type("abc")
            page.wait_for_timeout(200)
            cdp.send("Input.imeSetComposition", {"text": "quxiao", "selectionStart": 6, "selectionEnd": 6})
            cdp.send("Input.imeSetComposition", {"text": "", "selectionStart": 0, "selectionEnd": 0})
            page.keyboard.type("Z")
            page.wait_for_timeout(200)
            cancelled = screenshot(page, args.output / "ime-cancelled-keyboard.png")
            page.keyboard.press("Control+a")
            page.keyboard.type("abcZ")
            page.wait_for_timeout(200)
            expected = screenshot(page)
            assert changed(cancelled, expected, (325, 209, 440, 232)) < 10, "Cancelled composition changed the committed text"
            page.locator("[data-jadix-ime]").evaluate("input => input.dispatchEvent(new InputEvent('beforeinput', {inputType: 'deleteContentBackward', bubbles: true, cancelable: true}))")
            page.wait_for_timeout(200)
            deleted = screenshot(page)
            page.keyboard.press("Control+a")
            page.keyboard.type("abc")
            page.wait_for_timeout(200)
            expected = screenshot(page)
            assert changed(deleted, expected, (325, 209, 440, 232)) < 10, "Virtual-keyboard deletion did not reach the framework"
            page.mouse.click(710, 220)
            page.wait_for_timeout(200)
            before_switch = screenshot(page)
            cdp.send("Input.insertText", {"text": "\u4e2d\u6587"})
            page.wait_for_timeout(200)
            switched = screenshot(page, args.output / "ime-field-switch.png")
            assert changed(before_switch, switched, (630, 209, 840, 232)) > 25, "Commit did not follow focus to the next field"
            page.mouse.click(700, 130)
            page.wait_for_timeout(300)
            assert page.locator("[data-jadix-ime]").is_disabled(), "IME stayed enabled after leaving text fields"
            results["checks"].append({"ime": "preedit, commit, repeat, cancel, keyboard, field switch and blur passed"})
            page.close()

            page = open_page("overlays", "light")
            before = screenshot(page, args.output / "modal-before.png")
            page.mouse.click(410, 487)
            page.wait_for_timeout(1000)
            after = screenshot(page, args.output / "modal-open.png")
            bounds = (1080, 360, 1100, 380)
            assert changed(before, after, bounds) > 100, "Configured black modal scrim did not affect the page"
            source = before.getpixel((1090, 370))
            actual = after.getpixel((1090, 370))
            assert_scrim(source, actual, (0, 0, 0), 0.55)
            page.keyboard.press("Escape")
            page.wait_for_timeout(1000)
            closed = screenshot(page, args.output / "modal-closed.png")
            assert changed(before, closed, bounds) == 0, "Modal scrim left stale pixels"
            results["checks"].append({"modal_scrim": "55% black, linear-light compositing and dismissal passed", "before": source, "after": actual})
            page.mouse.click(410, 557)
            page.wait_for_timeout(1000)
            sheet = screenshot(page, args.output / "modal-builder-blue.png")
            actual = sheet.getpixel((1090, 370))
            assert_scrim(source, actual, (29, 78, 216), 0.3)
            page.keyboard.press("Escape")
            page.wait_for_timeout(1000)
            closed = screenshot(page)
            assert changed(before, closed, bounds) == 0, "Colored scrim left stale pixels"
            results["checks"].append({"builder_scrim": "30% #1d4ed8, linear-light compositing and dismissal passed", "after": actual})
            page.close()
            results["passed"] = True
        finally:
            browser.close()
            results["errors"].extend(entry["text"] for entry in logs if entry["type"] == "error")
            results["passed"] = results["passed"] and not results["errors"]
            (args.output / "console.json").write_text(json.dumps(logs, ensure_ascii=False, indent=2), encoding="utf-8")
            (args.output / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    assert not results["errors"], results["errors"]
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
