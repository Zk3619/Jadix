"""Capture real Windows video composition and dismiss the OS share picker."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import time

spec = importlib.util.spec_from_file_location('desktop', Path(__file__).with_name('native-windows-desktop.py'))
desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)
gui = desktop.win32gui


def run_test(directory, output, stem, test, capture):
    executable = max((directory / 'debug/deps').glob(stem + '-*.exe'), key=lambda p: p.stat().st_mtime)
    with (output / (stem + '.log')).open('w', encoding='utf-8') as log:
        process = subprocess.Popen([str(executable), test, '--ignored', '--nocapture'], stdout=log, stderr=log)
        try:
            capture(process)
            assert process.wait(timeout=35) == 0, f'{stem} failed; see log'
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target-directory', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    def video(process):
        def capture():
            assert process.poll() is None, 'Video process exited before capture'
            window = gui.FindWindow(None, 'Jadix video desktop acceptance')
            if not window:
                return False
            nodes = desktop.descendants(window)
            surface = next((n for n in nodes if n['text'] == 'Video'), None)
            if not surface:
                return False
            left, top, right, bottom = surface['rectangle']
            image = desktop.ImageGrab.grab()
            color = image.getpixel(((left + right) // 2, top + (bottom - top) // 3))
            if color[0] < 200 or color[1] > 40 or color[2] > 40:
                return False
            image.crop(gui.GetWindowRect(window)).save(args.output / 'video-composed.png')
            (args.output / 'video-hwnds.json').write_text(json.dumps(nodes, ensure_ascii=False, indent=2), encoding='utf-8')
            return True
        desktop.wait_for(capture, timeout=20)

    def share(process):
        started = time.monotonic()
        def find():
            assert process.poll() is None, 'Share process exited before dismissal'
            window = gui.GetForegroundWindow()
            if time.monotonic() - started < 6 or gui.GetClassName(window) != 'ApplicationFrameWindow':
                return None
            left, top, right, bottom = gui.GetWindowRect(window)
            return window if 300 < right - left < 1000 and bottom - top > 400 else None
        window = desktop.wait_for(find, timeout=20)
        left, top, right, bottom = gui.GetWindowRect(window)
        desktop.ImageGrab.grab(bbox=(left, top, right, bottom)).save(args.output / 'share-picker.png')
        desktop.win32api.SetCursorPos((right - 35, top + 25))
        desktop.win32api.mouse_event(2, 0, 0)
        desktop.win32api.mouse_event(4, 0, 0)

    run_test(args.target_directory, args.output, 'windows_visual',
             'media_foundation_video_decodes_to_visible_native_surface', video)
    run_test(args.target_directory, args.output, 'windows_share',
             'system_share_picker_reports_user_dismissal', share)
    (args.output / 'acceptance.json').write_text(json.dumps({'video_composed_frame': True,
        'share_picker_user_dismissal': True, 'input': 'OS pointer; no recipient selected'}, indent=2), encoding='utf-8')


if __name__ == '__main__':
    main()
