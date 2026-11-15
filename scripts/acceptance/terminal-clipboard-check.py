#!/usr/bin/env python3
"""Owned Xvfb clipboard + real PTY: external X11 readback, cut/paste and IO failure."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess as sp
import termios
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--executable', type=Path, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    folder = args.directory.resolve()
    folder.mkdir(parents=True, exist_ok=False)
    exe = args.executable.resolve()
    read_fd, write_fd = os.pipe()
    server_log = (folder / 'xvfb.log').open('wb')
    server = sp.Popen(['Xvfb', '-displayfd', str(write_fd), '-screen', '0', '800x600x24', '-nolisten', 'tcp', '-ac'], pass_fds=(write_fd,), stdout=server_log, stderr=server_log)
    os.close(write_fd)
    owners = []
    cases = []
    report = dict(status='incomplete', scope='private Xvfb X11 CLIPBOARD + actual controlling PTY; no desktop clipboard mutation, OSC52, emulator raster, GPU, Wayland or Apple claim', executable_sha256=sha(exe))
    try:
        assert select.select([read_fd], [], [], 10)[0], 'owned Xvfb startup timeout'
        number = os.read(read_fd, 32).decode().strip()
        assert number.isdigit(), number
        env = dict(os.environ, DISPLAY=':' + number, TERM='xterm-256color')
        env.pop('WAYLAND_DISPLAY', None)
        report['display'] = env['DISPLAY']
        report['packages'] = sp.check_output(['dpkg-query', '-W', '-f=${Package} ${Version}\n', 'xvfb', 'xclip'], text=True)

        def read_clipboard():
            return sp.check_output(['xclip', '-selection', 'clipboard', '-out', '-target', 'UTF8_STRING'], env=env, stderr=sp.PIPE, timeout=3).decode('utf-8')

        def set_clipboard(value, path):
            log = path.open('wb')
            owner = sp.Popen(['xclip', '-quiet', '-selection', 'clipboard', '-in'], stdin=sp.PIPE, stdout=log, stderr=log, env=env)
            owners.append((owner, log))
            owner.stdin.write(value.encode('utf-8'))
            owner.stdin.close()
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                try:
                    if read_clipboard() == value:
                        return
                except (sp.CalledProcessError, sp.TimeoutExpired):
                    pass
                assert owner.poll() is None, owner.returncode
                time.sleep(.03)
            raise TimeoutError('external clipboard owner')

        for name in ['system', 'memory-default', 'read-budget', 'write-failure']:
            case_folder = folder / name
            case_folder.mkdir()
            source = 'x' * (100 * 1024 + 1) if name == 'read-budget' else 'External 中文 🦀\r\nrow'
            expected = 'External 中文 🦀\nrow'
            set_clipboard(source, case_folder / 'external-owner.log')
            master, slave = pty.openpty()
            before = termios.tcgetattr(slave)
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
            os.set_blocking(master, False)
            receipt = case_folder / 'receipt.json'
            checkpoint = case_folder / 'checkpoint.json'
            stderr = (case_folder / 'stderr.log').open('wb')
            output = bytearray()
            trace = []

            def session():
                os.setsid()
                fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

            command = [str(exe), '--colors', 'mono', '--receipt', str(receipt), '--checkpoint', str(checkpoint)]
            if name != 'memory-default':
                command.append('--system-clipboard')
            app = sp.Popen(command, stdin=slave, stdout=slave, stderr=stderr, env=env, preexec_fn=session)

            def drain():
                while select.select([master], [], [], 0)[0]:
                    try:
                        data = os.read(master, 65536)
                    except BlockingIOError:
                        break
                    if not data:
                        break
                    output.extend(data)

            def current():
                try:
                    envelope = json.loads(checkpoint.read_text())
                    payload = json.loads(envelope['payload'])
                    return json.loads(payload['fields']['draft'])['current']['text']
                except (OSError, KeyError, json.JSONDecodeError):
                    return None

            def until(predicate):
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    drain()
                    if predicate():
                        return
                    assert app.poll() is None, (app.returncode, (case_folder / 'stderr.log').read_text())
                    select.select([master], [], [], .03)
                raise TimeoutError((name, current(), bytes(output)[-500:]))

            def send(label, data):
                trace.append(dict(step=label, hex=data.hex()))
                assert os.write(master, data) == len(data)

            case = dict(name=name, status='incomplete')
            try:
                until(lambda: b'portable' in output and b'row' in output)
                if name == 'read-budget':
                    send('oversized-external-paste', b'\x16')
                    expected = ''
                elif name == 'memory-default':
                    send('local-input', b'local')
                    until(lambda: current() == 'local')
                    send('copy-cut-local', b'\x01\x03\x18')
                    until(lambda: current() == '')
                    send('paste-local', b'\x16')
                    until(lambda: current() == 'local')
                    assert read_clipboard() == source
                    expected = 'local'
                else:
                    send('external-paste', b'\x16')
                    until(lambda: current() == expected)
                    send('copy-to-x11', b'\x01\x03')
                    until(lambda: read_clipboard() == expected)
                    if name == 'system':
                        send('cut-to-x11', b'\x18')
                        until(lambda: current() == '')
                        assert read_clipboard() == expected
                        send('paste-from-x11', b'\x16')
                        until(lambda: current() == expected)
                        send('undo', b'\x1a')
                        until(lambda: current() == '')
                        send('redo', b'\x19')
                        until(lambda: current() == expected)
                        for label, empty in [('empty', ''), ('filtered-controls', '\u001b\0')]:
                            set_clipboard(empty, case_folder / (label + '-owner.log'))
                            send(label + '-paste-then-undo', b'\x01\x16\x1a')
                            until(lambda: current() == '')
                            send(label + '-redo', b'\x19')
                            until(lambda: current() == expected)
                        replacement = 'Another peer 🦀'
                        set_clipboard(replacement, case_folder / 'replacement-owner.log')
                        send('external-owner-change', b'\x01\x16')
                        until(lambda: current() == replacement)
                        expected = replacement
                    else:
                        case['checkpoint_before_failure'] = sha(checkpoint)
                        server.terminate()
                        server.wait(timeout=5)
                        send('cut-after-server-disconnect', b'\x18')
                if name not in ['read-budget', 'write-failure']:
                    send('quit', b'\x11')
                app.wait(timeout=10)
                drain()
                value = json.loads(receipt.read_text())
                assert value['value'] == expected and value['raw_restored'] and value['output_cleanup_complete'], value
                assert termios.tcgetattr(slave) == before, 'PTY raw flags were not restored'
                assert current() in [None, ''] if name == 'read-budget' else current() == expected
                if name in ['read-budget', 'write-failure']:
                    assert app.returncode != 0 and value['error'], value
                    if name == 'write-failure':
                        case['checkpoint_after_failure'] = sha(checkpoint)
                        assert case['checkpoint_before_failure'] == case['checkpoint_after_failure']
                else:
                    assert app.returncode == 0 and value['error'] is None, value
                case.update(status='pass', exit_code=app.returncode, receipt=value, pty_restored=True, retained_text=expected)
            finally:
                if app.poll() is None:
                    app.terminate()
                    try:
                        app.wait(timeout=5)
                    except sp.TimeoutExpired:
                        app.kill()
                        app.wait()
                drain()
                case['trace'] = trace
                (case_folder / 'output.ansi').write_bytes(output)
                (case_folder / 'acceptance.json').write_text(json.dumps(case, ensure_ascii=False, indent=2))
                cases.append(case)
                stderr.close()
                os.close(master)
                os.close(slave)
        report.update(status='pass', cases=cases)
    finally:
        for owner, log in owners:
            if owner.poll() is None:
                owner.terminate()
            try:
                owner.wait(timeout=3)
            except sp.TimeoutExpired:
                owner.kill()
                owner.wait()
            log.close()
        if server.poll() is None:
            server.terminate()
            server.wait(timeout=5)
        server_log.close()
        os.close(read_fd)
        report['cases'] = cases
        (folder / 'acceptance.json').write_text(json.dumps(report, ensure_ascii=False, indent=2))
    print(json.dumps(dict(status=report['status'], cases=len(cases), backend='owned X11 + PTY')))

if __name__ == '__main__':
    main()
