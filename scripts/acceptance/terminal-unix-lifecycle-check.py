#!/usr/bin/env python3
"""Private Linux PTYs: signals, full output queues, timeout, stdin and hangup.

Only fixture-owned descriptors/processes are touched. No user terminal or
settings are changed. Normal idle/emulator/SSH checks live in separate tools.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import termios
import time


class Fixture:
    def __init__(self, executable, folder, controlling, *, timeout_ms=10000, stdin=True):
        self.folder = folder
        folder.mkdir()
        self.master, self.slave = pty.openpty()
        self.probe = os.open(os.ttyname(self.slave), os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        self.original = termios.tcgetattr(self.probe)
        self.flags = fcntl.fcntl(self.slave, fcntl.F_GETFL)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        os.set_blocking(self.master, False)
        self.log = (folder / "stderr.log").open("wb")

        def child_session():
            os.setsid()
            if controlling:
                fcntl.ioctl(self.slave, termios.TIOCSCTTY, 0)

        self.app = subprocess.Popen([str(executable), "--receipt", str(folder / "receipt.json"),
                                     "--output-timeout-ms", str(timeout_ms)],
                                    stdin=self.slave if stdin else subprocess.DEVNULL,
                                    stdout=self.slave, stderr=self.log, preexec_fn=child_session)

    def drain(self):
        result = bytearray()
        while self.master >= 0 and select.select([self.master], [], [], 0)[0]:
            try:
                value = os.read(self.master, 65536)
            except (BlockingIOError, OSError):
                break
            if not value:
                break
            result.extend(value)
        return bytes(result)

    def ready(self):
        output = bytearray()
        end = time.monotonic() + 10
        while b"Jadix character surface" not in output:
            assert self.app.poll() is None, (self.app.returncode, self.folder)
            assert time.monotonic() < end, self.folder
            select.select([self.master], [], [], .05)
            output.extend(self.drain())
        (self.folder / "startup.bin").write_bytes(output)
        assert fcntl.fcntl(self.slave, fcntl.F_GETFL) & os.O_NONBLOCK

    def fill(self):
        self.drain()
        end = time.monotonic() + 2
        for width in (8192, 1024, 64, 1):
            while True:
                assert time.monotonic() < end, "fixture queue did not fill"
                try:
                    os.write(self.probe, b"x" * width)
                except BlockingIOError:
                    break

    def pressured_redraw(self):
        self.fill()
        # Force a full, larger character frame as well as interaction frames.
        # A tiny cursor change can fit space the PTY line discipline frees
        # asynchronously after EAGAIN; it does not prove a blocked writer.
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 60, 200, 0, 0))
        os.write(self.master, b"\t\r" * 12)

    def wait(self, *, expected=None, attrs=True):
        start = time.monotonic()
        code = self.app.wait(timeout=3)
        elapsed = time.monotonic() - start
        if expected is not None:
            assert code == expected, (self.folder, code)
        if attrs:
            assert termios.tcgetattr(self.probe) == self.original, self.folder
        assert fcntl.fcntl(self.slave, fcntl.F_GETFL) == self.flags, self.folder
        receipt = self.folder / "receipt.json"
        result = {"exit_code": code, "exit_wait_s": elapsed, "flags_restored": True,
                  "termios_restored": True if attrs else "unobservable after PTY hangup",
                  "receipt": json.loads(receipt.read_text()) if receipt.exists() else None}
        (self.folder / "result.json").write_text(json.dumps(result, indent=2))
        return result

    def close(self):
        if self.app.poll() is None:
            os.killpg(self.app.pid, signal.SIGKILL)
            self.app.wait()
        try:
            termios.tcsetattr(self.probe, termios.TCSANOW, self.original)
        except (OSError, termios.error):
            pass
        fcntl.fcntl(self.slave, fcntl.F_SETFL, self.flags)
        for descriptor in (self.master, self.slave, self.probe):
            if descriptor >= 0:
                os.close(descriptor)
        self.log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--session-contract", type=Path)
    args = parser.parse_args()
    args.executable = args.executable.resolve()
    args.directory = args.directory.resolve()
    args.directory.mkdir(parents=True, exist_ok=False)
    results = []

    def scenario(name, controlling, operation, **options):
        fixture = Fixture(args.executable, args.directory / name, controlling, **options)
        try:
            result = operation(fixture)
            result.update(name=name, status="pass", controlling=controlling)
            results.append(result)
            print(name, "pass", flush=True)
        finally:
            fixture.close()
            (args.directory / "acceptance.json").write_text(json.dumps({"scenarios": results}, indent=2))

    for controlling in (False, True):
        suffix = "controlling" if controlling else "noncontrolling"
        for number in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
            def terminate(fixture, number=number):
                fixture.ready()
                fixture.app.send_signal(number)
                result = fixture.wait(expected=0)
                assert result["receipt"]["signal"] == number, result
                return result
            scenario(f"{suffix}-{number.name}", controlling, terminate)

        def blocked(fixture):
            fixture.ready()
            fixture.pressured_redraw()
            time.sleep(.15)
            assert fixture.app.poll() is None, "output pressure must wait"
            fixture.app.send_signal(signal.SIGTERM)
            result = fixture.wait(expected=0)
            assert result["receipt"]["signal"] == signal.SIGTERM
            assert not result["receipt"]["output_cleanup_complete"], result
            return result
        scenario(f"{suffix}-full-queue-signal", controlling, blocked)

        def deadline(fixture):
            fixture.ready()
            fixture.pressured_redraw()
            result = fixture.wait()
            assert result["exit_code"] != 0 and result["receipt"]["error"] == "terminal output stalled", result
            return result
        scenario(f"{suffix}-full-queue-timeout", controlling, deadline, timeout_ms=200)

        def closed(fixture):
            result = fixture.wait()
            assert result["exit_code"] != 0 and result["receipt"] is None
            assert b"closed or redirected" in (fixture.folder / "stderr.log").read_bytes()
            return result
        scenario(f"{suffix}-closed-stdin", controlling, closed, stdin=False)

        def hangup(fixture):
            fixture.ready()
            os.close(fixture.master)
            fixture.master = -1
            result = fixture.wait(attrs=False)
            assert result["exit_code"] >= 0, "host must handle hangup rather than die from SIGHUP"
            return result
        scenario(f"{suffix}-hangup", controlling, hangup)
    assert len(results) == 14
    if args.session_contract:
        for controlling in (False, True):
            fixture = Fixture(args.session_contract.resolve(), args.directory / f"ownership-{controlling}", controlling)
            try:
                result = fixture.wait(expected=0)
                assert result["receipt"]["status"] == "pass"
                output = fixture.drain()
                assert b"discard this unfinished frame" not in output
                assert b"cleanup command" in output
                result.update(name=f"ownership-{controlling}", status="pass", controlling=controlling)
                results.append(result)
            finally:
                fixture.close()
                (args.directory / "acceptance.json").write_text(json.dumps({"scenarios": results}, indent=2))
    print(json.dumps({"status": "pass", "scenarios": len(results)}))


if __name__ == "__main__":
    main()
