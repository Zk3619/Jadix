import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import json
from pathlib import Path
import threading
from playwright.sync_api import sync_playwright
from native_browser import artifact, launch_options, validate_output

class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass

def main():
    parser = argparse.ArgumentParser(description="Verify native DOM Select and shared Rust state")
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--browser", choices=("chromium", "firefox", "webkit"), default="chromium")
    parser.add_argument("--channel")
    parser.add_argument("--executable", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    try:
        validate_output(output)
        build = artifact(args.site, "controls")
        if build.get("selection") is not True or build.get("extended") is not False or build.get("retained_rows") != 0:
            raise ValueError("requires --selection release artifact")
        options = launch_options(args.browser, args.channel, args.executable)
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
    output.mkdir(parents=True, exist_ok=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(args.site.resolve())))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    report = {"passed": False, "build": build, "checks": [], "page_errors": [], "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "limits": ["Browser keyboard and programmatic rejection probes; no physical IME, assistive technology, Android or UIKit execution.", "Select authoring subset recorded in build metadata; no arbitrary GPU visual styles, rich popup content or browser minimum-version acceptance."]}
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, args.browser).launch(**options)
            report.update(browser=args.browser, browser_version=browser.version, launch=options)
            page = browser.new_page(viewport={"width": 420, "height": 760})
            page.on("pageerror", lambda error: report["page_errors"].append(str(error)))
            page.goto("http://127.0.0.1:" + str(server.server_port))
            page.wait_for_function("window.jadixNative")
            choose = page.get_by_role("combobox", name="Choose fruit", exact=True)
            disabled = page.get_by_role("combobox", name="Disabled fruit", exact=True)
            def state():
                return json.loads(page.evaluate("jadixNative.state()"))
            def selected(value):
                page.wait_for_function("value => JSON.parse(jadixNative.state()).value === value", arg=value)
                assert choose.input_value() == value
            assert choose.evaluate("element => element.tagName") == "SELECT"
            assert page.locator("canvas").count() == 0
            assert choose.locator("option").count() == 4
            assert choose.input_value() == "" and disabled.input_value() == "apple" and disabled.is_disabled()
            assert choose.locator('option[value="blocked"]').is_disabled()
            report["checks"].append("real HTMLSelectElement, empty placeholder, disabled control and disabled option")
            choose.evaluate("element => { window.retainedSelect = element; window.selectEvents = []; element.addEventListener('change', event => selectEvents.push({trusted:event.isTrusted,value:element.value})); }")
            choose.select_option("apple")
            selected("apple")
            report["checks"].append("programmatic native option change round-trips to Rust selected value")
            choose.focus()
            choose.press("End")
            choose.press("Tab")
            selected("banana")
            assert page.get_by_role("button", name="Reset selection", exact=True).evaluate("element => document.activeElement === element")
            assert any(event["trusted"] and event["value"] == "banana" for event in page.evaluate("selectEvents"))
            report["checks"].append("trusted native keyboard skips disabled option, commits to Rust and Tabs past disabled control")
            choose.focus()
            choose.press("ArrowUp")
            choose.press("Tab")
            selected("apple")
            report["checks"].append("trusted native reverse selection updates Rust")
            page.set_viewport_size({"width": 360, "height": 520})
            page.evaluate("dispatchEvent(new Event('resize'))")
            assert choose.evaluate("element => element === retainedSelect")
            selected("apple")
            page.get_by_role("button", name="Continue", exact=True).click()
            assert state()["count"] == 1
            selected("apple")
            report["checks"].append("unrelated state and environment publication retain native select identity and value")
            choose.evaluate("element => { element.value = 'blocked'; element.dispatchEvent(new Event('change', {bubbles:true})); }")
            selected("apple")
            assert page.evaluate("jadixNative.take_error()")
            assert page.evaluate("jadixNative.take_error()") is None
            report["checks"].append("disabled option injection rejected; native published value restored and one error drained")
            choose.evaluate("element => { const option = new Option('Unknown', 'missing'); element.add(option); element.value = 'missing'; element.dispatchEvent(new Event('change', {bubbles:true})); option.remove(); }")
            selected("apple")
            assert page.evaluate("jadixNative.take_error()")
            report["checks"].append("unknown injected option rejected without changing Rust selection")
            disabled.evaluate("element => { element.value = 'banana'; element.dispatchEvent(new Event('change', {bubbles:true})); }")
            assert disabled.input_value() == "apple"
            selected("apple")
            assert page.evaluate("jadixNative.take_error()")
            report["checks"].append("disabled control event rejected and native value restored")
            page.get_by_role("button", name="Reset selection", exact=True).click()
            selected("")
            assert choose.evaluate("element => element === retainedSelect")
            choose.select_option("banana")
            selected("banana")
            report["checks"].append("external controlled reset restores empty prompt; next selection remains operational")
            report["events"] = page.evaluate("selectEvents")
            (output / "native-select.png").write_bytes(page.screenshot(full_page=True))
            (output / "accessibility.yaml").write_text(page.locator("body").aria_snapshot(), encoding="utf8")
            page.evaluate("window.oldSelect = retainedSelect; jadixNative.unmount(); jadixNative.mount('app')")
            assert choose.count() == 1
            page.evaluate("oldSelect.value = 'apple'; oldSelect.dispatchEvent(new Event('change', {bubbles:true}))")
            selected("")
            assert choose.evaluate("element => element !== oldSelect")
            assert page.evaluate("jadixNative.take_error()") is None
            report["checks"].append("disposed select callbacks cannot mutate replacement root")
            assert not report["page_errors"]
            report["passed"] = True
            browser.close()
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        server.shutdown()
        server.server_close()
        (output / "checks.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print("PASS native Select: " + str(len(report["checks"])) + " scenarios")

if __name__ == "__main__":
    main()
