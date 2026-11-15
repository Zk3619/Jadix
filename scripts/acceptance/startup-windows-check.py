"""Windows startup integration: instance ownership, DPI, responsiveness and UIA."""
import argparse
from contextlib import contextmanager
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=Path("target/debug/examples/startup.exe"))
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/splash-startup/windows-candidate"))
    parser.add_argument("--scenario", action="append", choices=("single-instance", "uia-component", "uia-window", "background-system", "background-component", "background-window", "background-window-unfocused", "background-system-early"))
    parser.add_argument("--prepare-ms", type=int, default=1800)
    arguments = parser.parse_args()
    arguments.output.mkdir(parents=True, exist_ok=True)
    user = C.WinDLL("user32", use_last_error=True)
    user.SetProcessDPIAware()
    callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    user.EnumWindows.argtypes = [callback, W.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    user.GetWindowTextW.argtypes = [W.HWND, W.LPWSTR, C.c_int]
    user.IsWindowVisible.argtypes = [W.HWND]
    user.IsIconic.argtypes = [W.HWND]
    user.ShowWindow.argtypes = [W.HWND, C.c_int]
    user.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
    user.GetDpiForWindow.argtypes = [W.HWND]
    user.GetClientRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
    user.SendMessageTimeoutW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM, W.UINT, W.UINT, C.POINTER(C.c_size_t)]
    user.SendMessageTimeoutW.restype = C.c_size_t
    user.GetForegroundWindow.restype = W.HWND
    user.SetForegroundWindow.argtypes = [W.HWND]
    user.CreateWindowExW.argtypes = [W.DWORD, W.LPCWSTR, W.LPCWSTR, W.DWORD, C.c_int, C.c_int, C.c_int, C.c_int, W.HWND, W.HMENU, W.HINSTANCE, W.LPVOID]
    user.CreateWindowExW.restype = W.HWND
    user.DestroyWindow.argtypes = [W.HWND]
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

    user.PeekMessageW.argtypes = [C.POINTER(W.MSG), W.HWND, W.UINT, W.UINT, W.UINT]
    user.TranslateMessage.argtypes = [C.POINTER(W.MSG)]
    user.DispatchMessageW.argtypes = [C.POINTER(W.MSG)]
    user.DispatchMessageW.restype = C.c_ssize_t
    results = []

    def pump_messages():
        message = W.MSG()
        while user.PeekMessageW(C.byref(message), None, 0, 0, 1):
            user.TranslateMessage(C.byref(message))
            user.DispatchMessageW(C.byref(message))

    def windows(process):
        found = {}
        @callback
        def visit(handle, _):
            owner = W.DWORD()
            user.GetWindowThreadProcessId(handle, C.byref(owner))
            if owner.value == process.pid:
                title = C.create_unicode_buffer(256)
                user.GetWindowTextW(handle, title, len(title))
                if title.value in ("Jadix Startup Example", "Jadix Startup Splash"):
                    found[title.value] = handle
            return True
        user.EnumWindows(visit, 0)
        return found

    def wait_for(process, log, predicate, timeout=20):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            pump_messages()
            text = log.read_text(encoding="utf-8", errors="replace")
            if predicate(text):
                return text
            assert process.poll() is None, f'owner exited {process.returncode}: {text[-1800:]}'
            time.sleep(0.04)
        raise AssertionError(f"timed out: {log}")

    @contextmanager
    def owner(name, options):
        env = os.environ.copy()
        env.update(RUST_LOG="info", JADIX_WINDOW_STATE_PATH=str((arguments.output / f"{name}.toml").resolve()))
        log = arguments.output / f"{name}.log"
        with log.open("w", encoding="utf-8") as output:
            process = subprocess.Popen([str(arguments.exe.resolve()), *options], stdout=output, stderr=output, env=env)
            try:
                yield process, log, env
            finally:
                for handle in windows(process).values():
                    user.PostMessageW(handle, 0x10, 0, 0)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=5)

    def snapshot(handle, name, invoke=None):
        path = arguments.output / f"{name}-uia.json"
        command = ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(Path(__file__).with_name("startup-uia-check.ps1").resolve()), "-WindowHandle", str(handle), "-Output", str(path.resolve())]
        if invoke:
            command += ["-Invoke", invoke]
        result = subprocess.run(command, capture_output=True, text=True, timeout=15)
        assert result.returncode == 0, result.stdout + result.stderr
        return json.loads(path.read_text(encoding="utf-8-sig"))

    for name in arguments.scenario or ("single-instance", "uia-component", "uia-window", "background-system", "background-component", "background-window", "background-window-unfocused", "background-system-early"):
        if name == "single-instance":
            identity = "jadix-splash-acceptance-" + uuid.uuid4().hex
            options = ["--window", "--prepare-ms", str(arguments.prepare_ms), "--single-instance", identity]
            with owner(name, options) as (process, log, env):
                wait_for(process, log, lambda text: "startup window_frame" in text)
                hosts = windows(process)
                primary = hosts["Jadix Startup Example"]
                splash = hosts["Jadix Startup Splash"]
                assert not user.IsWindowVisible(primary)
                dpi = user.GetDpiForWindow(splash)
                client = W.RECT()
                assert user.GetClientRect(splash, C.byref(client))
                assert (client.right, client.bottom) == (round(480 * dpi / 96), round(320 * dpi / 96))
                assert "phase=Ready" not in log.read_text(encoding="utf-8"), "preparation completed before instance ownership probe; increase --prepare-ms"
                pings = []
                for _ in range(8):
                    response = C.c_size_t()
                    started = time.perf_counter()
                    assert user.SendMessageTimeoutW(splash, 0, 0, 0, 2, 250, C.byref(response)), "startup UI stopped processing native messages"
                    pings.append((time.perf_counter() - started) * 1000)
                second = subprocess.run([str(arguments.exe.resolve()), *options, "--second-launch-marker"], env=env, capture_output=True, timeout=10)
                assert second.returncode == 0
                wait_for(process, log, lambda text: "second instance args=" in text)
                assert not user.IsWindowVisible(primary), "second launch exposed business primary before readiness"
                assert windows(process) == hosts, "second launch duplicated startup hosts"
                wait_for(process, log, lambda text: "startup content_frame" in text)
                assert user.IsWindowVisible(primary)
                user.ShowWindow(primary, 6)
                second = subprocess.run([str(arguments.exe.resolve()), *options, "--restore-marker"], env=env, capture_output=True, timeout=10)
                assert second.returncode == 0
                wait_for(process, log, lambda text: text.count("second instance args=") == 2)
                assert not user.IsIconic(primary), "second launch did not restore the ready primary"
                text = log.read_text(encoding="utf-8")
                assert text.count("business root mounted") == 1
                assert text.count("startup begin epoch=1") == 1
                results.append(dict(scenario=name, passed=True, dpi=dpi, client_size=[client.right, client.bottom], maximum_preparation_ui_ping_ms=max(pings), second_launch_deliveries=2))
            with owner(name + "-restart", options) as (process, log, env):
                wait_for(process, log, lambda text: "startup window_frame" in text)
                results[-1]["ownership_released_and_reacquired"] = True
        elif name.startswith("background-"):
            independent = name.startswith("background-window")
            options = ["--prepare-ms", str(arguments.prepare_ms)]
            if independent:
                options.append("--window")
            elif name.startswith("background-system"):
                options.append("--system")
            with owner(name, options) as (process, log, env):
                marker = "startup system=cpu-buffer" if name.endswith("early") else "startup window_frame" if independent else "startup startup_frame"
                wait_for(process, log, lambda text: marker in text)
                if name.endswith("early"):
                    assert "startup startup_frame" not in log.read_text(encoding="utf-8"), "missed early CPU presentation window"
                hosts = windows(process)
                handle = hosts["Jadix Startup Splash" if independent else "Jadix Startup Example"]
                minimized = not name.endswith("unfocused")
                if minimized:
                    user.ShowWindow(handle, 6)
                    assert user.IsIconic(handle), "could not minimize owned startup host"
                time.sleep(0.1)
                assert "phase=Ready" not in log.read_text(encoding="utf-8"), "preparation finished before background setup"
                background = user.CreateWindowExW(0x80, "STATIC", "Jadix acceptance foreground", 0x80000006, 200, 200, 400, 200, None, None, None, None)
                assert background
                try:
                    user.ShowWindow(background, 5)
                    foreground(background)
                    assert user.GetForegroundWindow() == background, "could not establish controlled foreground"
                    wait_for(process, log, lambda text: "phase=Ready" in text)
                    time.sleep(0.25)
                    active = user.GetForegroundWindow()
                    title = C.create_unicode_buffer(256)
                    user.GetWindowTextW(active, title, len(title))
                    state = dict(foreground=active, foreground_title=title.value, expected_foreground=background, hosts={name: dict(handle=window, visible=bool(user.IsWindowVisible(window)), minimized=bool(user.IsIconic(window))) for name, window in windows(process).items()})
                    (arguments.output / (name + "-background-state.json")).write_text(json.dumps(state, indent=2), encoding="utf-8")
                    assert active == background, f'background readiness stole foreground: {state}'
                    text = log.read_text(encoding="utf-8")
                    assert "startup content_frame" not in text, "minimized startup acknowledged a visible content frame"
                    current = windows(process)
                    restore = current.get("Jadix Startup Splash", current["Jadix Startup Example"])
                    user.ShowWindow(restore, 9)
                    foreground(restore)
                    wait_for(process, log, lambda text: "startup content_frame" in text)
                    assert log.read_text(encoding="utf-8").count("startup begin epoch=1") == 1
                    results.append(dict(scenario=name, passed=True, background_ready_no_activation=True, content_acknowledged_after_restore=True, minimized=minimized))
                finally:
                    user.DestroyWindow(background)
        else:
            independent = name.endswith("window")
            options = ["--prepare-ms", "8000"] + (["--window"] if independent else [])
            with owner(name, options) as (process, log, env):
                wait_for(process, log, lambda text: ("startup window_frame" if independent else "startup startup_frame") in text)
                hosts = windows(process)
                handle = hosts["Jadix Startup Splash" if independent else "Jadix Startup Example"]
                tree = snapshot(handle, name)
                names = [node["name"] for node in tree["nodes"]]
                assert "Jadix" in names and "Cancel" in names, f'startup UIA content missing: {names}'
                assert "Application ready" not in names, "business semantics leaked during preparation"
                assert any(node["role"] == "ControlType.ProgressBar" for node in tree["nodes"]), "startup progress missing from UIA"
                progress = next(node for node in tree["nodes"] if node["role"] == "ControlType.ProgressBar")
                assert progress["range"]["minimum"] == 0 and progress["range"]["maximum"] == 100
                assert 0 <= progress["range"]["value"] <= 100 and progress["range"]["read_only"]
                startup_nodes = len(tree["nodes"])
                snapshot(handle, name + "-cancel", "Cancel")
                wait_for(process, log, lambda text: "phase=Cancelled" in text)
                snapshot(handle, name + "-retry", "Retry")
                wait_for(process, log, lambda text: "startup content_frame epoch=2" in text)
                primary = windows(process)["Jadix Startup Example"]
                tree = snapshot(primary, name + "-ready")
                names = [node["name"] for node in tree["nodes"]]
                assert "Application ready" in names, f'business UIA missing after handoff: {names}'
                assert "Cancel" not in names and "Retry" not in names, "startup semantics survived handoff"
                results.append(dict(scenario=name, passed=True, startup_nodes=startup_nodes, cancel_retry_invoked=True, business_semantics_verified=True))
        (arguments.output / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
        print(json.dumps(results[-1]), flush=True)
    manifest = dict(executable=str(arguments.exe.resolve()), executable_sha256=hashlib.sha256(arguments.exe.read_bytes()).hexdigest(), backend=os.environ.get("JADIX_GPU_BACKEND"), results=results)
    (arguments.output / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
