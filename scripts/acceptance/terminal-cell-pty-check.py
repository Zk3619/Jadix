#!/usr/bin/env python3
"""Linux private-PTY idle CPU/output and normal raw-mode restoration check.

No emulator, Kitty responses, GPU or SSH. A separate host check covers those.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import termios
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--auto-layout", action="store_true")
    parser.add_argument("--portable", action="store_true")
    args = parser.parse_args()
    args.directory = args.directory.resolve()
    args.directory.mkdir(parents=True, exist_ok=False)
    master, slave = pty.openpty()
    original = termios.tcgetattr(slave)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    os.set_blocking(master, False)

    def session():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

    def drain():
        output = bytearray()
        while select.select([master], [], [], 0)[0]:
            try:
                value = os.read(master, 65536)
            except BlockingIOError:
                break
            if not value:
                break
            output.extend(value)
        return bytes(output)

    def ticks(pid):
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return int(fields[11]) + int(fields[12])

    log = (args.directory / "stderr.log").open("wb")
    command=[str(args.executable.resolve()), "--receipt", str(args.directory / "receipt.json")]
    if args.auto_layout: command.append("--auto-layout")
    app = subprocess.Popen(command,
                           stdin=slave, stdout=slave, stderr=log, preexec_fn=session)
    try:
        initial = bytearray()
        deadline = time.monotonic() + 15
        while (b"Jadix portable app" if args.portable else b"Jadix character surface") not in initial:
            assert app.poll() is None, app.returncode
            assert time.monotonic() < deadline
            select.select([master], [], [], .1)
            initial.extend(drain())
        time.sleep(2)
        initial.extend(drain())
        (args.directory / "startup.bin").write_bytes(initial)
        before = ticks(app.pid)
        start = time.monotonic()
        time.sleep(5)
        elapsed = time.monotonic() - start
        cpu = (ticks(app.pid) - before) / os.sysconf("SC_CLK_TCK")
        idle = drain()
        assert not idle, idle
        os.write(master, b"\x11")
        assert app.wait(timeout=5) == 0
        final = drain()
        (args.directory / "teardown.bin").write_bytes(final)
        receipt = json.loads((args.directory / "receipt.json").read_text())
        assert receipt["frames"] == 1, receipt
        assert receipt["auto_layout"]==(args.auto_layout or args.portable),receipt
        if args.portable: assert receipt["portable"],receipt
        assert receipt["raw_restored"] and termios.tcgetattr(slave) == original
        assert b"\x1b_G" not in initial + final
        result = {"status": "pass", "emulator": False, "kitty_required": False,
                  "idle_seconds": elapsed, "cpu_seconds": cpu, "one_core_percent": cpu / elapsed * 100,
                  "clock_ticks_per_second": os.sysconf("SC_CLK_TCK"), "idle_bytes": len(idle), "receipt": receipt}
        assert result["one_core_percent"] < 2
        (args.directory / "acceptance.json").write_text(json.dumps(result, indent=2))
        print(json.dumps(result))
    finally:
        if app.poll() is None:
            app.kill()
            app.wait()
        termios.tcsetattr(slave, termios.TCSANOW, original)
        os.close(master)
        os.close(slave)
        log.close()


if __name__ == "__main__":
    main()
