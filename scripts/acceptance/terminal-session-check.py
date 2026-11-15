#!/usr/bin/env python3
"""Exercise the actual GPU terminal_app process against a controlled Kitty peer.

Pipes test the application input/output loop and real fences, not a graphical
terminal emulator. PNGs are decoded from the live stream. Requires Pillow.
"""
from __future__ import annotations
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import threading
import time
import zlib
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
IMAGE_ID = 0x4A4460
FIRST_FENCE = 0x70000000


class Peer:
    def __init__(self, executable: Path, directory: Path, name: str, timeout_ms: int = 3000, compress: bool = False, extra_args=()):
        self.directory = directory / name
        self.directory.mkdir()
        self.receipt = self.directory / "receipt.txt"
        self.process = subprocess.Popen(
            [str(executable), "--stdio", "--timeout-ms", str(timeout_ms), "--receipt", str(self.receipt)] + (["--compress"] if compress else []) + list(extra_args),
            cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        self.queue: queue.Queue[bytes | None] = queue.Queue()
        self.buffer = bytearray()
        self.stream = bytearray()
        self.frames: list[dict] = []
        self.queries: list[int] = []
        self.query_fields: list[dict] = []
        self.chunks: list[bytes] = []
        self.tiled = "--tiles" in extra_args
        self.tile_updates = 0
        self.canvas = Image.new("RGBA", (480,384))
        self.auto_ack = False
        self.thread = threading.Thread(target=self.read_output, daemon=True)
        self.thread.start()

    def read_output(self):
        while True:
            data = os.read(self.process.stdout.fileno(), 65536)
            if not data:
                self.queue.put(None)
                return
            self.queue.put(data)

    def send(self, data: bytes):
        self.process.stdin.write(data)
        self.process.stdin.flush()

    def ack(self, query_id: int, message: str = "OK"):
        # Fragment the reply across actual pipe writes.
        response = f"\x1b_Gi={query_id};{message}\x1b\\".encode()
        self.send(response[:5])
        self.send(response[5:])

    def pump(self, seconds: float = 0.02):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            try:
                data = self.queue.get(timeout=max(0.001, until - time.monotonic()))
            except queue.Empty:
                break
            if data is None:
                break
            self.stream.extend(data)
            self.buffer.extend(data)
            while True:
                start = self.buffer.find(b"\x1b_G")
                if start < 0:
                    self.buffer[:] = self.buffer[-2:]
                    break
                end = self.buffer.find(b"\x1b\\", start)
                if end < 0:
                    del self.buffer[:start]
                    break
                command = bytes(self.buffer[start + 3:end])
                del self.buffer[:end + 2]
                control, _, payload = command.partition(b";")
                fields = dict(v.split("=", 1) for v in control.decode().split(","))
                if fields.get("a") == "q":
                    if self.tiled and self.tile_updates:
                        path = self.directory / f"frame-{len(self.frames):03d}.png"
                        self.canvas.save(path)
                        pixels = self.canvas.tobytes()
                        self.frames.append({"png":path.name,"rgba_sha256":hashlib.sha256(pixels).hexdigest(),
                            "bytes":len(pixels),"tile_updates":self.tile_updates})
                        self.tile_updates = 0
                    query_id = int(fields["i"])
                    self.queries.append(query_id)
                    self.query_fields.append(fields)
                    raw = base64.b64decode(payload, validate=True)
                    if fields.get("o") == "z":
                        raw = zlib.decompress(raw)
                    assert raw == bytes(3), "invalid capability probe payload"
                    if self.auto_ack:
                        self.ack(query_id)
                elif fields.get("a") == "d":
                    assert fields.get("d") == "I" and IMAGE_ID <= int(fields["i"]) <= IMAGE_ID + (20 if self.tiled else 0)
                else:
                    assert len(payload) <= 4096 and len(payload) % 4 == 0
                    if fields.get("a") == "T":
                        assert not self.chunks
                        assert fields["f"] == "32" and fields["U"] == "1"
                        image_id = int(fields["i"])
                        assert IMAGE_ID <= image_id <= IMAGE_ID + (20 if self.tiled else 0)
                        self.size = int(fields["s"]), int(fields["v"])
                        self.position = (0,0)
                        if image_id != IMAGE_ID:
                            index = image_id - IMAGE_ID - 1
                            col,row = index % 5, index // 5
                            x,y = col*96,row*8*384//28
                            bottom = min((row+1)*8,28)*384//28
                            assert self.size == (96,bottom-y)
                            assert int(fields["c"]) == 16 and int(fields["r"]) == min(8,28-row*8)
                            self.position = (x,y)
                        else:
                            assert self.size == (480, 384)
                        self.compressed = fields.get("o") == "z"
                    self.chunks.append(payload)
                    if fields.get("m") == "0":
                        encoded = base64.b64decode(b"".join(self.chunks), validate=True)
                        pixels = encoded
                        if self.compressed:
                            decoder = zlib.decompressobj()
                            pixels = decoder.decompress(encoded, 480 * 384 * 4 + 1)
                            assert decoder.eof and not decoder.unused_data and not decoder.unconsumed_tail
                        assert len(pixels) == self.size[0] * self.size[1] * 4
                        assert all(v == 255 for v in pixels[3::4])
                        if self.tiled:
                            self.canvas.paste(Image.frombytes("RGBA",self.size,pixels),self.position)
                            self.tile_updates += 1
                            self.chunks.clear()
                            continue
                        path = self.directory / f"frame-{len(self.frames):03d}.png"
                        Image.frombytes("RGBA", self.size, pixels).save(path)
                        self.frames.append({"png": path.name, "rgba_sha256": hashlib.sha256(pixels).hexdigest(), "bytes": len(pixels),
                                            "encoded_bytes": len(encoded), "compressed": self.compressed,
                                            "resource_colors": {str(g): pixels.count(bytes([40 + g, 160, 210, 255])) for g in range(1, 7)}})
                        self.chunks.clear()

    def wait(self, predicate, timeout: float = 15):
        end = time.monotonic() + timeout
        while not predicate():
            self.pump()
            if self.process.poll() is not None:
                raise AssertionError(f"application exited early: {self.process.stderr.read().decode(errors='replace')}")
            if time.monotonic() > end:
                raise AssertionError("peer wait timed out")

    def finish(self, expected_success: bool):
        # A final image can still be buffered after the child has processed quit.
        # Drain it without writing replies into an already closed stdin pipe.
        self.auto_ack = False
        try:
            self.process.wait(timeout=10)
            self.pump(0.1)
            stderr = self.process.stderr.read().decode(errors="replace")
            (self.directory / "stderr.txt").write_text(stderr, encoding="utf-8")
            (self.directory / "stream.ansi").write_bytes(self.stream)
            assert (self.process.returncode == 0) == expected_success, stderr
            assert f"\x1b_Ga=d,d=I,i={IMAGE_ID},q=2;\x1b\\".encode() in self.stream, "owned image cleanup missing"
            values = dict(line.split("=", 1) for line in self.receipt.read_text(encoding="utf-8").splitlines())
            assert values["raw_restored"] == "true"
            clipboard = [base64.b64decode(value, validate=True).decode("utf-8")
                         for value in re.findall(rb'\x1b\]52;c;([A-Za-z0-9+/=]*)\x1b\\', self.stream)]
            return {"exit_code": self.process.returncode, "receipt": values, "frames": self.frames, "clipboard": clipboard,
                    "queries": self.queries, "wire_bytes": len(self.stream)}
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait(timeout=5)
            self.thread.join(timeout=1)
            for stream in [self.process.stdin, self.process.stdout, self.process.stderr]:
                try:
                    stream.close()
                except OSError:
                    pass


def run(executable: Path, directory: Path):
    peer = Peer(executable, directory, "interaction")
    try:
        peer.wait(lambda: len(peer.queries) == 1)
        assert not peer.frames, "unconfirmed terminal must not render"
        peer.ack(FIRST_FENCE)
        peer.wait(lambda: len(peer.frames) == 1 and len(peer.queries) == 2)
        # Ordered inputs keep running while both frame credits are held.
        peer.send(b"\x01\x1b[200~" + "中e\u0301👩‍💻\nsecond".encode() + b"\x1b[201~\x7f\x1a\x19!")
        peer.wait(lambda: len(peer.frames) == 2 and len(peer.queries) == 3)
        peer.pump(0.2)
        assert len(peer.frames) == 2, "third upload bypassed backpressure"
        # Graphics-looking pasted text is application data, not an ACK.
        peer.send(b"\x1b[200~\x1b_Gi=1879048193;OK\x1b\\\x1b[201~")
        peer.pump(0.1)
        assert len(peer.frames) == 2, "paste forged an owned frame fence"
        # Remove that literal paste using undo, then accept the real fence.
        peer.send(b"\x1a")
        peer.ack(FIRST_FENCE + 1)
        peer.wait(lambda: len(peer.frames) == 3 and len(peer.queries) == 4)
        peer.ack(FIRST_FENCE + 2)
        peer.ack(FIRST_FENCE + 3)
        peer.auto_ack = True
        # Tab to real Element button; Enter activation goes through SceneInput.
        before = len(peer.frames)
        peer.send(b"\t\r\x1b[113;5:3u")  # Ctrl+Q RELEASE must not exit.
        peer.wait(lambda: len(peer.frames) > before)
        peer.pump(0.1)
        idle = len(peer.stream)
        peer.pump(0.25)
        assert len(peer.stream) == idle, "unfocused editor/idle app produced output"
        # A real mouse press changes focus and triggers a tree rebuild before
        # release. The gesture must survive that redraw, using current handlers.
        peer.send(b"\x1b[Z")
        before = len(peer.frames)
        peer.wait(lambda: len(peer.frames) > before)
        image = Image.open(peer.directory / peer.frames[-1]["png"]).convert("RGB")
        positions = [(x, y) for y in range(image.height) for x in range(image.width) if image.getpixel((x, y)) == (37, 99, 235)]
        assert positions, "button background missing"
        x = (min(p[0] for p in positions) + max(p[0] for p in positions)) // 2
        y = (min(p[1] for p in positions) + max(p[1] for p in positions)) // 2
        column, row = int(x * 80 / 480) + 1, int(y * 28 / 384) + 1
        peer.send(f"\x1b[<0;{column};{row}M".encode())
        before = len(peer.frames)
        peer.wait(lambda: len(peer.frames) > before)
        peer.send(f"\x1b[<0;{column};{row}m".encode())
        before = len(peer.frames)
        peer.wait(lambda: len(peer.frames) > before)
        # Animation runs only on deadlines, pauses on focus loss, resumes later.
        peer.send(b"\x1bOQ")
        start = len(peer.frames)
        peer.wait(lambda: len(peer.frames) >= start + 2)
        peer.send(b"\x1b[O")
        peer.pump(0.2)
        paused = len(peer.frames)
        peer.pump(0.2)
        assert len(peer.frames) == paused, "background animation did not suspend"
        peer.send(b"\x1b[I")
        peer.wait(lambda: len(peer.frames) > paused)
        peer.send(b"\x1bOQ")
        # Zero-size regions suspend safely; normal resize resumes and repositions.
        peer.send(b"\x1b[8;0;0t")
        peer.pump(0.1)
        peer.send(b"\x1b[8;30;90t")
        resized = len(peer.frames)
        peer.wait(lambda: len(peer.frames) > resized)
        peer.send(b"\x11")  # Ctrl+Q press
        result = peer.finish(True)
        text = bytes.fromhex(result["receipt"]["text_hex"]).decode()
        assert text == "中e\u0301👩‍💻\nsecon!", repr(text)
        assert result["receipt"]["count"] == "2"
        assert len({f["rgba_sha256"] for f in result["frames"]}) > 4
        result["checks"] = ["probe-before-render", "two-frame-credit", "ordered-input-during-stall", "paste-cannot-ack",
                            "real-element-focus-enter", "focus-redraw-click", "key-release", "idle-zero-output", "animation-deadline",
                            "background-suspend", "zero-size-resize", "unicode-edit-undo-redo", "owned-cleanup"]
    finally:
        if peer.process.poll() is None:
            peer.process.kill()

    failures = {}
    for scenario in ["timeout", "rejected", "disconnect"]:
        peer = Peer(executable, directory, scenario, timeout_ms=150)
        try:
            peer.wait(lambda: len(peer.queries) == 1)
            if scenario == "rejected":
                peer.ack(FIRST_FENCE, "ENOTSUP")
            elif scenario == "disconnect":
                peer.process.stdin.close()
            failures[scenario] = peer.finish(False)
            assert not peer.frames, "failed probe rendered pixels"
        finally:
            if peer.process.poll() is None:
                peer.process.kill()
    return {"status": "pass", "real_gpu": True, "real_process": True, "transport": "anonymous-pipes-controlled-Kitty-peer",
            "real_terminal_emulator": False, "ssh": False, "interaction": result, "failures": failures}



def run_background(executable: Path, directory: Path):
    peer = Peer(executable, directory, "background-compressed", compress=True)
    try:
        peer.wait(lambda: len(peer.queries) == 1)
        peer.ack(FIRST_FENCE)
        peer.wait(lambda: len(peer.queries) == 2)
        assert not peer.frames
        assert peer.query_fields[1].get("o") == "z"
        peer.auto_ack = True
        peer.ack(FIRST_FENCE + 1)
        peer.wait(lambda: len(peer.frames) >= 1)
        peer.send(b"\t")  # Remove caret timer: subsequent wake must come from work.
        peer.pump(0.2)
        idle = len(peer.stream)
        peer.pump(0.15)
        assert len(peer.stream) == idle
        peer.send(b"\x1bOR")  # F3, generation 1: no more keyboard input until ready.
        peer.wait(lambda: peer.frames[-1]["resource_colors"]["1"] >= 400)
        first_ready = peer.frames[-1]
        peer.send(b"\x1b[15~")  # F5, generation 2: failure retains image 1.
        before = len(peer.frames)
        peer.wait(lambda: len(peer.frames) >= before + 2)
        peer.pump(0.15)
        assert peer.frames[-1]["resource_colors"]["1"] >= 400
        peer.send(b"\x1bOR\x1bOR")  # Replace slow generation 3 with fast 4.
        peer.wait(lambda: peer.frames[-1]["resource_colors"]["4"] >= 400)
        peer.pump(0.2)
        assert peer.frames[-1]["resource_colors"]["3"] == 0
        peer.send(b"\x1bOR\x1bOS")  # Generation 5 then F4 cancellation.
        peer.pump(0.3)
        assert peer.frames[-1]["resource_colors"]["4"] >= 400
        assert peer.frames[-1]["resource_colors"]["5"] == 0
        idle = len(peer.stream)
        peer.pump(0.15)
        assert len(peer.stream) == idle, "completed tasks kept the host rendering"
        peer.send(b"\x1bOR\x11")  # Dispose with generation 6 still in flight.
        result = peer.finish(True)
        values = result["receipt"]
        assert values["compressed"] == "true"
        assert values["tasks_started"] == "6" and values["tasks_applied"] == "2", values
        assert values["task_errors"] == "1" and values["task_cancelled"] == "1"
        assert values["resource_generation"] == "4" and int(values["tasks_stale"]) >= 3
        assert values["task_wakes"] == "3", values
        assert all(frame["compressed"] for frame in result["frames"])
        result["checks"] = ["zlib-probe-before-GPU", "background-wake-without-keyboard", "real-texture-update",
                            "failure-retains-image", "latest-request-wins", "cancel-retains-image",
                            "disposed-workers-stop", "task-idle-zero-output", "bounded-zlib-decode"]
        result["encoding_ratio_first_ready_resource_frame"] = first_ready["encoded_bytes"] / first_ready["bytes"]
    finally:
        if peer.process.poll() is None:
            peer.process.kill()
    fallback = Peer(executable, directory, "compression-fallback", compress=True)
    try:
        fallback.wait(lambda: len(fallback.queries) == 1)
        fallback.ack(FIRST_FENCE)
        fallback.wait(lambda: len(fallback.queries) == 2)
        fallback.auto_ack = True
        fallback.ack(FIRST_FENCE + 1, "ENOTSUP")
        fallback.wait(lambda: len(fallback.frames) >= 1)
        assert not fallback.frames[0]["compressed"]
        fallback.send(b"\x11")
        fallback_result = fallback.finish(True)
        assert fallback_result["receipt"]["compressed"] == "false"
    finally:
        if fallback.process.poll() is None:
            fallback.process.kill()
    timeout = Peer(executable, directory, "compression-timeout", timeout_ms=150, compress=True)
    try:
        timeout.wait(lambda: len(timeout.queries) == 1)
        timeout.ack(FIRST_FENCE)
        timeout.wait(lambda: len(timeout.queries) == 2)
        timeout_result = timeout.finish(False)
        assert not timeout.frames
    finally:
        if timeout.process.poll() is None:
            timeout.process.kill()
    return {"interaction": result, "fallback": fallback_result, "timeout": timeout_result}


def run_documents(executable: Path, directory: Path):
    path = directory / "editor.checkpoint.json"
    text = "中e\u0301👩‍💻\ncheckpoint"
    local = "newer local edit"
    f6, f7, f8, f9 = [f"\x1b[{n}~".encode() for n in [17, 18, 19, 20]]

    def start(name, *args):
        peer = Peer(executable, directory, name, extra_args=["--checkpoint", str(path), "--io-delay-ms", "160", *args])
        peer.wait(lambda: len(peer.queries) == 1)
        peer.auto_ack = True
        peer.ack(FIRST_FENCE)
        peer.wait(lambda: bool(peer.frames))
        return peer

    def paste(peer, value):
        peer.send(b"\x1b[200~" + value.encode() + b"\x1b[201~")

    def generation(number):
        return path.exists() and json.loads(path.read_text(encoding="utf-8"))["generation"] == number

    peer = start("documents-save", "--clipboard")
    try:
        peer.send(b"\x01")
        paste(peer, text)
        peer.send(b"!\x1a\x01" + f9 + f7)  # Save both undo/redo stacks and full selection.
        peer.wait(lambda: generation(1))
        peer.pump(0.2)
        first = json.loads(json.loads(path.read_text(encoding="utf-8"))["payload"])
        assert first["current"]["text"] == text and first["redo"][-1]["text"] == text + "!"
        assert first["current"]["anchor"] == 0 and first["current"]["caret"] == len(text.encode())
        assert "clipboard" not in first
        # A live, GPU-rendered modal must block edits, background shortcuts and
        # outside clicks; closing Enter must not also insert a document newline.
        peer.send(f6)
        peer.pump(0.35)
        modal_frame = peer.frames[-1]
        image = Image.open(peer.directory / modal_frame["png"]).convert("RGB")
        assert image.tobytes().count(bytes([38, 54, 79])) > 1000
        paste(peer, "blocked modal edit")
        peer.send(b"\x1bOR" + f7 + b"\x1b[<0;10;22M\x1b[<0;10;22m\t\x1b[Z\r")
        peer.pump(0.3)
        peer.send(f6)
        peer.pump(0.3)
        peer.send(b"\x1b")
        peer.pump(0.2)
        # Read snapshot revision is checked on the UI thread after disk work.
        peer.send(f8)
        paste(peer, "during reload")
        peer.pump(0.45)
        peer.send(f8)
        peer.pump(0.45)
        peer.send(b"\x19\x1a" + f7)
        paste(peer, local)  # Saved snapshot must not clobber later edits.
        peer.wait(lambda: generation(2))
        peer.pump(0.2)
        second = json.loads(json.loads(path.read_text(encoding="utf-8"))["payload"])
        assert second["current"]["text"] == text and second["redo"][-1]["text"] == text + "!"
        peer.send(b"\x11")
        saved = peer.finish(True)
        r = saved["receipt"]
        assert bytes.fromhex(r["text_hex"]).decode() == local, r
        assert r["file_saved"] == "2" and r["file_loaded"] == "1" and r["file_conflicts"] == "1" and r["file_errors"] == "0", r
        assert r["tasks_started"] == "0" and r["count"] == "0", r
        assert [r[k] for k in ["modal_opens", "modal_accepts", "modal_cancels", "modal_depth"]] == ["2", "1", "1", "0"], r
        assert r["focused"] == "document" and saved["clipboard"] == [text], r
        saved["modal_frame"] = modal_frame["png"]
    finally:
        if peer.process.poll() is None:
            peer.process.kill()

    peer = start("documents-restart")
    try:
        peer.send(b"\x19\x1a" + f9)
        peer.pump(0.25)
        # Model another cooperating writer advancing the generation. This
        # fixture modifies only the file this run created, never user data.
        external = json.loads(path.read_text(encoding="utf-8"))
        external["generation"] = 3
        path.write_text(json.dumps(external, ensure_ascii=False), encoding="utf-8")
        preserved = path.read_bytes()
        peer.send(f7)
        peer.pump(0.4)
        assert path.read_bytes() == preserved
        lock = Path(str(path) + ".lock")
        lock.write_text("controlled external writer", encoding="utf-8")
        peer.send(f7)
        peer.pump(0.4)
        assert path.read_bytes() == preserved and lock.read_text() == "controlled external writer"
        lock.unlink()
        external["sha256"] = "0" * 64
        path.write_text(json.dumps(external, ensure_ascii=False), encoding="utf-8")
        corrupt = path.read_bytes()
        peer.send(f8)
        peer.pump(0.4)
        peer.send(b"\x19\x11")  # Failed read must retain restart redo history.
        restarted = peer.finish(True)
        r = restarted["receipt"]
        assert bytes.fromhex(r["text_hex"]).decode() == text + "!", r
        assert r["file_loaded"] == "1" and r["file_generation"] == "2" and r["file_errors"] == "3", r
        assert r["clipboard_errors"] == "1" and not restarted["clipboard"], r
        assert path.read_bytes() == corrupt
    finally:
        if peer.process.poll() is None:
            peer.process.kill()

    peer = start("documents-corrupt-startup")
    try:
        peer.send(b"\x11")
        damaged = peer.finish(True)
        assert damaged["receipt"]["file_errors"] == "1" and damaged["receipt"]["file_loaded"] == "0"
        assert path.read_bytes() == corrupt
    finally:
        if peer.process.poll() is None:
            peer.process.kill()
    return {"saved": saved, "restarted": restarted, "corrupt_startup": damaged,
            "checks": ["unicode-history-and-selection-on-disk", "explicit-OSC52-UTF8-roundtrip", "clipboard-disabled-by-default",
                       "GPU-modal-overlay", "modal-Tab-and-pointer-isolation", "modal-close-does-not-edit",
                       "modal-focus-return", "stale-reload-retains-local-edits", "save-snapshot-retains-newer-edits",
                       "restart-restores-redo", "external-generation-conflict", "external-lock-preserved",
                       "checksum-failure-retains-history", "corrupt-startup-preserves-file"]}


def run_environment(executable: Path, directory: Path):
    path = directory / "environment.json"
    base = {"locale": {"locale": "en-US", "languages": ["en-US"],
                       "time_zone": {"kind": "iana", "id": "UTC"}, "hour_cycle": "h12",
                       "first_day_of_week": 7, "measurement_system": "us", "temperature_unit": "fahrenheit"},
            "color_scheme": "dark", "reduced_motion": False, "font_scale": 1.0, "lifecycle": "active"}
    def write(value):
        path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
    def frame(peer):
        return Image.open(peer.directory / peer.frames[-1]["png"]).convert("RGB")
    f10 = b"\x1b[21~"
    write(base)
    peer = Peer(executable, directory, "environment-live", extra_args=["--environment", str(path), "--io-delay-ms", "160"])
    text = "Environment Unicode 中e\u0301👩‍💻"
    try:
        peer.wait(lambda: len(peer.queries) == 1)
        peer.auto_ack = True
        peer.ack(FIRST_FENCE)
        peer.wait(lambda: bool(peer.frames))
        peer.send(b"\x01\x1b[200~" + text.encode() + b"\x1b[201~!\x1a\x01\t")
        peer.pump(0.3)
        dark = peer.frames[-1]["png"]
        next_value = json.loads(json.dumps(base))
        next_value.update(color_scheme="light", reduced_motion=True)
        write(next_value)
        peer.send(f10)
        peer.wait(lambda: frame(peer).getpixel((0, 0)) == (241, 245, 249))
        peer.pump(0.1)
        light = peer.frames[-1]["png"]
        # Compare the glyph mask at the same colors; background/theme changes
        # alone cannot satisfy the font-scale check.
        crop = (24, 80, 460, 242)
        small = frame(peer).crop(crop).tobytes().count(bytes([20, 36, 58]))
        next_value["font_scale"] = 1.4
        write(next_value)
        before = len(peer.frames)
        peer.send(f10)
        peer.wait(lambda: len(peer.frames) > before)
        large = frame(peer).crop(crop).tobytes().count(bytes([20, 36, 58]))
        assert large > small * 1.2, (small, large)
        scaled = peer.frames[-1]["png"]
        peer.send(b"\x1bOQ")  # Animation intent on; reduced motion must suppress its deadlines.
        peer.pump(0.1)
        idle = len(peer.stream)
        peer.pump(0.25)
        assert len(peer.stream) == idle, "reduced-motion animation produced frames"
        next_value["locale"].update(locale="zh-CN", languages=["zh-CN", "en-US"], hour_cycle="h23",
                                     time_zone={"kind": "iana", "id": "Asia/Shanghai"})
        write(next_value)
        before = len(peer.frames)
        peer.send(f10)
        peer.wait(lambda: len(peer.frames) > before)
        peer.pump(0.2)
        idle = len(peer.stream)
        peer.send(f10)  # Identical preferences: no revision advance or repaint.
        peer.pump(0.4)
        assert len(peer.stream) == idle, "identical environment caused output"
        malformed = json.loads(json.dumps(next_value)); malformed["font_scale"] = 0
        write(malformed)
        peer.send(f10)
        peer.pump(0.4)
        assert frame(peer).getpixel((0, 0)) == (241, 245, 249)
        assert json.loads(path.read_text(encoding="utf-8")) == malformed
        next_value["locale"]["measurement_system"] = "metric"
        write(next_value)
        # OS lifecycle changes after dispatch invalidate the pending request.
        peer.send(f10 + b"\x1b[O\x1b[I")
        peer.pump(0.45)
        peer.send(f10)
        peer.pump(0.4)
        next_value["lifecycle"] = "background"
        write(next_value)
        peer.send(f10)
        peer.pump(0.4)
        idle = len(peer.stream)
        peer.send(b"\x1b[200~must not edit background\x1b[201~\r")
        peer.pump(0.25)
        assert len(peer.stream) == idle, "background input or timer produced output"
        next_value.update(lifecycle="active", reduced_motion=False)
        write(next_value)
        before = len(peer.frames)
        peer.send(f10)
        peer.wait(lambda: len(peer.frames) >= before + 3)
        peer.send(b"\x1bOQ")  # Turn animation intent off after restored deadlines run.
        peer.pump(0.15)
        peer.send(b"\x1b[Z\x19\x1a\x11")
        result = peer.finish(True)
        r = result["receipt"]
        assert bytes.fromhex(r["text_hex"]).decode() == text, r
        assert r["selection"] == f"0..{len(text.encode())}" and r["focused"] == "document", r
        assert r["revision"] == "2", "environment or redo/undo changed immutable document identity"
        assert r["environment_updates"] == "6" and r["environment_errors"] == "1" and r["environment_conflicts"] == "1", r
        assert r["environment_revision"] == "9" and r["hour_effect_runs"] == "2", r
        assert int(r["animation_ticks"]) > 0 and json.loads(r["environment_json"]) == next_value, r
        result["dark_frame"] = dark; result["light_frame"] = light
        result["scaled_frame"] = scaled; result["font_mask_pixels"] = {"small": small, "large": large}
    finally:
        if peer.process.poll() is None:
            peer.process.kill()
    # Read-only real OS adapter: no terminal modes, GPU or preference mutation.
    probe = subprocess.run([str(executable), "--dump-environment"], cwd=ROOT, capture_output=True, check=True)
    system = json.loads(probe.stdout.decode("utf-8"))
    assert system["locale"]["languages"] and system["locale"]["locale"]
    (directory / "system-environment.json").write_text(json.dumps(system, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return {"interaction": result, "system_snapshot": system,
            "checks": ["bounded-injected-environment", "real-GPU-light-theme", "font-scale-changes-glyphs",
                       "reduced-motion-suppresses-animation", "locale-and-time-zone-refresh", "equal-update-zero-output",
                       "malformed-preferences-retain-state", "stale-lifecycle-refresh-rejected", "background-input-isolation",
                       "resume-animation-intent", "document-revision-preserved", "selection-focus-and-redo-preserved",
                       "sibling-hour-effect-filtered", "actual-Windows-preferences-read-only"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--backend", choices=["dx12", "gles"], default="dx12")
    args = parser.parse_args()
    directory = args.directory.resolve()
    directory.mkdir()  # Preserve previous evidence; new directory required.
    subprocess.run(["cargo", "build", "-p", "jadix_tui", "--features", f"backend-{args.backend}", "--example", "terminal_app", "--locked"], cwd=ROOT, check=True)
    executable = ROOT / "target" / "debug" / "examples" / ("terminal_app.exe" if os.name == "nt" else "terminal_app")
    report = run(executable, directory)
    report["background"] = run_background(executable, directory)
    report["documents"] = run_documents(executable, directory)
    report["environment"] = run_environment(executable, directory)
    frames = report["background"]["interaction"]["frames"]
    rgba_bytes = sum(frame["bytes"] for frame in frames)
    encoded_bytes = sum(frame["encoded_bytes"] for frame in frames)
    report["transport_metrics"] = {"compressed_frames": len(frames), "rgba_bytes": rgba_bytes,
                                   "zlib_bytes": encoded_bytes, "zlib_to_rgba_ratio": encoded_bytes / rgba_bytes,
                                   "wire_bytes_including_queries_and_placeholders": report["background"]["interaction"]["wire_bytes"],
                                   "latency_benchmark": False, "ssh_benchmark": False}
    paths = list((ROOT / "crates/jadix_tui/src").glob("*.rs")) + [ROOT / "crates/jadix_tui/examples/terminal_app.rs", Path(__file__).resolve(), ROOT / "Cargo.lock", ROOT / "crates/jadix_tui/Cargo.toml", ROOT / "crates/jadix/src/renderer/offscreen_scene.rs", ROOT / "crates/jadix_tasks/src/lib.rs", ROOT / "crates/jadix_tui/tests/session.rs", ROOT / "crates/jadix_tui/tests/scene.rs"]
    report["sources"] = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
    for name in ["crates/jadix_document/src/checkpoint.rs", "crates/jadix_document/Cargo.toml", "crates/jadix_native/src/checkpoint.rs", "crates/jadix_native/Cargo.toml", "crates/jadix_tui/tests/document_session.rs"]:
        report["sources"][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    for name in ["crates/jadix_core/src/environment.rs", "crates/jadix_core/src/environment_windows.rs", "crates/jadix_core/src/reactivity.rs", "crates/jadix_core/src/store.rs", "crates/jadix_core/Cargo.toml", "crates/jadix_native/src/host.rs", "crates/jadix_native/src/layout.rs", "crates/jadix_native/tests/windows_environment.rs", "crates/jadix_core/tests/environment.rs"]:
        report["sources"][name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    (directory / "acceptance.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": report["status"], "checks": len(report["interaction"]["checks"]), "failure_scenarios": list(report["failures"]), "background_checks": len(report["background"]["interaction"]["checks"]), "document_checks": len(report["documents"]["checks"]), "environment_checks": len(report["environment"]["checks"]), "real_terminal_emulator": False}))


if __name__ == "__main__":
    main()
