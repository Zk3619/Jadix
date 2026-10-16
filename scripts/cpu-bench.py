"""Windows native Gallery CPU gate: 100% = one fully occupied logical CPU.

python scripts/cpu-bench.py --exe target/release/gallery.exe --output .tmp/cpu-before.json
Measures the actual GUI process, with native wheel messages at 60 Hz.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import hashlib
import math
import os
import platform
from pathlib import Path
import subprocess
import statistics
import time


class POINT(C.Structure):
    _fields_ = [("x", W.LONG), ("y", W.LONG)]


class RECT(C.Structure):
    _fields_ = [("left", W.LONG), ("top", W.LONG), ("right", W.LONG), ("bottom", W.LONG)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=Path("target/release/gallery.exe"))
    parser.add_argument("--output", type=Path, default=Path(".tmp/cpu-bench.json"))
    parser.add_argument("--seconds", type=float, default=10)
    parser.add_argument("--warmup", type=float, default=5)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--sections", default="about,ordinary,forms,layout")
    parser.add_argument("--no-gate", action="store_true")
    parser.add_argument("--profile", action="store_true", help="also write stage timings (separate diagnostic run)")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("native Windows benchmark requires Windows")
    if not math.isfinite(args.seconds) or not math.isfinite(args.warmup) or args.seconds < 1 or args.rounds < 1 or args.warmup < 1:
        parser.error("seconds, rounds and warmup must be positive")
    if args.width < 640 or args.height < 480:
        parser.error("window must be at least 640 x 480")
    sections = args.sections.split(",")
    if not sections or len(set(sections)) != len(sections) or any(s not in {"about", "ordinary", "forms", "layout"} for s in sections):
        parser.error("sections must contain about, ordinary, forms or layout")
    user = C.WinDLL("user32", use_last_error=True)
    kernel = C.WinDLL("kernel32", use_last_error=True)
    callback_type = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    user.EnumWindows.argtypes = [callback_type, W.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    user.GetWindowThreadProcessId.restype = W.DWORD
    user.AttachThreadInput.argtypes = [W.DWORD, W.DWORD, W.BOOL]
    user.PeekMessageW.argtypes = [C.c_void_p, W.HWND, W.UINT, W.UINT, W.UINT]
    user.BringWindowToTop.argtypes = [W.HWND]
    user.SwitchToThisWindow.argtypes = [W.HWND, W.BOOL]
    kernel.GetCurrentThreadId.restype = W.DWORD
    user.IsWindowVisible.argtypes = [W.HWND]
    user.GetWindowTextW.argtypes = [W.HWND, W.LPWSTR, C.c_int]
    user.GetClientRect.argtypes = [W.HWND, C.POINTER(RECT)]
    user.ClientToScreen.argtypes = [W.HWND, C.POINTER(POINT)]
    user.SetWindowPos.argtypes = [W.HWND, W.HWND, C.c_int, C.c_int, C.c_int, C.c_int, W.UINT]
    user.ShowWindow.argtypes = [W.HWND, C.c_int]
    user.SetForegroundWindow.argtypes = [W.HWND]
    user.GetForegroundWindow.restype = W.HWND
    user.IsIconic.argtypes = [W.HWND]
    user.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
    kernel.GetProcessTimes.argtypes = [W.HANDLE] + [C.POINTER(W.FILETIME)] * 4

    def cpu_time(proc):
        stamps = [W.FILETIME() for _ in range(4)]
        if not kernel.GetProcessTimes(W.HANDLE(int(proc._handle)), *[C.byref(s) for s in stamps]):
            raise C.WinError(C.get_last_error())
        return sum((s.dwHighDateTime << 32) | s.dwLowDateTime for s in stamps[2:]) / 10_000_000

    def window_for(proc):
        found = []
        @callback_type
        def visit(hwnd, _):
            pid = W.DWORD()
            user.GetWindowThreadProcessId(hwnd, C.byref(pid))
            if pid.value == proc.pid and user.IsWindowVisible(hwnd):
                title = C.create_unicode_buffer(256)
                user.GetWindowTextW(hwnd, title, len(title))
                if title.value:  # exclude the visible winit event-target helper
                    found.append(hwnd)
            return True
        user.EnumWindows(visit, 0)
        return found[0] if found else None

    results = []
    args.output = args.output.resolve()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    for section in sections:
        env = os.environ.copy()
        for name in ("JADIX_RENDER_PROFILE", "JADIX_CPU_PROFILE"):
            env.pop(name, None)
        env["JADIX_GALLERY_SECTION"] = section
        telemetry_path = args.output.with_suffix(f".{section}.frames.json")
        telemetry_path.unlink(missing_ok=True)
        env["JADIX_CPU_TELEMETRY"] = str(telemetry_path)
        if args.profile:
            env["JADIX_RENDER_PROFILE"] = "1"
            env["JADIX_CPU_PROFILE"] = str(args.output.with_suffix(f".{section}.profile.log"))
        with subprocess.Popen([str(args.exe.resolve())], env=env, cwd=Path(__file__).resolve().parents[1]) as proc:
            try:
                deadline = time.perf_counter() + 30
                hwnd = None
                while not hwnd and time.perf_counter() < deadline:
                    if proc.poll() is not None:
                        raise RuntimeError(f"Gallery exited: {proc.returncode}")
                    hwnd = window_for(proc)
                    time.sleep(.05)
                if not hwnd:
                    raise RuntimeError("Gallery did not create a visible window")
                user.ShowWindow(hwnd, 9)  # restore persisted maximization
                user.SetWindowPos(hwnd, None, 40, 40, args.width, args.height, 0x10)
                user.SetForegroundWindow(hwnd)
                time.sleep(.25)
                if user.GetForegroundWindow() != hwnd:
                    message = C.create_string_buffer(64)
                    user.PeekMessageW(message, None, 0, 0, 0)  # create this thread's input queue
                    foreground_thread = user.GetWindowThreadProcessId(user.GetForegroundWindow(), None)
                    current_thread = kernel.GetCurrentThreadId()
                    attached = bool(user.AttachThreadInput(current_thread, foreground_thread, True))
                    try:
                        user.BringWindowToTop(hwnd)
                        user.SetForegroundWindow(hwnd)
                    finally:
                        if attached:
                            user.AttachThreadInput(current_thread, foreground_thread, False)
                    time.sleep(.25)
                if user.GetForegroundWindow() != hwnd:
                    user.SwitchToThisWindow(hwnd, True)
                    time.sleep(.25)
                rect = RECT()
                user.GetClientRect(hwnd, C.byref(rect))
                x, y = int(rect.right * .65), int(rect.bottom * .55)
                user.PostMessageW(hwnd, 0x200, 0, (y << 16) | x)
                point = POINT(x, y)
                user.ClientToScreen(hwnd, C.byref(point))
                position = ((point.y & 0xffff) << 16) | (point.x & 0xffff)
                time.sleep(args.warmup)
                phases = ["idle"] if section == "about" else ["scroll", "settled"]
                for phase in phases:
                    if phase == "settled":
                        time.sleep(3)
                    for round_index in range(args.rounds):
                        start_cpu = cpu_time(proc)
                        start_unix = time.time()
                        start = time.perf_counter()
                        ticks = 0
                        visible = True
                        while (elapsed := time.perf_counter() - start) < args.seconds:
                            if proc.poll() is not None:
                                raise RuntimeError(f"Gallery exited during measurement: {proc.returncode}")
                            visible = visible and bool(user.IsWindowVisible(hwnd)) and not user.IsIconic(hwnd) and user.GetForegroundWindow() == hwnd
                            if phase == "scroll":
                                # 60 ticks/s, reverse each second so small pages never sit at an edge.
                                delta = -40 if int(elapsed) % 2 == 0 else 40
                                if not user.PostMessageW(hwnd, 0x20a, (delta & 0xffff) << 16, position):
                                    raise C.WinError(C.get_last_error())
                                ticks += 1
                            deadline = start + (ticks / 60 if phase == "scroll" else elapsed + .1)
                            time.sleep(max(0, deadline - time.perf_counter()))
                        wall = time.perf_counter() - start
                        used = cpu_time(proc) - start_cpu
                        limit = .5 if phase != "scroll" else (10 if section == "ordinary" else 25)
                        result = dict(section=section, phase=phase, round=round_index + 1,
                                      wall_seconds=wall, cpu_seconds=used, single_core_percent=100 * used / wall,
                                      limit_percent=limit, wheel_ticks=ticks,
                                      start_unix=start_unix,
                                      foreground_visible=visible,
                                      client_size=[rect.right, rect.bottom], pid=proc.pid)
                        result["passed"] = result["single_core_percent"] < limit
                        results.append(result)
                        print(f"{section}/{phase} round {round_index + 1}: {result['single_core_percent']:.2f}% of one CPU (validating telemetry on exit)", flush=True)
            finally:
                user.PostMessageW(hwnd, 0x10, 0, 0) if hwnd else None
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
        if not telemetry_path.exists():
            raise RuntimeError("missing telemetry: build Gallery with --features cpu-bench")
        telemetry = json.loads(telemetry_path.read_text(encoding="utf-8"))
        for row in [r for r in results if r["section"] == section]:
            start = row["start_unix"] - telemetry["start_unix"]
            samples = [s for s in telemetry["redraws"]
                       if start <= s[0] < start + row["wall_seconds"]]
            row["redraws"] = len(samples)
            row["fps"] = len(samples) / row["wall_seconds"]
            if samples and len(samples[0]) == 3:
                # Counters are sampled before painting: the boundary error is at
                # most one frame and cannot inflate the sustained frame rate.
                row["presented_frames"] = samples[-1][2] - samples[0][2]
                row["fps"] = row["presented_frames"] / row["wall_seconds"]
            offsets = [s[1] for s in samples]
            row["scroll_range_px"] = max(offsets) - min(offsets) if offsets else 0
            intervals = sorted(1000 * (b[0] - a[0]) for a, b in zip(samples, samples[1:]))
            row["frame_interval_p95_ms"] = intervals[int((len(intervals) - 1) * .95)] if intervals else None
            row["delivered_wheel_events"] = telemetry["wheel_events"]
            delivery_ok = telemetry["wheel_events"] == sum(r["wheel_ticks"] for r in results if r["section"] == section)
            row["valid"] = row["foreground_visible"] and delivery_ok and (row["phase"] != "scroll" or
                (row["scroll_range_px"] > 20 and row["fps"] >= 55))
            row["passed"] = row["passed"] and row["valid"]
            print(json.dumps(row), flush=True)
    summaries = []
    for section in sections:
        for phase in sorted({r["phase"] for r in results if r["section"] == section}):
            rows = [r for r in results if r["section"] == section and r["phase"] == phase]
            values = [r["single_core_percent"] for r in rows]
            summaries.append(dict(section=section, phase=phase, cpu_min=min(values),
                cpu_median=statistics.median(values), cpu_max=max(values),
                fps_min=min(r["fps"] for r in rows), passed=all(r["passed"] for r in rows)))
    args.output.write_text(json.dumps(dict(executable=str(args.exe.resolve()),
        executable_sha256=hashlib.sha256(args.exe.read_bytes()).hexdigest(),
        platform=platform.platform(), processor=platform.processor(), logical_cpus=os.cpu_count(),
        diagnostic_profiling=args.profile, input_hz=60,
        cpu_definition="100% = one logical CPU, all process threads included",
        width=args.width, height=args.height, summary=summaries, results=results), indent=2), encoding="utf-8")
    if not args.no_gate and not all(row["passed"] for row in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
