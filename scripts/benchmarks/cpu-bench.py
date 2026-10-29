"""Windows native Gallery CPU gate: 100% = one fully occupied logical CPU.

python scripts/benchmarks/cpu-bench.py --exe target/release/gallery.exe --output .tmp/cpu-before.json
Measures the actual GUI process, with native wheel messages at 60 Hz.
"""
import argparse
from contextlib import nullcontext
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


def calibration_content_visible(image):
    """Reject blank clients; window borders and counters cannot prove rendering."""
    image = image.convert("RGB").crop((32, 32, image.width - 32, image.height - 32))
    count = image.width * image.height
    colors = image.getcolors(count)
    return max(n for n, _ in colors) < count * .995 and sum(
        n for n, rgb in colors if max(rgb) >= 96) > count * .001


def calibration_palette_matches(image):
    """The shared fixture's background is sRGB #18181b, not linear-darkened."""
    image = image.convert("RGB").crop((32, 32, image.width - 32, image.height - 32))
    count = image.width * image.height
    return sum(n for n, rgb in image.getcolors(count)
        if all(abs(a - b) <= 1 for a, b in zip(rgb, (24, 24, 27)))) > count * .30


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=Path("target/release/gallery.exe"))
    parser.add_argument("--output", type=Path, default=Path(".tmp/cpu-bench.json"))
    parser.add_argument("--seconds", type=float, default=10)
    parser.add_argument("--warmup", type=float, default=5)
    parser.add_argument("--scroll-warmup", type=float, default=0,
                        help="unmeasured wheel input before timing; 60Hz, reverses each second")
    parser.add_argument("--verify-round-trip", action="store_true",
                        help="calibration: reject changed pixels after even-second scroll warmup")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--sections", default="about,ordinary,forms,layout")
    parser.add_argument("--no-gate", action="store_true")
    parser.add_argument("--profile", action="store_true", help="also write stage timings (separate diagnostic run)")
    parser.add_argument("--thread-cpu", action="store_true", help="separate UI-thread and remaining process CPU (diagnostic run)")
    parser.add_argument("--cpu-timeline", action="store_true", help="record half-second process/UI CPU intervals (diagnostic run)")
    parser.add_argument("--frame-budget-ms", type=float, help="capture complete UI event-loop CPU frame traces (diagnostic run)")
    parser.add_argument("--calibration", action="store_true", help="fixed-content cross-framework fixtures; accepts explicit paint counters, never labels them presentations")
    parser.add_argument("--client-size", action="store_true", help="width/height specify physical client pixels instead of the outer window")
    parser.add_argument("--screenshots", action="store_true", help="save foreground client screenshots outside CPU measurement intervals (requires Pillow)")
    parser.add_argument("--scroll-only", action="store_true", help="measure only scrolling for paired runtime regression checks; no idle acceptance")
    parser.add_argument("--cpu-affinity", type=lambda value: int(value, 0), help="optional Windows process CPU affinity mask for controlled diagnostics; default keeps OS scheduling")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("native Windows benchmark requires Windows")
    if not math.isfinite(args.seconds) or not math.isfinite(args.warmup) or args.seconds < 1 or args.rounds < 1 or args.warmup < 1:
        parser.error("seconds, rounds and warmup must be positive")
    if not math.isfinite(args.scroll_warmup) or args.scroll_warmup < 0:
        parser.error("scroll-warmup must be finite and nonnegative")
    if args.verify_round_trip and (not args.calibration or args.scroll_warmup < 2 or args.scroll_warmup % 2):
        parser.error("verify-round-trip requires calibration and positive even-second scroll warmup")
    if args.width < 640 or args.height < 480:
        parser.error("window must be at least 640 x 480")
    if args.frame_budget_ms is not None and (not math.isfinite(args.frame_budget_ms) or args.frame_budget_ms <= 0):
        parser.error("frame-budget-ms must be finite and positive")
    if args.cpu_affinity is not None and not 0 < args.cpu_affinity < (1 << (8 * C.sizeof(C.c_size_t))):
        parser.error("cpu-affinity must be a positive pointer-sized mask")
    sections = args.sections.split(",")
    allowed = {"ordinary", "dense", "nested"} if args.calibration else {"about", "ordinary", "forms", "layout"}
    if not sections or len(set(sections)) != len(sections) or any(s not in allowed for s in sections):
        parser.error("sections must contain about, ordinary, forms or layout")
    user = C.WinDLL("user32", use_last_error=True)
    # Make calibration client dimensions physical on mixed-DPI desktops.
    if args.calibration:
        user.SetProcessDpiAwarenessContext.argtypes = [W.HANDLE]
        user.SetProcessDpiAwarenessContext(W.HANDLE(-4))
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
    user.GetWindowRect.argtypes = [W.HWND, C.POINTER(RECT)]
    user.GetDpiForWindow.argtypes = [W.HWND]
    user.ClientToScreen.argtypes = [W.HWND, C.POINTER(POINT)]
    user.SetWindowPos.argtypes = [W.HWND, W.HWND, C.c_int, C.c_int, C.c_int, C.c_int, W.UINT]
    user.ShowWindow.argtypes = [W.HWND, C.c_int]
    user.SetForegroundWindow.argtypes = [W.HWND]
    user.GetForegroundWindow.restype = W.HWND
    user.IsIconic.argtypes = [W.HWND]
    user.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
    kernel.SetProcessAffinityMask.argtypes = [W.HANDLE, C.c_size_t]
    kernel.SetProcessAffinityMask.restype = W.BOOL
    kernel.GetProcessAffinityMask.argtypes = [W.HANDLE, C.POINTER(C.c_size_t), C.POINTER(C.c_size_t)]
    kernel.GetProcessAffinityMask.restype = W.BOOL
    kernel.GetProcessTimes.argtypes = [W.HANDLE] + [C.POINTER(W.FILETIME)] * 4
    kernel.GetThreadTimes.argtypes = [W.HANDLE] + [C.POINTER(W.FILETIME)] * 4
    kernel.OpenThread.argtypes = [W.DWORD, W.BOOL, W.DWORD]
    kernel.OpenThread.restype = W.HANDLE
    kernel.CloseHandle.argtypes = [W.HANDLE]

    def cpu_time(handle, query=kernel.GetProcessTimes):
        stamps = [W.FILETIME() for _ in range(4)]
        if not query(handle, *[C.byref(s) for s in stamps]):
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
        visual_checks = {}
        palette_checks = {}
        env = os.environ.copy()
        for name in ("JADIX_RENDER_PROFILE", "JADIX_CPU_PROFILE", "JADIX_FRAME_BUDGET_MS", "JADIX_FRAME_BUDGET_TRACE"):
            env.pop(name, None)
        env["JADIX_GALLERY_SECTION"] = section
        telemetry_path = args.output.with_suffix(f".{section}.frames.json")
        telemetry_path.unlink(missing_ok=True)
        env["JADIX_CPU_TELEMETRY"] = str(telemetry_path)
        trace_path = args.output.with_suffix(f".{section}.budget.log")
        if args.frame_budget_ms is not None:
            env["JADIX_FRAME_BUDGET_MS"] = str(args.frame_budget_ms)
            env["JADIX_FRAME_BUDGET_TRACE"] = "1"
        if args.profile:
            env["JADIX_RENDER_PROFILE"] = "1"
        diagnostic_path = (trace_path if args.frame_budget_ms is not None
                           else args.output.with_suffix(f".{section}.profile.log"))
        with (diagnostic_path.open("w", encoding="utf-8") if args.frame_budget_ms is not None or args.profile else nullcontext(None)) as frame_log, subprocess.Popen([str(args.exe.resolve())], env=env, stderr=frame_log, cwd=Path(__file__).resolve().parents[2]) as proc:
            ui_handle = None
            try:
                affinity_mask = None
                if args.cpu_affinity is not None:
                    process_handle = W.HANDLE(int(proc._handle))
                    if not kernel.SetProcessAffinityMask(process_handle, args.cpu_affinity):
                        raise C.WinError(C.get_last_error())
                    actual, available = C.c_size_t(), C.c_size_t()
                    if not kernel.GetProcessAffinityMask(process_handle, C.byref(actual), C.byref(available)):
                        raise C.WinError(C.get_last_error())
                    if actual.value != args.cpu_affinity:
                        raise RuntimeError("child CPU affinity did not match the requested diagnostic mask")
                    affinity_mask = actual.value
                deadline = time.perf_counter() + 30
                hwnd = None
                while not hwnd and time.perf_counter() < deadline:
                    if proc.poll() is not None:
                        raise RuntimeError(f"Gallery exited: {proc.returncode}")
                    hwnd = window_for(proc)
                    time.sleep(.05)
                if not hwnd:
                    raise RuntimeError("Gallery did not create a visible window")
                ui_thread_id = user.GetWindowThreadProcessId(hwnd, None)
                if args.thread_cpu or args.cpu_timeline:
                    ui_handle = kernel.OpenThread(0x0800, False, ui_thread_id)  # THREAD_QUERY_LIMITED_INFORMATION
                    if not ui_handle:
                        raise C.WinError(C.get_last_error())
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
                if args.client_size:
                    outer = RECT()
                    user.GetWindowRect(hwnd, C.byref(outer))
                    user.SetWindowPos(hwnd, None, 40, 40,
                        args.width + outer.right - outer.left - rect.right,
                        args.height + outer.bottom - outer.top - rect.bottom, 0x10)
                    time.sleep(.25)
                    user.GetClientRect(hwnd, C.byref(rect))
                    if [rect.right, rect.bottom] != [args.width, args.height]:
                        raise RuntimeError("client size does not match the requested calibration size")
                x, y = int(rect.right * .65), int(rect.bottom * .55)
                user.PostMessageW(hwnd, 0x200, 0, (y << 16) | x)
                point = POINT(x, y)
                user.ClientToScreen(hwnd, C.byref(point))
                position = ((point.y & 0xffff) << 16) | (point.x & 0xffff)
                time.sleep(args.warmup)
                def screenshot(label):
                    if args.screenshots or args.verify_round_trip:
                        from PIL import ImageGrab
                        origin = POINT(0, 0)
                        user.ClientToScreen(hwnd, C.byref(origin))
                        image = ImageGrab.grab(bbox=(origin.x, origin.y, origin.x + rect.right,
                            origin.y + rect.bottom), all_screens=True)
                        image.save(args.output.with_suffix(f".{section}.{label}.png"))
                        if args.calibration:
                            visual_checks[label] = calibration_content_visible(image)
                            palette_checks[label] = calibration_palette_matches(image)
                        return image
                before_warmup = screenshot("before-warmup") if args.verify_round_trip else None
                warmup_ticks = 0
                if args.scroll_warmup > 0 and section != 'about':
                    warmup_start = time.perf_counter()
                    for tick in range(int(args.scroll_warmup * 60)):
                        if proc.poll() is not None:
                            raise RuntimeError("Gallery exited during scroll warmup")
                        delta = -40 if (tick // 60) % 2 == 0 else 40
                        if not user.PostMessageW(hwnd, 0x20a, (delta & 0xffff) << 16, position):
                            raise C.WinError(C.get_last_error())
                        warmup_ticks += 1
                        time.sleep(max(0, warmup_start + warmup_ticks / 60 - time.perf_counter()))
                    time.sleep(1)  # drain work caused by the last warmup input
                initial = screenshot("initial")
                round_trip_changed_pixels = None
                if before_warmup is not None:
                    from PIL import ImageChops
                    # Ignore the overlay scrollbar and OS pointer at the client edge.
                    crop = (32, 0, rect.right - 32, rect.bottom)
                    delta = ImageChops.difference(before_warmup.convert('RGB').crop(crop),
                                                 initial.convert('RGB').crop(crop))
                    round_trip_changed_pixels = sum(max(pixel) > 6 for pixel in delta.get_flattened_data())
                    if round_trip_changed_pixels:
                        raise RuntimeError(f"round-trip pixels changed: {round_trip_changed_pixels}; inspect before-warmup and initial PNGs")
                phases = ["idle"] if section == "about" else (["scroll"] if args.scroll_only else ["scroll", "settled"])
                for phase in phases:
                    if phase == "settled":
                        time.sleep(3)
                    for round_index in range(args.rounds):
                        start_cpu = cpu_time(W.HANDLE(int(proc._handle)))
                        start_ui = cpu_time(ui_handle, kernel.GetThreadTimes) if ui_handle else None
                        start_unix = time.time()
                        start = time.perf_counter()
                        ticks = 0
                        visible = True
                        cpu_intervals = []
                        interval_start, interval_cpu, interval_ui = start, start_cpu, start_ui
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
                            if args.cpu_timeline and time.perf_counter() - interval_start >= .5:
                                now = time.perf_counter()
                                current_cpu = cpu_time(W.HANDLE(int(proc._handle)))
                                current_ui = cpu_time(ui_handle, kernel.GetThreadTimes)
                                cpu_intervals.append(dict(start_seconds=interval_start - start,
                                    wall_seconds=now - interval_start,
                                    cpu_seconds=current_cpu - interval_cpu,
                                    ui_cpu_seconds=current_ui - interval_ui))
                                interval_start, interval_cpu, interval_ui = now, current_cpu, current_ui
                        wall = time.perf_counter() - start
                        used = cpu_time(W.HANDLE(int(proc._handle))) - start_cpu
                        ui_used = cpu_time(ui_handle, kernel.GetThreadTimes) - start_ui if ui_handle else None
                        limit = .5 if phase != "scroll" else (10 if section == "ordinary" else 25)
                        result = dict(section=section, phase=phase, round=round_index + 1,
                                      process_cpu_at_start_seconds=start_cpu,
                                      wall_seconds=wall, cpu_seconds=used, single_core_percent=100 * used / wall,
                                      limit_percent=limit, wheel_ticks=ticks,
                                      start_unix=start_unix,
                                      foreground_visible=visible,
                                      dpi=user.GetDpiForWindow(hwnd),
                                      client_size=[rect.right, rect.bottom], pid=proc.pid)
                        if affinity_mask is not None:
                            result["cpu_affinity_mask"] = affinity_mask
                        result["passed"] = result["single_core_percent"] < limit
                        if args.cpu_timeline:
                            cpu_intervals.append(dict(start_seconds=interval_start - start,
                                wall_seconds=start + wall - interval_start,
                                cpu_seconds=start_cpu + used - interval_cpu,
                                ui_cpu_seconds=start_ui + ui_used - interval_ui))
                            result["cpu_intervals"] = cpu_intervals
                        if ui_used is not None:
                            result.update(ui_thread_id=ui_thread_id, ui_cpu_seconds=ui_used,
                                          ui_single_core_percent=100 * ui_used / wall,
                                          remaining_cpu_seconds=used - ui_used,
                                          remaining_single_core_percent=100 * (used - ui_used) / wall)
                        results.append(result)
                        print(f"{section}/{phase} round {round_index + 1}: {result['single_core_percent']:.2f}% of one CPU (validating telemetry on exit)", flush=True)
                    screenshot(phase)
            finally:
                if ui_handle:
                    kernel.CloseHandle(ui_handle)
                user.PostMessageW(hwnd, 0x10, 0, 0) if hwnd else None
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
        if not telemetry_path.exists():
            raise RuntimeError("missing telemetry: build Gallery with --features cpu-bench")
        telemetry = json.loads(telemetry_path.read_text(encoding="utf-8"))
        counter_kind = telemetry.get("frame_counter_kind", "presented_frames")
        if counter_kind not in {"paint_callbacks", "presented_frames"}:
            raise RuntimeError(f"unknown frame counter: {counter_kind}")
        if counter_kind == "paint_callbacks" and not args.calibration:
            raise RuntimeError("paint counters cannot satisfy the production presentation gate")
        for row in [r for r in results if r["section"] == section]:
            row["renderer"] = telemetry.get("renderer")
            row["visual_nonblank"] = all(visual_checks.values()) if visual_checks else None
            row["visual_palette"] = all(palette_checks.values()) if palette_checks else None
            start = row["start_unix"] - telemetry["start_unix"]
            samples = [s for s in telemetry["redraws"]
                       if start <= s[0] < start + row["wall_seconds"]]
            row["redraws"] = len(samples)
            row["fps"] = len(samples) / row["wall_seconds"]
            if samples and len(samples[0]) == 3:
                # Counters are sampled before painting: the boundary error is at
                # most one frame and cannot inflate the sustained frame rate.
                count = samples[-1][2] - samples[0][2]
                row["frame_counter_kind"] = counter_kind
                row["presented_frames" if counter_kind == "presented_frames" else "painted_frames"] = count
                row["fps"] = count / row["wall_seconds"]
            frames = row.get("presented_frames", 0)
            row["cpu_ms_per_present"] = 1000 * row["cpu_seconds"] / frames if frames else None
            row["cpu_ms_per_observed_frame"] = 1000 * row["cpu_seconds"] / row.get("painted_frames", frames) if row.get("painted_frames", frames) else None
            offsets = [s[1] for s in samples]
            row["scroll_range_px"] = max(offsets) - min(offsets) if offsets else 0
            intervals = sorted(1000 * (b[0] - a[0]) for a, b in zip(samples, samples[1:]))
            row["frame_interval_p95_ms"] = intervals[int((len(intervals) - 1) * .95)] if intervals else None
            row["delivered_wheel_events"] = telemetry["wheel_events"]
            row["warmup_wheel_events"] = warmup_ticks
            row["round_trip_changed_pixels"] = round_trip_changed_pixels
            delivery_ok = telemetry["wheel_events"] == warmup_ticks + sum(r["wheel_ticks"] for r in results if r["section"] == section)
            row["valid"] = row["visual_nonblank"] is not False and row["visual_palette"] is not False and row["foreground_visible"] and delivery_ok and (row["phase"] != "scroll" or
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
                cpu_weighted_percent=100 * sum(r["cpu_seconds"] for r in rows) / sum(r["wall_seconds"] for r in rows),
                cpu_ms_per_present_median=statistics.median(r["cpu_ms_per_present"] for r in rows)
                    if all(r["cpu_ms_per_present"] is not None for r in rows) else None,
                fps_min=min(r["fps"] for r in rows), passed=all(r["passed"] for r in rows)))
    args.output.write_text(json.dumps(dict(executable=str(args.exe.resolve()),
        executable_sha256=hashlib.sha256(args.exe.read_bytes()).hexdigest(),
        platform=platform.platform(), processor=platform.processor(), logical_cpus=os.cpu_count(),
        diagnostic_profiling=args.profile or args.thread_cpu or args.cpu_timeline or args.frame_budget_ms is not None, input_hz=60,
        thread_cpu=args.thread_cpu or args.cpu_timeline,
        cpu_timeline=args.cpu_timeline,
        frame_budget_ms=args.frame_budget_ms,
        calibration=args.calibration, physical_client_size=args.client_size, scroll_only=args.scroll_only,
        scroll_warmup_seconds=args.scroll_warmup,
        frame_budget_logs=([str(args.output.with_suffix(f".{section}.budget.log")) for section in sections]
            if args.frame_budget_ms is not None else []),
        profile_logs=([str(args.output.with_suffix(f".{section}.budget.log" if args.frame_budget_ms is not None
            else f".{section}.profile.log")) for section in sections] if args.profile else []),
        cpu_definition="100% = one logical CPU, all process threads included",
        width=args.width, height=args.height, summary=summaries, results=results), indent=2), encoding="utf-8")
    if not args.no_gate and not all(row["passed"] for row in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
