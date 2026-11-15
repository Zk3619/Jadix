"""Replay the Field product against an identified local DOM build; no physical IME/AT claims."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading
from playwright.sync_api import sync_playwright
from native_browser import artifact, launch_options, validate_output


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--browser", choices=["chromium", "firefox", "webkit"], default="chromium")
    parser.add_argument("--channel")
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    validate_output(output)
    build = artifact(arguments.site, "field")
    output.mkdir(parents=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(arguments.site.resolve())))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    report = {"build": build, "checks": [], "errors": [], "passed": False,
              "physical_ime_accepted": False, "assistive_technology_accepted": False,
              "performance_comparison": False, "runtime_acceptance": False}
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, arguments.browser).launch(**launch_options(arguments.browser, arguments.channel))
            report.update(browser=arguments.browser, browser_version=browser.version)
            page = browser.new_page(viewport={"width": 480, "height": 800})
            page.on("pageerror", lambda error: report["errors"].append(str(error)))
            url = f"http://127.0.0.1:{server.server_port}"
            page.goto(url)
            page.wait_for_function("window.jadixField")
            state = lambda: json.loads(page.evaluate("jadixField.state()"))
            click = lambda caption: page.get_by_role("button", name=caption, exact=True).click()
            click("Edit request")
            click("Review request")
            assert state()["navigation"]["entries"][-1] == "edit"
            assert page.get_by_text("A contact name is required", exact=True).is_visible()
            report["checks"].append("required-field-validation-keeps-editor")
            name = "Contact 東京 😀"
            page.get_by_role("textbox", name="Contact name", exact=True).fill(name)
            page.get_by_role("textbox", name="Work details", exact=True).fill("Repair door\nRetain original lock")
            page.get_by_role("combobox", name="Request priority", exact=True).select_option("urgent")
            page.get_by_role("checkbox", name="Confirm details", exact=True).check()
            click("Review request")
            assert state()["navigation"]["entries"][-1] == "review"
            assert page.get_by_role("textbox").count() == 0
            page.screenshot(path=str(output / "review.png"))
            click("Back to editor")
            assert page.get_by_role("textbox", name="Contact name", exact=True).input_value() == name
            report["checks"].append("unicode-review-back-and-hidden-editors")
            before = state()
            page.get_by_role("textbox", name="Contact name", exact=True).fill("x" * 257)
            assert state() == before
            assert page.get_by_role("textbox", name="Contact name", exact=True).input_value() == name
            report["checks"].append("over-limit-edit-restores-native-text")
            page.reload(); page.wait_for_function("window.jadixField")
            assert state() == before
            assert page.get_by_role("textbox", name="Contact name", exact=True).input_value() == name
            report["checks"].append("page-reload-restores-unsaved-draft-and-route")
            click("Review request"); click("Save request")
            saved = state()
            assert saved["revision"] == 1 and saved["saved"]["name"] == name
            click("Edit request")
            page.get_by_role("textbox", name="Contact name", exact=True).fill("Cancelled")
            click("Cancel draft")
            assert state() == saved
            report["checks"].append("save-and-cancel-preserve-committed-request")
            page.reload(); page.wait_for_function("window.jadixField")
            assert state() == saved
            report["checks"].append("saved-request-reloads")
            page.evaluate("localStorage.setItem('jadix.field.v1', 'external writer')")
            click("Edit request")
            assert page.evaluate("localStorage.getItem('jadix.field.v1')") == "external writer"
            assert page.get_by_text("Storage failed:", exact=False).is_visible()
            report["checks"].append("external-writer-conflict-is-not-overwritten")
            page.reload(); page.wait_for_function("window.jadixField")
            click("Edit request")
            assert page.evaluate("localStorage.getItem('jadix.field.v1')") == "external writer"
            assert state()["saved"] is None
            report["checks"].append("corrupt-storage-blocks-replacement")
            assert page.evaluate("jadixField.take_error()") is None
            assert not report["errors"]
            report["passed"] = True
            browser.close()
    finally:
        server.shutdown(); server.server_close()
        (output / "checks.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if not report["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
