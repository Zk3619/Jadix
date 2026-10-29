"""Native Windows Gallery acceptance: mobile sidebar, resizing, caption and startup.

Run against a freshly built Gallery. Screenshots use Pillow; window operations and
startup observations use Win32. A separate state file preserves the user's layout.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import subprocess
import threading
import time
from PIL import ImageGrab, ImageChops, ImageStat


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, default=Path('target/debug/gallery.exe'))
    parser.add_argument('--output', type=Path, default=Path('.tmp/responsive-native/acceptance'))
    parser.add_argument('--verify', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    u = C.WinDLL('user32', use_last_error=True)
    u.SetProcessDPIAware()
    callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    signatures = {
        'EnumWindows': [callback, W.LPARAM],
        'GetWindowThreadProcessId': [W.HWND, C.POINTER(W.DWORD)],
        'IsWindowVisible': [W.HWND], 'GetWindowTextLengthW': [W.HWND],
        'GetClientRect': [W.HWND, C.POINTER(W.RECT)],
        'GetWindowRect': [W.HWND, C.POINTER(W.RECT)],
        'PostMessageW': [W.HWND, W.UINT, W.WPARAM, W.LPARAM],
        'SetWindowPos': [W.HWND, W.HWND, C.c_int, C.c_int, C.c_int, C.c_int, W.UINT],
        'SetForegroundWindow': [W.HWND], 'ShowWindow': [W.HWND, C.c_int],
        'ClientToScreen': [W.HWND, C.POINTER(W.POINT)], 'GetDpiForWindow': [W.HWND],
        'GetWindowLongW': [W.HWND, C.c_int],
        'AdjustWindowRectExForDpi': [C.POINTER(W.RECT), W.DWORD, W.BOOL, W.DWORD, W.UINT],
    }
    for name, signature in signatures.items():
        getattr(u, name).argtypes = signature
    state = args.output.resolve() / 'window-state.toml'
    state.write_text('[windows.main]\nwidth = 700\nheight = 650\nx = 40\ny = 40\nmaximized = false\n', encoding='utf-8')
    env = os.environ.copy()
    env.update(JADIX_WINDOW_STATE_PATH=str(state), RUST_LOG='off', JADIX_GALLERY_SECTION='basics')
    results = {'startup': [], 'sidebar': [], 'caption': {}}
    log = (args.output / 'gallery.log').open('w', encoding='utf-8')
    proc = subprocess.Popen([str(args.exe.resolve())], env=env, stdout=log, stderr=log)
    hwnd = None
    resize_thread = None
    stop = threading.Event()

    def visit(h, _):
        nonlocal hwnd
        pid = W.DWORD()
        u.GetWindowThreadProcessId(h, C.byref(pid))
        if pid.value == proc.pid and u.IsWindowVisible(h) and u.GetWindowTextLengthW(h):
            hwnd = h
        return True

    cb = callback(visit)
    def client_size():
        r = W.RECT()
        u.GetClientRect(hwnd, C.byref(r))
        return [r.right, r.bottom]

    def resize(width, height=600):
        dpi = u.GetDpiForWindow(hwnd)
        scale = dpi / 96
        r = W.RECT(0, 0, round(width * scale), round(height * scale))
        assert u.AdjustWindowRectExForDpi(C.byref(r), u.GetWindowLongW(hwnd, -16), False,
                                         u.GetWindowLongW(hwnd, -20), dpi)
        u.SetWindowPos(hwnd, None, 40, 40, r.right-r.left, r.bottom-r.top, 0x4)
        return scale

    def capture(name):
        r = W.RECT()
        u.GetWindowRect(hwnd, C.byref(r))
        ImageGrab.grab(bbox=(r.left, r.top, r.right, r.bottom)).save(args.output / f'{name}.png')
        pt = W.POINT(0, 0)
        u.ClientToScreen(hwnd, C.byref(pt))
        w, h = client_size()
        im = ImageGrab.grab(bbox=(pt.x, pt.y, pt.x+w, pt.y+h))
        scale = u.GetDpiForWindow(hwnd) / 96
        return im.resize((round(w/scale), round(h/scale)))

    def click(x, y):
        scale = u.GetDpiForWindow(hwnd) / 96
        x, y = round(x*scale), round(y*scale)
        u.PostMessageW(hwnd, 0x200, 0, (y << 16) | x)
        u.PostMessageW(hwnd, 0x201, 1, (y << 16) | x)
        u.PostMessageW(hwnd, 0x202, 0, (y << 16) | x)
        time.sleep(.65)

    def escape():
        scan = u.MapVirtualKeyW(0x1B, 0)
        u.PostMessageW(hwnd, 0x100, 0x1B, 1 | (scan << 16))
        u.PostMessageW(hwnd, 0x101, 0x1B, 0xC0000001 | (scan << 16))
        time.sleep(.65)

    def header_difference(first, second):
        # Static Gallery title; avoid button hover and the changing performance HUD.
        a, b = first.crop((70, 2, 275, 48)), second.crop((70, 2, 275, 48))
        return sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3

    try:
        deadline = time.monotonic() + 20
        while not hwnd and time.monotonic() < deadline:
            assert proc.poll() is None, f'Gallery exited {proc.returncode}'
            u.EnumWindows(cb, 0)
            time.sleep(.002)
        assert hwnd, 'No visible Gallery window'
        start = time.monotonic()
        for _ in range(120):
            size = client_size()
            if not results['startup'] or results['startup'][-1]['size'] != size:
                results['startup'].append(dict(t=time.monotonic()-start, size=size))
            time.sleep(.005)
        assert all(s['size'] == [700, 650] for s in results['startup']), results['startup']
        u.ShowWindow(hwnd, 9)
        u.SetForegroundWindow(hwnd)
        time.sleep(.25)
        for width in (320, 640, 767):
            resize(width)
            time.sleep(.5)
            closed = capture(f'{width}-closed')
            click(30, 20)
            opened = capture(f'{width}-open')
            # A fully open Sheet exposes the sharp, dark close glyph at x=256.
            glyph = opened.crop((246, 18, 266, 38)).convert('RGB')
            ink = sum(max(rgb) < 110 for rgb in glyph.get_flattened_data())
            click(256, 28)
            closed_again = capture(f'{width}-close')
            difference = header_difference(closed, closed_again)
            row = dict(width=width, close_glyph_pixels=ink, close_header_difference=difference)
            results['sidebar'].append(row)
            if args.verify:
                assert ink >= 10, f'Sheet is not fully inside viewport: {row}'
                assert difference < 3, f'Close did not dismiss the scrim: {row}'
            click(30, 20)
            escape()
            escaped = capture(f'{width}-escape')
            row['escape_header_difference'] = header_difference(closed_again, escaped)
            if args.verify:
                assert row['escape_header_difference'] < 3, f'Esc failed: {row}'
            click(30, 20)
            click(width-10, 180)
            backdrop = capture(f'{width}-backdrop')
            row['backdrop_header_difference'] = header_difference(escaped, backdrop)
            if args.verify:
                assert row['backdrop_header_difference'] < 3, f'Backdrop dismissal failed: {row}'
        resize(700)
        time.sleep(.35)
        click(30, 20)
        resize(768)
        time.sleep(.6)
        capture('768-desktop-after-open-mobile')
        resize(1024)
        time.sleep(.5)
        capture('1024-desktop')
        # Capture the native caption while another thread repeatedly resizes the
        # same HWND. The title's pixels should stay stable at a fixed position.
        r = W.RECT()
        u.GetWindowRect(hwnd, C.byref(r))
        caption_box = (r.left+45, r.top+10, r.left+200, r.top+34)
        baseline = ImageGrab.grab(bbox=caption_box)
        def stress():
            for i in range(80):
                if stop.is_set():
                    break
                resize(600 + (i % 20)*28)
                time.sleep(.012)
        resize_thread = threading.Thread(target=stress)
        resize_thread.start()
        changes = []
        for i in range(90):
            image = ImageGrab.grab(bbox=caption_box)
            delta = sum(ImageStat.Stat(ImageChops.difference(baseline, image)).mean) / 3
            changes.append(delta)
            if delta > 2:
                image.save(args.output / f'caption-change-{i}.png')
            time.sleep(.005)
        resize_thread.join()
        results['caption'] = dict(samples=len(changes), max_difference=max(changes), changed_samples=sum(v > 2 for v in changes))
        resize(640)
        time.sleep(.5)
        capture('640-after-resize-stress')
        if args.verify:
            assert results['caption']['max_difference'] < 3, results['caption']
        print(json.dumps(results), flush=True)
    finally:
        stop.set()
        if resize_thread:
            resize_thread.join(timeout=3)
        if hwnd:
            u.PostMessageW(hwnd, 0x10, 0, 0)
        try:
            proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        log.close()
        (args.output / 'result.json').write_text(json.dumps(results, indent=2), encoding='utf-8')


if __name__ == '__main__':
    main()
