#!/usr/bin/env python3
"""Same SDK maxLength fixture through an owned Windows ConPTY character host."""
import argparse, hashlib, importlib.util, json, time
from pathlib import Path
spec = importlib.util.spec_from_file_location("conpty", Path(__file__).with_name("terminal-conpty-check.py"))
conpty = importlib.util.module_from_spec(spec)
spec.loader.exec_module(conpty)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=False)
    receipt = args.directory.resolve() / "receipt.json"
    console = conpty.Console([args.executable.resolve(), "--bundle", args.bundle.resolve(), "--receipt", receipt])
    result = {"status":"incomplete"}
    def state():
        try: return json.loads(receipt.read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError): return {}
    def until(test):
        end = time.monotonic()+8
        while time.monotonic()<end:
            current = state()
            if test(current): return current
            time.sleep(.03)
        raise AssertionError(state())
    def field(current, kind, value):
        prefix = kind + " { value: " + json.dumps(value,ensure_ascii=False)
        return any(text.startswith(prefix) for text in current.get("content",[]))
    def click(row):
        console.send(f"\x1b[<0;3;{row}M\x1b[<0;3;{row}m".encode())
    try:
        until(lambda s: s.get("phase")=="active")
        click(2); console.send("ab中🦀x".encode())
        initial = until(lambda s: field(s,"Input","ab中🦀"))
        click(6); console.send("中🦀xy".encode())
        notes = until(lambda s: field(s,"Textarea","中🦀x"))
        click(11)
        tightened = until(lambda s: any("Limit:2" in text for text in s.get("content",[])))
        click(2); console.send(b"\x1b[FZ"); time.sleep(.3)
        assert field(state(),"Input","ab中🦀"), state()
        console.send(b"\x01\x7f"); time.sleep(.1); console.send("🦀x".encode())
        final = until(lambda s: field(s,"Input","🦀"))
        console.send(b"\x11"); console.finish()
        closed = state()
        assert closed["phase"]=="closed" and closed["restored"] and closed["error"] is None, closed
        result.update(status="pass",runtime="owned Windows ConPTY / Ratatui / SDK TSX / existing QuickJS",
            input_utf16_limit=True,textarea_utf16_limit=True,dynamic_limit=True,
            initial=initial,notes=notes,tightened=tightened,final=final,closed=closed,
            transport="Console UTF-8 input and SGR mouse; no atomic bracketed-paste claim")
    finally:
        console.close()
        result.update(executable_sha256=hashlib.sha256(args.executable.read_bytes()).hexdigest(),
            bundle_sha256=hashlib.sha256(args.bundle.read_bytes()).hexdigest(),last_state=state())
        (args.directory/"output.ansi").write_bytes(console.output)
        (args.directory/"acceptance.json").write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding="utf-8")
    print(json.dumps({"status":result["status"]}))
if __name__ == "__main__": main()
