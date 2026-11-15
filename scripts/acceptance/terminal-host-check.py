#!/usr/bin/env python3
"""Linux/WSLg real Kitty + optional isolated loopback OpenSSH acceptance.

Creates its own private dbus-run-session. Only owned Kitty windows are captured;
GSettings uses a private XDG config directory. SSH keys never enter evidence.
Requires kitty, xdotool, xwd, xwdtopnm, Pillow, and explicit executable/sshd paths.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import socket
import subprocess as sp
import sys
import tempfile
import time
from PIL import Image


def run(args, **kwargs):
    return sp.run(list(map(str, args)), check=True, stdout=sp.PIPE, stderr=sp.PIPE, **kwargs)


def until(predicate, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(.1)
    raise TimeoutError("acceptance condition not met")


def receipt(path):
    return dict(line.split("=", 1) for line in path.read_text().splitlines() if "=" in line)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--sshd", type=Path)
    parser.add_argument("--ssh-only", action="store_true")
    parser.add_argument("--ssh-probe-only", action="store_true", help="measure persistent-channel echo without opening Kitty")
    parser.add_argument("--ssh-rtt-ms", type=int, default=0)
    parser.add_argument("--ssh-bandwidth-kib", type=int, default=1024)
    parser.add_argument("--tmux", action="store_true", help="private tmux server, explicit passthrough")
    parser.add_argument("--screen", action="store_true", help="owned GNU Screen session, private config, character hosts only")
    parser.add_argument("--screen-csi-escape", action="store_true", help="explicit CSI-u Escape input for Screen mouse-mode compatibility; does not certify bare Escape")
    parser.add_argument("--menus", action="store_true", help="exercise shared terminal menu callbacks")
    parser.add_argument("--tiles", action="store_true")
    parser.add_argument("--character", action="store_true", help="run the CPU-only cell_app contract")
    parser.add_argument("--colors", choices=("rgb", "256", "16"), default="rgb")
    parser.add_argument("--auto-layout", action="store_true", help="exercise shared layout and mounted search with cell_app")
    parser.add_argument("--portable", action="store_true", help="exercise mounted portable_app callbacks/navigation")
    parser.add_argument("--scrollbars", action="store_true", help="also drag a real portable scrollbar using SGR input")
    parser.add_argument("--shortcuts", action="store_true", help="exercise named shared command from editor and modal")
    parser.add_argument("--signal-term", action="store_true", help="terminate owned GPU app during cooperative file work")
    args = parser.parse_args()
    if (args.ssh_only or args.ssh_probe_only) and not args.sshd:
        parser.error("SSH-only modes require --sshd")
    if args.screen_csi_escape and not args.screen:
        parser.error("--screen-csi-escape requires --screen")
    if args.screen and not (args.character or args.portable):
        parser.error("--screen requires --character or --portable; graphics negotiation is a separate matrix")
    if args.signal_term and args.character:
        parser.error("--signal-term exercises the GPU worker path; use the Unix lifecycle tool for character signals")
    args.directory = args.directory.resolve()
    if os.environ.get("_JADIX_HOST_SESSION") != "1":
        # D-Bus must inherit the private configuration BEFORE service activation.
        # Setting XDG_CONFIG_HOME after starting the bus isolates readers only,
        # while its dconf writer would still use the real user profile.
        user_settings = Path.home() / ".config/dconf/user"
        before = user_settings.read_bytes() if user_settings.exists() else None
        with tempfile.TemporaryDirectory(prefix="jadix-settings-") as config:
            env = dict(os.environ, XDG_CONFIG_HOME=config, XDG_CACHE_HOME=str(Path(config) / "cache"), GSETTINGS_BACKEND="dconf",
                       _JADIX_HOST_SESSION="1")
            outcome = sp.run(["dbus-run-session", "--", sys.executable, __file__, *sys.argv[1:]], env=env)
        after = user_settings.read_bytes() if user_settings.exists() else None
        assert before == after, "user dconf database changed during isolated acceptance"
        (args.directory / "settings-isolation.json").write_text(json.dumps({"user_database_unchanged": True}))
        if outcome.returncode: raise SystemExit(outcome.returncode)
        return
    args.directory.mkdir(parents=True, exist_ok=False)
    results = {"platform": os.uname()._asdict() if hasattr(os.uname(), "_asdict") else list(os.uname()),
               "kitty": run(["kitty", "--version"]).stdout.decode().strip(), "scenarios": []}
    if args.screen:
        results["screen"] = run(["screen", "--version"]).stdout.decode().strip()
        results["screen_csi_escape"] = args.screen_csi_escape
        if args.tmux:
            results["tmux"] = run(["tmux", "-V"]).stdout.decode().strip()
    with tempfile.TemporaryDirectory(prefix="jadix-host-") as temporary:
        temp = Path(temporary)
        os.environ["LIBGL_ALWAYS_SOFTWARE"] = "1"
        schema = "org.gnome.desktop.interface"
        def setting(key, value):
            run(["gsettings", "set", schema, key, value])
        daemon = None
        daemon_log = None
        ssh = None
        relay = None
        try:
            if args.sshd:
                for key in ("host", "client"):
                    run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", temp / key])
                with socket.socket() as sock:
                    sock.bind(("127.0.0.1", 0))
                    port = sock.getsockname()[1]
                user = run(["id", "-un"]).stdout.decode().strip()
                config = temp / "sshd_config"
                config.write_text(f"""Port {port}
ListenAddress 127.0.0.1
HostKey {temp}/host
PidFile {temp}/sshd.pid
AuthorizedKeysFile {temp}/client.pub
AllowUsers {user}
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM no
AllowTcpForwarding no
AllowAgentForwarding no
X11Forwarding no
PermitUserEnvironment no
LogLevel VERBOSE
""")
                daemon_log = (args.directory / "sshd.log").open("wb")
                daemon = sp.Popen([str(args.sshd.resolve()), "-D", "-e", "-f", str(config)],
                                  stdout=daemon_log, stderr=sp.STDOUT)
                def listening():
                    if daemon.poll() is not None:
                        raise RuntimeError("isolated sshd exited; see sshd.log")
                    try:
                        with socket.create_connection(("127.0.0.1", port), .2):
                            return True
                    except OSError:
                        return False
                until(listening)
                from terminal_ssh_relay import Relay
                relay = Relay(port, args.ssh_rtt_ms, args.ssh_bandwidth_kib)
                port = relay.port
                (temp / "known_hosts").write_text(f"[127.0.0.1]:{port} " + (temp / "host.pub").read_text())
                ssh = ["ssh", "-F", "/dev/null", "-i", str(temp / "client"), "-p", str(port),
                       "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=yes",
                       "-o", f"UserKnownHostsFile={temp}/known_hosts", f"{user}@127.0.0.1"]
                start = time.monotonic()
                run(ssh + ["true"], timeout=20)
                results["ssh_connect_and_true_observed_ms"] = (time.monotonic() - start) * 1000
                if args.ssh_probe_only:
                    import select
                    probe = sp.Popen(ssh + ["sh", "-c", shlex.quote("while IFS= read -r line; do printf '%s\\n' \"$line\"; done")],
                                     stdin=sp.PIPE, stdout=sp.PIPE, stderr=sp.PIPE)
                    echoes = []
                    try:
                        for index in range(6):
                            message = f"echo-{index}\n".encode()
                            started = time.monotonic()
                            probe.stdin.write(message)
                            probe.stdin.flush()
                            assert select.select([probe.stdout], [], [], 10)[0], "SSH echo timed out"
                            assert probe.stdout.readline(64) == message, "SSH echo differs"
                            if index: echoes.append((time.monotonic() - started) * 1000)
                        probe.stdin.close()
                        assert probe.wait(timeout=5) == 0
                        results["persistent_ssh_echo_observed_ms"] = echoes
                    finally:
                        if probe.poll() is None:
                            probe.terminate()
                            probe.wait(timeout=5)
                    return
            for name in (["ssh"] if args.ssh_only else ["local", "ssh"] if ssh else ["local"]):
                scenario_started = time.monotonic()
                folder = args.directory / name
                folder.mkdir()
                setting("color-scheme", "prefer-dark")
                setting("enable-animations", "true")
                setting("text-scaling-factor", "1.0")
                setting("clock-format", "24h")
                app = [str(args.executable.resolve()), "--system-environment", "--compress",
                       "--timeout-ms", "10000", "--receipt", str(folder / "receipt.txt")]
                if args.character or args.portable:
                    app = [str(args.executable.resolve()), "--receipt", str(folder / "receipt.txt"), "--colors", args.colors]
                    if args.auto_layout: app.append("--auto-layout")
                if args.tiles: app.append("--tiles")
                if args.menus: app.append("--menus")
                if args.tmux:
                    if not (args.character or args.portable): app.append("--tmux")
                    tmux_config = temp / "tmux.conf"
                    tmux_config.write_text("set -g status off\nset -g allow-passthrough on\nset -g default-terminal tmux-256color\n")
                    inner = shlex.join(app) + " 2> " + shlex.quote(str(folder / "tmux-app-stderr.log"))
                    # SSH with a private config intentionally forwards no user
                    # locale. Select UTF-8 explicitly instead of tmux's C-locale
                    # underscore substitution for Kitty placeholder characters.
                    app = ["tmux", "-u", "-S", str(temp / f"{name}-tmux.socket"), "-f", str(tmux_config), "new-session", inner]
                if args.screen:
                    # Each invocation owns its socket directory and session;
                    # inherited STY or user screenrc cannot select another one.
                    screen_socket = temp / f"{name}-screen"
                    screen_socket.mkdir(mode=0o700)
                    screen_config = temp / "screenrc"
                    screen_config.write_text("startup_message off\ndefutf8 on\nvbell off\n")
                    (folder / "screenrc.accepted").write_bytes(screen_config.read_bytes())
                    if args.tmux:
                        (folder / "tmux.conf.accepted").write_bytes(tmux_config.read_bytes())
                    screen_name = f"jadix-{temp.name}-{name}"
                    app = ["script", "-q", "-e", "-f", "--log-in", str(folder / "screen-fixture-input.log"), "--log-timing", str(folder / "screen-timing.log"), "-c", shlex.join(app), "--log-out", str(folder / "screen-application.typescript")]
                    app = ["env", "-u", "STY", f"SCREENDIR={screen_socket}", "screen", "-U", "-c", str(screen_config), "-S", screen_name, "-m", *app]
                if args.signal_term:
                    app += ["--checkpoint", str(folder / "cancelled-checkpoint.json"), "--io-delay-ms", "2000"]
                    app = ["bash", "-c", f"echo $$ > {shlex.quote(str(folder / 'app.pid'))}; exec " + shlex.join(app)]
                if name == "ssh":
                    # These environment values are fixture-local, explicitly
                    # propagated to the server command, not SSH AcceptEnv globals.
                    remote = ["env"] + [f"{k}={os.environ[k]}" for k in
                        ("DBUS_SESSION_BUS_ADDRESS", "XDG_CONFIG_HOME", "GSETTINGS_BACKEND", "XDG_RUNTIME_DIR", "LIBGL_ALWAYS_SOFTWARE")]
                    app = ssh[:-1] + ["-tt", ssh[-1], shlex.join(remote + app)]
                if args.screen:
                    (folder / "launch-command.json").write_text(json.dumps(app, indent=2))
                wrapper = temp / f"{name}.sh"
                wrapper.write_text("#!/bin/bash\n" +
                    f"stty -g > {shlex.quote(str(folder / 'stty-before.txt'))}\n" +
                    shlex.join(app) + f" 2> {shlex.quote(str(folder / 'app-stderr.log'))}\n" +
                    f"echo $? > {shlex.quote(str(folder / 'exit-code.txt'))}\n" +
                    f"stty -g > {shlex.quote(str(folder / 'stty-after.txt'))}\n" +
                    "printf '\\nJadix acceptance exited\\n'\nsleep 60\n")
                address = f"unix:{temp}/{name}.socket"
                log = (folder / "kitty.log").open("wb")
                terminal = sp.Popen(["kitty", "--config", "NONE", "--listen-on", address,
                    "-o", "allow_remote_control=socket-only", "-o", "linux_display_server=x11",
                    "-o", "remember_window_size=no",
                    "-o", "initial_window_width=900", "-o", "initial_window_height=660",
                    "--title", f"Jadix acceptance {name}", "bash", str(wrapper)], stdout=log, stderr=log)
                try:
                    until(lambda: Path(address[5:]).exists())
                    def remote(*command, data=None):
                        if args.screen_csi_escape and command[0] == "send-text" and data is not None and data.endswith(b"\x1b"):
                            # Only these fixture key packets end in a bare Esc;
                            # bracketed paste packets end in CSI 201~ instead.
                            data = data[:-1] + b"\x1b[27u"
                        if args.screen and command[0] == "send-text" and data is not None:
                            # GNU Screen owns Ctrl-A; Ctrl-A a sends a literal
                            # Ctrl-A to the app, including select-all scenarios.
                            data = data.replace(b"\x01", b"\x01a")
                        if args.portable and command[0] == "send-text" and data is not None:
                            # Explicit synthetic terminal focus keeps this owned
                            # input scenario independent of desktop focus theft.
                            data = b"\x1b[I" + data
                        return run(["kitty", "@", "--to", address, *command], input=data, timeout=10).stdout
                    until(lambda: remote("ls"))
                    xid = until(lambda: run(["xdotool", "search", "--pid", terminal.pid]).stdout.strip()).splitlines()[0].decode()
                    run(["xdotool", "windowfocus", "--sync", xid])
                    def capture(label):
                        xwd = folder / f"{label}.xwd"
                        run(["xwd", "-silent", "-id", xid, "-out", xwd])
                        import io
                        decoded = run(["xwdtopnm", xwd]).stdout
                        with Image.open(io.BytesIO(decoded)) as image:
                            image.convert("RGB").save(folder / f"{label}.png")
                        xwd.unlink()
                    if args.portable:
                        until(lambda: "Jadix portable app" in remote("get-text").decode())
                        capture("initial")
                        operations = results.setdefault("semantic_operation_observed_ms", {}).setdefault(name, {})
                        typed="Jadix portable 中文 🦀"
                        operation_started = time.monotonic()
                        remote("send-text","--stdin",data=b"\x01\x1b[200~"+typed.encode()+b"\x1b[201~")
                        until(lambda: typed in remote("get-text").decode())
                        operations["unicode_paste"] = (time.monotonic() - operation_started) * 1000
                        operation_started = time.monotonic()
                        remote("send-text","--stdin",data=b"\x1a\x19\t\r")
                        until(lambda: "count 1" in remote("get-text").decode())
                        operations["undo_redo_focus_activate"] = (time.monotonic() - operation_started) * 1000
                        if args.menus:
                            operation_started = time.monotonic()
                            remote("send-text", "--stdin", data=b"\x1b[21~\r")  # F10, shared callback
                            until(lambda: "count 2" in remote("get-text").decode())
                            operations["menu_callback"] = (time.monotonic() - operation_started) * 1000
                        if args.shortcuts:
                            remote("send-text","--stdin",data=b"\x1b[Z\x07")
                            until(lambda:f"count {3 if args.menus else 2}" in remote("get-text").decode())
                            remote("send-text","--stdin",data=b"\t")
                        remote("send-text","--stdin",data=b"\t \t\r")
                        until(lambda: "Review page" in remote("get-text").decode())
                        capture("review")
                        remote("send-text","--stdin",data=b"\t\r")
                        until(lambda: typed in remote("get-text").decode())
                        remote("send-text","--stdin",data=b"\x1b[13~")
                        until(lambda: "Close / Esc" in remote("get-text").decode())
                        capture("modal")
                        if args.shortcuts:
                            remote("send-text","--stdin",data=b"\x07")
                            time.sleep(.15)
                        remote("send-text","--stdin",data=b"\x1b")
                        until(lambda: "Close / Esc" not in remote("get-text").decode())
                        if args.scrollbars:
                            import collections, unicodedata
                            def tracks():
                                lines=remote("get-text").decode().splitlines()
                                cells=[]
                                for y,line in enumerate(lines):
                                    x=0
                                    for c in line:
                                        if c in "│█": cells.append((x,y,c))
                                        x+=0 if unicodedata.combining(c) else 2 if unicodedata.east_asian_width(c) in "WF" else 1
                                columns=collections.Counter(x for x,y,c in cells if c=="│")
                                if not columns: return None
                                column=columns.most_common(1)[0][0]
                                bar=[(y,c) for x,y,c in cells if x==column]
                                thumb=next((y for y,c in bar if c=="█"),None)
                                return (column,thumb,max(y for y,c in bar)) if thumb is not None else None
                            column,thumb,bottom=until(tracks)
                            remote("send-text","--stdin",data=f"\x1b[<0;{column+1};{thumb+1}M\x1b[<32;{column+1};{bottom+1}M\x1b[<0;{column+1};{bottom+1}m".encode())
                            until(lambda:"500 shared" in remote("get-text").decode())
                            capture("scrollbar-drag")
                        remote("resize-os-window","--unit","pixels","--width","760","--height","560")
                        time.sleep(.4)
                        remote("send-text","--stdin",data=b"\x1b[15~")
                        until(lambda: "500 shared" in remote("get-text").decode())
                        capture("search")
                        (folder/"terminal-text.txt").write_bytes(remote("get-text"))
                        remote("send-text","--stdin",data=b"\x11")
                        until(lambda:(folder/"exit-code.txt").exists())
                        value=json.loads((folder/"receipt.txt").read_text())
                        assert value["portable"] and value["value"]==typed and value["count"]==1+int(args.shortcuts)+int(args.menus),value
                        if args.menus: assert value["last_menu_command"]=="menu.increment", value
                        if args.shortcuts:
                            assert value["layout_created"]==0 and value["layout_reused"]>500,value
                        assert value["checked"] and value["route"]=="edit" and value["search_matches"]==1,value
                        assert value["dismissals"]==1 and not value["modal_open_on_exit"],value
                        if args.scrollbars:
                            assert value["scrollbars"] and value["scroll_events"]>=1 and value["last_scroll_y"]>0,value
                        assert value["raw_restored"] and value["error"] is None and value["resizes"]>=1,value
                        assert (folder/"stty-before.txt").read_bytes()==(folder/"stty-after.txt").read_bytes()
                        assert (folder/"exit-code.txt").read_text().strip()=="0"
                        results["scenarios"].append({"name":name,"status":"pass","portable":True,"receipt":value,"executable_sha256":hashlib.sha256(args.executable.read_bytes()).hexdigest()})
                        continue
                    if args.character:
                        until(lambda: "Jadix character surface" in remote("get-text").decode())
                        time.sleep(.3)
                        capture("initial")
                        typed = "Jadix 字符后端 🦀"
                        remote("send-text", "--stdin", data=typed.encode())
                        until(lambda: typed in remote("get-text").decode())
                        remote("send-text", "--stdin", data=b"\t\r")
                        until(lambda: "count 1" in remote("get-text").decode())
                        # Modal traps Tab. Escape restores Increment focus.
                        remote("send-text", "--stdin", data=b"\x1b[13~")
                        until(lambda: "Close / Esc" in remote("get-text").decode())
                        remote("send-text", "--stdin", data=b"\t\x1b")
                        time.sleep(.2)
                        remote("send-text", "--stdin", data=b"\r")
                        until(lambda: "count 2" in remote("get-text").decode())
                        # Real PTY/SSH SGR wheel changes shared scroll geometry.
                        remote("send-text", "--stdin", data=b"\x1b[<65;30;16M")
                        until(lambda: "  4  shared" in remote("get-text").decode())
                        remote("resize-os-window", "--unit", "pixels", "--width", "760", "--height", "560")
                        time.sleep(.5)
                        capture("resized")
                        if args.auto_layout:
                            remote("send-text", "--stdin", data=b"\x06500  shared\r")
                            until(lambda: "500  shared" in remote("get-text").decode())
                            remote("send-text", "--stdin", data=b"\x1b[14~")  # F4 cycles search.
                            time.sleep(.2)
                            capture("search")
                        (folder / "terminal-text.txt").write_bytes(remote("get-text"))
                        remote("send-text", "--stdin", data=b"\x11")
                        until(lambda: (folder / "exit-code.txt").exists())
                        value = json.loads((folder / "receipt.txt").read_text())
                        assert value["count"] == 2 and value["value"] == typed, value
                        assert value["raw_restored"] and not value["kitty_required"], value
                        assert value["resizes"] >= 1 and value["frames"] < 30 + value.get("search_requests",0), value
                        if args.auto_layout:
                            assert value["auto_layout"] and value["search_requests"] >= 1 and value["search_matches"] == 1, value
                        assert (folder / "stty-before.txt").read_bytes() == (folder / "stty-after.txt").read_bytes()
                        assert (folder / "exit-code.txt").read_text().strip() == "0"
                        results["scenarios"].append({"name": name, "status": "pass", "character": True, "receipt": value})
                        continue
                    until(lambda: "\U0010eeee" in remote("get-text").decode())
                    time.sleep(.5)
                    capture("dark")
                    def editor_pixels(label, color):
                        # Decoding upload bytes cannot validate Kitty's virtual
                        # placement coordinates. Detect holes in the visible
                        # filled editor rectangle, allowing text/caret pixels.
                        with Image.open(folder / f"{label}.png") as screen:
                            screen = screen.convert("RGB")
                            mask = Image.new("1", screen.size)
                            mask.putdata([pixel == color for pixel in screen.getdata()])
                            box = mask.getbbox()
                            assert box is not None, (label, "editor background missing")
                            left, top, right, bottom = box
                            assert right - left > 400 and bottom - top > 140, (label, box)
                            for x in range(left + 2, right - 2):
                                coverage = sum(screen.getpixel((x, y)) == color for y in range(top + 2, bottom - 2)) / (bottom - top - 4)
                                assert coverage >= .75, (label, "visible tile hole", x, coverage, box)
                    editor_pixels("dark", (27, 53, 87))
                    if args.signal_term:
                        pid = int((folder / "app.pid").read_text())
                        assert Path(f"/proc/{pid}/exe").resolve() == args.executable.resolve()
                        assert str(folder / "receipt.txt").encode() in Path(f"/proc/{pid}/cmdline").read_bytes()
                        remote("send-text", "--stdin", data=b"\x1b[18~")  # F7: delayed save
                        time.sleep(.3)
                        start = time.monotonic()
                        os.kill(pid, signal.SIGTERM)
                        until(lambda: (folder / "exit-code.txt").exists(), seconds=3)
                        elapsed = time.monotonic() - start
                        value = receipt(folder / "receipt.txt")
                        assert value["exit"] == "signal:15" and value["raw_restored"] == "true", value
                        assert value["output_cleanup_complete"] == "true", value
                        assert value["file_inflight_on_exit"] == "true" and value["file_saved"] == "0", value
                        assert not (folder / "cancelled-checkpoint.json").exists()
                        assert elapsed < 1.5, elapsed
                        assert (folder / "stty-before.txt").read_bytes() == (folder / "stty-after.txt").read_bytes()
                        assert (folder / "exit-code.txt").read_text().strip() == "0"
                        results["scenarios"].append({"name": name, "status": "pass", "termination": "SIGTERM",
                            "cooperative_worker_cancelled": True, "exit_observed_s": elapsed, "receipt": value})
                        continue
                    # Locate the visible button, then send SGR mouse packets
                    # through Kitty's child PTY (and SSH in that scenario).
                    # This verifies terminal input, not physical OS mouse input.
                    def click_button(label):
                        with Image.open(folder / f"{label}.png") as screen:
                            mask = Image.new("1", screen.size)
                            mask.putdata([pixel in ((37, 99, 235), (124, 58, 237)) for pixel in screen.convert("RGB").getdata()])
                            left, top, right, bottom = mask.getbbox()
                        lines = remote("get-text").decode().splitlines()
                        rows = [line.count("\U0010eeee") for line in lines if "\U0010eeee" in line]
                        column = int((left + right) / 2 / screen.width * max(rows)) + 1
                        row = int((top + bottom) / 2 / screen.height * len(rows)) + 1
                        remote("send-text", "--stdin", data=f"\x1b[<0;{column};{row}M\x1b[<0;{column};{row}m".encode())
                    click_button("dark")
                    time.sleep(.4)
                    capture("clicked")
                    # Back-tab returns from the clicked button to the document.
                    typed = "Jadix 真实终端 🦀"
                    remote("send-text", "--stdin", data=("\x1b[Z\x01" + typed).encode())
                    time.sleep(.5)
                    setting("color-scheme", "prefer-light")
                    setting("clock-format", "12h")
                    setting("enable-animations", "false")
                    setting("text-scaling-factor", "1.25")
                    time.sleep(2)
                    remote("resize-os-window", "--unit", "pixels", "--width", "760", "--height", "560")
                    time.sleep(1)
                    capture("light")
                    editor_pixels("light", (219, 234, 254))
                    click_button("light")
                    time.sleep(.4)
                    (folder / "terminal-text.txt").write_bytes(remote("get-text"))
                    remote("send-text", "--stdin", data=b"\x11")
                    until(lambda: (folder / "exit-code.txt").exists())
                    value = receipt(folder / "receipt.txt")
                    assert value["exit"] == "quit", value
                    assert value["raw_restored"] == "true", value
                    assert int(value["frames"]) >= 3 and int(value["gpu_frames"]) >= 3, value
                    assert int(value["system_notifications"]) >= 2, value
                    assert int(value["environment_updates"]) >= 1, value
                    assert value["environment_errors"] == "0", value
                    assert bytes.fromhex(value["text_hex"]).decode() == typed, value
                    assert value["count"] == "2", "visible mouse hits did not activate button: " + str(value)
                    assert int(value["resizes"]) >= 1, value
                    prefs = json.loads(value["environment_json"])
                    assert prefs["color_scheme"] == "light" and prefs["font_scale"] == 1.25, prefs
                    assert prefs["reduced_motion"] and prefs["locale"]["hour_cycle"] == "h12", prefs
                    assert (folder / "stty-before.txt").read_bytes() == (folder / "stty-after.txt").read_bytes()
                    assert (folder / "exit-code.txt").read_text().strip() == "0"
                    with Image.open(folder / "dark.png") as dark, Image.open(folder / "light.png") as light:
                        assert dark.size != light.size, "real OS window did not resize"
                    results["scenarios"].append({"name": name, "status": "pass", "receipt": value,
                        "screenshots": {label: hashlib.sha256((folder / f"{label}.png").read_bytes()).hexdigest()
                                        for label in ("dark", "light")}})
                except Exception:
                    try:
                        (folder/"failure-terminal-text.txt").write_bytes(remote("get-text"))
                        capture("failure")
                        remote("send-text","--stdin",data=b"\x11")
                        time.sleep(.2)
                    except Exception:
                        pass
                    raise
                finally:
                    results.setdefault("scenario_observed_ms", {})[name] = (time.monotonic() - scenario_started) * 1000
                    terminal.terminate()
                    try: terminal.wait(timeout=5)
                    except sp.TimeoutExpired:
                        terminal.kill()
                        terminal.wait()
                    log.close()
                    if args.screen:
                        sp.run(["screen", "-S", screen_name, "-X", "quit"], env=dict(os.environ, SCREENDIR=str(screen_socket)), stdout=sp.DEVNULL, stderr=sp.DEVNULL)
                    if args.tmux:
                        sp.run(["tmux", "-S", str(temp / f"{name}-tmux.socket"), "kill-server"], stdout=sp.DEVNULL, stderr=sp.DEVNULL)
        finally:
            if relay is not None:
                relay.close()
                results["relay"] = relay.metrics()
            if daemon is not None:
                daemon.terminate()
                try: daemon.wait(timeout=5)
                except sp.TimeoutExpired:
                    daemon.kill()
                    daemon.wait()
            if daemon_log is not None: daemon_log.close()
            (args.directory / "acceptance.json").write_text(json.dumps(results, indent=2))
    print(json.dumps({"scenarios": [(v["name"], v["status"]) for v in results["scenarios"]]}))


if __name__ == "__main__":
    main()
