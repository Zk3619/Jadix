"""Record the production Gallery during Fluent hover, HUD, scroll and window reentry."""

import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from PIL import Image, ImageFilter, ImageGrab


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, default=Path('target/debug/gallery.exe'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--frames', type=int, default=540)
    parser.add_argument('--scenario', choices=['fluent', 'joined-groups', 'optical-edge'], default='fluent')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    user = ctypes.WinDLL('user32', use_last_error=True)
    user.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user.GetWindowRect.argtypes = user.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    user.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
    user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    user.GetDpiForWindow.argtypes = [wintypes.HWND]
    user.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.UINT]
    cursor = wintypes.POINT()
    user.GetCursorPos(ctypes.byref(cursor))
    records = []
    with tempfile.TemporaryDirectory(prefix='jadix-fluent-frames-') as temporary:
        environment = os.environ.copy()
        environment['JADIX_WINDOW_STATE_PATH'] = str(Path(temporary) / 'window.toml')
        environment['JADIX_GALLERY_SECTION'] = 'theme'
        with (args.output / 'runtime.log').open('w', encoding='utf-8') as log:
            process = subprocess.Popen([str(args.exe.resolve())], env=environment, stdout=log, stderr=log)
            handle = None
            try:
                deadline = time.monotonic() + 30
                while handle is None and time.monotonic() < deadline:
                    assert process.poll() is None, f'Gallery exited: {process.returncode}'
                    @callback_type
                    def visit(candidate, _):
                        nonlocal handle
                        owner = wintypes.DWORD()
                        user.GetWindowThreadProcessId(candidate, ctypes.byref(owner))
                        client = wintypes.RECT()
                        user.GetClientRect(candidate, ctypes.byref(client))
                        if owner.value == process.pid and user.IsWindowVisible(candidate) and client.right > 300:
                            handle = candidate
                        return True
                    user.EnumWindows(visit, 0)
                    time.sleep(.05)
                assert handle, 'Gallery did not open'
                scale = user.GetDpiForWindow(handle) / 96

                def geometry():
                    outer, client = wintypes.RECT(), wintypes.RECT()
                    origin = wintypes.POINT()
                    user.GetWindowRect(handle, ctypes.byref(outer))
                    user.GetClientRect(handle, ctypes.byref(client))
                    user.ClientToScreen(handle, ctypes.byref(origin))
                    return outer, client, origin

                def resize(width, height):
                    outer, client, _ = geometry()
                    assert user.SetWindowPos(handle, -1, 40, 40,
                        round(width * scale) + outer.right - outer.left - client.right,
                        round(height * scale) + outer.bottom - outer.top - client.bottom, 0x40)

                def click(horizontal, vertical, settle=.2):
                    _, _, origin = geometry()
                    user.SetForegroundWindow(handle)
                    user.SetCursorPos(round(origin.x + horizontal * scale), round(origin.y + vertical * scale))
                    time.sleep(.08)
                    user.mouse_event(0x2, 0, 0, 0, 0)
                    time.sleep(.04)
                    user.mouse_event(0x4, 0, 0, 0, 0)
                    if settle:
                        time.sleep(settle)

                resize(840, 570)
                user.SetForegroundWindow(handle)
                time.sleep(.8)
                click(560, 228, .7)
                click(380, 496, .7)
                if args.scenario in ('joined-groups', 'optical-edge'):
                    click(790, 376)
                    _, _, origin = geometry()
                    packed = (round(origin.x + 500 * scale) & 0xffff) | ((round(origin.y + 300 * scale) & 0xffff) << 16)
                    user.PostMessageW(handle, 0x20a, ((-1200 & 0xffff) << 16), packed)
                    time.sleep(.8)
                    if args.scenario == 'joined-groups':
                        user.PostMessageW(handle, 0x20a, ((-360 & 0xffff) << 16), packed)
                        time.sleep(.8)
                        click(510, 310, .4)
                started = time.perf_counter()
                seam_samples = []
                seam_failures = []
                optical_reference = None
                optical_failures = []
                for frame in range(args.frames):
                    phase = (frame // 60) % 9
                    step = frame % 60
                    if args.scenario == 'optical-edge':
                        phase = frame // 30
                        point = [[450, 278], [385, 278], [329.2, 278], [328.5, 278],
                            [329.2 if frame % 2 else 328.5, 278],
                            [385, 263.2 if frame % 2 else 262.5],
                            [439.2 if frame % 2 else 438.5, 278],
                            [385, 295.2 if frame % 2 else 294.5],
                            [450, 278], [385, 278]][phase % 10]
                    elif args.scenario == 'joined-groups':
                        phase = (frame // 30) % 10
                        point = [[650, 400], [510, 273], [510, 310], [510, 347],
                            [350, 273], [510, 273], [510, 347], [510, 310],
                            [475 + (step % 24) * .1, 310], [-4, 310]][phase]
                        if phase == 9 and frame % 30 >= 15:
                            point = [510, 310]
                        if phase in (5, 6, 7) and frame % 30 == 0:
                            click(*point, settle=0)
                    elif phase == 0:
                        point = [495 + (step % 24) * .25, 376 + (step % 8) * .25]
                    elif phase == 1:
                        point = [85, 105 + (step % 44) * 8]
                    elif phase == 2:
                        point = [842 if step < 24 else 835 - (step - 24) * 9, 280]
                    elif phase == 3:
                        point = [780, 360]
                        if step in (0, 30):
                            click(790, 376)
                    elif phase == 4:
                        point = [500, 300]
                        if step in (0, 30):
                            _, _, origin = geometry()
                            packed = (round(origin.x + point[0] * scale) & 0xffff) | ((round(origin.y + point[1] * scale) & 0xffff) << 16)
                            delta = -360 if step == 0 else 360
                            assert user.PostMessageW(handle, 0x20a, (delta & 0xffff) << 16, packed)
                    elif phase == 5:
                        point = [-4 if step < 24 else 4 + (step - 24) * 7, 280]
                        if step in (0, 30):
                            resize(860 if step == 0 else 840, 570)
                    else:
                        point = [500, 300]
                        if phase == 6 and step == 0:
                            click(790, 376)
                            _, _, origin = geometry()
                            packed = (round(origin.x + 500 * scale) & 0xffff) | ((round(origin.y + 300 * scale) & 0xffff) << 16)
                            user.PostMessageW(handle, 0x20a, ((-720 & 0xffff) << 16), packed)
                        if phase == 6 and step in (15, 30, 45):
                            outer, _, origin = geometry()
                            screen = ImageGrab.grab(bbox=(outer.left, outer.top, outer.right, outer.bottom)).convert('RGB')
                            grid_rows = [vertical for vertical in range(screen.height)
                                if sum(1 for horizontal in range(round(300 * scale), screen.width)
                                    if (lambda pixel: pixel[2] > pixel[0] + 70 and pixel[1] > pixel[0] + 30)(screen.getpixel((horizontal, vertical)))) > 300]
                            delta = -120 if not grid_rows or max(grid_rows) > 440 * scale else 120 if min(grid_rows) < 180 * scale else 0
                            if delta:
                                packed = (round(origin.x + 500 * scale) & 0xffff) | ((round(origin.y + 300 * scale) & 0xffff) << 16)
                                user.PostMessageW(handle, 0x20a, ((delta & 0xffff) << 16), packed)
                        if phase == 7 and step == 0:
                            outer, _, origin = geometry()
                            screen = ImageGrab.grab(bbox=(outer.left, outer.top, outer.right, outer.bottom)).convert('RGB')
                            grid_rows = [vertical for vertical in range(screen.height)
                                if sum(1 for horizontal in range(round(300 * scale), screen.width)
                                    if (lambda pixel: pixel[2] > pixel[0] + 70 and pixel[1] > pixel[0] + 30)(screen.getpixel((horizontal, vertical)))) > 300]
                            assert grid_rows, 'Gallery optical grid must be visible'
                            trigger_vertical = (max(grid_rows) + outer.top - origin.y) / scale - 3
                            assert 0 < trigger_vertical < 550, 'Optical trigger must be inside Gallery'
                            click(385, trigger_vertical)
                        if phase == 8 and step == 0:
                            user.PostMessageW(handle, 0x100, 0x1b, 0)
                            user.PostMessageW(handle, 0x101, 0x1b, 0)
                    outer, client, origin = geometry()
                    user.SetCursorPos(round(origin.x + point[0] * scale), round(origin.y + point[1] * scale))
                    image = ImageGrab.grab(bbox=(outer.left, outer.top, outer.right, outer.bottom))
                    image.save(args.output / f'frame-{frame:04}.png')
                    if args.scenario == 'optical-edge':
                        crop = image.crop((origin.x - outer.left + round(333 * scale),
                            origin.y - outer.top + round(267 * scale),
                            origin.x - outer.left + round(435 * scale),
                            origin.y - outer.top + round(291 * scale))).convert('RGB')
                        mask = Image.frombytes('L', crop.size, bytes(
                            255 if blue > red + 70 and green > red + 30 else 0
                            for red, green, blue in crop.getdata()))
                        if optical_reference is None:
                            optical_reference = mask.filter(ImageFilter.MaxFilter(5))
                        extra = sum(bool(value) and not prior
                            for value, prior in zip(mask.getdata(), optical_reference.getdata()))
                        if extra > 16:
                            optical_failures.append(dict(frame=frame, unexpected_grid_pixels=extra))
                    if args.scenario == 'joined-groups':
                        horizontal = origin.x - outer.left + round(480 * scale)
                        if frame == 0:
                            for vertical in (291, 327):
                                expected = origin.y - outer.top + round(vertical * scale)
                                candidates = range(expected - round(4 * scale), expected + round(4 * scale) + 1)
                                neutral = [row for row in candidates
                                    if max(image.getpixel((horizontal, row))[:3])
                                    - min(image.getpixel((horizontal, row))[:3]) < 10]
                                assert neutral, 'ToggleGroup connecting border must be visible before capture'
                                row = max(neutral, key=lambda row: min(image.getpixel((horizontal, row))[:3]))
                                assert min(image.getpixel((horizontal, row))[:3]) > 60
                                seam_samples.extend((column, row, image.getpixel((column, row)))
                                    for column in range(horizontal, horizontal + round(45 * scale)))
                        difference = max(abs(left - right) for column, row, reference in seam_samples
                            for left, right in zip(reference, image.getpixel((column, row))))
                        if difference > 3:
                            seam_failures.append(dict(frame=frame, maximum_channel_difference=difference))
                    records.append(dict(frame=frame, seconds=time.perf_counter() - started,
                        phase=phase, pointer=point, client=[client.right, client.bottom]))
                    time.sleep(max(0, started + (frame + 1) / 30 - time.perf_counter()))
                contact = Image.new('RGB', (1260, 300 * ((args.frames + 59) // 60)))
                for index, frame in enumerate(range(15, args.frames, 30)):
                    image = Image.open(args.output / f'frame-{frame:04}.png').convert('RGB')
                    image.thumbnail((630, 300))
                    contact.paste(image, ((index % 2) * 630, (index // 2) * 300))
                contact.save(args.output / 'contact.png')
                if args.scenario == 'optical-edge':
                    (args.output / 'optical-check.json').write_text(json.dumps(dict(
                        frames=len(records), corrupted_frames=len(optical_failures),
                        failures=optical_failures), indent=2), encoding='utf-8')
                    assert not optical_failures, f'{len(optical_failures)} Gallery frames spread grid color across the button'
                if args.scenario == 'joined-groups':
                    (args.output / 'seam-check.json').write_text(json.dumps(dict(
                        frames=len(records), changed_seam_frames=len(seam_failures),
                        samples=[[column, row] for column, row, _ in seam_samples],
                        failures=seam_failures), indent=2), encoding='utf-8')
                    assert not seam_failures, f'{len(seam_failures)} Gallery frames recolored the connecting borders'
            finally:
                user.SetCursorPos(cursor.x, cursor.y)
                (args.output / 'frames.json').write_text(json.dumps(records, indent=2), encoding='utf-8')
                if handle:
                    user.PostMessageW(handle, 0x10, 0, 0)
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
    print(f'Captured {len(records)} complete Gallery screen frames: {args.output.resolve()}')


if __name__ == '__main__':
    main()
