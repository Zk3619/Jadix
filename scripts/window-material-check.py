"""Windows Gallery regression for Fluent's composed client-area background.

Build with `cargo build -p gallery --features runtime-logging`, then run this
script. Requires Pillow for window-only PNG evidence. Uses an isolated window
state file and drives the real theme buttons; it never changes DWM attributes.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import winreg

from PIL import ImageGrab


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, default=Path('target/debug/gallery.exe'))
    parser.add_argument('--output', type=Path, default=Path('.tmp/window-material'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    user = C.WinDLL('user32', use_last_error=True)
    dwm = C.WinDLL('dwmapi')
    user.SetProcessDPIAware()
    callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    user.EnumWindows.argtypes = [callback, W.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    user.GetWindowTextLengthW.argtypes = user.IsWindowVisible.argtypes = [W.HWND]
    user.GetClientRect.argtypes = user.GetWindowRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
    user.ClientToScreen.argtypes = [W.HWND, C.POINTER(W.POINT)]
    user.GetDpiForWindow.argtypes = [W.HWND]
    user.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
    user.SendMessageTimeoutW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM,
                                        W.UINT, W.UINT, C.POINTER(C.c_size_t)]
    user.GetCursorPos.argtypes = [C.POINTER(W.POINT)]
    cursor = W.POINT()
    assert user.GetCursorPos(C.byref(cursor))
    user.SetWindowPos.argtypes = [W.HWND, W.HWND, C.c_int, C.c_int, C.c_int, C.c_int, W.UINT]
    dwm.DwmGetWindowAttribute.argtypes = [W.HWND, W.DWORD, C.c_void_p, W.DWORD]
    rows = []
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER,
                        r'Software\Microsoft\Windows\CurrentVersion\Themes\Personalize') as key:
        system_dark = not bool(winreg.QueryValueEx(key, 'AppsUseLightTheme')[0])

    def windows(pid):
        result = []
        @callback
        def visit(hwnd, _):
            owner = W.DWORD()
            user.GetWindowThreadProcessId(hwnd, C.byref(owner))
            rect = W.RECT()
            user.GetClientRect(hwnd, C.byref(rect))
            if (owner.value == pid and user.IsWindowVisible(hwnd)
                    and user.GetWindowTextLengthW(hwnd) and rect.right > 200 and rect.bottom > 200):
                result.append(hwnd)
            return True
        user.EnumWindows(visit, 0)
        return result

    def attribute(hwnd, key):
        value = C.c_int()
        hr = dwm.DwmGetWindowAttribute(hwnd, key, C.byref(value), C.sizeof(value))
        assert hr == 0, (key, hr)
        return value.value

    with tempfile.TemporaryDirectory(prefix='jadix-material-') as folder:
        env = os.environ.copy()
        env['JADIX_WINDOW_STATE_PATH'] = str(Path(folder) / 'state.toml')
        env['JADIX_GALLERY_SECTION'] = 'theme'
        env['RUST_LOG'] = 'info,wgpu_core=warn,wgpu_hal=warn,fontdb=error'
        with (args.output / 'runtime.log').open('w', encoding='utf-8') as log:
            proc = subprocess.Popen([str(args.exe.resolve())], env=env, stdout=log, stderr=log)
            hwnd = None
            try:
                deadline = time.monotonic() + 25
                while not hwnd and time.monotonic() < deadline:
                    assert proc.poll() is None, f'Gallery exited: {proc.returncode}'
                    found = windows(proc.pid)
                    if found:
                        assert len(found) == 1, found
                        hwnd = found[0]
                    else:
                        time.sleep(.05)
                assert hwnd, 'Gallery did not show a window'
                scale = user.GetDpiForWindow(hwnd) / 96

                def resize(width):
                    outer, client = W.RECT(), W.RECT()
                    user.GetWindowRect(hwnd, C.byref(outer))
                    user.GetClientRect(hwnd, C.byref(client))
                    assert user.SetWindowPos(hwnd, -1, 40, 40,
                                             round(width * scale) + outer.right - outer.left - client.right,
                                             round(570 * scale) + outer.bottom - outer.top - client.bottom,
                                             0x40)
                    time.sleep(.7)

                def click(x, y):
                    point = round(x * scale) | (round(y * scale) << 16)
                    for message, keys in [(0x200, 0), (0x201, 1), (0x202, 0)]:
                        assert user.PostMessageW(hwnd, message, keys, point)
                    time.sleep(.9)

                def check(name):
                    assert windows(proc.pid) == [hwnd], 'Theme switch created another main window'
                    dark = bool(attribute(hwnd, 20))
                    outer, client = W.RECT(), W.RECT()
                    user.GetWindowRect(hwnd, C.byref(outer))
                    user.GetClientRect(hwnd, C.byref(client))
                    origin = W.POINT()
                    user.ClientToScreen(hwnd, C.byref(origin))
                    user.SetCursorPos(outer.left + 200, outer.top + 12)
                    user.PostMessageW(hwnd, 0x2a3, 0, 0)
                    time.sleep(.3)
                    image = ImageGrab.grab(bbox=(outer.left, outer.top, outer.right, outer.bottom))
                    image.save(args.output / f'{name}.png')
                    assert attribute(hwnd, 38) == 2, 'Fluent must select DWMSBT_MAINWINDOW'
                    # Sample blank areas away from text and the telemetry HUD.
                    pixel = image.getpixel((origin.x - outer.left + client.right - round(35 * scale),
                                            origin.y - outer.top + round(65 * scale)))[:3]
                    passed = min(pixel) > 5 and (max(pixel) < 120 if dark else min(pixel) > 180)
                    surfaces = {
                        slot: image.getpixel((origin.x - outer.left + round(sample_x * scale),
                                              origin.y - outer.top + round(sample_y * scale)))[:3]
                        for slot, sample_x, sample_y in [('sidebar', 50, 70), ('card', 310, 100)]
                    }
                    surfaces_passed = all(
                        min(rgb) > 5 and (max(rgb) < 120 if dark else min(rgb) > 225)
                        for rgb in surfaces.values()
                    )
                    contrast_passed = (all(4 <= card - sidebar <= 24
                        for card, sidebar in zip(surfaces['card'], surfaces['sidebar']))
                        if dark else min(surfaces['card']) < 253)
                    row = dict(phase=name, hwnd=hwnd, dark=dark, header_rgb=pixel,
                               surfaces_rgb=surfaces,
                               passed=passed and surfaces_passed and contrast_passed)
                    rows.append(row)
                    print(json.dumps(row), flush=True)
                    assert passed, 'Fluent header must expose its material instead of black'
                    assert surfaces_passed, 'Fluent surfaces must retain alpha instead of sampling opaque black'
                    assert contrast_passed, 'Cards must have a subtle material contrast without becoming solid white'
                    return dark

                def settings_changed():
                    setting = C.create_unicode_buffer('ImmersiveColorSet')
                    result = C.c_size_t()
                    assert user.SendMessageTimeoutW(hwnd, 0x1a, 0,
                        C.cast(setting, C.c_void_p).value, 2, 2000, C.byref(result))
                    time.sleep(.5)

                def check_control_styles(name):
                    outer = W.RECT()
                    user.GetWindowRect(hwnd, C.byref(outer))
                    origin = W.POINT()
                    user.ClientToScreen(hwnd, C.byref(origin))
                    def sample(image, x, y):
                        return image.getpixel((origin.x - outer.left + round(x * scale),
                                               origin.y - outer.top + round(y * scale)))[:3]
                    user.PostMessageW(hwnd, 0x2a3, 0, 0)
                    time.sleep(.3)
                    image = ImageGrab.grab(bbox=(outer.left, outer.top, outer.right, outer.bottom))
                    selected = sample(image, 200, 465)
                    card = sample(image, 310, 100)
                    outline = sample(image, 330, 204)
                    surrounding = sample(image, 330, 194)
                    image.save(args.output / f'{name}-rest.png')
                    point = round(200 * scale) | (round(465 * scale) << 16)
                    user.PostMessageW(hwnd, 0x200, 0, point)
                    time.sleep(.4)
                    image = ImageGrab.grab(bbox=(outer.left, outer.top, outer.right, outer.bottom))
                    hovered = sample(image, 200, 465)
                    image.save(args.output / f'{name}-hover.png')
                    passed = (all(abs(left - right) <= 5 for left, right in zip(selected, card))
                        and all(abs(left - right) <= 2 for left, right in zip(outline, surrounding))
                        and all(hover >= rest for hover, rest in zip(hovered, selected))
                        and any(hover > rest for hover, rest in zip(hovered, selected)))
                    row = dict(phase=name, selected_rgb=selected, card_rgb=card,
                               outline_rgb=outline, surrounding_rgb=surrounding,
                               hovered_rgb=hovered, passed=passed)
                    rows.append(row)
                    print(json.dumps(row), flush=True)
                    assert passed, 'Outline must be clear; sidebar selection must match cards and brighten on hover'

                resize(840)
                click(790, 376)
                assert bool(attribute(hwnd, 20)) == system_dark, 'Startup must follow the system theme'
                click(560, 228)
                assert check('fluent-startup-system') == system_dark
                click(330, 496)
                first_mode = check('fluent-first')
                assert not first_mode, 'Light preference must resolve to light'
                check_control_styles('fluent-light-controls')
                resize(860)
                check('fluent-resized')
                resize(840)
                click(380, 496)
                assert check('fluent-toggle-mode') != first_mode, 'Theme mode button was not exercised'
                check_control_styles('fluent-dark-controls')
                click(330, 496)
                assert check('fluent-restored-mode') == first_mode
                click(450, 496)
                assert check('fluent-system') == system_dark, 'System preference must read the current OS theme'
                settings_changed()
                assert check('fluent-system-event') == system_dark
                click(330 if system_dark else 380, 496)
                settings_changed()
                assert check('fluent-manual-ignores-system') != system_dark
                click(330, 496)
                point = W.POINT(round(500 * scale), round(300 * scale))
                user.ClientToScreen(hwnd, C.byref(point))
                packed = (point.x & 0xffff) | ((point.y & 0xffff) << 16)
                for delta in [-360, 360]:
                    assert user.PostMessageW(hwnd, 0x20a, (delta & 0xffff) << 16, packed)
                    time.sleep(.9)
                check('fluent-scroll-restored')
                click(375, 214)  # shadcn
                assert attribute(hwnd, 38) == 1, 'shadcn must remove the system material'
                click(560, 228)
                check('fluent-reentered')
                click(630, 255)  # Liquid Glass
                assert attribute(hwnd, 38) == 1, 'Custom Acrylic must clear the system Mica backdrop'
                click(560, 214)
                check('fluent-after-acrylic')
            finally:
                user.SetCursorPos(cursor.x, cursor.y)
                (args.output / 'result.json').write_text(json.dumps(rows, indent=2), encoding='utf-8')
                if hwnd:
                    user.PostMessageW(hwnd, 0x10, 0, 0)
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()


if __name__ == '__main__':
    main()
