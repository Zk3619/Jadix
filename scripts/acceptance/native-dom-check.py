import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import json
import re
from pathlib import Path
import threading
import statistics
from playwright.sync_api import sync_playwright
from native_browser import artifact, launch_options, validate_output


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def composition(field, text, session, *, commit=False):
    if session is not None:
        method = "Input.insertText" if commit else "Input.imeSetComposition"
        length = len(text.encode("utf-16-le")) // 2
        payload = {"text": text} if commit else {"text": text, "selectionStart": length, "selectionEnd": length}
        session.send(method, payload)
        return
    field.evaluate("""(input, {text, commit}) => {
        let range = input.__jadixCompositionProbe;
        if (!range) {
            range = {start: input.selectionStart, end: input.selectionEnd};
            input.__jadixCompositionProbe = range;
            input.dispatchEvent(new CompositionEvent('compositionstart', {bubbles:true, data:''}));
        }
        input.setRangeText(text, range.start, range.end, 'end');
        range.end = range.start + text.length;
        if (commit) {
            delete input.__jadixCompositionProbe;
            input.dispatchEvent(new CompositionEvent('compositionend', {bubbles:true, data:text}));
        } else {
            input.dispatchEvent(new CompositionEvent('compositionupdate', {bubbles:true, data:text}));
        }
        input.dispatchEvent(new InputEvent('input', {bubbles:true, data:text, inputType:commit ? 'insertText' : 'insertCompositionText', isComposing:!commit}));
    }""", {"text": text, "commit": commit})


def retained_focus_after_repair(page, control, selection=None):
    control.focus()
    if selection is not None:
        control.evaluate("(element, range) => element.setSelectionRange(...range, 'backward')", selection)
    before = control.evaluate("""element => {
        const node = element.closest('[data-jadix-kind="checkbox"],[data-jadix-kind="switch"]') ?? element;
        const previous = node.previousElementSibling;
        const next = node.nextElementSibling;
        if (!previous || !next) throw new Error('focus repair probe requires adjacent siblings');
        const parent = node.parentElement;
        const order = [...parent.children];
        parent.insertBefore(next, previous);
        window.__jadixFocusRepair = {element, parent, order, width:parent.getBoundingClientRect().width};
        return {focused:document.activeElement === element, value:element.value,
            start:element.selectionStart, end:element.selectionEnd, direction:element.selectionDirection,
            scroll:[parent.scrollLeft, parent.scrollTop, window.scrollX, window.scrollY]};
    }""")
    assert before['focused'], 'probe moved focus before native publication'
    viewport = page.viewport_size
    page.set_viewport_size(dict(width=viewport['width'] + 8, height=viewport['height']))
    page.wait_for_function("() => {const {parent, order} = window.__jadixFocusRepair; return order.every((node,index) => parent.children[index] === node);}")
    after = control.evaluate("""element => ({focused:document.activeElement === element,
        same:window.__jadixFocusRepair.element === element && element.isConnected,
        value:element.value, start:element.selectionStart, end:element.selectionEnd,
        direction:element.selectionDirection,
        scroll:[window.__jadixFocusRepair.parent.scrollLeft, window.__jadixFocusRepair.parent.scrollTop,
            window.scrollX, window.scrollY]})""")
    assert after.pop('same'), 'native publication replaced the retained control'
    assert after == before, ('native publication lost retained focus or selection', before, after)
    page.set_viewport_size(viewport)
    page.wait_for_function("() => {const {parent, width} = window.__jadixFocusRepair; return Math.abs(parent.getBoundingClientRect().width - width) < 0.01;}")
    page.evaluate('delete window.__jadixFocusRepair')


def keyboard_navigation(page, extended):
    controls = ([
        page.get_by_role("checkbox", name="Accept", exact=True),
        page.get_by_role("switch", name="Airplane mode", exact=True),
        page.get_by_placeholder("Password / 密码", exact=True),
        page.get_by_placeholder("Multiline / 多行", exact=True),
        page.get_by_role("button", name="Open editor", exact=True),
    ] if extended else []) + [
        page.get_by_role("button", name="Continue", exact=True),
        page.get_by_placeholder("Name / 中文", exact=True),
        page.get_by_role("button", name="Reset", exact=True),
    ]
    before = page.evaluate("jadixNative.state()")
    trace = []
    controls[0].focus()
    page.evaluate("""() => {
        window.__jadixTabEvents = [];
        window.__jadixTabListener = event => {
            if (event.key === 'Tab' && __jadixTabEvents.length < 32)
                __jadixTabEvents.push({key:event.key, shift:event.shiftKey, trusted:event.isTrusted});
        };
        document.addEventListener('keydown', __jadixTabListener, true);
    }""")
    for direction, indices in (("forward", range(len(controls))), ("reverse", range(len(controls) - 2, -1, -1))):
        for index in indices:
            control = controls[index]
            if direction == "forward" and index > 0:
                page.keyboard.press("Tab")
            elif direction == "reverse":
                page.keyboard.press("Shift+Tab")
            assert control.evaluate("element => document.activeElement === element"), (direction, index, "unexpected focus")
            visible = page.wait_for_function("""element => {
                let rect = element.getBoundingClientRect();
                let visible = {left:Math.max(0, rect.left), top:Math.max(0, rect.top),
                    right:Math.min(innerWidth, rect.right), bottom:Math.min(innerHeight, rect.bottom)};
                for (let parent = element.parentElement; parent; parent = parent.parentElement) {
                    const style = getComputedStyle(parent);
                    const bounds = parent.getBoundingClientRect();
                    if (/(auto|scroll|hidden|clip)/.test(style.overflowX)) {
                        visible.left = Math.max(visible.left, bounds.left); visible.right = Math.min(visible.right, bounds.right);
                    }
                    if (/(auto|scroll|hidden|clip)/.test(style.overflowY)) {
                        visible.top = Math.max(visible.top, bounds.top); visible.bottom = Math.min(visible.bottom, bounds.bottom);
                    }
                }
                if (visible.right-visible.left < rect.width-1 || visible.bottom-visible.top < rect.height-1) return false;
                return {name:element.getAttribute('aria-label') || element.getAttribute('placeholder') || element.textContent || element.type,
                    width:visible.right-visible.left, height:visible.bottom-visible.top,
                    controlWidth:rect.width, controlHeight:rect.height,
                    scrollTop:document.querySelector('[data-jadix-kind="scroll"]').scrollTop};
            }""", arg=control.element_handle(), timeout=5000).json_value()
            assert visible["width"] >= visible["controlWidth"] - 1 and visible["height"] >= visible["controlHeight"] - 1, (direction, index, "focused control is clipped", visible)
            trace.append({"direction": direction, "index": index, **visible})
    events = page.evaluate("""() => {
        document.removeEventListener('keydown', __jadixTabListener, true);
        const events = __jadixTabEvents;
        delete window.__jadixTabListener; delete window.__jadixTabEvents;
        return events;
    }""")
    assert len(events) == 2 * (len(controls) - 1) and all(event["trusted"] for event in events), "Missing trusted Tab events"
    assert [event["shift"] for event in events] == [False] * (len(controls) - 1) + [True] * (len(controls) - 1), "Unexpected Tab directions"
    assert page.evaluate("jadixNative.state()") == before, "Tab navigation changed business state"
    assert page.evaluate("jadixNative.take_error()") is None, "Tab navigation emitted a rejected native event"
    return {"initial_focus": "programmatic focus on first authored control", "steps": trace, "keyboard_events": events}


def native_edit_history(page, extended):
    controls = [("input:not([type=password]):not([type=checkbox])", "value")]
    if extended:
        controls.extend([("input[type=password]", None), ("textarea", "note")])
    observations = []
    for selector, state_key in controls:
        control = page.locator(selector).first
        assert control.input_value() == "", "history scenario requires a fresh editor"
        control.focus()
        page.evaluate("""() => {
            window.__jadixHistoryEvents = [];
            window.__jadixHistoryListener = event => {
                __jadixHistoryEvents.push({trusted:event.isTrusted, inputType:event.inputType,
                    kind:event.target.tagName, length:event.target.value.length});
            };
            document.addEventListener('input', __jadixHistoryListener, true);
        }""")
        value = "中文😀 e\u0301 אבג"
        page.keyboard.insert_text(value)
        if selector == "textarea":
            page.keyboard.press("Enter")
            page.keyboard.insert_text("第二行 مرحبا")
            value += "\n第二行 مرحبا"

        def verify(expected, phase):
            assert control.input_value() == expected, (selector, phase, "platform value")
            state = json.loads(page.evaluate("jadixNative.state()"))
            if state_key == "value":
                assert state["value"] == expected, (selector, phase, "Rust value")
            elif state_key == "note":
                assert state["extensions"]["note"] == expected, (selector, phase, "Rust note")
            else:
                assert expected not in (state["value"], state["extensions"]["note"]) or not expected
            selection = control.evaluate("element => [element.selectionStart, element.selectionEnd]")
            boundaries = {0}
            offset = 0
            for character in expected:
                offset += len(character.encode("utf-16-le")) // 2
                boundaries.add(offset)
            assert all(index in boundaries for index in selection), (selector, phase, "split UTF-16 selection", selection)
            assert page.evaluate("jadixNative.take_error()") is None, (selector, phase, "rejected event")
            return {"phase": phase, "utf16_length": offset, "selection": selection}

        steps = [verify(value, "trusted Unicode insertion")]
        page.keyboard.press("Control+a")
        page.keyboard.press("Backspace")
        steps.append(verify("", "selection deletion"))
        page.keyboard.press("Control+z")
        steps.append(verify(value, "native undo"))
        viewport = page.viewport_size
        page.set_viewport_size({"width": viewport["width"] - 12, "height": viewport["height"]})
        page.wait_for_function("Math.abs(document.querySelector('[data-jadix-kind=scroll]').getBoundingClientRect().width - innerWidth) < 1")
        steps.append(verify(value, "Rust environment publication"))
        page.set_viewport_size(viewport)
        page.wait_for_function("Math.abs(document.querySelector('[data-jadix-kind=scroll]').getBoundingClientRect().width - innerWidth) < 1")
        assert control.evaluate("element => document.activeElement === element"), "environment publication lost editor focus"
        retained_focus_after_repair(page, control)
        steps.append(verify(value, "retained structural repair"))
        page.keyboard.press("Control+Shift+z")
        steps.append(verify("", "native redo after publication and structural repair"))
        events = page.evaluate("""() => {
            document.removeEventListener('input', __jadixHistoryListener, true);
            const events = __jadixHistoryEvents;
            delete window.__jadixHistoryEvents; delete window.__jadixHistoryListener;
            return events;
        }""")
        assert len(events) >= 4 and all(event["trusted"] for event in events), (selector, "untrusted or missing input events", events)
        assert all(event["length"] <= len(value.encode("utf-16-le")) // 2 for event in events)
        observations.append({"selector": selector, "steps": steps, "input_events": events,
                             "password_state": "not exported by sample; retained through Rust publication" if state_key is None else None})
    return observations


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--site", type=Path, default=Path(".theme-evidence/native-controls/web"))
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/native-controls/dom"))
    parser.add_argument("--browser", choices=["chromium", "firefox", "webkit"], default="chromium")
    parser.add_argument("--channel")
    parser.add_argument("--executable", type=Path, help="Existing Chromium executable when channel discovery cannot locate the installed browser")
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--large-list", action="store_true")
    arguments = parser.parse_args()
    arguments.output = arguments.output.resolve()
    rows = 1000 if arguments.large_list else 100
    try:
        validate_output(arguments.output)
        options = launch_options(arguments.browser, arguments.channel, arguments.executable)
        build = artifact(arguments.site, "controls", release=False)
        if build.get("extended") is not arguments.extended or build.get("retained_rows") != rows:
            raise ValueError("artifact control features or retained rows differ from requested checks")
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))
    arguments.output.mkdir(parents=True, exist_ok=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(arguments.site.resolve())))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    checks, errors = [], []
    report = {"passed": False, "browser_engine": arguments.browser, "channel": options.get("channel"),
              "browser_executable": options.get("executable_path"),
              "retained_rows": rows, "build": build, "checks": checks, "page_errors": errors,
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "composition_transport": "Chromium CDP IME" if arguments.browser == "chromium" else "synthetic DOM composition events; not native IME",
              "limits": ["Owned browser controls; keyboard/pointer actions and synthetic script probes are distinguished. No physical IME or screen-reader acceptance.",
                         "Chromium alone uses CDP IME and raw AX capture. Firefox/WebKit composition is an untrusted DOM adapter probe; ARIA snapshots are not OS accessibility trees.",
                         "One installed version per selected engine; Playwright WebKit is not Safari application or minimum-version evidence.",
                         "Structural repair explicitly perturbs sibling DOM order then triggers Rust publication by resize; not an application-authored reorder or physical keyboard probe.",
                         "Finite script timing and rAF waits are not physical input-to-display latency, peak memory, leak freedom or an absolute performance SLA."]}
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, arguments.browser).launch(**options)
            report["browser"] = browser.version
            page = browser.new_page(viewport={"width": 420, "height": 760})
            page.add_init_script("""window.jadixInputTrace = [];
                for (const type of ['compositionstart', 'compositionupdate', 'compositionend', 'input']) {
                    document.addEventListener(type, event => {
                        if (jadixInputTrace.length < 512) jadixInputTrace.push({type, trusted:event.isTrusted,
                            placeholder:event.target.placeholder, length:event.target.value?.length,
                            composing:event.isComposing ?? null, inputType:event.inputType ?? null,
                            data:event.data?.slice(0,64) ?? null});
                    }, true);
                }""")
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.goto(f"http://127.0.0.1:{server.server_port}")
            page.wait_for_function("window.jadixNative && document.querySelector('input')")
            assert page.locator("canvas").count() == 0
            assert page.locator("span").count() >= 102
            assert page.locator("span").filter(has_text=re.compile(r"^Row \d{3} —")).count() == rows
            assert page.get_by_role("button").count() == (3 if arguments.extended else 2)
            checks.append(f"real DOM controls and {rows} retained rows; no canvas")
            report["keyboard_navigation"] = {"initial": keyboard_navigation(page, arguments.extended)}
            checks.append("full authored forward/reverse Tab order keeps every native control visible without business edits")
            report["native_edit_history"] = native_edit_history(page, arguments.extended)
            checks.append("trusted mixed Unicode insertion, native selection deletion, undo and redo retain Rust values and UTF-16 boundaries across environment publication")
            page.set_viewport_size({"width": 420, "height": 480})
            page.wait_for_function("Math.abs(document.querySelector('[data-jadix-kind=scroll]').getBoundingClientRect().height - 480) < 1")
            report["keyboard_navigation"]["compact"] = keyboard_navigation(page, arguments.extended)
            page.set_viewport_size({"width": 420, "height": 760})
            page.wait_for_function("Math.abs(document.querySelector('[data-jadix-kind=scroll]').getBoundingClientRect().height - 760) < 1")
            checks.append("compact viewport retains complete forward/reverse Tab order and focused-control visibility")
            button = page.get_by_role("button", name="Continue", exact=True)
            button.click()
            button.press("Enter")
            assert json.loads(page.evaluate("jadixNative.state()"))["count"] == 2
            checks.append("pointer and native keyboard button activation update Rust state")
            field = page.get_by_placeholder("Name / 中文", exact=True)
            button.press("Tab")
            assert field.evaluate("input => document.activeElement === input")
            checks.append("native Tab order moves focus from button to input")
            field.fill("中文😀")
            assert json.loads(page.evaluate("jadixNative.state()"))["value"] == "中文😀"
            session = page.context.new_cdp_session(page) if arguments.browser == "chromium" else None
            composition(field, "pin😀", session)
            assert json.loads(page.evaluate("jadixNative.state()"))["value"] == "中文😀"
            field.evaluate("element => element.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', bubbles:true, cancelable:true}))")
            assert json.loads(page.evaluate("jadixNative.state()"))["count"] == 2
            composition(field, "拼", session)
            composition(field, "拼", session, commit=True)
            assert json.loads(page.evaluate("jadixNative.state()"))["value"] == "中文😀拼"
            checks.append("UTF-16 Chinese emoji edits and draft/commit via " + report["composition_transport"])
            page.get_by_role("button", name="Reset", exact=True).click()
            assert field.input_value() == ""
            checks.append("controlled reset preserves retained input node")
            assert page.evaluate("jadixNative.take_error()") is None
            field.evaluate("element => {element.value = 'x'.repeat(512*1024); element.dispatchEvent(new Event('input'))}")
            assert field.evaluate("element => element.value.length") == 512 * 1024
            assert page.evaluate("JSON.parse(jadixNative.state()).value.length") == 512 * 1024
            assert page.evaluate("jadixNative.take_error()") is None
            page.get_by_role("button", name="Reset", exact=True).click()
            overflow = field.evaluate("""element => {
                element.value = '中'.repeat(400000);
                const observed = {native_utf16:element.value.length, utf8_bytes:new TextEncoder().encode(element.value).length};
                element.dispatchEvent(new Event('input'));
                return observed;
            }""")
            assert overflow == {"native_utf16": 400000, "utf8_bytes": 1200000}, overflow
            report["oversized_edit"] = overflow
            page.evaluate("() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))")
            assert page.evaluate("typeof jadixNative.take_error()") == "string"
            assert page.evaluate("jadixNative.take_error()") is None
            assert json.loads(page.evaluate("jadixNative.state()"))["value"] == ""
            assert field.input_value() == "", "rejected DOM edit remained visible instead of the last published value"
            field.fill("Recovered 中文😀")
            assert json.loads(page.evaluate("jadixNative.state()"))["value"] == "Recovered 中文😀"
            field.fill("A😀אב e\u0301中")
            retained_focus_after_repair(page, field, [1, 3])
            checks.append("retained Input focus, backward emoji selection and scroll survive native structural repair")
            field.press("Backspace")
            assert field.input_value() == "Aאב e\u0301中"
            assert json.loads(page.evaluate("jadixNative.state()"))["value"] == "Aאב e\u0301中"
            field.press("Control+z")
            assert field.input_value() == "A😀אב e\u0301中"
            assert json.loads(page.evaluate("jadixNative.state()"))["value"] == "A😀אב e\u0301中"
            field.fill("Recovered 中文😀")
            checks.append("native emoji selection deletion and undo preserve RTL and combining characters after rejected edit recovery and structural repair")
            edit_error = page.evaluate("jadixNative.take_error()")
            assert edit_error is None, ("edit", edit_error)
            button.click()
            assert json.loads(page.evaluate("jadixNative.state()"))["count"] == 3
            field.press("Enter")
            submitted = json.loads(page.evaluate("jadixNative.state()"))
            assert submitted["count"] == 4 and submitted["value"] == "Recovered 中文😀", submitted
            field.press("Shift+Enter")
            assert json.loads(page.evaluate("jadixNative.state()"))["count"] == 5
            ignored_submit_keys = field.evaluate("""element => {
                const variants = [{repeat:true}, {isComposing:true}, {keyCode:229},
                    {ctrlKey:true}, {altKey:true}, {metaKey:true}, {key:'Escape'}];
                for (const options of variants) element.dispatchEvent(new KeyboardEvent('keydown',
                    {key:'Enter', bubbles:true, cancelable:true, ...options}));
                return variants.length;
            }""")
            assert ignored_submit_keys == 7
            assert json.loads(page.evaluate("jadixNative.state()"))["count"] == 5
            checks.append("trusted Enter and Shift+Enter submit the retained Unicode input; IME drafts, repeat, composition keyCode 229 and modified keys cannot submit")
            click_error = page.evaluate("jadixNative.take_error()")
            assert click_error is None, ("click", click_error)
            page.get_by_role("button", name="Reset", exact=True).click()
            recovered_error = page.evaluate("jadixNative.take_error()")
            assert recovered_error is None, recovered_error
            checks.append("oversized native edit is rejected with a drainable error; subsequent Unicode edits and clicks recover")
            scroll = page.locator('[data-jadix-kind="scroll"]').first
            assert scroll.count() == 1
            scroll.evaluate("element => element.scrollTop = 900")
            assert scroll.evaluate("element => element.scrollTop") > 0
            page.set_viewport_size({"width": 640, "height": 480})
            page.wait_for_function("document.querySelector('input').getBoundingClientRect().width > 0")
            assert scroll.evaluate("element => element.scrollTop") > 0
            assert button.evaluate("element => element.getBoundingClientRect().height") >= 44
            assert field.evaluate("element => element.getBoundingClientRect().height") >= 44
            checks.append("native scrolling and retained offset across viewport resize")
            if session is not None:
                session.send("Emulation.setPageScaleFactor", {"pageScaleFactor": 1.5})
            else:
                page.evaluate("document.documentElement.style.zoom = '1.5'")
            field.fill("zoom")
            assert json.loads(page.evaluate("jadixNative.state()"))["value"] == "zoom"
            if session is not None:
                session.send("Emulation.setPageScaleFactor", {"pageScaleFactor": 1})
            else:
                page.evaluate("document.documentElement.style.zoom = ''")
            checks.append("native input remains operational under " + ("CDP page scale" if session is not None else "CSS zoom; not browser UI zoom"))
            font = page.evaluate("""async () => {
                const face = new FontFace('JadixNativeProbe', 'local(Arial), local("Liberation Sans"), local("DejaVu Sans")');
                await face.load(); document.fonts.add(face); await document.fonts.ready;
                document.fonts.dispatchEvent(new Event('loadingdone'));
                return face.status;
            }""")
            assert font == "loaded"
            checks.append("browser FontFace load and environment invalidation")
            if arguments.extended:
                image = page.get_by_role("img", name="Native image", exact=True)
                image.wait_for()
                page.wait_for_function("document.querySelector('img').complete && document.querySelector('img').naturalWidth === 1")
                assert image.evaluate("element => element.getBoundingClientRect().width") == 24
                checks.append("bounded embedded PNG decodes into native DOM image with accessible alternative")
                failed_image = page.get_by_role("img", name="Unavailable image", exact=True)
                page.wait_for_function("document.querySelector('img[alt=\"Unavailable image\"]').dataset.jadixImageState === 'error'")
                assert failed_image.get_attribute("aria-description") == "Image unavailable"
                assert failed_image.evaluate("element => getComputedStyle(element).backgroundColor") == "rgb(238, 238, 238)"
                assert failed_image.evaluate("element => element.getBoundingClientRect().width") == 24
                checks.append("corrupt PNG preserves dimensions, a visible error placeholder and accessible alternative")
                checkbox = page.get_by_role("checkbox", name="Accept", exact=True)
                checkbox.check()
                assert checkbox.is_checked()
                assert "Checked: true" in page.locator("#app").inner_text()
                checkbox.focus()
                checkbox.press("Space")
                assert not checkbox.is_checked()
                assert "Checked: false" in page.locator("#app").inner_text()
                retained_focus_after_repair(page, checkbox)
                checks.append("retained Checkbox focus survives native structural repair")
                toggle = page.get_by_role("switch", name="Airplane mode", exact=True)
                assert toggle.get_attribute("type") == "checkbox"
                toggle.check()
                assert toggle.is_checked() and checkbox.is_checked()
                assert json.loads(page.evaluate("jadixNative.state()"))["extensions"]["checked"] is True
                toggle.focus()
                toggle.press("Space")
                assert not toggle.is_checked() and not checkbox.is_checked()
                assert json.loads(page.evaluate("jadixNative.state()"))["extensions"]["checked"] is False
                retained_focus_after_repair(page, toggle)
                assert page.evaluate("jadixNative.take_error()") is None
                checks.append("native Switch pointer/Space activation and Checkbox mirror use one Rust signal and retain focus after structural repair")
                password = page.get_by_placeholder("Password / 密码", exact=True)
                assert password.get_attribute("type") == "password"
                password.fill("secret-密😀")
                assert password.input_value() == "secret-密😀", (password.input_value(), password.get_attribute("type"), page.evaluate("jadixNative.state()"), page.evaluate("jadixNative.take_error()"))
                assert json.loads(page.evaluate("jadixNative.state()"))["value"] == field.input_value()
                checks.append("password input keeps native masking while its value remains outside public sample state")
                note = page.locator("textarea")
                note.fill("第一行😀\nsecond line")
                assert "Note: 第一行😀\nsecond line" in page.locator("#app").inner_text()
                assert page.evaluate("jadixNative.take_error()") is None
                note.evaluate("element => {element.value = '中'.repeat(400000); element.setSelectionRange(element.value.length,element.value.length); element.dispatchEvent(new Event('input'))}")
                rejection = page.evaluate("jadixNative.take_error()")
                assert isinstance(rejection, str), rejection
                assert page.evaluate("jadixNative.take_error()") is None
                assert note.input_value() == "第一行😀\nsecond line", "rejected textarea edit remained visible"
                assert json.loads(page.evaluate("jadixNative.state()"))["extensions"]["note"] == "第一行😀\nsecond line"
                checks.append("rejected oversized textarea edit restores the published Unicode value and drains one error")
                retained_focus_after_repair(page, note, [3, 5])
                checks.append("retained Textarea focus and backward emoji selection survive native structural repair")
                note.press("Control+End")
                assert note.evaluate("element => element.selectionStart === element.value.length && element.selectionEnd === element.value.length")
                composition(note, "中", session)
                assert "Note: 第一行😀\nsecond line中" not in page.locator("#app").inner_text()
                composition(note, "中", session, commit=True)
                assert "Note: 第一行😀\nsecond line中" in page.locator("#app").inner_text()
                before_newline = json.loads(page.evaluate("jadixNative.state()"))["count"]
                note.press("Enter")
                newline_state = json.loads(page.evaluate("jadixNative.state()"))
                assert newline_state["extensions"]["note"] == "第一行😀\nsecond line中\n", newline_state
                assert newline_state["count"] == before_newline
                note.press("Backspace")
                assert note.input_value() == "第一行😀\nsecond line中"
                checks.append("trusted Textarea Enter inserts a controlled newline without submitting the form")
                checks.append("extended checkbox pointer/Space signal updates and multiline draft/commit via " + report["composition_transport"])
                trigger = page.get_by_role("button", name="Open editor", exact=True)
                trigger.click()
                modal = page.get_by_role("dialog", name="Edit details", exact=True)
                modal.wait_for()
                assert modal.evaluate("element => element.matches(':modal')")
                modal_field = page.get_by_placeholder("Dialog name / 中文", exact=True)
                modal_field.fill("Dialog 中文😀")
                assert json.loads(page.evaluate("jadixNative.state()"))["value"] == "Dialog 中文😀"
                assert field.input_value() == "Dialog 中文😀"
                for _ in range(7):
                    page.keyboard.press("Tab")
                    assert modal.evaluate("element => element.contains(document.activeElement)")
                for _ in range(7):
                    page.keyboard.press("Shift+Tab")
                    assert modal.evaluate("element => element.contains(document.activeElement)")
                page.screenshot(path=str(arguments.output / "native-dialog.png"), full_page=True)
                page.get_by_role("button", name="Advanced", exact=True).click()
                nested = page.get_by_role("dialog", name="Confirm changes", exact=True)
                nested.wait_for()
                assert page.locator("dialog:modal").count() == 2
                page.get_by_role("button", name="Return", exact=True).click()
                assert not nested.is_visible() and modal.is_visible()
                assert page.get_by_role("button", name="Advanced", exact=True).evaluate("element => document.activeElement === element")
                page.get_by_role("button", name="Advanced", exact=True).click()
                page.keyboard.press("Escape")
                assert not nested.is_visible() and modal.is_visible()
                assert page.get_by_role("button", name="Advanced", exact=True).evaluate("element => document.activeElement === element")
                checks.append("nested native modals isolate contexts and restore the parent focus after callback/Escape dismissal")
                page.keyboard.press("Escape")
                assert not modal.is_visible()
                assert trigger.evaluate("element => document.activeElement === element")
                checks.append("native modal top layer, shared Rust input state, Tab containment, Escape and focus restoration")
                trigger.click()
                modal_field.fill("Retained dialog")
                page.get_by_role("button", name="Done", exact=True).click()
                assert not modal.is_visible()
                assert trigger.evaluate("element => document.activeElement === element")
                trigger.click()
                assert modal_field.input_value() == "Retained dialog"
                rect = modal.bounding_box()
                assert rect and rect["x"] >= 0 and rect["y"] >= 0
                page.mouse.click(2,2)
                assert not modal.is_visible()
                assert trigger.evaluate("element => document.activeElement === element")
                checks.append("contextual dialog_close, retained controls across reopen and backdrop dismissal")
                trigger.click()
                page.evaluate("""() => {
                    const original = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
                    window.jadixOriginalInputValue = original;
                    Object.defineProperty(HTMLInputElement.prototype, 'value', {...original, set(value) {
                        original.set.call(this, this.isConnected && this.placeholder === 'Name / 中文' ? value.slice(0,1) : value);
                    }});
                }""")
                try:
                    modal_field.fill("Forced truncation 中文😀")
                finally:
                    page.evaluate("Object.defineProperty(HTMLInputElement.prototype, 'value', jadixOriginalInputValue); delete window.jadixOriginalInputValue")
                rejection = page.evaluate("jadixNative.take_error()")
                assert isinstance(rejection, str) and "controlled input value" in rejection, rejection
                assert page.locator("#app").locator("*").count() == 0
                assert page.locator("dialog:modal").count() == 0
                page.evaluate("jadixNative.mount('app')")
                assert json.loads(page.evaluate("jadixNative.state()"))["value"] == ""
                checks.append("controlled native-value truncation reports an error, revokes the partial root and permits a healthy remount")
            scroll.evaluate("element => element.scrollTop = 0")
            page.set_viewport_size({"width": 420, "height": 900})
            (arguments.output / "accessibility.yaml").write_text(page.locator("#app").aria_snapshot(), encoding="utf8")
            if session is not None:
                tree = session.send("Accessibility.getFullAXTree")
                (arguments.output / "accessibility.json").write_text(json.dumps(tree, ensure_ascii=False, indent=2), encoding="utf8")
            page.screenshot(path=str(arguments.output / "native-dom.png"), full_page=True)
            if arguments.extended:
                trigger.click()
                assert modal.is_visible()
            assert page.evaluate("jadixNative.take_error()") is None
            page.evaluate("window.retiredInput = document.querySelector('input:not([type=password]):not([type=checkbox])'); jadixNative.unmount()")
            assert page.locator("#app").locator("*").count() == 0
            assert page.locator("dialog:modal").count() == 0
            page.evaluate("retiredInput.value='late'; retiredInput.dispatchEvent(new Event('input')); retiredInput.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter'})); jadixNative.mount('app')")
            state = json.loads(page.evaluate("jadixNative.state()"))
            assert state["count"] == 0 and state["value"] == ""
            checks.append("unmount removes listeners; detached stale node cannot update remounted root")
            performance = page.evaluate("""async () => {
                await document.fonts.ready;
                await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
                const mutations = [];
                const observer = new MutationObserver(records => mutations.push(...records));
                observer.observe(document.querySelector('#app'), {subtree:true, attributes:true, childList:true, characterData:true});
                await new Promise(resolve => setTimeout(resolve, 200));
                observer.disconnect();
                const updates = [];
                const button = [...document.querySelectorAll('button')].find(element => element.textContent === 'Continue');
                const host = document.querySelector('#app');
                const originalBounds = host.getBoundingClientRect;
                let hostBoundsReads = 0;
                host.getBoundingClientRect = function() {hostBoundsReads++; return originalBounds.call(this);};
                try {
                    for (let index=0; index<30; index++) {
                        const start = performance.now(); button.click(); updates.push(performance.now()-start);
                    }
                } finally {
                    delete host.getBoundingClientRect;
                }
                const edits = [];
                const input = document.querySelector('input:not([type=password]):not([type=checkbox])');
                for (let index=0; index<30; index++) {
                    const start = performance.now();
                    input.value = `Measured 中文😀 ${index}`;
                    input.setSelectionRange(input.value.length, input.value.length);
                    input.dispatchEvent(new Event('input'));
                    edits.push(performance.now()-start);
                }
                const scroll = document.querySelector('[data-jadix-kind="scroll"]');
                const scrollFrames = [];
                for (let index=0; index<30; index++) {
                    const start = performance.now();
                    scroll.scrollTop = index * 80;
                    await new Promise(resolve => requestAnimationFrame(resolve));
                    scrollFrames.push(performance.now()-start);
                }
                if (scroll.scrollTop <= 0) throw new Error('continuous scroll did not move');
                if (jadixNative.take_error()) throw new Error('native event rejected during measured updates');
                const nodeCount = document.querySelector('#app').querySelectorAll('*').length;
                const mounts = [];
                for (let index=0; index<25; index++) {
                    jadixNative.unmount();
                    if (document.querySelector('#app').childElementCount !== 0) throw new Error('dispose retained DOM');
                    const start = performance.now(); jadixNative.mount('app'); mounts.push(performance.now()-start);
                    if (document.querySelector('#app').querySelectorAll('*').length !== nodeCount) throw new Error('mount node growth');
                }
                return {idleMutations:mutations.length,updatesMs:updates,hostBoundsReads,editsMs:edits,scrollFrameWaitsMs:scrollFrames,mountsMs:mounts,nodeCount};
            }""")
            assert performance["idleMutations"] == 0, performance
            assert performance["hostBoundsReads"] <= (30 if arguments.extended else 0), performance
            performance["updateMedianMs"] = statistics.median(performance["updatesMs"])
            performance["updateP95Ms"] = sorted(performance["updatesMs"])[28]
            performance["editMedianMs"] = statistics.median(performance["editsMs"])
            performance["editP95Ms"] = sorted(performance["editsMs"])[28]
            performance["mountMedianMs"] = statistics.median(performance["mountsMs"])
            assert performance["updateP95Ms"] < 250, performance
            assert performance["editP95Ms"] < 250, performance
            assert json.loads(page.evaluate("jadixNative.state()"))["count"] == 0
            geometry = page.locator('[data-jadix-kind="scroll"]').first.evaluate("element => ({width:element.getBoundingClientRect().width, height:element.getBoundingClientRect().height, parentWidth:element.parentElement.getBoundingClientRect().width, parentHeight:element.parentElement.getBoundingClientRect().height})")
            assert abs(geometry["width"] - geometry["parentWidth"]) < 1 and abs(geometry["height"] - geometry["parentHeight"]) < 1, ("remount used a stale viewport", geometry)
            report["remount_geometry"] = geometry
            report["keyboard_navigation"]["after_remount"] = keyboard_navigation(page, arguments.extended)
            checks.append("full native Tab order and focused visibility remain correct after 25 mount/dispose cycles")
            checks.append("200ms idle has no DOM mutations; 30 updates, Unicode edits and scroll frames plus 25 mount/dispose cycles retain bounded nodes")
            assert errors == [], errors
            report.update({"passed": True, "performance": performance})
            (arguments.output / "input-events.json").write_text(json.dumps(page.evaluate("jadixInputTrace"), ensure_ascii=False, indent=2), encoding="utf8")
            browser.close()
    except Exception as error:
        report["failure"] = repr(error)
        raise
    finally:
        server.shutdown()
        server.server_close()
        (arguments.output / "checks.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
    print(f"PASS: {len(checks)} DOM scenarios")


if __name__ == "__main__":
    main()
