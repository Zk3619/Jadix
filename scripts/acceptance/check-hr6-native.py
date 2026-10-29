import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
import socket
import subprocess
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--modes", nargs="+", default=["unregistered_builder_inspector", "registered_builder_sources_watcher", "static_ui_sources_watcher"])
    parser.add_argument("--edits", type=int, default=31)
    arguments = parser.parse_args()
    executable = arguments.executable.resolve(strict=True)
    cli = arguments.cli.resolve(strict=True)
    evidence = arguments.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    records = []
    executable_sha256 = hashlib.sha256(executable.read_bytes()).hexdigest()
    cli_sha256 = hashlib.sha256(cli.read_bytes()).hexdigest()
    for mode in arguments.modes:
        directory = evidence / mode
        directory.mkdir()
        root = directory / "fixture"
        root.mkdir()
        (root / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
        slot = " {stateful_scene()}" if "stateful" in mode else ""
        (root / "source.rs").write_text('fn demo() { ui! { <Box w=96.0 h=64.0 bg="#ff0000">"HR6 A"' + slot + '</Box> } }\n', encoding="utf-8")
        if mode.startswith("recompiled_"):
            source = root / "source.rs"
            source.write_text(source.read_text().replace("96.0", "128.0").replace("HR6 A", "HR6 B30"), encoding="utf-8")
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            address = "127.0.0.1:" + str(listener.getsockname()[1])
        environment = dict(os.environ, JADIX_HR6_EVIDENCE=str(directory), JADIX_DEVTOOLS_ROOT=str(root),
                           JADIX_HOT_RELOAD_ADDR=address, JADIX_HR6_CLI=str(cli), NO_COLOR="1", JADIX_HR6_EDITS=str(arguments.edits))
        selector = "app::native_acceptance_tests::native_" + mode
        command = [str(executable), "--ignored", "--exact", selector, "--test-threads=1", "--nocapture"]
        log = directory / "native.log"
        timed_out = False
        with log.open("wb") as output:
            process = subprocess.Popen(command, env=environment, stdout=output, stderr=subprocess.STDOUT)
            idle_cpu_start = None
            def cpu_seconds():
                created, ended, kernel, user = (wintypes.FILETIME() for _ in range(4))
                function = ctypes.WinDLL("kernel32", use_last_error=True).GetProcessTimes
                function.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
                if not function(int(process._handle), ctypes.byref(created), ctypes.byref(ended), ctypes.byref(kernel), ctypes.byref(user)):
                    raise ctypes.WinError(ctypes.get_last_error())
                return ((kernel.dwHighDateTime << 32) + kernel.dwLowDateTime + (user.dwHighDateTime << 32) + user.dwLowDateTime) / 10_000_000
            try:
                if mode == "idle_static":
                    deadline = time.monotonic() + 60
                    while process.poll() is None:
                        if time.monotonic() >= deadline:
                            raise subprocess.TimeoutExpired(command, 60)
                        if idle_cpu_start is None and (directory / "idle-start.json").exists():
                            idle_cpu_start = cpu_seconds()
                        try:
                            process.wait(timeout=0.05)
                        except subprocess.TimeoutExpired:
                            pass
                    exit_code = process.returncode
                else:
                    deadline = time.monotonic() + 120 + arguments.edits * 2
                    while True:
                        try:
                            exit_code = process.wait(timeout=min(10,max(0.01,deadline-time.monotonic())))
                            break
                        except subprocess.TimeoutExpired:
                            with log.open("rb") as pending:
                                pending.seek(max(0,log.stat().st_size-8192))
                                panicked = b"panicked at" in pending.read()
                            if panicked:
                                subprocess.run(["taskkill","/PID",str(process.pid),"/T","/F"],stdout=output,stderr=subprocess.STDOUT)
                                process.wait()
                                exit_code=101
                                break
                            if time.monotonic() >= deadline:
                                raise subprocess.TimeoutExpired(command,120 + arguments.edits * 2)
            except subprocess.TimeoutExpired:
                timed_out = True
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], stdout=output, stderr=subprocess.STDOUT)
                process.wait()
                exit_code = 124
        record = {"mode":mode,"arguments":command,"exit_code":exit_code,"timed_out":timed_out,"log_sha256":hashlib.sha256(log.read_bytes()).hexdigest(),
                  "executable_sha256":executable_sha256,
                  "cli_sha256":cli_sha256}
        if idle_cpu_start is not None:
            record["idle_process_cpu_seconds"] = cpu_seconds() - idle_cpu_start
        records.append(record)
        (evidence / "results.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
        print(f"{mode}: exit={exit_code}", flush=True)
        if exit_code or "1 passed; 0 failed" not in log.read_text(encoding="utf-8"):
            raise SystemExit(1)


if __name__ == "__main__":
    main()
