#!/usr/bin/env python3
"""Controlled pipe peer verifies color-query replies and bounded timeout."""
import argparse
import hashlib
import json
import importlib.util
import shlex
import tempfile
import subprocess
import threading
import time
from pathlib import Path

def real_terminal(args):
    spec=importlib.util.spec_from_file_location("xterm_check",Path(__file__).with_name("terminal-xterm-check.py"))
    xterm=importlib.util.module_from_spec(spec);spec.loader.exec_module(xterm)
    folder=args.directory.resolve();receipt=folder/"receipt.json"
    process=None
    try:
        with tempfile.TemporaryDirectory(prefix="jadix-colors-") as temporary, (folder/"runtime.log").open("wb") as log:
            script=Path(temporary)/"run.sh"
            script.write_text("stty -g > "+shlex.quote(str(folder/"stty-before.txt"))+"\n"+
                shlex.join([str(args.executable.resolve()),"--query-colors","--colors","16","--receipt",str(receipt)])+"\nstatus=$?\n"+
                "stty -g > "+shlex.quote(str(folder/"stty-after.txt"))+"\nprintf '%s' \"$status\" > "+shlex.quote(str(folder/"exit-code.txt"))+"\n")
            process=subprocess.Popen([str(args.terminal.resolve()),"-geometry","80x24","-title","Jadix owned color query","-e","/bin/sh",str(script)],stdout=log,stderr=log)
            def window():
                result=subprocess.run(["xdotool","search","--pid",str(process.pid)],capture_output=True,timeout=3)
                return result.stdout.strip().splitlines()[0].decode() if result.returncode==0 else None
            xid=xterm.until(window)
            xterm.run("xdotool","windowfocus","--sync",xid)
            time.sleep(1.2)
            xterm.run("xwd","-silent","-id",xid,"-out",folder/"queried.xwd")
            try: xterm.run("xdotool","key","--window",xid,"ctrl+q")
            except subprocess.CalledProcessError:
                if not receipt.exists(): raise
            xterm.until(receipt.exists);process.wait(timeout=10)
            value=json.loads(receipt.read_text())
            assert process.returncode==0 and (folder/"exit-code.txt").read_text()=="0",value
            assert value["error"] is None and value["raw_restored"] and value["output_cleanup_complete"],value
            assert value["default_colors"] is not None,value
            assert (folder/"stty-before.txt").read_bytes()==(folder/"stty-after.txt").read_bytes()
            report={"status":"pass","real_terminal_palette_observed":value["default_colors"]["complete"],"receipt":value,
                    "executable_sha256":hashlib.sha256(args.executable.read_bytes()).hexdigest(),
                    "terminal_sha256":hashlib.sha256(args.terminal.read_bytes()).hexdigest()}
            (folder/"acceptance.json").write_text(json.dumps(report,indent=2))
            print(json.dumps(report))
    finally:
        if process and process.poll() is None:
            process.terminate()
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)

def real_conpty(args):
    spec=importlib.util.spec_from_file_location("conpty_check",Path(__file__).with_name("terminal-conpty-check.py"))
    conpty=importlib.util.module_from_spec(spec);spec.loader.exec_module(conpty)
    receipt=args.directory.resolve()/"receipt.json"
    console=conpty.Console([args.executable.resolve(),"--query-colors","--receipt",receipt])
    try:
        console.ready("portable");time.sleep(.7);console.send(b"\x11");console.finish()
        value=json.loads(receipt.read_text(encoding="utf-8"))
        assert value["default_colors"] is not None and value["error"] is None,value
        assert value["raw_restored"] and value["output_cleanup_complete"],value
    finally:
        console.close();(args.directory/"output.ansi").write_bytes(console.output)
    report={"status":"pass","real_windows_conpty":True,
            "real_terminal_palette_observed":value["default_colors"]["complete"],
            "executable_sha256":hashlib.sha256(args.executable.read_bytes()).hexdigest(),
            "reader_finished":not console.reader.is_alive(),"receipt":value}
    (args.directory/"acceptance.json").write_text(json.dumps(report,indent=2),encoding="utf-8")
    print(json.dumps(report))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable",type=Path,required=True)
    parser.add_argument("--directory",type=Path,required=True)
    parser.add_argument("--terminal",type=Path,help="Optional real X11 terminal, e.g. urxvt")
    parser.add_argument("--conpty",action="store_true",help="Observe the actual Windows ConPTY reply or timeout")
    args=parser.parse_args()
    if args.conpty and args.terminal: parser.error("choose one terminal target")
    args.directory.mkdir(parents=True,exist_ok=False)
    if args.conpty:
        return real_conpty(args)
    if args.terminal:
        return real_terminal(args)
    results=[]
    for mode in ("reply","timeout"):
        receipt=args.directory.resolve()/f"{mode}.json"
        process=subprocess.Popen([str(args.executable.resolve()),"--stdio","--query-colors","--receipt",str(receipt)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        output=bytearray()
        ready=threading.Event()
        def drain():
            while byte:=process.stdout.read(1):
                output.extend(byte)
                if b"\x1b]11;?\x1b\\" in output: ready.set()
        reader=threading.Thread(target=drain,daemon=True)
        reader.start()
        try:
            assert ready.wait(5),"query was not issued"
            started=time.monotonic()
            if mode=="reply":
                # Invalid, unrelated OSC and literal paste cannot resolve defaults.
                process.stdin.write(b"\x1b]11;rgb:bad/xx/0\x07\x1b]52;c;ignored\x07")
                process.stdin.write(b"\x1b]10;rgb:1234/abcd/ffff\x1b\\\x1b]11;rgb:eeee/dddd/cccc\x07")
                process.stdin.flush()
            # The timeout is 500ms; this bounded wait lets its deadline run.
            time.sleep(.7 if mode=="timeout" else .1)
            process.stdin.write(b"\x11");process.stdin.flush()
            assert process.wait(timeout=5)==0,process.stderr.read()
            reader.join(timeout=2);assert not reader.is_alive()
            value=json.loads(receipt.read_text(encoding="utf-8"))
            colors=value["default_colors"]
            assert value["error"] is None and value["value"]=="",value
            if mode=="reply":
                assert colors=={"foreground":[18,171,255],"background":[238,221,204],"complete":True},value
            else:
                assert colors=={"foreground":None,"background":None,"complete":False},value
            results.append({"mode":mode,"receipt":value,"elapsed_s":time.monotonic()-started,"reader_finished":True})
        finally:
            if process.poll() is None: process.kill();process.wait(timeout=5)
            process.stdin.close();reader.join(timeout=2)
            (args.directory/f"{mode}.ansi").write_bytes(output)
            (args.directory/f"{mode}.stderr").write_bytes(process.stderr.read())
            process.stdout.close();process.stderr.close()
    report={"status":"pass","controlled_pipe":True,"real_terminal_palette_observed":False,
            "executable_sha256":hashlib.sha256(args.executable.read_bytes()).hexdigest(),"scenarios":results}
    (args.directory/"acceptance.json").write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding="utf-8")
    print(json.dumps({"status":"pass","scenarios":len(results)}))

if __name__=="__main__":main()
