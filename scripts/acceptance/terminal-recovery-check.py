#!/usr/bin/env python3
"""Owned ConPTY process restart checks for CellApplication recovery."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location("conpty", Path(__file__).with_name("terminal-conpty-check.py"))
conpty = importlib.util.module_from_spec(spec)
spec.loader.exec_module(conpty)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=False)
    checkpoint = args.directory.resolve() / "draft.checkpoint.json"
    text = "恢复 中文 🦀"
    results = []
    for name, keys, expected, restored in [("write", text.encode("utf-8"), text, False),
                                           ("forced-exit", "宕".encode("utf-8"), text+"宕", True),
                                           ("restart", b"", text+"宕", True),
                                           ("undo-after-restart", b"\x1a", text, True),
                                           ("redo-after-restart", b"\x19", text+"宕", True)]:
        receipt = args.directory.resolve() / f"{name}.json"
        console = conpty.Console([args.executable.resolve(), "--colors", "mono", "--menus",
                                  "--checkpoint", checkpoint, "--receipt", receipt])
        result = None
        try:
            console.ready("portable")
            if keys:
                if name == "write":
                    console.send(b"\x1b[200~" + keys + b"\x1b[201~")
                    console.ready("🦀")
                else:
                    console.send(keys)
            if name == "forced-exit":
                # Paint follows successful automatic save; terminate only this owned child.
                console.ready("宕")
                console.check(console.terminate(console.proc.process, 7))
                assert console.wait(console.proc.process, 5000) == 0
                saved = json.loads(checkpoint.read_text(encoding="utf-8"))
                assert hashlib.sha256(saved["payload"].encode()).hexdigest() == saved["sha256"]
                editors = json.loads(saved["payload"])["fields"]
                assert json.loads(editors["draft"])["current"]["text"] == expected
                assert not Path(str(checkpoint)+".lock").exists()
                result = {"name":name,"owned_forced_exit":True,"terminal_cleanup_claimed":False}
                continue
            console.send(b"\x11")
            console.finish()
            value = json.loads(receipt.read_text(encoding="utf-8"))
            assert value["value"] == expected and value["checkpoint_restored"] == restored, value
            assert value["error"] is None and value["raw_restored"] and value["output_cleanup_complete"], value
            result = {"name": name, "receipt": value}
        finally:
            console.close()
            (args.directory / f"{name}.ansi").write_bytes(console.output)
            if result is not None:
                result["reader_finished"] = not console.reader.is_alive()
                results.append(result)
    envelope = json.loads(checkpoint.read_text(encoding="utf-8"))
    assert envelope["generation"] >= 2 and not Path(str(checkpoint)+".lock").exists()
    report = {"status":"pass", "real_windows_conpty":True, "owned_process_restarts":4,
              "executable_sha256":hashlib.sha256(args.executable.read_bytes()).hexdigest(),
              "checkpoint_sha256":hashlib.sha256(checkpoint.read_bytes()).hexdigest(), "scenarios":results}
    (args.directory/"acceptance.json").write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding="utf-8")
    print(json.dumps({"status":"pass", "scenarios":len(results), "owned_process_restarts":4}))

if __name__ == "__main__":
    main()
