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
    parser = argparse.ArgumentParser()
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--browser", choices=["chromium", "firefox", "webkit"], default="chromium")
    parser.add_argument("--channel")
    parser.add_argument("--executable", type=Path)
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    try:
        validate_output(output)
        options = launch_options(arguments.browser, arguments.channel, arguments.executable)
        build = artifact(arguments.site)
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))
    output.mkdir(parents=True, exist_ok=True)
    site = arguments.site.resolve()
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(site)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    checks, errors = [], []
    report = {"checks": checks, "page_errors": errors, "build": build, "passed": False}
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, arguments.browser).launch(**options)
            report.update(browser_engine=arguments.browser, browser_version=browser.version, launch=options)
            context = browser.new_context(viewport={"width": 420, "height": 760})
            page = context.new_page()
            page.on("pageerror", lambda error: errors.append(str(error)))
            url = f"http://127.0.0.1:{server.server_port}"
            page.goto(url)
            page.wait_for_function("window.jadixNotes")
            key = "jadix.notes.v1"

            def state():
                return json.loads(page.evaluate("jadixNotes.state()"))

            def click(name):
                page.get_by_role("button", name=name, exact=True).click()

            def edit(hint, value):
                page.get_by_placeholder(hint, exact=True).fill(value)

            def saved():
                page.wait_for_function("!JSON.parse(jadixNotes.state()).storage_pending")
                assert state()["status"] == "Saved to device storage"

            def stored():
                return page.evaluate("key => localStorage.getItem(key)", key)

            assert page.locator("canvas").count() == 0
            assert state()["archive"]["notes"] == []
            assert page.get_by_text("No saved notes", exact=True).is_visible()
            checks.append("real native DOM notes controls and empty state")
            edit("Title / 标题", "First 中文😀")
            edit("Write a note / 正文", "First line\n第二行😀")
            click("Save note")
            saved()
            click("New note")
            edit("Title / 标题", "Second")
            edit("Write a note / 正文", "safe body")
            click("Save note")
            saved()
            assert len(json.loads(stored())["notes"]) == 2
            click("Open: First 中文😀")
            assert state()["draft"]["body"] == "First line\n第二行😀"
            edit("Write a note / 正文", "unsaved")
            click("Open: Second")
            assert state()["selected"] == "1" and "Save or revert" in state()["status"]
            click("Revert draft")
            assert not state()["dirty"]
            edit("Search notes / 搜索", "第二行")
            assert page.get_by_role("button", name="Open: First 中文😀", exact=True).count() == 1
            assert page.get_by_role("button", name="Open: Second", exact=True).count() == 0
            edit("Search notes / 搜索", "missing")
            assert page.get_by_text("No matching notes", exact=True).count() == 1
            edit("Search notes / 搜索", "")
            checks.append("Unicode multi-document save/open/search and dirty navigation guard/revert")
            page.reload()
            page.wait_for_function("window.jadixNotes")
            assert len(state()["archive"]["notes"]) == 2 and state()["selected"] is None
            click("Open: Second")
            assert state()["draft"]["body"] == "safe body"
            click("Delete note")
            click("Cancel")
            assert len(state()["archive"]["notes"]) == 2
            click("Delete note")
            click("Confirm")
            saved()
            assert len(json.loads(stored())["notes"]) == 1
            checks.append("reload retains documents; confirmed delete persists and cancel preserves")
            click("Export notebook")
            transfer = page.get_by_placeholder("Notebook JSON for export or import", exact=True)
            exported = json.loads(transfer.input_value())
            assert exported == json.loads(stored())
            before = stored()
            transfer.fill("{}")
            click("Import notebook")
            click("Confirm")
            assert stored() == before and len(state()["archive"]["notes"]) == 1
            assert "Unsupported notebook schema" in state()["status"]
            exported["notes"][0]["title"] = "Imported"
            transfer.fill(json.dumps(exported, ensure_ascii=False))
            click("Import notebook")
            click("Confirm")
            saved()
            assert not state()["replacement_confirmed"]
            assert json.loads(stored()) == exported
            checks.append("versioned export/import; invalid import atomicity; one-use replacement authorization")
            click("Open: Imported")
            before = stored()
            page.evaluate("() => {window.originalSetItem = Storage.prototype.setItem; Storage.prototype.setItem = function(){throw new DOMException('test quota','QuotaExceededError')};}")
            edit("Title / 标题", "Retry saved")
            click("Save note")
            assert state()["storage_pending"] and "Storage failed" in state()["status"]
            assert stored() == before
            page.evaluate("() => {Storage.prototype.setItem = window.originalSetItem;}")
            click("Save note")
            saved()
            assert json.loads(stored())["notes"][0]["title"] == "Retry saved"
            checks.append("quota failure visible, previous document preserved, explicit save retry recovers")
            other = context.new_page()
            other.on("pageerror", lambda error: errors.append(str(error)))
            other.goto(url)
            other.wait_for_function("window.jadixNotes")
            other.get_by_role("button", name="Open: Retry saved", exact=True).click()
            edit("Title / 标题", "Latest winner")
            click("Save note")
            saved()
            latest = stored()
            other.get_by_placeholder("Title / 标题", exact=True).fill("Stale loser")
            other.get_by_role("button", name="Save note", exact=True).click()
            stale = json.loads(other.evaluate("jadixNotes.state()"))
            assert stale["storage_pending"] and "another tab" in stale["status"]
            assert stored() == latest
            other.close()
            checks.append("observed cross-tab stale write refused without replacing newer document")
            page.evaluate("key => localStorage.setItem(key, '{invalid notebook')", key)
            page.reload()
            page.wait_for_function("window.jadixNotes")
            assert "Storage failed" in state()["status"]
            edit("Title / 标题", "Do not overwrite")
            click("Save note")
            assert stored() == "{invalid notebook" and state()["storage_pending"]
            edit("Notebook JSON for export or import", json.dumps(exported))
            click("Import notebook")
            click("Confirm")
            saved()
            assert json.loads(stored()) == exported
            checks.append("corrupt stored bytes preserved until explicit confirmed valid import")
            page.screenshot(path=str(output / "notes.png"), full_page=True)
            (output / "semantics.txt").write_text(page.locator("#app").aria_snapshot(), encoding="utf8")
            (output / "final-state.json").write_text(json.dumps(state(), indent=2, ensure_ascii=False), encoding="utf8")
            page.evaluate("jadixNotes.unmount()")
            assert page.locator("#app").inner_html() == ""
            assert json.loads(stored()) == exported
            checks.append("root disposal leaves no DOM and preserves durable notebook")
            assert not errors
            report.update({"passed": True, "browser": browser.version, "engine": arguments.browser, "launch": options,
                           "user_agent": page.evaluate("navigator.userAgent"),
                           "scope": "One observed engine/origin; semantic snapshot is not a screen-reader/OS accessibility audit. No physical IME, minimum-version, crash/power-loss or atomic multi-tab CAS guarantee. WebKit is the Playwright build, not Safari application acceptance."})
            browser.close()
    finally:
        server.shutdown()
        server.server_close()
        (output / "notes.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf8")
    print(json.dumps({"passed": report["passed"], "checks": len(checks)}, indent=2))


if __name__ == "__main__":
    main()
