"""Exercise the real Windows startup example: handoff, cancellation, retry and early close."""
import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import subprocess
import time

from PIL import Image, ImageGrab


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=Path("target/debug/examples/startup.exe"))
    parser.add_argument("--media-exe", type=Path)
    parser.add_argument("--scenario", action="append", default=[])
    parser.add_argument("--require-alpha", action="store_true")
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/splash-startup/native"))
    arguments = parser.parse_args()
    arguments.output.mkdir(parents=True, exist_ok=True)
    user = C.WinDLL("user32", use_last_error=True)
    user.SetProcessDPIAware()
    callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    user.EnumWindows.argtypes = [callback, W.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    user.GetWindowRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
    user.GetClientRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
    user.ClientToScreen.argtypes = [W.HWND, C.POINTER(W.POINT)]
    user.GetWindowTextW.argtypes = [W.HWND, W.LPWSTR, C.c_int]
    user.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
    user.SetForegroundWindow.argtypes = [W.HWND]
    user.GetForegroundWindow.restype = W.HWND
    user.AttachThreadInput.argtypes = [W.DWORD, W.DWORD, W.BOOL]
    user.BringWindowToTop.argtypes = [W.HWND]
    kernel = C.WinDLL("kernel32")
    kernel.GetCurrentThreadId.restype = W.DWORD

    def foreground(handle):
        current = kernel.GetCurrentThreadId()
        active = user.GetWindowThreadProcessId(user.GetForegroundWindow(), None)
        attached = active != current and user.AttachThreadInput(current, active, True)
        try:
            user.BringWindowToTop(handle)
            user.SetForegroundWindow(handle)
        finally:
            if attached:
                user.AttachThreadInput(current, active, False)
        assert user.GetForegroundWindow() == handle, "could not focus owned startup host"

    user.IsWindowVisible.argtypes = [W.HWND]
    user.IsWindowVisible.restype = W.BOOL
    user.GetWindowLongW.argtypes = [W.HWND, C.c_int]
    user.GetWindowLongW.restype = C.c_long
    dwm = C.WinDLL("dwmapi")
    dwm.DwmGetWindowAttribute.argtypes = [W.HWND, W.DWORD, C.c_void_p, W.DWORD]
    dwm.DwmGetWindowAttribute.restype = C.c_long
    user.CreateWindowExW.argtypes = [W.DWORD, W.LPCWSTR, W.LPCWSTR, W.DWORD, C.c_int, C.c_int, C.c_int, C.c_int, W.HWND, W.HMENU, W.HINSTANCE, W.LPVOID]
    user.CreateWindowExW.restype = W.HWND
    user.SetWindowPos.argtypes = [W.HWND, W.HWND, C.c_int, C.c_int, C.c_int, C.c_int, W.UINT]
    user.ShowWindow.argtypes = [W.HWND, C.c_int]
    user.UpdateWindow.argtypes = [W.HWND]
    user.DestroyWindow.argtypes = [W.HWND]
    results = []

    def owned_windows(process):
        windows = []

        @callback
        def visit(handle, _):
            owner = W.DWORD()
            user.GetWindowThreadProcessId(handle, C.byref(owner))
            if owner.value == process.pid:
                title = C.create_unicode_buffer(256)
                user.GetWindowTextW(handle, title, len(title))
                if title.value in ("Jadix Startup Example", "Jadix Startup Media", "Jadix Startup Splash"):
                    windows.append(handle)
            return True

        user.EnumWindows(visit, 0)
        return windows

    def window(process):
        windows = owned_windows(process)
        return next((handle for handle in windows if user.IsWindowVisible(handle)), windows[0] if windows else None)

    def chrome_insets(handle):
        outer = W.RECT()
        client = W.RECT()
        origin = W.POINT()
        assert user.GetWindowRect(handle, C.byref(outer))
        assert user.GetClientRect(handle, C.byref(client))
        assert user.ClientToScreen(handle, C.byref(origin))
        return (origin.x - outer.left, origin.y - outer.top,
                outer.right - origin.x - client.right,
                outer.bottom - origin.y - client.bottom)

    def native_chrome(handle):
        attributes = {}
        for name, attribute in (("ncrendering_enabled", 1), ("corner_preference", 33), ("border_color", 34)):
            value = W.DWORD()
            status = dwm.DwmGetWindowAttribute(handle, attribute, C.byref(value), C.sizeof(value))
            attributes[name] = value.value if status >= 0 else None
        attributes["insets"] = chrome_insets(handle)
        return attributes

    def shadow_pixels(handle, name):
        bounds = W.RECT()
        assert user.GetWindowRect(handle, C.byref(bounds))
        margin = 96
        if bounds.left < margin or bounds.top < margin:
            assert user.SetWindowPos(handle, None, max(margin, bounds.left), max(margin, bounds.top), 0, 0, 0x15)
            assert user.GetWindowRect(handle, C.byref(bounds))
        left, top = bounds.left - margin, bounds.top - margin
        width = bounds.right - bounds.left + margin * 2
        height = bounds.bottom - bounds.top + margin * 2
        background = user.CreateWindowExW(0x08000080, "STATIC", "Jadix chrome test background", 0x80000006, left, top, width, height, None, None, None, None)
        assert background, "could not create controlled shadow background"
        try:
            user.ShowWindow(background, 4)
            assert user.SetWindowPos(background, handle, left, top, width, height, 0x10)
            user.UpdateWindow(background)
            user.SetForegroundWindow(handle)
            time.sleep(0.3)
            dwm.DwmFlush()
            image = ImageGrab.grab(bbox=(left, top, left + width, top + height)).convert("RGB")
            image.save(arguments.output / f"{name}-shadow.png")
            assert all(image.getpixel(point) == (255, 255, 255) for point in ((0, 0), (width - 1, 0), (0, height - 1), (width - 1, height - 1))), "shadow background was obscured"
            strips = [(0, 0, width, margin), (0, height - margin, width, height), (0, margin, margin, height - margin), (width - margin, margin, width, height - margin)]
            return sum(count for strip in strips for count, color in image.crop(strip).getcolors(width * height) if color != (255, 255, 255))
        finally:
            user.DestroyWindow(background)

    def assert_borderless(handle, name):
        chrome = native_chrome(handle)
        chrome["shadow_pixels"] = shadow_pixels(handle, name)
        (arguments.output / f"{name}-chrome.json").write_text(json.dumps(chrome, indent=2), encoding="utf-8")
        capture(handle, f"{name}-chrome.png")
        assert chrome["insets"] == (0, 0, 0, 0), f'borderless non-client insets: {chrome}'
        assert chrome["border_color"] in (None, 0xFFFFFFFE), f'DWM border is not disabled: {chrome}'
        assert chrome["corner_preference"] in (None, 1), f'DWM still forces rounded corners: {chrome}'
        assert chrome["ncrendering_enabled"] in (None, 0), f'DWM non-client rendering remains enabled: {chrome}'
        assert chrome["shadow_pixels"] == 0, f'native shadow remains: {chrome}'
        return chrome

    def capture(handle, name):
        foreground(handle)
        time.sleep(0.25)
        bounds = W.RECT()
        if not user.GetWindowRect(handle, C.byref(bounds)):
            raise RuntimeError("Owned window disappeared before capture")
        image = ImageGrab.grab(bbox=(bounds.left, bounds.top, bounds.right, bounds.bottom))
        image.save(arguments.output / name)
        return image

    def activate(handle):
        foreground(handle)
        for key in (9, 13):
            user.PostMessageW(handle, 0x100, key, 0)
            user.PostMessageW(handle, 0x101, key, 0xC0000001)
            time.sleep(0.15)

    scenarios = [(name, options, arguments.exe) for name, options in [
        ("component", []),
        ("system", ["--system"]),
        ("disabled", ["--no-splash"]),
        ("failure-retry", ["--fail-once"]),
        ("cancel-retry", []),
        ("early-close", ["--system"]),
        ("window-component", ["--window"]),
        ("window-borderless", ["--window", "--borderless"]),
        ("window-borderless-failure-retry", ["--window", "--borderless", "--fail-once"]),
        ("window-disabled", ["--window", "--no-splash"]),
        ("window-failure-retry", ["--window", "--fail-once"]),
        ("window-cancel-retry", ["--window"]),
        ("window-early-close", ["--window"]),
    ]]
    if arguments.media_exe:
        poster = arguments.output / "poster.png"
        animation = arguments.output / "animation.gif"
        frames = [Image.new("RGBA", (128, 128), color) for color in ("#305bd9", "#5cdbc4")]
        frames[0].save(poster)
        frames[0].save(animation, save_all=True, append_images=frames[1:], duration=150, loop=0)
        scenarios.extend((name, options, arguments.media_exe) for name, options in [
            ("media-stream", []),
            ("media-transparent", ["--transparent"]),
            ("media-blur", ["--blur"]),
            ("media-video-surface", ["--video-surface"]),
            ("media-image", ["--image", str(poster.resolve())]),
            ("media-gif", ["--image", str(animation.resolve())]),
            ("media-reduced-motion", ["--reduced-motion"]),
            ("media-gif-reduced-motion", ["--reduced-motion", "--image", str(animation.resolve())]),
            ("media-early-close", []),
            ("window-media-stream", ["--window"]),
            ("window-media-borderless", ["--window", "--borderless"]),
            ("window-media-transparent", ["--window", "--transparent"]),
            ("window-media-blur", ["--window", "--blur"]),
            ("window-media-video-surface", ["--window", "--video-surface"]),
            ("window-media-image", ["--window", "--image", str(poster.resolve())]),
            ("window-media-gif", ["--window", "--image", str(animation.resolve())]),
            ("window-media-reduced-motion", ["--window", "--reduced-motion"]),
            ("window-media-gif-reduced-motion", ["--window", "--reduced-motion", "--image", str(animation.resolve())]),
            ("window-media-early-close", ["--window"]),
        ])
    if arguments.scenario:
        unknown = set(arguments.scenario) - {name for name, _, _ in scenarios}
        if unknown:
            parser.error(f"unknown scenarios: {sorted(unknown)}")
        scenarios = [item for item in scenarios if item[0] in arguments.scenario]
    for name, options, executable in scenarios:
        independent = "--window" in options and "--no-splash" not in options
        media = executable == arguments.media_exe
        log_path = arguments.output / f"{name}.log"
        environment = os.environ.copy()
        environment.update(RUST_LOG="info", JADIX_WINDOW_STATE_PATH=str((arguments.output / f"{name}.toml").resolve()))
        started = time.monotonic()
        with log_path.open("w", encoding="utf-8") as log:
            process = subprocess.Popen([str(executable.resolve()), *options, "--slow", "--exit-ready"], stdout=log, stderr=log, env=environment)

            def wait_for(predicate, timeout=25):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    text = log_path.read_text(encoding="utf-8", errors="replace")
                    if predicate(text):
                        return text
                    if process.poll() is not None:
                        raise AssertionError(f"{name} exited {process.returncode}: {text[-3000:]}")
                    time.sleep(0.04)
                raise AssertionError(f"{name} timed out: {log_path.read_text(encoding='utf-8', errors='replace')[-3000:]}")

            try:
                wait_for(lambda text: "startup window_frame" in text if independent else ("system=cpu-buffer" in text if name == "early-close" else "startup startup_frame" in text))
                handle = window(process)
                assert handle, name
                hosts = owned_windows(process)
                assert len(hosts) == (2 if independent else 1), "unexpected native host count"
                primary = next((candidate for candidate in hosts if candidate != handle), handle)
                if independent:
                    assert not user.IsWindowVisible(primary), "business primary became visible before readiness"
                    assert user.IsWindowVisible(handle), "startup window is not visible"
                    assert user.GetWindowLongW(primary, -16) & 0x00C00000, "splash chrome affected primary decorations"
                    primary_insets = chrome_insets(primary)
                    insets = chrome_insets(handle)
                    borderless = any(option in options for option in ("--borderless", "--transparent", "--blur"))
                    if borderless:
                        assert_borderless(handle, name)
                    else:
                        assert insets[1] > 2, f'decorated splash lost its title bar: {insets}'
                if not independent and name in ("media-transparent", "media-blur"):
                    assert_borderless(handle, name)
                if name == "window-component":
                    pixels = shadow_pixels(handle, name)
                    (arguments.output / "decorated-shadow.json").write_text(json.dumps({"shadow_pixels": pixels}), encoding="utf-8")
                    assert pixels > 0, "decorated control must expose a measurable shadow"
                assert "business root mounted" not in log_path.read_text(encoding="utf-8", errors="replace")
                startup_image = capture(handle, f"{name}-startup.png")
                if name.removeprefix("window-") in ("media-image", "media-gif", "media-gif-reduced-motion"):
                    colors = startup_image.convert("RGB").getcolors(startup_image.width * startup_image.height)
                    expected = {(48, 91, 217), (92, 219, 196)} if name.endswith("media-gif") else {(48, 91, 217)}
                    deadline = time.monotonic() + 3
                    while sum(count for count, color in colors if color in expected) <= 5000 and time.monotonic() < deadline:
                        assert "startup content_frame" not in log_path.read_text(encoding="utf-8", errors="replace"), "media was not visible during preparation"
                        startup_image = capture(handle, f"{name}-startup.png")
                        colors = startup_image.convert("RGB").getcolors(startup_image.width * startup_image.height)
                    assert sum(count for count, color in colors if color in expected) > 5000, "startup media pixels missing"
                    if name == "window-media-gif":
                        first = max((count, color) for count, color in colors if color in expected)[1]
                        changed = False
                        for _ in range(5):
                            time.sleep(0.11)
                            animated = capture(handle, f"{name}-animated.png")
                            colors = animated.convert("RGB").getcolors(animated.width * animated.height)
                            current = max((count, color) for count, color in colors if color in expected)[1]
                            if current != first:
                                changed = True
                                break
                        assert changed, "independent GIF did not advance its displayed pixels"
                if name.endswith("early-close"):
                    user.PostMessageW(handle, 0x10, 0, 0)
                    process.wait(timeout=10)
                    text = log_path.read_text(encoding="utf-8", errors="replace")
                    assert "business root mounted" not in text, text
                else:
                    if name.endswith("failure-retry"):
                        wait_for(lambda text: "phase=Failed" in text)
                        time.sleep(0.2)
                        capture(handle, f"{name}-failed.png")
                        activate(handle)
                        wait_for(lambda text: text.count("startup begin") == 2)
                    if name.endswith("cancel-retry"):
                        activate(handle)
                        wait_for(lambda text: "phase=Cancelled" in text)
                        time.sleep(1.8)
                        text = log_path.read_text(encoding="utf-8", errors="replace")
                        assert "business root mounted" not in text, text
                        capture(handle, f"{name}-cancelled.png")
                        activate(handle)
                        wait_for(lambda text: text.count("startup begin") == 2)
                    wait_for(lambda text: "startup content_frame" in text)
                    if independent:
                        text = wait_for(lambda text: "reason=content_presented" in text)
                        assert text.index("startup content_frame") < text.index("startup window_closed"), text
                        assert text.count("startup window_opened") == 1, "retry duplicated the startup window"
                        wait_for(lambda _: owned_windows(process) == [primary], timeout=3)
                    if independent:
                        assert chrome_insets(primary) == primary_insets, "splash decorations leaked to the business window"
                    assert owned_windows(process) == [primary], "handoff replaced the primary or leaked the splash"
                    assert user.IsWindowVisible(primary), "primary stayed hidden after handoff"
                    capture(primary, f"{name}-content.png")
                    process.wait(timeout=10)
                    text = log_path.read_text(encoding="utf-8", errors="replace")
                    assert text.count("business root mounted") == 1, text
                    assert ("system=cpu-buffer" in text) == (name == "system"), text
                    if name.endswith("disabled"):
                        assert "enabled=false" in text, text
                        assert "startup window_opened" not in text, text
                    if media:
                        assert text.index("source stopped") < text.index("business root mounted"), text
                    if name.removeprefix("window-") == "media-reduced-motion":
                        assert "produced=1" in text, text
                    if name == "media-gif":
                        assert "gif=true" in text, "animated GIF did not activate its frame pump"
                    if name == "media-gif-reduced-motion":
                        assert "gif=false" in text and "gif=true" not in text, text
                assert process.returncode == 0, (name, process.returncode)
                assert "panicked at" not in text, text
                result = {"scenario": name, "passed": True, "seconds": round(time.monotonic() - started, 3)}
                if independent:
                    result["borderless"] = borderless
                    result["startup_chrome_insets"] = insets
                    result["primary_chrome_preserved"] = True
                    result["startup_host_count"] = len(hosts)
                    result["primary_initially_hidden"] = True
                    result["handoff_verified"] = not name.endswith("early-close")
                if name.removeprefix("window-") in ("media-transparent", "media-blur"):
                    modes = [
                        line.split("alpha_mode: ", 1)[1].strip()
                        for line in text.splitlines()
                        if "surface present_mode:" in line and "alpha_mode: " in line
                    ]
                    if independent:
                        modes = [line.split("window_surface alpha_mode=", 1)[1].strip() for line in text.splitlines() if "window_surface alpha_mode=" in line]
                    alpha = bool(modes) and modes[-1] in ("PreMultiplied", "PostMultiplied")
                    result["surface_alpha_mode"] = modes[-1] if modes else "Unknown"
                    result["alpha_composited"] = alpha
                    if arguments.require_alpha:
                        assert alpha, "backend fell back to an opaque surface"
                results.append(result)
            finally:
                if process.poll() is None:
                    user.PostMessageW(window(process) or 0, 0x10, 0, 0)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.terminate()
                        process.wait(timeout=5)
    (arguments.output / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
