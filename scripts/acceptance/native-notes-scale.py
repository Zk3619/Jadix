import argparse
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import math
from pathlib import Path
import statistics
import threading

from native_browser import artifact, launch_options, validate_output


def notebook_fixture():
    notes = [{"id": str(index), "title": f"Load {index:03d} 中文😀",
              "body": ("中文😀 אבג\n" * 200)} for index in range(1, 129)]
    value = {"schema": 1, "next_id": "129", "notes": notes}
    target = 512 * 1024 - 2048
    remaining = target - len(encoded(value).encode("utf8"))
    if remaining < 0:
        raise ValueError("fixture cannot fit the document and per-note limits")
    for note in reversed(notes):
        added = min(remaining, 32768 - len(note["body"].encode("utf8")))
        note["body"] += "x" * added
        remaining -= added
    if remaining:
        raise ValueError("fixture exceeds the per-note limits")
    return value


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def main():
    from playwright.sync_api import sync_playwright
    parser = argparse.ArgumentParser(description="Exercise the real notebook at its count and aggregate size boundary.")
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--browser", choices=["chromium", "firefox", "webkit"], default="chromium")
    parser.add_argument("--channel")
    parser.add_argument("--executable", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    try:
        validate_output(output)
        build = artifact(args.site)
        options = launch_options(args.browser, args.channel, args.executable)
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))
    output.mkdir(parents=True, exist_ok=True)
    fixture = notebook_fixture()
    source = encoded(fixture)
    (output / "fixture.json").write_text(source, encoding="utf8")
    report = {"passed": False, "engine": args.browser, "launch": options, "build": build,
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "fixture_bytes": len(source.encode("utf8")), "fixture_sha256": hashlib.sha256(source.encode("utf8")).hexdigest(),
              "checks": [], "page_errors": [],
              "limits": ["128 notes near the shared 512 KiB document ceiling; finite workflow, not infinite-session or maximum-process-memory certification.",
                         "Trusted browser event to second requestAnimationFrame includes a rendering opportunity, not physical input-to-display or compositor presentation.",
                         "No absolute latency SLA, cross-engine timing comparison, OS IME, screen-reader, minimum-version or Safari application acceptance."]}
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(args.site.resolve())))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, args.browser).launch(**options)
            context = browser.new_context(viewport={"width": 420, "height": 760})
            page = context.new_page()
            page.on("pageerror", lambda error: report["page_errors"].append(str(error)))
            page.goto(f"http://127.0.0.1:{server.server_port}")
            page.wait_for_function("window.jadixNotes")
            report.update(browser=browser.version, user_agent=page.evaluate("navigator.userAgent"))

            def state():
                return json.loads(page.evaluate("jadixNotes.state()"))

            def click(name):
                page.get_by_role("button", name=name, exact=True).click()

            def stored():
                return page.evaluate("localStorage.getItem('jadix.notes.v1')")

            def saved():
                page.wait_for_function("JSON.parse(jadixNotes.state()).status === 'Saved to device storage' && !JSON.parse(jadixNotes.state()).storage_pending")

            def frames():
                page.evaluate("() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))")

            transfer = page.get_by_placeholder("Notebook JSON for export or import", exact=True)
            title = page.get_by_placeholder("Title / 标题", exact=True)
            body = page.get_by_placeholder("Write a note / 正文", exact=True)
            transfer.fill(source)
            click("Import notebook")
            click("Confirm")
            saved()
            assert state()["archive"] == fixture == json.loads(stored())
            assert page.get_by_role("button", name="Open: Load").count() == 128
            report["checks"].append("confirmed import and durable acknowledgement for 128 notes within 2 KiB of the aggregate ceiling")
            before = stored()
            click("New note")
            title.fill("Overflow draft")
            body.fill("Keep rejected draft")
            click("Save note")
            current = state()
            assert "full" in current["status"] and current["dirty"] and current["draft"]["body"] == "Keep rejected draft"
            assert current["archive"] == fixture and stored() == before
            click("Revert draft")
            click("Open: Load 001 中文😀")
            body.fill("m" * 32768)
            click("Save note")
            current = state()
            assert "512 KiB" in current["status"] and current["draft"]["body"] == "m" * 32768
            assert current["archive"] == fixture and stored() == before
            click("Revert draft")
            invalid = json.loads(source)
            invalid["next_id"] = "130"
            invalid["notes"].append({"id": "129", "title": "Excess", "body": ""})
            transfer.fill(encoded(invalid))
            click("Import notebook")
            click("Confirm")
            assert "full" in state()["status"] and state()["archive"] == fixture and stored() == before
            report["checks"].append("129th save/import and aggregate overflow are atomic; rejected edits remain recoverable")
            frames()
            page.evaluate("""() => {
                window.__scaleSamples = [];
                window.__scaleBody = document.querySelector('textarea[placeholder="Write a note / 正文"]');
                for (const kind of ['click', 'input']) document.addEventListener(kind, event => {
                    const matches = kind === 'input' ? event.target === window.__scaleBody : event.target.closest('button')?.textContent.startsWith('Open: ');
                    if (!matches) return;
                    const start = performance.now(), trusted = event.isTrusted;
                    requestAnimationFrame(() => requestAnimationFrame(() => window.__scaleSamples.push({kind, trusted, milliseconds: performance.now() - start, visibility: document.visibilityState})));
                }, true);
            }""")
            node_count = page.locator("#app *").count()
            for index in range(30):
                body.press("End")
                body.press("x")
                page.wait_for_function("expected => __scaleSamples.filter(value => value.kind === 'input').length === expected", arg=index + 1)
            assert state()["dirty"] and body.evaluate("element => element === window.__scaleBody")
            click("Revert draft")
            for index in range(30):
                click(f"Open: Load {index + 1:03d} 中文😀")
                page.wait_for_function("expected => __scaleSamples.filter(value => value.kind === 'click').length === expected", arg=index + 1)
                assert state()["selected"] == str(index + 1)
            assert page.locator("#app *").count() == node_count
            assert body.evaluate("element => element === window.__scaleBody")
            samples = page.evaluate("__scaleSamples")
            assert len(samples) == 60 and all(value["trusted"] and value["visibility"] == "visible" for value in samples)
            report["event_to_second_raf"] = {}
            for kind in ["click", "input"]:
                values = [sample["milliseconds"] for sample in samples if sample["kind"] == kind]
                assert len(values) == 30 and all(math.isfinite(value) and value >= 0 for value in values)
                report["event_to_second_raf"][kind] = {"p50_ms": statistics.median(values), "p95_ms": sorted(values)[math.ceil(len(values) * 0.95) - 1], "samples": values}
            frames()
            idle = page.evaluate("""() => new Promise(resolve => {
                let mutations = 0; const start = performance.now();
                const observer = new MutationObserver(records => mutations += records.length);
                observer.observe(document.getElementById('app'), {subtree:true, childList:true, attributes:true, characterData:true});
                setTimeout(() => {observer.disconnect(); resolve({milliseconds:performance.now()-start, mutations});}, 300);
            })""")
            assert idle["mutations"] == 0
            report.update(idle=idle, retained_dom_nodes=node_count)
            report["checks"].append("30 trusted keyboard edits and 30 open actions retain editor/node count; finite idle has no DOM mutations")
            title.fill("Saved after quota recovery")
            page.evaluate("() => {window.__originalSetItem = Storage.prototype.setItem; Storage.prototype.setItem = function(){throw new DOMException('scale quota', 'QuotaExceededError')};}")
            click("Save note")
            assert state()["storage_pending"] and "Storage failed" in state()["status"] and stored() == before
            page.evaluate("() => {Storage.prototype.setItem = window.__originalSetItem;}")
            click("Save note")
            saved()
            assert len(json.loads(stored())["notes"]) == 128
            click("Open: Load 128 中文😀")
            click("Delete note")
            click("Confirm")
            saved()
            click("New note")
            title.fill("Replacement 129")
            body.fill("New identity 中文😀")
            click("Save note")
            saved()
            assert state()["selected"] == "129" and state()["archive"]["next_id"] == "130"
            assert len(state()["archive"]["notes"]) == 128
            click("Export notebook")
            expected = json.loads(stored())
            assert json.loads(transfer.input_value()) == expected
            report["checks"].append("near-ceiling quota failure preserves old document; retry, delete, non-reused identity and export round-trip succeed")
            for _ in range(5):
                page.reload()
                page.wait_for_function("window.jadixNotes")
                assert state()["archive"] == expected
                assert page.get_by_role("button", name="Open: ").count() == 128
                assert page.evaluate("jadixNotes.take_error()") is None
            page.screenshot(path=str(output / "scale.png"), full_page=True)
            (output / "semantics.txt").write_text(page.locator("#app").aria_snapshot(), encoding="utf8")
            (output / "final-state.json").write_text(encoded(state()), encoding="utf8")
            page.evaluate("jadixNotes.unmount()")
            assert page.locator("#app").inner_html() == "" and json.loads(stored()) == expected
            report["checks"].append("five full large-document reloads and explicit disposal preserve saved documents")
            assert not report["page_errors"]
            report["passed"] = True
            browser.close()
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        server.shutdown()
        server.server_close()
        (output / "scale.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf8")
    print(json.dumps({"passed": report["passed"], "checks": len(report["checks"]), "engine": args.browser}))


if __name__ == "__main__":
    main()
