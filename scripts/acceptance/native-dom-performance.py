import argparse
from datetime import datetime
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import math
from pathlib import Path
import platform
import statistics
import threading

from playwright.sync_api import sync_playwright


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def artifact(site):
    site = site.resolve()
    report_path = site / "build.json"
    report = json.loads(report_path.read_text(encoding="utf8"))
    if report["rust_profile"] != "release":
        raise ValueError("comparison requires release packages")
    names = [entry["path"].replace("\\", "/") for entry in report["files"]]
    required = {"index.html", "pkg/jadix_native_controls.js", "pkg/jadix_native_controls_bg.wasm"}
    if len(names) != len(set(names)) or not required.issubset(names):
        raise ValueError("artifact inventory is missing required files or contains duplicates")
    for entry in report["files"]:
        path = (site / entry["path"].replace("\\", "/")).resolve()
        if not path.is_relative_to(site):
            raise ValueError("artifact path escaped its site")
        payload = path.read_bytes()
        if len(payload) != entry["bytes"] or hashlib.sha256(payload).hexdigest() != entry["sha256"]:
            raise ValueError(f"artifact does not match its build report: {entry['path']}")
    return {"site": str(site), "build_report_sha256": hashlib.sha256(report_path.read_bytes()).hexdigest(), **report}


def metrics(session):
    return {entry["name"]: entry["value"] for entry in session.send("Performance.getMetrics")["metrics"]}


def probe(page, session):
    page.evaluate("jadixNative.unmount()")
    before = metrics(session)
    mounted = page.evaluate(r"""() => {
        const started = performance.now();
        jadixNative.mount('app');
        document.getElementById('app').getBoundingClientRect();
        const mountMs = performance.now() - started;
        const rows = [...document.querySelectorAll('span')].filter(node => /^Row \d{3} —/.test(node.textContent)).length;
        return {mount_ms: mountMs, rows, node_count: document.getElementById('app').querySelectorAll('*').length};
    }""")
    after = metrics(session)
    result = page.evaluate("""() => {
        const button = [...document.querySelectorAll('button')].find(node => node.textContent === 'Continue');
        const field = document.querySelector('input[placeholder="Name / 中文"]');
        if (!button || !field) throw new Error('missing measured controls');
        const updates = [], edits = [];
        for (let index = 0; index < 30; index++) {
            const started = performance.now();
            button.click();
            updates.push(performance.now() - started);
        }
        for (let index = 0; index < 30; index++) {
            const started = performance.now();
            field.value = '同机基线😀' + index;
            field.dispatchEvent(new Event('input'));
            edits.push(performance.now() - started);
        }
        const state = JSON.parse(jadixNative.state());
        if (state.count !== 30 || state.value !== '同机基线😀29' || jadixNative.take_error() !== undefined) {
            throw new Error('measured events did not update Rust state cleanly');
        }
        jadixNative.unmount();
        if (document.getElementById('app').childElementCount !== 0) throw new Error('root retained DOM children after disposal');
        return {update_ms: updates, edit_ms: edits};
    }""")
    return {**mounted, **result, "mount_layout_count": after["LayoutCount"] - before["LayoutCount"]}


def summarize(samples):
    updates = sorted(value for sample in samples for value in sample["update_ms"])
    edits = sorted(value for sample in samples for value in sample["edit_ms"])
    return {
        "mount_median_ms": statistics.median(sample["mount_ms"] for sample in samples),
        "update_p95_ms": updates[math.ceil(len(updates) * 0.95) - 1],
        "edit_p95_ms": edits[math.ceil(len(edits) * 0.95) - 1],
        "mount_layout_median": statistics.median(sample["mount_layout_count"] for sample in samples),
    }


def main():
    parser = argparse.ArgumentParser(description="Interleaved same-browser release artifact regression gate; synchronous commits, not end-to-end input or paint latency")
    parser.add_argument("--baseline-site", type=Path, required=True)
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--channel", default="msedge")
    parser.add_argument("--runs", type=int, default=12)
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--max-ratio", type=float, default=1.5)
    parser.add_argument("--noise-ms", type=float, default=3.0)
    arguments = parser.parse_args()
    if arguments.runs < 5 or arguments.warmup < 1 or not math.isfinite(arguments.max_ratio) or arguments.max_ratio < 1 or not math.isfinite(arguments.noise_ms) or arguments.noise_ms < 0:
        parser.error("require at least five runs and one warmup, finite ratio >= 1, and finite noise allowance >= 0")
    output = arguments.output.resolve()
    if any((directory / "snapshot.json").exists() for directory in (output, *output.parents)):
        parser.error("output contains an archived snapshot; select a new output directory")
    artifacts = {"baseline": artifact(arguments.baseline_site), "candidate": artifact(arguments.site)}
    for key in ("target", "rust_profile", "extended", "retained_rows", "cargo_lock_sha256", "wasm_bindgen"):
        if artifacts["baseline"][key] != artifacts["candidate"][key]:
            parser.error(f"comparison package metadata differs: {key}")
    output.mkdir(parents=True, exist_ok=True)
    servers, threads = [], []
    samples = {name: [] for name in artifacts}
    warmups = {name: [] for name in artifacts}
    errors = []
    try:
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(channel=arguments.channel, headless=True)
            pages = {}
            for name, package in artifacts.items():
                server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=package["site"]))
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                servers.append(server)
                threads.append(thread)
                thread.start()
                page = browser.new_page(viewport={"width": 420, "height": 760})
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.goto(f"http://127.0.0.1:{server.server_port}")
                page.wait_for_function("window.jadixNative && document.querySelector('input')")
                page.evaluate("document.fonts.ready")
                session = page.context.new_cdp_session(page)
                session.send("Performance.enable")
                pages[name] = (page, session)
            for iteration in range(arguments.warmup + arguments.runs):
                order = ("baseline", "candidate") if iteration % 2 == 0 else ("candidate", "baseline")
                for name in order:
                    sample = probe(*pages[name])
                    if sample["rows"] != artifacts[name]["retained_rows"]:
                        raise AssertionError("measured retained row count differs from package")
                    (warmups if iteration < arguments.warmup else samples)[name].append(sample)
            browser_version = browser.version
            browser.close()
        summaries = {name: summarize(values) for name, values in samples.items()}
        gates = []
        for key in ("mount_median_ms", "update_p95_ms", "edit_p95_ms"):
            baseline = summaries["baseline"][key]
            candidate = summaries["candidate"][key]
            limit = baseline * arguments.max_ratio + arguments.noise_ms
            gates.append({"metric": key, "baseline": baseline, "candidate": candidate, "limit": limit, "passed": candidate <= limit})
        node_counts = {sample["node_count"] for values in samples.values() for sample in values}
        passed = all(gate["passed"] for gate in gates) and len(node_counts) == 1 and not errors
        report = {
            "observed_at": datetime.now().astimezone().isoformat(),
            "passed": passed, "browser": browser_version, "host": platform.platform(),
            "processor": platform.processor(), "artifacts": artifacts,
            "policy": {"runs_per_artifact": arguments.runs, "warmup_rounds": arguments.warmup, "order": "alternating baseline/candidate and candidate/baseline",
                       "limit_rule": "baseline * max_ratio + noise_ms", "max_ratio": arguments.max_ratio, "noise_ms": arguments.noise_ms, "p95": "nearest rank ceil(n * 0.95) - 1"},
            "summaries": summaries, "gates": gates, "node_counts": sorted(node_counts),
            "samples": samples, "warmups": warmups, "page_errors": errors,
            "limits": ["Warm Wasm module; network startup and shader/GPU work are outside this gate",
                       "Synchronous mount and event-handler commits; not input-to-paint latency",
                       "One installed browser and one host; no mobile, minimum-version, power or accessibility claim",
                       "Finite measurements and retained node checks do not prove absence of leaks"],
        }
        (output / "performance.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf8")
        print(json.dumps({"passed": passed, "gates": gates, "node_counts": sorted(node_counts)}))
        if not passed:
            raise RuntimeError("same-browser performance regression gate failed")
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()
        for thread in threads:
            thread.join(timeout=5)


if __name__ == "__main__":
    main()
