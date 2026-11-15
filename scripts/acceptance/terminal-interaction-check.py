#!/usr/bin/env python3
"""Owned actual ConPTY navigation, modal, search and controlled edit across two builds."""
import argparse
import hashlib
import importlib.util
import json
import time
from pathlib import Path
spec=importlib.util.spec_from_file_location("conpty",Path(__file__).with_name("terminal-conpty-check.py"))
conpty=importlib.util.module_from_spec(spec);spec.loader.exec_module(conpty)
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before",type=Path,required=True)
    parser.add_argument("--after",type=Path,required=True)
    parser.add_argument("--directory",type=Path,required=True)
    args=parser.parse_args();args.directory.mkdir(parents=True,exist_ok=False)
    cases=[]
    for label,executable in [("before",args.before),("after",args.after)]:
        folder=args.directory/label;folder.mkdir();receipt=folder.resolve()/"receipt.json"
        console=conpty.Console([executable.resolve(),"--receipt",receipt]);case={"name":label,"executable_sha256":sha(executable),"status":"incomplete"}
        trace=[]
        def send(name,data):
            console.send(data);trace.append({"name":name,"hex":data.hex()});time.sleep(.15)
        try:
            console.ready("Jadix portable app")
            send("edit-unicode","导航 中文 🦀".encode())
            send("tab-increment",b"\t");send("activate-increment",b"\r")
            send("tab-checkbox",b"\t");send("toggle",b" ")
            send("tab-review",b"\t");send("open-review",b"\r")
            console.ready("Review page: draft Signal is retained")
            send("navigate-back",b"\r")
            send("open-modal",b"\x0f");send("modal-tab",b"\t");send("dismiss-modal",b"\x1b")
            time.sleep(.1)
            send("search-offscreen",b"\x06");console.ready("500 shared")
            assert console.resize(console.pc,conpty.Coord(100,30))==0
            time.sleep(.2);send("repeat-search-after-resize",b"\x06")
            with console.lock:(folder/"application.ansi").write_bytes(bytes(console.output))
            console.send(b"\x11");console.finish()
            value=json.loads(receipt.read_text(encoding="utf8"))
            assert value["count"]==1 and value["checked"] and value["route"]=="edit",value
            assert value["value"]=="导航 中文 🦀" and value["dismissals"]==1 and not value["modal_open_on_exit"],value
            assert value["search_matches"]==1 and value["resizes"]>=1,value
            assert value["raw_restored"] and value["output_cleanup_complete"] and value["error"] is None,value
            case.update(status="pass",receipt=value)
        finally:
            console.close();case["reader_finished"]=not console.reader.is_alive();case["trace"]=trace
            (folder/"output.ansi").write_bytes(console.output)
            (folder/"case.json").write_text(json.dumps(case,ensure_ascii=False,indent=2),encoding="utf8")
        cases.append(case)
    fields=["count","checked","route","value","dismissals","modal_open_on_exit","search_matches","raw_restored","output_cleanup_complete","error"]
    assert all(cases[0]["receipt"][k]==cases[1]["receipt"][k] for k in fields)
    result={"status":"pass","real_windows_conpty":True,"processes":2,"business_receipt_fields_equal":fields,"cases":cases,"limits":"Actual Console IO and business state; no complete ANSI, emulator pixel, scanout or latency equality claim."}
    (args.directory/"acceptance.json").write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding="utf8")
    print(json.dumps({"status":"pass","processes":2}))
if __name__=="__main__":main()
