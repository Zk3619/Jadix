"""Windows Gallery startup gate: process launch -> visible window -> painted content.

Build Gallery first, then run python scripts/benchmarks/startup-bench.py. Compilation time
is excluded. Only windows belonging to the launched process are inspected.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import time


def observe_startup(text, elapsed, observed):
    for name, marker in (
        ("system_feedback", "startup system=cpu-buffer"),
        ("startup_frame", "startup startup_frame epoch="),
        ("window_frame", "startup window_frame epoch="),
        ("ready", "phase=Ready"),
        ("content_frame", "startup content_frame epoch="),
    ):
        if name not in observed and marker in text:
            observed[name] = elapsed


def sample_percentile(samples, percentile):
    if not samples:
        return None
    ordered = sorted(samples)
    return ordered[max(0, math.ceil(len(ordered) * percentile) - 1)]


def maximum_logged_duration_ms(text, marker):
    values = []
    for line in text.splitlines():
        if marker in line:
            try:
                value = float(line.partition(marker)[2].split()[0])
            except (ValueError, IndexError):
                continue
            if math.isfinite(value) and value >= 0:
                values.append(value)
    return max(values) if values else None


def is_startup_window(title):
    return title.endswith((" Startup", " Splash"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=Path("target/debug/gallery.exe"))
    parser.add_argument("--output", type=Path, default=Path(".tmp/startup.json"))
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--cwd", type=Path, help="working directory (use target/release for Explorer launches)")
    parser.add_argument("--state", type=Path, help="isolated window state file")
    parser.add_argument("--max-content-seconds", type=float, default=3.0)
    parser.add_argument("--max-blank-seconds", type=float, default=0.2)
    parser.add_argument("--splash-mode", choices=("off", "system", "component", "window"), default="component")
    parser.add_argument("--observe-startup", action="store_true", help="require runtime content_frame and retain startup logs")
    parser.add_argument("--arg", action="append", default=[], help="extra executable argument; use --arg=--flag")
    args = parser.parse_args()
    if os.name != "nt" or args.rounds < 1:
        parser.error("requires Windows and at least one round")
    user = C.WinDLL("user32", use_last_error=True)
    gdi = C.WinDLL("gdi32", use_last_error=True)
    user.SetProcessDPIAware()
    kernel = C.WinDLL("kernel32", use_last_error=True)
    psapi = C.WinDLL("psapi", use_last_error=True)
    kernel.OpenProcess.argtypes = [W.DWORD, W.BOOL, W.DWORD]
    kernel.OpenProcess.restype = W.HANDLE
    kernel.CloseHandle.argtypes = [W.HANDLE]

    class ProcessMemory(C.Structure):
        _fields_ = [("cb", W.DWORD), ("page_faults", W.DWORD)] + [(name, C.c_size_t) for name in ("peak_working_set", "working_set", "peak_paged_pool", "paged_pool", "peak_nonpaged_pool", "nonpaged_pool", "pagefile", "peak_pagefile", "private")]

    psapi.GetProcessMemoryInfo.argtypes = [W.HANDLE, C.POINTER(ProcessMemory), W.DWORD]

    def private_bytes(pid):
        handle = kernel.OpenProcess(0x410, False, pid)
        if not handle:
            return None
        try:
            memory = ProcessMemory(cb=C.sizeof(ProcessMemory))
            return memory.private if psapi.GetProcessMemoryInfo(handle, C.byref(memory), memory.cb) else None
        finally:
            kernel.CloseHandle(handle)

    callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    user.EnumWindows.argtypes = [callback, W.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    user.IsWindowVisible.argtypes = [W.HWND]
    user.IsIconic.argtypes = [W.HWND]
    user.GetWindowTextLengthW.argtypes = [W.HWND]
    user.GetWindowTextW.argtypes = [W.HWND, W.LPWSTR, C.c_int]
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
    gdi.SetStretchBltMode.argtypes = [W.HDC, C.c_int]
    gdi.SetBrushOrgEx.argtypes = [W.HDC, C.c_int, C.c_int, C.c_void_p]
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
            if not gdi.SetStretchBltMode(memory, 4) or not gdi.SetBrushOrgEx(memory, 0, 0, None):
                raise C.WinError(C.get_last_error())
            if not gdi.StretchBlt(memory, 0, 0, 128, 96, dc, origin.x, origin.y,
                                  rect.right, rect.bottom, 0x00CC0020):
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

    def visible_window(pid, business=False):
        found = []
        @callback
        def visit(hwnd, _):
            owner = W.DWORD()
            user.GetWindowThreadProcessId(hwnd, C.byref(owner))
            if owner.value == pid and user.IsWindowVisible(hwnd) and not user.IsIconic(hwnd):
                title = C.create_unicode_buffer(256)
                user.GetWindowTextW(hwnd, title, len(title))
                if business and is_startup_window(title.value):
                    return True
                rect = W.RECT()
                user.GetClientRect(hwnd, C.byref(rect))
                if rect.right >= 200 and rect.bottom >= 200 and user.GetWindowTextLengthW(hwnd):
                    found.append((hwnd, rect))
            return True
        user.EnumWindows(visit, 0)
        return found[0] if found else None

    args.output.parent.mkdir(parents=True, exist_ok=True)
    results = []
    for index in range(args.rounds):
        env = os.environ.copy()
        for key in ("JADIX_CPU_TELEMETRY", "JADIX_CPU_PROFILE", "JADIX_RENDER_PROFILE"):
            env.pop(key, None)
        env["RUST_LOG"] = "info,jadix::app::lifecycle=debug"
        if args.splash_mode is not None:
            env["JADIX_SPLASH"] = args.splash_mode
        if args.state:
            env["JADIX_WINDOW_STATE_PATH"] = str(args.state.resolve())
        log_path = args.output.with_name(f"{args.output.stem}-round-{index + 1}.log")
        start = time.perf_counter()
        visible = feedback = content = None
        blank_started = None
        maximum_blank = 0.0
        sampled_color_max = 0
        content_color_count = None
        observed = {}
        memory_samples = []
        hwnd = None
        with open(log_path, "w", encoding="utf-8") as sink:
            proc = subprocess.Popen([str(args.exe.resolve()), *args.arg], env=env,
                                    cwd=args.cwd.resolve() if args.cwd else Path(__file__).resolve().parents[2],
                                    stdout=sink, stderr=sink)
            try:
                while time.perf_counter() - start < max(10, args.max_content_seconds * 2):
                    if proc.poll() is not None:
                        raise RuntimeError(f"application exited with {proc.returncode}")
                    memory = private_bytes(proc.pid)
                    if memory is not None:
                        memory_samples.append(memory)
                    elapsed = time.perf_counter() - start
                    observe_startup(log_path.read_text(encoding="utf-8", errors="replace"), elapsed, observed)
                    window = visible_window(proc.pid, business="content_frame" in observed)
                    if window:
                        hwnd, rect = window
                        if visible is None:
                            visible = elapsed
                        if user.GetForegroundWindow() != hwnd:
                            user.BringWindowToTop(hwnd)
                            user.SetForegroundWindow(hwnd)
                        if user.GetForegroundWindow() != hwnd:
                            user.SwitchToThisWindow(hwnd, True)
                            time.sleep(.01)
                            continue
                        colors = content_colors(hwnd, rect)
                        sampled_color_max = max(sampled_color_max, colors)
                        elapsed = time.perf_counter() - start
                        if colors >= 12:
                            if feedback is None:
                                feedback = elapsed
                            if blank_started is not None:
                                maximum_blank = max(maximum_blank, elapsed - blank_started)
                                blank_started = None
                            if "content_frame" in observed:
                                content = elapsed
                                content_color_count = colors
                                break
                        elif blank_started is None:
                            blank_started = elapsed
                    time.sleep(.01)
            finally:
                if hwnd:
                    user.PostMessageW(hwnd, 0x10, 0, 0)
                try:
                    proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
        if blank_started is not None:
            maximum_blank = max(maximum_blank, time.perf_counter() - start - blank_started)
        blank = maximum_blank
        startup_log = log_path.read_text(encoding="utf-8", errors="replace")
        row = dict(round=index + 1, visible_seconds=visible, feedback_seconds=feedback,
                   content_seconds=content, visible_blank_seconds=blank,
                   sampled_color_max=sampled_color_max, content_color_count=content_color_count,
                   client_size=[rect.right, rect.bottom] if visible is not None else None,
                   observed_events=observed, log=str(log_path),
                   maximum_bootstrap_ui_poll_ms=maximum_logged_duration_ms(startup_log, "startup bootstrap_poll_ms="),
                   maximum_primary_mount_ms=maximum_logged_duration_ms(startup_log, "startup primary_ui_mounted_ms="),
                   observed_private_peak_bytes=max(memory_samples) if memory_samples else None,
                   private_increment_bytes=max(memory_samples) - memory_samples[0] if memory_samples else None,
                   passed=content is not None and content < args.max_content_seconds
                          and blank is not None and blank < args.max_blank_seconds)
        results.append(row)
        print(json.dumps(row), flush=True)
    measured = [r["content_seconds"] for r in results if r["content_seconds"] is not None]
    output = dict(executable=str(args.exe.resolve()),
                  executable_sha256=hashlib.sha256(args.exe.read_bytes()).hexdigest(),
                  content_median_seconds=statistics.median(measured) if measured else None,
                  content_p95_seconds=sample_percentile(measured, 0.95),
                  feedback_median_seconds=sample_percentile([row["feedback_seconds"] for row in results if row["feedback_seconds"] is not None], 0.5),
                  content_gate="runtime content_frame and business-window pixels",
                  mode=args.splash_mode, arguments=args.arg,
                  environment={key: os.environ.get(key) for key in ("JADIX_GPU_BACKEND", "LOCALAPPDATA")},
                  continuous_ui_occupancy_ms=None,
                  continuous_ui_occupancy_unavailable_reason="external pixel sampler does not instrument UI scheduling",
                  results=results)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2), encoding="utf-8")
    if not all(r["passed"] for r in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
