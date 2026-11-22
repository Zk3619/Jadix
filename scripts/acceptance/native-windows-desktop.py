"""Exercise visible native windows with OS input and retain desktop evidence."""
import argparse
from contextlib import contextmanager
import ctypes as C
from ctypes import wintypes as W
import json
from pathlib import Path
import subprocess
import time
import traceback

user = C.windll.user32
user.SetProcessDpiAwarenessContext(C.c_void_p(-4))
import win32api
import win32con
import win32gui
import win32process
from PIL import ImageGrab, ImageChops


class Keyboard(C.Structure):
    _fields_ = [('vk', W.WORD), ('scan', W.WORD), ('flags', W.DWORD),
                ('time', W.DWORD), ('extra', C.c_size_t)]


class Mouse(C.Structure):
    _fields_ = [('x', W.LONG), ('y', W.LONG), ('data', W.DWORD),
                ('flags', W.DWORD), ('time', W.DWORD), ('extra', C.c_size_t)]


class Payload(C.Union):
    _fields_ = [('keyboard', Keyboard), ('mouse', Mouse)]


class Input(C.Structure):
    _fields_ = [('kind', W.DWORD), ('payload', Payload)]


def unicode_input(text):
    raw = text.replace('\r\n', '\n').replace('\n', '\r').encode('utf-16-le')
    events = []
    for i in range(0, len(raw), 2):
        unit = int.from_bytes(raw[i:i + 2], 'little')
        for flags in (4, 6):
            events.append(Input(1, Payload(keyboard=Keyboard(0, unit, flags, 0, 0))))
    array = (Input * len(events))(*events)
    assert user.SendInput(len(events), array, C.sizeof(Input)) == len(events), 'SendInput failed'


def key(vk):
    win32api.keybd_event(vk, 0, 0, 0)
    win32api.keybd_event(vk, 0, 2, 0)


def replace_text(handle, text):
    click(handle)
    win32api.keybd_event(win32con.VK_CONTROL, 0, 0, 0)
    key(ord('A'))
    win32api.keybd_event(win32con.VK_CONTROL, 0, 2, 0)
    unicode_input(text)
    time.sleep(.2)


def window_text(handle):
    buffer = C.create_unicode_buffer(65536)
    win32gui.SendMessage(handle, win32con.WM_GETTEXT, len(buffer), C.addressof(buffer))
    return buffer.value


def has_text(window, text):
    return any(n['visible'] and n['text'] == text for n in descendants(window))


def editor(window):
    return next(n['handle'] for n in descendants(window)
                if n['class'] == 'Edit' and n['visible'] and n['enabled']
                and n['rectangle'][2] > n['rectangle'][0] and n['rectangle'][3] > n['rectangle'][1])


def foreground(window):
    current = C.windll.kernel32.GetCurrentThreadId()
    active = win32process.GetWindowThreadProcessId(win32gui.GetForegroundWindow())[0]
    attached = active != current and user.AttachThreadInput(current, active, True)
    try:
        win32gui.BringWindowToTop(window)
        win32gui.SetForegroundWindow(window)
    finally:
        if attached:
            user.AttachThreadInput(current, active, False)
    assert win32gui.GetForegroundWindow() == window, 'Could not foreground owned window'


def descendants(window):
    result = []
    def visit(child, _):
        result.append({'handle': child, 'class': win32gui.GetClassName(child),
                       'text': win32gui.GetWindowText(child),
                       'rectangle': win32gui.GetWindowRect(child),
                       'visible': bool(win32gui.IsWindowVisible(child)),
                       'enabled': bool(win32gui.IsWindowEnabled(child))})
    win32gui.EnumChildWindows(window, visit, None)
    return result


def wait_for(predicate, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(.04)
    raise AssertionError('Desktop condition timed out')


def click(handle):
    x, y, r, b = win32gui.GetWindowRect(handle)
    win32api.SetCursorPos(((x + r) // 2, (y + b) // 2))
    win32api.mouse_event(2, 0, 0)
    win32api.mouse_event(4, 0, 0)
    time.sleep(.15)


def named_button(window, label):
    for node in descendants(window):
        if node['visible'] and node['enabled'] and node['text'].removeprefix('● ') == label:
            handle = node['handle']
            while handle != window:
                if win32gui.GetClassName(handle).lower() == 'button':
                    return handle
                handle = win32gui.GetParent(handle)
    raise AssertionError(f'Missing button: {label}')


def snapshot(window, output, name):
    nodes = descendants(window)
    (output / f'{name}-hwnds.json').write_text(json.dumps(nodes, ensure_ascii=False, indent=2), encoding='utf-8')
    ImageGrab.grab(bbox=win32gui.GetWindowRect(window)).save(output / f'{name}.png')
    subprocess.run(['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                    str(Path(__file__).with_name('native-windows-uia.ps1')),
                    '-WindowHandle', str(window), '-Output', str(output / f'{name}-uia.json')],
                   check=True, capture_output=True, timeout=20)
    return nodes


@contextmanager
def application(exe, output, arguments=()):
    with (output / f'{exe.stem}.log').open('w', encoding='utf-8') as log:
        process = subprocess.Popen([str(exe), *map(str, arguments)], stdout=log, stderr=log)
        def find():
            assert process.poll() is None, f'{exe.name} exited: {process.returncode}'
            found = []
            win32gui.EnumWindows(lambda w, _: found.append(w) if win32process.GetWindowThreadProcessId(w)[1] == process.pid
                                 and win32gui.IsWindowVisible(w) and win32gui.GetClassName(w).startswith('Jadix') else None, None)
            return found[0] if found else None
        window = None
        try:
            window = wait_for(find, 30)
            foreground(window)
            time.sleep(.3)
            yield process, window
        except Exception:
            if window and win32gui.IsWindow(window):
                snapshot(window, output, f'{exe.stem}-failure')
            raise
        finally:
            if window and win32gui.IsWindow(window):
                win32gui.PostMessage(window, win32con.WM_CLOSE, 0, 0)
            try:
                process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)


def controls(exe, output):
    with application(exe, output) as (process, window):
        snapshot(window, output, 'controls-initial')
        click(named_button(window, 'Continue'))
        wait_for(lambda: any(n['text'].startswith('Count: 1 /') for n in descendants(window)))
        edit = next(n['handle'] for n in descendants(window) if n['class'] == 'Edit')
        click(edit)
        value = '桌面验收 中文 😀'
        unicode_input(value)
        wait_for(lambda: any(n['text'] == f'Count: 1 / Name: {value}' for n in descendants(window)))
        key(win32con.VK_RETURN)
        wait_for(lambda: any(n['text'] == f'Count: 2 / Name: {value}' for n in descendants(window)))
        key(win32con.VK_TAB)
        key(win32con.VK_SPACE)
        wait_for(lambda: any(n['text'] == 'Count: 2 / Name: ' for n in descendants(window)))
        scroll = next(n['handle'] for n in descendants(window) if win32gui.GetWindowLong(n['handle'], -16) & 0x200000)
        x, y, r, b = win32gui.GetWindowRect(scroll)
        before = next(n['rectangle'][1] for n in descendants(window) if n['text'].startswith('Row 001'))
        win32api.SetCursorPos(((x + r) // 2, (y + b) // 2))
        win32api.mouse_event(win32con.MOUSEEVENTF_WHEEL, 0, 0, -360)
        time.sleep(.4)
        after = next(n['rectangle'][1] for n in descendants(window) if n['text'].startswith('Row 001'))
        assert after < before, 'OS wheel did not scroll native children'
        win32api.SetCursorPos((r + 20, b + 20))
        time.sleep(.1)
        first = ImageGrab.grab(bbox=win32gui.GetWindowRect(window))
        win32gui.RedrawWindow(window, None, None, 0x185)
        time.sleep(.15)
        second = ImageGrab.grab(bbox=win32gui.GetWindowRect(window))
        first.save(output / 'controls-scroll.png')
        second.save(output / 'controls-scroll-forced-repaint.png')
        assert ImageChops.difference(first, second).getbbox() is None, 'Scroll left stale pixels'
        box = win32gui.GetWindowRect(window)
        win32gui.SetWindowPos(window, 0, box[0], box[1], 980, 760, 0x14)
        time.sleep(.3)
        snapshot(window, output, 'controls-resized')
        win32gui.ShowWindow(window, win32con.SW_MINIMIZE)
        time.sleep(.15)
        win32gui.ShowWindow(window, win32con.SW_RESTORE)
        foreground(window)
        time.sleep(.2)
        start = win32process.GetProcessTimes(process._handle)
        time.sleep(2)
        end = win32process.GetProcessTimes(process._handle)
        cpu = sum(end[k] - start[k] for k in ('KernelTime', 'UserTime'))
        assert cpu < .15, f'Idle CPU exceeded budget: {cpu}'
        return {'pointer_callback': True, 'unicode_input': value, 'enter_submit': True,
                'tab_space_reset': True, 'wheel_pixels': before - after,
                'scroll_repaint_stable': True, 'resize_restore': True, 'idle_cpu_seconds_2s': cpu}


def interactive(scenario, exe, output):
    extra = ['--workspace', output / f'{scenario}-interactive-workspace'] if scenario in ('tsx', 'code', 'dock') else []
    with application(exe, output, extra) as (process, window):
        snapshot(window, output, f'{scenario}-initial')
        if scenario == 'tabs':
            value = '保留页签数据 😀'
            replace_text(editor(window), value)
            click(named_button(window, 'Password'))
            wait_for(lambda: has_text(window, 'Password settings panel'))
            assert not any(n['class'] == 'Edit' for n in descendants(window))
            key(win32con.VK_HOME)
            restored = wait_for(lambda: next((n['handle'] for n in descendants(window) if n['class'] == 'Edit'), None))
            assert window_text(restored) == value, repr(window_text(restored))
            key(win32con.VK_RIGHT)
            wait_for(lambda: has_text(window, 'Password settings panel'))
            receipt = {'inactive_panel_unmounted': True, 'draft_restored': value, 'home_arrow_navigation': True}
        elif scenario == 'radio':
            standard, express = (named_button(window, label) for label in ('Standard', 'Express'))
            click(express)
            wait_for(lambda: win32gui.SendMessage(express, 0xF0, 0, 0) == 1)
            key(win32con.VK_RIGHT)
            wait_for(lambda: win32gui.SendMessage(standard, 0xF0, 0, 0) == 1)
            assert win32gui.SendMessage(express, 0xF0, 0, 0) == 0
            receipt = {'pointer_selection': True, 'arrow_wrap_skips_disabled': True}
        elif scenario == 'collapsible':
            click(named_button(window, 'Account details'))
            first = wait_for(lambda: next((n['handle'] for n in descendants(window) if n['class'] == 'Edit'), None))
            replace_text(first, '折叠状态 😀')
            click(named_button(window, 'Preferences'))
            wait_for(lambda: has_text(window, 'Only one account panel is open.'))
            assert not win32gui.IsWindow(first)
            click(named_button(window, 'Account details'))
            restored = wait_for(lambda: next((n['handle'] for n in descendants(window) if n['class'] == 'Edit'), None))
            assert window_text(restored) == '折叠状态 😀'
            receipt = {'exclusive_disclosure': True, 'draft_restored': True}
        elif scenario == 'gpu-chart':
            replace_text(editor(window), '.75')
            wait_for(lambda: has_text(window, 'Sine amplitude 0.75; 512 GPU vertices; 0 resets'))
            snapshot(window, output, 'gpu-chart-updated')
            replace_text(editor(window), 'NaN')
            wait_for(lambda: any(n['text'].startswith('Invalid amplitude') for n in descendants(window)))
            click(named_button(window, 'Reset amplitude'))
            wait_for(lambda: has_text(window, 'Sine amplitude 0.50; 512 GPU vertices; 1 resets'))
            receipt = {'edit_updates_dx12_plot': True, 'invalid_value_rejected': True, 'reset': True}
        elif scenario == 'tsx':
            value = 'Shell 桌面恢复 😀'
            original = editor(window)
            replace_text(original, value)
            wait_for(lambda: has_text(window, f'Business value: {value}'))
            click(named_button(window, 'Count 0'))
            wait_for(lambda: has_text(window, 'Native Popover · Count 1'))
            assert editor(window) == original, 'Keyed Shell editor HWND changed'
            key(win32con.VK_ESCAPE)
            wait_for(lambda: not has_text(window, 'Native Popover · Count 1'))
            time.sleep(.5)
            receipt = {'native_edit_to_tsx': value, 'keyed_editor_retained': True, 'escape_dismiss': True}
        elif scenario == 'tree':
            def status():
                return next(n['text'] for n in descendants(window) if ' items; ' in n['text'])
            click(named_button(window, 'Last'))
            wait_for(lambda: 'focused Some(100000)' in status())
            key(win32con.VK_RIGHT)
            wait_for(lambda: status().startswith('101002 items;'))
            key(win32con.VK_RIGHT)
            wait_for(lambda: 'focused Some(200000)' in status())
            assert '1 selected' in status(), status()
            snapshot(window, output, 'tree-lazy-child')
            key(win32con.VK_LEFT)
            wait_for(lambda: 'focused Some(100000)' in status())
            key(win32con.VK_LEFT)
            wait_for(lambda: '100002 visible;' in status())
            key(win32con.VK_HOME)
            wait_for(lambda: 'focused Some(0)' in status())
            assert len(descendants(window)) < 300, 'Tree mounted the whole model'
            receipt = {'model_items': 101002, 'keyboard_lazy_load': True,
                       'collapse_and_home': True, 'bounded_child_windows': len(descendants(window))}
        elif scenario == 'code':
            original_box = win32gui.GetWindowRect(window)
            for window_width in (720, 480, original_box[2] - original_box[0]):
                win32gui.SetWindowPos(window, 0, 0, 0, window_width,
                                      original_box[3] - original_box[1],
                                      win32con.SWP_NOMOVE | win32con.SWP_NOZORDER)
                time.sleep(.2)
                left, top = win32gui.ClientToScreen(window, (0, 0))
                _, _, client_width, client_height = win32gui.GetClientRect(window)
                for label in ('Find next', 'Save source', 'Cancel syntax', 'Reset example'):
                    x, y, right, bottom = win32gui.GetWindowRect(named_button(window, label))
                    assert left <= x < right <= left + client_width, (label, window_width, x, right)
                    assert top <= y < bottom <= top + client_height, (label, window_width, y, bottom)
                if window_width == 480:
                    snapshot(window, output, 'code-narrow-toolbar')
            value = '// 桌面代码恢复 😀\r\nfn main() { println!("中文 😀"); }\r\n'
            source = max((n for n in descendants(window) if n['class'] == 'Edit'),
                         key=lambda n: n['rectangle'][3] - n['rectangle'][1])['handle']
            replace_text(source, value)
            wait_for(lambda: any(n['text'].startswith('Syntax revision ') for n in descendants(window)))
            search = next(n['handle'] for n in descendants(window) if n['class'] == 'Edit' and n['handle'] != source)
            replace_text(search, '中文 😀')
            click(named_button(window, 'Find next'))
            wait_for(lambda: any(n['text'].startswith('Match at UTF-16 ') for n in descendants(window)))
            selection = win32gui.SendMessage(source, 0xB0, 0, 0)
            assert ((selection >> 16) & 0xFFFF) - (selection & 0xFFFF) == 5, selection
            click(named_button(window, 'Save source'))
            wait_for(lambda: has_text(window, 'Source saved'))
            click(named_button(window, 'Save source'))
            wait_for(lambda: has_text(window, 'Source saved'))
            click(named_button(window, 'Cancel syntax'))
            wait_for(lambda: has_text(window, 'Syntax cancelled; source retained'))
            assert window_text(source) == value, repr(window_text(source))
            receipt = {'unicode_source': value, 'utf16_search_selection': True,
                       'repeated_atomic_save': True, 'cancel_retains_source': True,
                       'toolbar_buttons_inside_client_at_widths': [720, 480, original_box[2] - original_box[0]]}
        elif scenario == 'dock':
            value = 'Dock 桌面恢复 😀'
            click(named_button(window, 'main.rs'))
            original = editor(window)
            replace_text(original, value)
            click(named_button(window, 'Split right'))
            assert win32gui.IsWindow(original) and window_text(original) == value
            click(named_button(window, 'Float pane'))
            def floating():
                found = []
                win32gui.EnumWindows(lambda w, _: found.append(w) if w != window
                                     and win32process.GetWindowThreadProcessId(w)[1] == process.pid
                                     and win32gui.IsWindowVisible(w)
                                     and win32gui.GetClassName(w).startswith('Jadix') else None, None)
                return found[0] if found else None
            other = wait_for(floating)
            snapshot(other, output, 'dock-floating')
            assert any(window_text(n['handle']) == value for n in descendants(other) if n['class'] == 'Edit')
            win32gui.PostMessage(other, win32con.WM_CLOSE, 0, 0)
            wait_for(lambda: not floating())
            foreground(window)
            assert any(window_text(n['handle']) == value for n in descendants(window) if n['class'] == 'Edit')
            click(named_button(window, 'Save workspace'))
            wait_for(lambda: any(n['text'].startswith('Workspace saved ·') for n in descendants(window)))
            receipt = {'split_retains_editor': True, 'floating_native_window': True,
                       'close_redocks_content': True, 'workspace_value': value}
        elif scenario == 'template':
            click(named_button(window, 'Continue'))
            wait_for(lambda: has_text(window, 'Count: 1'))
            key(win32con.VK_SPACE)
            wait_for(lambda: has_text(window, 'Count: 2'))
            receipt = {'external_template_pointer_keyboard': True}
        snapshot(window, output, f'{scenario}-accepted')
    if scenario == 'tsx':
        with application(exe, output, extra) as (_, window):
            wait_for(lambda: has_text(window, 'Count 1') and has_text(window, f'Business value: {value}'))
            assert window_text(editor(window)) == value
            snapshot(window, output, 'tsx-recovered')
            receipt['independent_process_restore'] = True
    elif scenario in ('code', 'dock'):
        with application(exe, output, extra) as (_, window):
            if scenario == 'dock':
                click(named_button(window, 'main.rs'))
            wait_for(lambda: any(window_text(n['handle']) == value for n in descendants(window) if n['class'] == 'Edit'))
            snapshot(window, output, f'{scenario}-recovered')
            receipt['independent_process_restore'] = True
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-directory', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenario', action='append', default=[])
    parser.add_argument('--template-executable', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    scenarios = args.scenario or ['controls', 'workbench', 'inspector', 'table', 'tree', 'images',
                                  'dock', 'tabs', 'radio', 'collapsible', 'gpu-chart', 'code', 'tsx']
    report = {'input': 'OS SendInput / mouse / keyboard; visible desktop', 'scenarios': []}
    for scenario in scenarios:
        try:
            if scenario == 'controls':
                receipt = controls(args.bin_directory / 'native-windows.exe', args.output)
            elif scenario in ['tabs', 'radio', 'collapsible', 'gpu-chart', 'tsx', 'template', 'tree', 'code', 'dock']:
                exe = args.template_executable if scenario == 'template' else args.bin_directory / f'native-{scenario}.exe'
                receipt = interactive(scenario, exe, args.output)
            else:
                extra = ['--workspace', args.output / 'shell-workspace'] if scenario == 'tsx' else []
                with application(args.bin_directory / f'native-{scenario}.exe', args.output, extra) as (_, window):
                    nodes = snapshot(window, args.output, scenario)
                    receipt = {'visible': True, 'child_windows': len(nodes), 'title': win32gui.GetWindowText(window)}
            report['scenarios'].append({'name': scenario, 'pass': True, **receipt})
        except Exception as error:
            report['scenarios'].append({'name': scenario, 'pass': False, 'error': str(error), 'traceback': traceback.format_exc()})
        (args.output / 'desktop-report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
        print(json.dumps(report['scenarios'][-1], ensure_ascii=False), flush=True)
    assert all(row['pass'] for row in report['scenarios']), 'Desktop acceptance failed; see report'


if __name__ == '__main__':
    main()
