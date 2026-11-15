import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import json
from pathlib import Path
import threading
from native_browser import artifact, launch_options, validate_output


def require_popover_build(build):
    if build.get("popover") is not True or build.get("selection") is not False or build.get("extended") is not False or type(build.get("retained_rows")) is not int or build["retained_rows"] != 0:
        raise ValueError("requires the isolated --popover release artifact")


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def main():
    from playwright.sync_api import sync_playwright

    parser = argparse.ArgumentParser(description="Verify native DOM Popover and shared Rust controls")
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
        require_popover_build(build)
        options = launch_options(args.browser, args.channel, args.executable)
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
    output.mkdir(parents=True, exist_ok=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(args.site.resolve())))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    report = {"passed": False, "build": build, "checks": [], "page_errors": [],
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "limits": ["Owned browser pointer/touch/keyboard input plus separately identified native-API and DOM probes; no physical touch hardware or IME.",
                         "ARIA snapshots are not OS screen-reader evidence; Playwright WebKit is not Safari or an OS minimum-version test.",
                         "Only DOM Popover is exercised; Android and UIKit hosts remain unsupported."]}
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, args.browser).launch(**options)
            report.update(browser=args.browser, browser_version=browser.version, launch=options)
            page = browser.new_page(viewport={"width": 420, "height": 760}, has_touch=True)
            page.on("pageerror", lambda error: report["page_errors"].append(str(error)))
            page.goto("http://127.0.0.1:" + str(server.server_port))
            page.wait_for_function("window.jadixNative")
            trigger = page.get_by_role("button", name="Edit profile", exact=True)
            profile = page.get_by_role("dialog", name="Profile", exact=True)
            details = page.get_by_role("dialog", name="Details", exact=True)
            other = page.get_by_role("dialog", name="Other", exact=True)
            field = page.get_by_placeholder("Profile name", exact=True)
            background = page.get_by_role("button", name="Background action", exact=True)
            scroll = page.locator('[data-jadix-kind="scroll"]')
            def state():
                return json.loads(page.evaluate("jadixNative.state()"))
            def opened(panel, expected=True):
                if expected:
                    panel.wait_for(state="visible")
                    assert panel.evaluate("element => element.matches(':popover-open')")
                else:
                    panel.wait_for(state="hidden")
            def focused(control):
                assert control.evaluate("element => element === document.activeElement")
            def no_error():
                assert page.evaluate("jadixNative.take_error()") is None
            def bounded(panel):
                bounds = panel.bounding_box()
                viewport = page.viewport_size
                assert bounds["x"] >= 0 and bounds["y"] >= 0
                assert bounds["x"] + bounds["width"] <= viewport["width"] + 1
                assert bounds["y"] + bounds["height"] <= viewport["height"] + 1
            assert page.locator("canvas").count() == 0
            assert page.locator("[popover='manual']").count() == 3
            assert page.locator(":popover-open").count() == 0
            assert trigger.get_attribute("aria-expanded") == "false"
            page.evaluate("window.nativePopoverEvents=[]; window.pointerEvents=[]; document.addEventListener('pointerdown', e=>pointerEvents.push({trusted:e.isTrusted,type:e.pointerType}),true); document.querySelectorAll('[popover]').forEach(panel=>{ for(const type of ['beforetoggle','toggle']) panel.addEventListener(type,e=>nativePopoverEvents.push({type,trusted:e.isTrusted,newState:e.newState})); });")
            report["checks"].append("real manual HTML Popover nodes, closed controlled state and no canvas")
            trigger.tap()
            opened(profile)
            focused(field)
            bounded(profile)
            assert any(event["trusted"] and event["type"] == "touch" for event in page.evaluate("pointerEvents"))
            assert trigger.get_attribute("aria-expanded") == "true"
            assert profile.get_attribute("id") in trigger.get_attribute("aria-controls").split()
            report["checks"].append("trusted browser touch opens native top layer, resolves ARIA anchor and focuses native input")
            page.evaluate("window.externalButton=document.createElement('button'); externalButton.textContent='External focus'; document.body.append(externalButton); externalButton.focus();")
            page.keyboard.press("Escape")
            opened(profile)
            page.evaluate("externalButton.remove(); window.preventEscape=e=>{if(e.key==='Escape')e.preventDefault();}; window.addEventListener('keydown',preventEscape,true);")
            field.focus()
            page.keyboard.press("Escape")
            opened(profile)
            page.evaluate("window.removeEventListener('keydown',preventEscape,true)")
            report["checks"].append("trusted Escape outside this root or already prevented does not dismiss its panel")
            field.fill("中文 😀 é")
            assert state()["value"] == "中文 😀 é"
            field.evaluate("element => { window.retainedField=element; window.retainedPanel=element.closest('[popover]'); element.setSelectionRange(3,5,'backward'); }")
            page.set_viewport_size({"width": 360, "height": 620})
            page.wait_for_timeout(80)
            opened(profile)
            focused(field)
            assert field.evaluate("element => element===retainedField && element.selectionStart===3 && element.selectionEnd===5 && element.selectionDirection==='backward'")
            assert profile.evaluate("element => element===retainedPanel")
            bounded(profile)
            report["checks"].append("Unicode commits round-trip to Rust; resize retains panel, input focus and reverse UTF-16 selection")
            field.press("Tab")
            focused(page.get_by_role("button", name="Save profile", exact=True))
            for _ in range(4):
                page.keyboard.press("Tab")
                if not profile.evaluate("element => element.contains(document.activeElement)"):
                    break
            assert not profile.evaluate("element => element.contains(document.activeElement)")
            opened(profile)
            report["checks"].append("nonmodal native Tab order reaches background controls without trapping focus")
            field.focus()
            field.press("Escape")
            opened(profile, False)
            focused(trigger)
            assert trigger.get_attribute("aria-expanded") == "false"
            report["checks"].append("trusted Escape closes controlled panel and restores its trigger focus")
            trigger.click()
            opened(profile)
            trigger.click()
            opened(profile, False)
            focused(trigger)
            report["checks"].append("trigger pointer toggle closes one controlled panel without outside-dismiss double handling")
            trigger.click()
            opened(profile)
            before = state()["count"]
            background.click()
            opened(profile, False)
            assert state()["count"] == before + 1
            focused(background)
            report["checks"].append("outside pointer closes panel while preserving background activation and focus")
            trigger.click()
            opened(profile)
            more = page.get_by_role("button", name="More options", exact=True)
            more.click()
            opened(details)
            bounded(details)
            page.keyboard.press("Escape")
            opened(details, False)
            opened(profile)
            focused(more)
            more.click()
            before = state()["count"]
            page.get_by_role("button", name="Apply option", exact=True).click()
            opened(details, False)
            opened(profile)
            assert state()["count"] == before + 1
            focused(more)
            report["checks"].append("nested context callback and Escape close only child and restore parent trigger focus")
            more.click()
            opened(details)
            page.get_by_role("button", name="Other panel", exact=True).evaluate("element => element.click()")
            opened(other)
            page.keyboard.press("Escape")
            opened(other, False)
            opened(details)
            page.keyboard.press("Escape")
            opened(details, False)
            opened(profile)
            report["checks"].append("programmatic independent-panel opening and trusted Escape respect opening order across different node generations")
            field.focus()
            before_anchor = trigger.bounding_box()["y"]
            before_panel = profile.bounding_box()["y"]
            scroll.evaluate("element => { element.scrollTop += 36; }")
            page.wait_for_timeout(80)
            after_anchor = trigger.bounding_box()["y"]
            after_panel = profile.bounding_box()["y"]
            assert abs((after_anchor - before_anchor) - (after_panel - before_panel)) < 1
            assert after_anchor < before_anchor
            focused(field)
            bounded(profile)
            report["checks"].append("native scroll translates the visible anchor and top-layer panel together without losing input focus")
            page.get_by_role("button", name="Reset name", exact=True).evaluate("element => element.click()")
            opened(profile)
            assert field.input_value() == "Reset 中文 😀" and state()["value"] == "Reset 中文 😀"
            page.get_by_role("button", name="Save profile", exact=True).click()
            opened(profile, False)
            trigger.click()
            opened(profile)
            assert field.evaluate("element=>element===retainedField")
            assert field.input_value() == "Reset 中文 😀"
            report["checks"].append("external controlled reset and contextual close/reopen preserve native input identity and committed value")
            profile.evaluate("element => element.hidePopover()")
            opened(profile, False)
            page.wait_for_function("document.querySelector('button[aria-controls]').getAttribute('aria-expanded')==='false'")
            page.locator("[popover][aria-label='Profile']").evaluate("element => element.showPopover()")
            assert page.locator(":popover-open").count() == 0
            report["checks"].append("programmatic native hide synchronizes controlled state; unauthorized native show is canceled")
            bottom_trigger = page.get_by_role("button", name="Bottom panel", exact=True)
            bottom_trigger.click()
            bottom = page.get_by_role("dialog", name="Bottom", exact=True)
            opened(bottom)
            bounded(bottom)
            bounds = bottom.bounding_box()
            anchor = bottom_trigger.bounding_box()
            assert bounds["y"] + bounds["height"] <= anchor["y"] - 3
            page.keyboard.press("Escape")
            opened(bottom, False)
            report["checks"].append("native scrolled bottom anchor flips panel above and clamps it to the visible viewport")
            scroll.evaluate("element => element.scrollTop=0")
            page.wait_for_timeout(50)
            trigger.click()
            opened(profile)
            (output / "native-popover.png").write_bytes(page.screenshot(full_page=True))
            (output / "accessibility.yaml").write_text(page.locator("body").aria_snapshot(), encoding="utf8")
            report["native_events"] = page.evaluate("nativePopoverEvents")
            assert any(event["trusted"] for event in report["native_events"])
            page.evaluate("window.oldTrigger=document.querySelector('[popover]').previousElementSibling; window.oldField=retainedField; window.oldPanel=retainedPanel; jadixNative.unmount(); jadixNative.mount('app'); oldTrigger.click(); oldField.value='stale'; oldField.dispatchEvent(new Event('input',{bubbles:true}));")
            assert state() == {"count": 0, "value": ""}
            assert page.locator(":popover-open").count() == 0
            assert page.evaluate("['aria-haspopup','aria-controls','aria-expanded'].every(name=>!oldTrigger.hasAttribute(name))")
            no_error()
            report["checks"].append("unmount removes top-layer panels and stale native control callbacks cannot mutate a fresh root")
            unsupported = page.evaluate("() => { jadixNative.unmount(); const descriptor=Object.getOwnPropertyDescriptor(HTMLElement.prototype,'showPopover'); Object.defineProperty(HTMLElement.prototype,'showPopover',{value:undefined,configurable:true}); let failure=''; try { jadixNative.mount('app'); } catch(error) { failure=String(error); } Object.defineProperty(HTMLElement.prototype,'showPopover',descriptor); return {failure,children:document.getElementById('app').children.length}; }")
            assert "Unsupported(Popover)" in unsupported["failure"] and unsupported["children"] == 0
            page.evaluate("jadixNative.mount('app')")
            trigger.click()
            opened(profile)
            page.keyboard.press("Escape")
            opened(profile, False)
            report["checks"].append("missing native Popover API rejects mount before publication and a supported fresh root recovers")
            warm_baseline = page.locator("#app *").count()
            page.evaluate("jadixNative.unmount(); jadixNative.mount('app')")
            cold_baseline = page.locator("#app *").count()
            for _ in range(10):
                trigger.click()
                opened(profile)
                assert page.locator("#app *").count() == warm_baseline
                page.evaluate("jadixNative.unmount(); jadixNative.mount('app')")
                assert page.locator(":popover-open").count() == 0
                assert page.locator("#app *").count() == cold_baseline
            no_error()
            assert not report["page_errors"]
            report["checks"].append("ten open/unmount/remount cycles leave bounded native nodes and no stale top-layer state")
            report["passed"] = True
            browser.close()
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        server.shutdown()
        server.server_close()
        (output / "checks.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print("PASS native Popover: " + str(len(report["checks"])) + " scenarios")


if __name__ == "__main__":
    main()
