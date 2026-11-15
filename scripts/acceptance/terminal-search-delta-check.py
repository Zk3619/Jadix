#!/usr/bin/env python3
"""Active search survives SDK leaf updates in owned ConPTY; retain full IO."""
import argparse
import hashlib
import importlib.util
import json
import time
from pathlib import Path
spec = importlib.util.spec_from_file_location("conpty", Path(__file__).with_name("terminal-conpty-check.py"))
conpty = importlib.util.module_from_spec(spec)
spec.loader.exec_module(conpty)

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--offset-search", action="store_true", help="shift row 0 match ranges while every mounted row matches")
    parser.add_argument("--growing-rows", action="store_true", help="change row height and move following rows through the real SDK")
    args = parser.parse_args()
    query = "Row" if args.offset_search or args.growing_rows else "Row 0:"
    matches = 256 if args.offset_search or args.growing_rows else 1
    args.directory.mkdir(parents=True, exist_ok=False)
    outputs = []
    application_outputs = []
    cases = []
    # AB/BA performance claims belong to the separately instrumented host test.
    # These owned sessions certify actual Console IO and exact emitted bytes.
    for sparse in (False, True):
        folder = args.directory / ("sparse" if sparse else "full")
        folder.mkdir()
        bundle = folder / "scaling.js"
        bundle.write_text(f"globalThis.scaleRows=256;globalThis.leafPatches={str(sparse).lower()};globalThis.searchOffsets={str(args.offset_search).lower()};globalThis.growingRows={str(args.growing_rows).lower()};\n" + args.bundle.read_text(encoding="utf-8"), encoding="utf-8")
        receipt = folder.resolve() / "receipt.json"
        console = conpty.Console([args.executable.resolve(), "--bundle", bundle.resolve(), "--receipt", receipt, "--search-query", query])
        def state():
            try: return json.loads(receipt.read_text(encoding="utf-8"))
            except (FileNotFoundError, json.JSONDecodeError): return {}
        def until(test):
            end = time.monotonic() + 10
            while time.monotonic() < end:
                current = state()
                if test(current): return current
                time.sleep(.03)
            raise AssertionError(state())
        def wire():
            with console.lock: return bytes(console.output)
        case = {"sparse": sparse, "status": "incomplete", "bundle_sha256": sha(bundle)}
        application = None
        try:
            until(lambda value: value.get("phase") == "active")
            console.ready("Row 0: 0")
            console.send(b"\x06")
            until(lambda value: value.get("search_matches") == matches)
            for count in range(1, 4):
                before = len(wire())
                console.send(b"\x1b[<0;3;1M\x1b[<0;3;1m")
                prefix = "x" * (count % 7) if args.offset_search else ""
                suffix = "x" * (6 - count % 7) if args.offset_search else ""
                extra = "\nextra 中文 🦀\nthird line" if args.growing_rows and count % 2 else ""
                wanted = "Text(" + json.dumps(f"{prefix}Row 0: {count}{suffix}{extra}", ensure_ascii=False) + ")"
                until(lambda value: wanted in value.get("content", []))
                until(lambda value: value.get("search_matches") == matches and len(wire()) > before)
            before_frames = state()["frames"]
            assert console.resize(console.pc, conpty.Coord(100, 30)) == 0
            resized = until(lambda value: value.get("frames", 0) > before_frames
                and value.get("viewport") == [100, 30] and value.get("search_matches") == matches)
            # The receipt follows app draw/flush; ConPTY's output reader drains
            # separately. Require the observed resize and 100 ms without bytes,
            # bounded to 3 s. This is an acceptance drain, not a latency sample.
            deadline = time.monotonic() + 3
            stable_since = time.monotonic()
            application = wire()
            while time.monotonic() - stable_since < .1:
                if time.monotonic() >= deadline:
                    raise TimeoutError("ConPTY output did not settle after observed resize")
                time.sleep(.01)
                current = wire()
                if current != application:
                    application = current
                    stable_since = time.monotonic()
            assert b"\x1b[8;30;100t" in application, "ConPTY resize not in output"
            case.update(resize_before_frames=before_frames, resize_observed=resized, output_drain_quiet_ms=100)
            (folder / "application.ansi").write_bytes(application)
            application_outputs.append(application)
            console.send(b"\x11")
            console.finish()
            closed = state()
            assert closed["phase"] == "closed" and closed["restored"] and closed["error"] is None, closed
            case.update(status="pass", closed=closed)
        finally:
            console.close()
            output = bytes(console.output)
            (folder / "output.ansi").write_bytes(output)
            outputs.append(output)
            if application is not None:
                assert output.startswith(application)
                (folder / "shutdown.ansi").write_bytes(output[len(application):])
            (folder / "acceptance.json").write_text(json.dumps(case, ensure_ascii=False, indent=2), encoding="utf-8")
            cases.append(case)
    assert application_outputs[0] == application_outputs[1], "full/sparse application bytes differ before exit"
    report = dict(status="pass", offset_search=args.offset_search, growing_rows=args.growing_rows, expected_matches=matches, runtime="owned Windows ConPTY / Ratatui / existing QuickJS SDK", executable_sha256=sha(args.executable), source_bundle_sha256=sha(args.bundle), cases=cases, application_wire_bytes=len(application_outputs[0]), application_wire_sha256=hashlib.sha256(application_outputs[0]).hexdigest(), exact_application_wire_equal=True, exact_wire_equal=outputs[0] == outputs[1], raw_wire_bytes=[len(output) for output in outputs], raw_wire_sha256=[hashlib.sha256(output).hexdigest() for output in outputs], scope="same 256-row business, active Ctrl-F search, 3 Console activations and resize; exact startup/business bytes before exit; complete raw shutdown streams retained separately, mode restoration checked by each receipt; no OS raster or performance claim")
    (args.directory / "acceptance.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({key: report[key] for key in ("status", "exact_application_wire_equal", "application_wire_bytes", "exact_wire_equal", "raw_wire_bytes")}, ensure_ascii=False))
if __name__ == "__main__":
    main()

