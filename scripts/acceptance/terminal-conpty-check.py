#!/usr/bin/env python3
"""Windows-owned ConPTY: native Console input, resize, Unicode and mode restore.

Uses only OS APIs, with an independently drained synchronous output channel:
https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session
This is a character Console test, not a Kitty graphics emulator or UI screenshot.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import subprocess
import threading
import time


class Coord(C.Structure):
    _fields_ = [("x", C.c_short), ("y", C.c_short)]


class Startup(C.Structure):
    _fields_ = [("cb", W.DWORD), ("reserved", W.LPWSTR), ("desktop", W.LPWSTR), ("title", W.LPWSTR),
                *[(n, W.DWORD) for n in ("x", "y", "width", "height", "cols", "rows", "fill", "flags")],
                ("show", W.WORD), ("reserved_bytes", W.WORD), ("reserved_ptr", C.c_void_p),
                ("stdin", W.HANDLE), ("stdout", W.HANDLE), ("stderr", W.HANDLE)]


class StartupEx(C.Structure):
    _fields_ = [("info", Startup), ("attributes", C.c_void_p)]


class Process(C.Structure):
    _fields_ = [("process", W.HANDLE), ("thread", W.HANDLE), ("pid", W.DWORD), ("tid", W.DWORD)]


def bind(library, name, result, arguments):
    function = getattr(library, name)
    function.restype, function.argtypes = result, arguments
    return function


class Console:
    def __init__(self, command):
        self.k = C.WinDLL("kernel32", use_last_error=True)
        self.close_handle = bind(self.k, "CloseHandle", W.BOOL, [W.HANDLE])
        self.read = bind(self.k, "ReadFile", W.BOOL, [W.HANDLE, C.c_void_p, W.DWORD, C.POINTER(W.DWORD), C.c_void_p])
        self.write = bind(self.k, "WriteFile", W.BOOL, [W.HANDLE, C.c_void_p, W.DWORD, C.POINTER(W.DWORD), C.c_void_p])
        self.wait = bind(self.k, "WaitForSingleObject", W.DWORD, [W.HANDLE, W.DWORD])
        self.code = bind(self.k, "GetExitCodeProcess", W.BOOL, [W.HANDLE, C.POINTER(W.DWORD)])
        self.terminate = bind(self.k, "TerminateProcess", W.BOOL, [W.HANDLE, W.UINT])
        self.resize = bind(self.k, "ResizePseudoConsole", C.c_long, [W.HANDLE, Coord])
        self.close_console = bind(self.k, "ClosePseudoConsole", None, [W.HANDLE])
        create_pipe = bind(self.k, "CreatePipe", W.BOOL, [C.POINTER(W.HANDLE), C.POINTER(W.HANDLE), C.c_void_p, W.DWORD])
        handles = [W.HANDLE() for _ in range(4)]
        self.handles, self.pc, self.proc, self.reader = handles, W.HANDLE(), Process(), None
        self.output, self.lock = bytearray(), threading.Lock()
        self.paused = threading.Event()
        try:
            self.check(create_pipe(C.byref(handles[0]), C.byref(handles[1]), None, 0))
            self.check(create_pipe(C.byref(handles[2]), C.byref(handles[3]), None, 0))
            create = bind(self.k, "CreatePseudoConsole", C.c_long, [Coord, W.HANDLE, W.HANDLE, W.DWORD, C.POINTER(W.HANDLE)])
            assert create(Coord(80, 24), handles[0], handles[3], 0, C.byref(self.pc)) == 0
            # Drain during startup and shutdown, including ClosePseudoConsole.
            self.reader = threading.Thread(target=self.drain, daemon=True)
            self.reader.start()
            initialize = bind(self.k, "InitializeProcThreadAttributeList", W.BOOL, [C.c_void_p, W.DWORD, W.DWORD, C.POINTER(C.c_size_t)])
            update = bind(self.k, "UpdateProcThreadAttribute", W.BOOL, [C.c_void_p, W.DWORD, C.c_size_t, C.c_void_p, C.c_size_t, C.c_void_p, C.c_void_p])
            delete = bind(self.k, "DeleteProcThreadAttributeList", None, [C.c_void_p])
            size = C.c_size_t()
            initialize(None, 1, 0, C.byref(size))
            attributes = C.create_string_buffer(size.value)
            self.check(initialize(attributes, 1, 0, C.byref(size)))
            try:
                self.check(update(attributes, 0, 0x00020016, self.pc, C.sizeof(W.HANDLE), None, None))
                startup = StartupEx()
                startup.info.cb = C.sizeof(startup)
                startup.attributes = C.addressof(attributes)
                create_process = bind(self.k, "CreateProcessW", W.BOOL,
                    [W.LPCWSTR, W.LPWSTR, C.c_void_p, C.c_void_p, W.BOOL, W.DWORD, C.c_void_p, W.LPCWSTR, C.c_void_p, C.POINTER(Process)])
                line = C.create_unicode_buffer(subprocess.list2cmdline(list(map(str, command))))
                # A redirected launcher can otherwise copy invalid numerical
                # std handles into its attached child even with inheritance off.
                # This changes only this fixture process, only during creation.
                get_std = bind(self.k, "GetStdHandle", W.HANDLE, [W.DWORD])
                set_std = bind(self.k, "SetStdHandle", W.BOOL, [W.DWORD, W.HANDLE])
                saved = [get_std(n) for n in (-10, -11, -12)]
                try:
                    for n in (-10, -11, -12): self.check(set_std(n, None))
                    self.check(create_process(None, line, None, None, False, 0x00080000, None, None, C.byref(startup), C.byref(self.proc)))
                finally:
                    for n, handle in zip((-10, -11, -12), saved): set_std(n, handle)
            finally:
                delete(attributes)
            for index in (0, 3):
                self.close_handle(handles[index]); handles[index] = W.HANDLE()
        except BaseException:
            self.close()
            raise

    @staticmethod
    def check(success):
        if not success:
            raise C.WinError(C.get_last_error())

    def drain(self):
        buffer, count = C.create_string_buffer(65536), W.DWORD()
        while self.read(self.handles[2], buffer, len(buffer), C.byref(count), None) and count.value:
            with self.lock:
                self.output.extend(buffer.raw[:count.value])
            while self.paused.is_set():
                time.sleep(.005)

    def send(self, data):
        count = W.DWORD()
        self.check(self.write(self.handles[1], data, len(data), C.byref(count), None))
        assert count.value == len(data)

    def ready(self, text):
        end = time.monotonic() + 10
        while time.monotonic() < end:
            with self.lock:
                if text.encode() in self.output:
                    return
            assert self.wait(self.proc.process, 0) == 258, bytes(self.output)
            time.sleep(.02)
        raise TimeoutError(text)

    def finish(self):
        assert self.wait(self.proc.process, 10000) == 0, "owned Console process did not exit"
        code = W.DWORD()
        self.check(self.code(self.proc.process, C.byref(code)))
        assert code.value == 0, (code.value, bytes(self.output))

    def close(self):
        self.paused.clear()
        if self.proc.process and self.wait(self.proc.process, 0) == 258:
            self.terminate(self.proc.process, 1); self.wait(self.proc.process, 5000)
        if self.pc:
            self.close_console(self.pc); self.pc = W.HANDLE()
        if self.reader:
            self.reader.join(timeout=3)
        for handle in [self.proc.thread, self.proc.process, *self.handles]:
            if handle: self.close_handle(handle)
        assert self.reader is None or not self.reader.is_alive(), "ConPTY output reader did not finish"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--session-contract", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--auto-layout", action="store_true")
    parser.add_argument("--portable", action="store_true", help="exercise mounted portable_app callbacks/navigation")
    parser.add_argument("--shortcuts", action="store_true", help="also run named command from editor, block under modal")
    parser.add_argument("--workbench", type=Path, help="also exercise CPU Canvas to Braille over ConPTY")
    parser.add_argument("--output-stall", type=Path, help="pause real ConPTY consumer for timeout/cancel cases")
    parser.add_argument("--colors",choices=["rgb","256","16","mono"],default="rgb")
    parser.add_argument("--menus", action="store_true")
    args = parser.parse_args()
    if os.name != "nt": parser.error("Windows required")
    args.directory = args.directory.resolve()
    args.directory.mkdir(parents=True, exist_ok=False)
    results = []
    scenarios=[("interaction", args.executable), ("modes", args.session_contract)]
    if args.workbench: scenarios.append(("braille",args.workbench))
    if args.output_stall: scenarios += [("stall-timeout",args.output_stall),("stall-cancel",args.output_stall),("stall-continuous",args.output_stall)]
    for name, exe in scenarios:
        receipt = args.directory / f"{name}.json"
        command=[exe.resolve(), "--receipt", receipt]
        if name=="interaction": command += ["--colors",args.colors]
        if name=="interaction" and args.auto_layout: command.append("--auto-layout")
        if name=="interaction" and args.menus: command.append("--menus")
        if name=="stall-cancel": command.append("--cancel")
        if name=="stall-continuous": command.append("--continuous-stall")
        if name=="braille": command=[exe.resolve(),"--braille"]
        console = Console(command)
        result = None
        try:
            if name.startswith("stall-"):
                deadline = time.monotonic() + 10
                while not Path(str(receipt)+".ready").exists():
                    assert time.monotonic() < deadline, "stall fixture not ready"
                    time.sleep(.02)
                console.paused.set()
                Path(str(receipt)+".go").write_text("go")
                deadline = time.monotonic() + 3
                while not Path(str(receipt)+".output-done").exists():
                    assert time.monotonic() < deadline, "stalled ConPTY write did not return within 3 s"
                    time.sleep(.01)
                if name=="stall-continuous":
                    deadline=time.monotonic()+3
                    while not receipt.exists():
                        assert time.monotonic()<deadline,"Console restoration blocked while reader stayed paused"
                        time.sleep(.01)
                    value=json.loads(receipt.read_text(encoding="utf-8"))
                    assert value["status"]=="pass" and value["restore_ms"]<1000 and value["late_writer_rejected"],value
                    result={"name":name,"status":"pass","receipt":value,"executable_sha256":hashlib.sha256(exe.read_bytes()).hexdigest(),"boundary":"UI return is bounded; permanent driver stalls may defer mode restoration"}
                    console.paused.clear();console.finish();continue
                console.paused.clear()
                Path(str(receipt)+".resume").write_text("resume")
                console.finish()
                value = json.loads(receipt.read_text(encoding="utf-8"))
                assert value["status"]=="pass" and value["console_modes_and_codepage_restored"], value
                result={"name":name,"status":"pass","receipt":value,"executable_sha256":hashlib.sha256(exe.read_bytes()).hexdigest()}
                continue
            elif name == "braille":
                console.ready("Jadix terminal workbench")
                console.send(b"+");time.sleep(.2);console.send(b"q")
            elif name == "interaction":
                if args.portable:
                    console.ready("portable")
                    typed = "ConPTY 中文 🦀"
                    console.send(typed.encode());console.ready("🦀")
                    # A changed counter may be emitted as one diff cell; exact
                    # business state is checked in the exit receipt below.
                    console.send(b"\x1a\x19\t\r");time.sleep(.3)
                    if args.menus:
                        console.send(b"\x1b[21~\r");time.sleep(.2)
                    if args.shortcuts:
                        console.send(b"\x1b[Z\x07\t");time.sleep(.2)
                    console.send(b"\t \t\r");console.ready("Back")
                    console.send(b"\r");time.sleep(.2)
                    console.send(b"\x0f");time.sleep(.3)
                    if args.shortcuts:
                        console.send(b"\x07");time.sleep(.2)
                    console.send(b"\x1b");time.sleep(.2)
                    assert console.resize(console.pc, Coord(100,30))==0
                    time.sleep(.3);console.send(b"\x06");time.sleep(.2)
                    console.send(b"\x11")
                    console.finish()
                    value=json.loads(receipt.read_text(encoding="utf-8"))
                    assert value["portable"] and value["value"]==typed and value["count"]==1+int(args.shortcuts)+int(args.menus),value
                    if args.menus: assert value["last_menu_command"]=="menu.increment",value
                    if args.shortcuts:
                        assert value["layout_created"]==0 and value["layout_reused"]>500,value
                    assert value["checked"] and value["route"]=="edit" and value["search_matches"]==1,value
                    assert value["dismissals"]==1 and not value["modal_open_on_exit"],value
                    assert value["raw_restored"] and value["error"] is None and value["resizes"]>=1,value
                    result={"name":name,"status":"pass","receipt":value,"executable_sha256":hashlib.sha256(exe.read_bytes()).hexdigest(),"reader_finished":not console.reader.is_alive()}
                    continue
                # ConHost replaces monochrome blank cells with cursor-forward
                # sequences; readiness is a single word, state is checked below.
                console.ready("surface")
                typed = "ConPTY 中文 🦀"
                console.send(typed.encode())
                # ConHost can split a word across cursor/diff sequences. The
                # final scalar marks progress; exact text is checked in receipt.
                console.ready("🦀")
                console.send(b"\x1a\x19\t\r")  # Undo/redo, focus and activate.
                time.sleep(.2)
                assert console.resize(console.pc, Coord(100, 30)) == 0
                time.sleep(.4)
                if args.auto_layout:
                    console.send(b"\x06500  shared\r")
                    time.sleep(.4)
                console.send(b"\x11")
            console.finish()
            value = json.loads(receipt.read_text(encoding="utf-8")) if name!="braille" else {"kitty_required":False}
            if name == "interaction":
                assert value["value"] == typed and value["count"] == 1, value
                assert value["raw_restored"] and value["resizes"] >= 1 and value["error"] is None, value
                if args.auto_layout:
                    assert value["auto_layout"] and value["search_requests"]>0 and value["search_matches"]==1,value
            elif name=="modes":
                assert value["status"] == "pass" and value["console_modes_restored"], value
            result = {"name": name, "status": "pass", "receipt": value,
                      "executable_sha256": hashlib.sha256(exe.read_bytes()).hexdigest()}
        finally:
            console.close()
            (args.directory / f"{name}.ansi").write_bytes(console.output)
            if result is not None:
                if name=="braille":
                    text=console.output.decode("utf-8",errors="replace")
                    result["braille_characters"]=sum(0x2800<=ord(c)<=0x28ff for c in text)
                    assert result["braille_characters"]>40
                    assert b"\x1b_G" not in console.output
                result["reader_finished"] = True
                results.append(result)
            (args.directory / "acceptance.json").write_text(json.dumps({"scenarios": results}, indent=2), encoding="utf-8")
    print(json.dumps({"status": "pass", "scenarios": len(results), "real_windows_conpty": True, "graphics": False}))


if __name__ == "__main__":
    main()
