"""Windows Gallery startup gate: process launch -> visible window -> painted content.

Build Gallery first, then run python scripts/startup-bench.py. Compilation time
is excluded. Only windows belonging to the launched process are inspected.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=Path("target/debug/gallery.exe"))
    parser.add_argument("--output", type=Path, default=Path(".tmp/startup.json"))
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--cwd", type=Path, help="working directory (use target/release for Explorer launches)")
    parser.add_argument("--state", type=Path, help="isolated window state file")
    parser.add_argument("--max-content-seconds", type=float, default=3.0)
    parser.add_argument("--max-blank-seconds", type=float, default=0.2)
    args = parser.parse_args()
    if os.name != "nt" or args.rounds < 1:
        parser.error("requires Windows and at least one round")
    user = C.WinDLL("user32", use_last_error=True)
    gdi = C.WinDLL("gdi32", use_last_error=True)
    user.SetProcessDPIAware()
    callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    user.EnumWindows.argtypes = [callback, W.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    user.IsWindowVisible.argtypes = [W.HWND]
    user.IsIconic.argtypes = [W.HWND]
    user.GetWindowTextLengthW.argtypes = [W.HWND]
    user.BringWindowToTop.argtypes = [W.HWND]
    user.SetForegroundWindow.argtypes = [W.HWND]
    user.GetForegroundWindow.restype = W.HWND
    user.SwitchToThisWindow.argtypes = [W.HWND, W.BOOL]
    user.GetClientRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
    user.ClientToScreen.argtypes = [W.HWND, C.POINTER(W.POINT)]
    user.GetDC.argtypes = [W.HWND]
    user.GetDC.restype = W.HDC
    user.ReleaseDC.argtypes = [W.HWND, W.HDC]
    user.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
    gdi.CreateCompatibleDC.argtypes = [W.HDC]
    gdi.CreateCompatibleDC.restype = W.HDC
    gdi.CreateCompatibleBitmap.argtypes = [W.HDC, C.c_int, C.c_int]
    gdi.CreateCompatibleBitmap.restype = W.HBITMAP
    gdi.SelectObject.argtypes = [W.HDC, W.HGDIOBJ]
    gdi.SelectObject.restype = W.HGDIOBJ
    gdi.DeleteObject.argtypes = [W.HGDIOBJ]
    gdi.DeleteDC.argtypes = [W.HDC]
    gdi.StretchBlt.argtypes = [W.HDC, C.c_int, C.c_int, C.c_int, C.c_int,
                              W.HDC, C.c_int, C.c_int, C.c_int, C.c_int, W.DWORD]
    gdi.GetDIBits.argtypes = [W.HDC, W.HBITMAP, W.UINT, W.UINT, C.c_void_p, C.c_void_p, W.UINT]

    class BitmapHeader(C.Structure):
        _fields_ = [("size", W.DWORD), ("width", W.LONG), ("height", W.LONG),
                    ("planes", W.WORD), ("bits", W.WORD), ("compression", W.DWORD),
                    ("bytes", W.DWORD), ("xppm", W.LONG), ("yppm", W.LONG),
                    ("used", W.DWORD), ("important", W.DWORD)]

    def content_colors(hwnd, rect):
        origin = W.POINT()
        user.ClientToScreen(hwnd, C.byref(origin))
        # DXGI presents to the composed desktop, not the white window GDI DC.
        dc = user.GetDC(None)
        memory = gdi.CreateCompatibleDC(dc)
        bitmap = gdi.CreateCompatibleBitmap(dc, 128, 96)
        previous = gdi.SelectObject(memory, bitmap)
        try:
            if not gdi.StretchBlt(memory, 0, 0, 128, 96, dc, origin.x + 100, origin.y + 100,
                                  min(rect.right - 200, 700), min(rect.bottom - 200, 500), 0x00CC0020):
                raise C.WinError(C.get_last_error())
            gdi.SelectObject(memory, previous)
            pixels = (W.DWORD * (128 * 96))()
            header = BitmapHeader(C.sizeof(BitmapHeader), 128, -96, 1, 32)
            if gdi.GetDIBits(memory, bitmap, 0, 96, pixels, C.byref(header), 0) != 96:
                raise C.WinError(C.get_last_error())
            return len({pixel & 0xffffff for pixel in pixels})
        finally:
            gdi.SelectObject(memory, previous)
            gdi.DeleteObject(bitmap)
            gdi.DeleteDC(memory)
            user.ReleaseDC(None, dc)

    def visible_window(pid):
        found = []
        @callback
        def visit(hwnd, _):
            owner = W.DWORD()
            user.GetWindowThreadProcessId(hwnd, C.byref(owner))
            if owner.value == pid and user.IsWindowVisible(hwnd) and not user.IsIconic(hwnd):
                rect = W.RECT()
                user.GetClientRect(hwnd, C.byref(rect))
                if rect.right >= 200 and rect.bottom >= 200 and user.GetWindowTextLengthW(hwnd):
                    found.append((hwnd, rect))
            return True
        user.EnumWindows(visit, 0)
        return found[0] if found else None

    results = []
    for index in range(args.rounds):
        env = os.environ.copy()
        # Exercise the normal persisted startup path, without diagnostic logging.
        for key in ("JADIX_CPU_TELEMETRY", "JADIX_CPU_PROFILE", "JADIX_RENDER_PROFILE"):
            env.pop(key, None)
        env["RUST_LOG"] = "off"
        if args.state:
            env["JADIX_WINDOW_STATE_PATH"] = str(args.state.resolve())
        start = time.perf_counter()
        visible = content = None
        hwnd = None
        with open(os.devnull, "w") as sink:
            proc = subprocess.Popen([str(args.exe.resolve())], env=env,
                                    cwd=args.cwd.resolve() if args.cwd else Path(__file__).resolve().parents[1],
                                    stdout=sink, stderr=sink)
            try:
                while time.perf_counter() - start < max(10, args.max_content_seconds * 2):
                    if proc.poll() is not None:
                        raise RuntimeError(f"Gallery exited with {proc.returncode}")
                    window = visible_window(proc.pid)
                    if window:
                        hwnd, rect = window
                        if visible is None:
                            visible = time.perf_counter() - start
                            user.BringWindowToTop(hwnd)
                            user.SetForegroundWindow(hwnd)
                        if user.GetForegroundWindow() != hwnd:
                            user.SwitchToThisWindow(hwnd, True)
                            time.sleep(.01)
                            continue
                        if content_colors(hwnd, rect) >= 12:
                            content = time.perf_counter() - start
                            break
                    time.sleep(.01)
            finally:
                if hwnd:
                    user.PostMessageW(hwnd, 0x10, 0, 0)
                try:
                    proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
        blank = content - visible if content is not None else None
        row = dict(round=index + 1, visible_seconds=visible, content_seconds=content,
                   visible_blank_seconds=blank,
                   passed=content is not None and content < args.max_content_seconds
                          and blank < args.max_blank_seconds)
        results.append(row)
        print(json.dumps(row), flush=True)
    measured = [r["content_seconds"] for r in results if r["content_seconds"] is not None]
    output = dict(executable=str(args.exe.resolve()),
                  executable_sha256=hashlib.sha256(args.exe.read_bytes()).hexdigest(),
                  content_median_seconds=statistics.median(measured) if measured else None,
                  results=results)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2), encoding="utf-8")
    if not all(r["passed"] for r in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
