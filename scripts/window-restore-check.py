"""Windows Gallery regression: maximize, close, restart, verify actual monitor bounds.

Uses only the standard library and Win32. The temporary persistence file keeps
the user's saved window state intact. No UI automation service is required.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=Path("target/release/gallery.exe"))
    parser.add_argument("--output", type=Path, default=Path(".tmp/window-restore.json"))
    args = parser.parse_args()
    user = C.WinDLL("user32", use_last_error=True)
    user.SetProcessDPIAware()
    callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    user.EnumWindows.argtypes = [callback, W.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    user.IsWindowVisible.argtypes = user.IsZoomed.argtypes = [W.HWND]
    user.GetClientRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
    user.GetWindowTextLengthW.argtypes = [W.HWND]
    user.GetWindowRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
    user.ShowWindow.argtypes = [W.HWND, C.c_int]
    user.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
    user.MonitorFromWindow.argtypes = [W.HWND, W.DWORD]
    user.MonitorFromWindow.restype = W.HMONITOR

    class MonitorInfo(C.Structure):
        _fields_ = [("cbSize", W.DWORD), ("monitor", W.RECT),
                    ("work", W.RECT), ("flags", W.DWORD)]
    user.GetMonitorInfoW.argtypes = [W.HMONITOR, C.POINTER(MonitorInfo)]

    def window(pid):
        found = []
        @callback
        def visit(hwnd, _):
            owner = W.DWORD()
            user.GetWindowThreadProcessId(hwnd, C.byref(owner))
            if owner.value == pid and user.IsWindowVisible(hwnd) and user.GetWindowTextLengthW(hwnd):
                rect = W.RECT()
                user.GetClientRect(hwnd, C.byref(rect))
                if rect.right >= 200 and rect.bottom >= 200:
                    found.append(hwnd)
            return True
        user.EnumWindows(visit, 0)
        return found[0] if found else None

    def bounds(hwnd):
        rect = W.RECT()
        if not user.GetWindowRect(hwnd, C.byref(rect)):
            raise C.WinError(C.get_last_error())
        monitor = MonitorInfo(cbSize=C.sizeof(MonitorInfo))
        if not user.GetMonitorInfoW(user.MonitorFromWindow(hwnd, 2), C.byref(monitor)):
            raise C.WinError(C.get_last_error())
        values = lambda r: [r.left, r.top, r.right, r.bottom]
        # Maximized decorated HWNDs include invisible resize borders outside work area.
        covers = (rect.left <= monitor.work.left and rect.top <= monitor.work.top
                  and rect.right >= monitor.work.right and rect.bottom >= monitor.work.bottom)
        return dict(maximized=bool(user.IsZoomed(hwnd)), rect=values(rect),
                    work=values(monitor.work), covers_work_area=covers)

    results = []
    exe = args.exe.resolve()
    with tempfile.TemporaryDirectory(prefix="jadix-window-restore-") as folder:
        state = Path(folder) / "state.toml"
        state.write_text("[windows.main]\nwidth = 1000\nheight = 700\nx = 70\ny = 80\n"
                         "prev_x = 70\nprev_y = 80\nmaximized = false\n", encoding="utf-8")
        env = os.environ.copy()
        env["JADIX_WINDOW_STATE_PATH"] = str(state)
        env["RUST_LOG"] = "off"
        for key in ("JADIX_CPU_TELEMETRY", "JADIX_CPU_PROFILE", "JADIX_RENDER_PROFILE"):
            env.pop(key, None)
        for index in range(3):
            proc = subprocess.Popen([str(exe)], cwd=exe.parent, env=env,
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            hwnd = None
            try:
                deadline = time.monotonic() + 15
                while not hwnd and time.monotonic() < deadline:
                    if proc.poll() is not None:
                        raise RuntimeError(f"Gallery exited: {proc.returncode}")
                    hwnd = window(proc.pid)
                    if not hwnd:
                        time.sleep(.01)
                if not hwnd:
                    raise RuntimeError("Gallery did not show a window")
                # Observe from the first visible HWND. A delayed sample misses
                # a normal-size flash followed by maximization during startup.
                startup = []
                start = time.monotonic()
                for _ in range(100):
                    sample = bounds(hwnd)
                    if not startup or any(sample[k] != startup[-1][k] for k in sample):
                        startup.append(dict(elapsed=time.monotonic() - start, **sample))
                    time.sleep(.01)
                initial = bounds(hwnd)
                if index == 0:
                    assert not initial["maximized"], initial
                    user.ShowWindow(hwnd, 3)  # SW_MAXIMIZE
                    time.sleep(.3)
                row = dict(launch=index + 1, startup=startup, initial=initial, before_close=bounds(hwnd))
                if index == 2:
                    user.ShowWindow(hwnd, 9)  # SW_RESTORE: retain normal placement, too.
                    time.sleep(.2)
                    row["restored"] = bounds(hwnd)
                    row["normal_geometry_preserved"] = (
                        not row["restored"]["maximized"]
                        and row["restored"]["rect"] == results[0]["initial"]["rect"])
                    user.ShowWindow(hwnd, 3)
                    time.sleep(.2)
                results.append(row)
            finally:
                if hwnd:
                    user.PostMessageW(hwnd, 0x10, 0, 0)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                    raise RuntimeError("Gallery failed to close and save state")
            row["saved"] = tomllib.loads(state.read_text(encoding="utf-8"))["windows"]["main"]
            row["passed"] = (row["before_close"]["maximized"]
                             and row["before_close"]["covers_work_area"]
                             and row["saved"]["maximized"]
                             and row["saved"]["width"] == 1000
                             and row["saved"]["height"] == 700
                             and row.get("normal_geometry_preserved", True)
                             and (index == 0 or all(s["covers_work_area"] and s["maximized"] for s in startup)))
            print(json.dumps(row), flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(dict(executable=str(exe), results=results), indent=2), encoding="utf-8")
    if not all(r["passed"] for r in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
