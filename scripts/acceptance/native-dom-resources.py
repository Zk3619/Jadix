import argparse
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import math
from pathlib import Path
import threading

from native_browser import artifact, launch_options, validate_output


HEAP_GROWTH_BUDGET = 2 * 1024 * 1024


def validate_samples(samples):
    expected = [(cycle, phase) for cycle in range(0, 101, 20) for phase in ("mounted", "unmounted")]
    if [(sample.get("cycle"), sample.get("phase")) for sample in samples] != expected:
        raise ValueError("incomplete or reordered 100-cycle lifecycle checkpoints")
    baselines = {}
    for sample in samples:
        for name in ("documents", "nodes", "listeners", "js_heap_bytes", "wasm_memory_bytes"):
            value = sample.get(name)
            if isinstance(value, bool) or not isinstance(value, (float, int)) or not math.isfinite(value) or value < 0 or int(value) != value:
                raise ValueError(f"invalid resource metric: {name}")
        phase = sample.get("phase")
        if phase not in ("mounted", "unmounted"):
            raise ValueError("unknown lifecycle phase")
        if sample["documents"] != 1:
            raise ValueError("unexpected document retention")
        baseline = baselines.setdefault(phase, sample)
        for name in ("nodes", "listeners", "wasm_memory_bytes"):
            if sample[name] > baseline[name]:
                raise ValueError(f"{phase} {name} grew after warmup")
        if sample["js_heap_bytes"] > baseline["js_heap_bytes"] + HEAP_GROWTH_BUDGET:
            raise ValueError(f"{phase} JS heap exceeded the finite-session growth budget")
    if set(baselines) != {"mounted", "unmounted"}:
        raise ValueError("both lifecycle phases are required")


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def main():
    from playwright.sync_api import sync_playwright

    parser = argparse.ArgumentParser(description="Measure Chromium DOM, listener, JS heap and Wasm memory across actual application lifetimes.")
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--application", choices=("controls", "notes"), default="controls")
    parser.add_argument("--channel")
    parser.add_argument("--executable", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    try:
        validate_output(output)
        build = artifact(args.site, args.application)
        options = launch_options("chromium", args.channel, args.executable)
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))
    output.mkdir(parents=True, exist_ok=True)
    fixture = None
    if args.application == "notes":
        spec = importlib.util.spec_from_file_location("native_notes_scale", Path(__file__).with_name("native-notes-scale.py"))
        scale = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(scale)
        fixture = scale.encoded(scale.notebook_fixture())
        (output / "fixture.json").write_text(fixture, encoding="utf8")
    report = {
        "passed": False, "build": build, "launch": options, "application": args.application,
        "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "warmup_cycles": 10, "measured_cycles": 100, "samples": [], "raw_samples": [],
        "fixture_bytes": len(fixture.encode("utf8")) if fixture is not None else 0,
        "js_heap_growth_budget_bytes": HEAP_GROWTH_BUDGET, "checks": [], "page_errors": [],
        "limits": [
            "Owned Chromium page, exact release bundle, fixed workload and finite 100-cycle observation after 10 warmups.",
            "Post-GC DOM/listener counters and JS heap are separate from Wasm linear-memory capacity; neither is OS RSS/PSS or GPU memory.",
            "Raw sample maxima are observed maxima, not continuous or whole-process peak memory.",
            "Explicit collection changes runtime behavior; this is a retention gate, not an input-latency or infinite-session certificate.",
            "No Firefox/WebKit, Safari application, minimum-version, physical-device or assistive-technology acceptance.",
        ],
    }
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(args.site.resolve())))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(**options)
            try:
                page = browser.new_page(viewport={"width": 420, "height": 760})
                page.on("pageerror", lambda error: report["page_errors"].append(str(error)))
                page.goto(f"http://127.0.0.1:{server.server_port}")
                ready = page.wait_for_function("Boolean(window.jadixNative || window.jadixNotes)")
                ready.dispose()
                page.evaluate('async () => { window.resourceModule = await import("./pkg/jadix_native_controls.js"); window.resourceWasm = await resourceModule.default(); }')
                session = page.context.new_cdp_session(page)
                session.send("Performance.enable")
                report.update(browser=browser.version, user_agent=page.evaluate("navigator.userAgent"))

                def mount():
                    page.evaluate('fixture => { resourceModule.mount("app"); if (fixture !== null) resourceModule.restore(fixture); }', fixture)
                    if fixture is not None:
                        assert page.evaluate("JSON.parse(resourceModule.state()).archive.notes.length") == 128
                    assert page.evaluate("resourceModule.take_error()") is None

                def unmount():
                    page.evaluate("resourceModule.unmount()")
                    assert page.evaluate('document.querySelector("#app").childElementCount') == 0
                    assert page.evaluate("resourceModule.state() === undefined")

                def sample(phase, cycle, collect):
                    if collect:
                        page.evaluate("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))")
                        session.send("HeapProfiler.collectGarbage")
                    counters = session.send("Memory.getDOMCounters")
                    metrics = {item["name"]: item["value"] for item in session.send("Performance.getMetrics")["metrics"]}
                    value = {"phase": phase, "cycle": cycle, "documents": counters["documents"],
                             "nodes": counters["nodes"], "listeners": counters["jsEventListeners"],
                             "js_heap_bytes": metrics["JSHeapUsedSize"],
                             "wasm_memory_bytes": page.evaluate("resourceWasm.memory.buffer.byteLength")}
                    report["samples" if collect else "raw_samples"].append(value)

                unmount()
                for _ in range(report["warmup_cycles"]):
                    mount()
                    unmount()
                mount()
                sample("mounted", 0, True)
                unmount()
                sample("unmounted", 0, True)
                for cycle in range(1, report["measured_cycles"] + 1):
                    mount()
                    sample("mounted", cycle, False)
                    if cycle % 20 == 0:
                        sample("mounted", cycle, True)
                    unmount()
                    sample("unmounted", cycle, False)
                    if cycle % 20 == 0:
                        sample("unmounted", cycle, True)
                validate_samples(report["samples"])
                assert report["page_errors"] == [], report["page_errors"]
                report["observed_raw_maxima"] = {name: max(sample[name] for sample in report["raw_samples"])
                                                 for name in ("documents", "nodes", "listeners", "js_heap_bytes", "wasm_memory_bytes")}
                report["checks"] = ["100 real mount/dispose cycles after warmup and a collected baseline",
                                    "mounted/unmounted documents, nodes, listeners and Wasm memory do not grow after warmup",
                                    "post-collection JS heap stays within the recorded 2 MiB growth budget",
                                    "all disposals empty the mount and remove Rust application state; no page errors"]
                if fixture is not None:
                    report["checks"].append("every measured notes mount restores 128 notes near the shared document ceiling")
                report["passed"] = True
            finally:
                browser.close()
    except Exception as error:
        report["failure"] = repr(error)
        raise
    finally:
        server.shutdown()
        server.server_close()
        (output / "resources.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf8")


if __name__ == "__main__":
    main()
