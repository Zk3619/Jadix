"""Compare current full and sparse SDK updates in owned, real Windows App windows."""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess as sp
import tempfile
import time
from PIL import ImageGrab, ImageChops


def until(fn):
    end = time.monotonic() + 45
    while time.monotonic() < end:
        value = fn()
        if value:
            return value
        time.sleep(.05)
    raise TimeoutError('owned window condition')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--executable', type=Path, required=True)
    p.add_argument('--bundle', type=Path, required=True)
    p.add_argument('--directory', type=Path, required=True)
    p.add_argument('--rows', type=int, nargs='+', default=[32, 256, 1024, 3072])
    a = p.parse_args()
    a.directory.mkdir(parents=True, exist_ok=False)
    u = C.WinDLL('user32', use_last_error=True)
    u.SetProcessDPIAware()
    callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    for name, types in {'EnumWindows':[callback,W.LPARAM], 'GetWindowThreadProcessId':[W.HWND,C.POINTER(W.DWORD)],
                        'GetClientRect':[W.HWND,C.POINTER(W.RECT)], 'ClientToScreen':[W.HWND,C.POINTER(W.POINT)],
                        'IsWindowVisible':[W.HWND], 'GetDpiForWindow':[W.HWND],
                        'PostMessageW':[W.HWND,W.UINT,W.WPARAM,W.LPARAM], 'SetForegroundWindow':[W.HWND]}.items():
        getattr(u, name).argtypes = types
    summaries = []
    for rows in a.rows:
        captures = {}
        for sparse in [False, True]:
            folder = a.directory / f'{rows}-{"sparse" if sparse else "full"}'
            folder.mkdir()
            receipt = folder / 'receipt.json'
            def state():
                try:
                    return json.loads(receipt.read_text(encoding='utf-8'))
                except (FileNotFoundError, json.JSONDecodeError):
                    return {}
            with tempfile.TemporaryDirectory(prefix='jadix-owned-window-') as temp, (folder/'runtime.log').open('wb') as log:
                cmd = [str(a.executable.resolve()), '--bundle', str(a.bundle.resolve()), '--receipt', str(receipt.resolve()),
                       '--benchmark-rows', str(rows)]
                if sparse:
                    cmd += ['--retained', '--sparse']
                proc = sp.Popen(cmd, env=dict(os.environ, JADIX_WINDOW_STATE_PATH=str(Path(temp)/'state.toml')), stdout=log, stderr=log)
                hwnd = None
                try:
                    def find():
                        found = []
                        @callback
                        def visit(h, _):
                            pid = W.DWORD()
                            u.GetWindowThreadProcessId(h, C.byref(pid))
                            bounds = W.RECT(); u.GetClientRect(h,C.byref(bounds))
                            if pid.value == proc.pid and u.IsWindowVisible(h) and bounds.right > 200 and bounds.bottom > 100:
                                found.append(h)
                            return True
                        u.EnumWindows(visit, 0)
                        return found[0] if found else None
                    hwnd = until(find)
                    # DWM rounded client corners expose the desktop at slightly
                    # different positions. Disable rounding on these owned test
                    # windows so the complete client pixels remain comparable.
                    dwm = C.WinDLL('dwmapi')
                    dwm.DwmSetWindowAttribute.argtypes = [W.HWND,W.DWORD,C.c_void_p,W.DWORD]
                    square = C.c_int(1)
                    assert dwm.DwmSetWindowAttribute(hwnd,33,C.byref(square),C.sizeof(square)) == 0
                    until(lambda:state().get('updates'))
                    u.SetForegroundWindow(hwnd)
                    time.sleep(.8)
                    origin = W.POINT(); rect = W.RECT()
                    u.ClientToScreen(hwnd,C.byref(origin)); u.GetClientRect(hwnd,C.byref(rect))
                    ImageGrab.grab(bbox=(origin.x,origin.y,origin.x+rect.right,origin.y+rect.bottom)).save(folder/'initial.png')
                    scale = u.GetDpiForWindow(hwnd)/96
                    point = round(40*scale) | (round(12*scale)<<16)
                    initial_builds = sum(x['full_build'] for x in state()['updates'])
                    samples = []
                    for count in range(1, 8):
                        before = len(state()['updates'])
                        for msg, key in [(0x200,0),(0x201,1),(0x202,0)]:
                            assert u.PostMessageW(hwnd,msg,key,point)
                        until(lambda:any(f'Row 0: {count}' in text for text in state().get('content',[])))
                        current = state()
                        if count > 1:  # exclude initial template-path/cache admission
                            samples.extend(x['elapsed_ms'] for x in current['updates'][before:])
                        time.sleep(.12)
                    final = state()
                    if sparse:
                        assert sum(x['full_build'] for x in final['updates']) == initial_builds, final
                        assert final['work']['full_mounts'] == 1 and final['work']['leaf_patches'] == 7
                        assert all(x['patched_nodes'] == 1 for x in final['updates'] if not x['full_build'])
                    time.sleep(.5)
                    origin = W.POINT()
                    rect = W.RECT()
                    u.ClientToScreen(hwnd,C.byref(origin)); u.GetClientRect(hwnd,C.byref(rect))
                    capture = ImageGrab.grab(bbox=(origin.x,origin.y,origin.x+rect.right,origin.y+rect.bottom)).convert('RGB')
                    capture.save(folder/'final.png')
                    captures[sparse] = capture
                    summaries.append({'rows':rows,'sparse':sparse,'samples_ms':samples,'median_ms':statistics.median(samples),
                                      'work':final['work'],'initial_builds':initial_builds,'full_builds':sum(x['full_build'] for x in final['updates'])})
                    assert u.PostMessageW(hwnd,0x10,0,0)
                    proc.wait(timeout=20)
                    assert proc.returncode == 0
                finally:
                    if proc.poll() is None:
                        if hwnd:
                            u.PostMessageW(hwnd,0x10,0,0)
                        try:
                            proc.wait(timeout=8)
                        except sp.TimeoutExpired:
                            proc.terminate();proc.wait(timeout=8)
        difference = ImageChops.difference(captures[False],captures[True])
        difference.save(a.directory/f'{rows}-difference.png')
        assert difference.getbbox() is None, f'client output mismatch at {rows}'
    result = {'status':'pass','scope':'real Windows App/OS click queue; CPU update time only, no input-to-present latency',
              'same_client_pixels':True,'samples':summaries,'executable_sha256':hashlib.sha256(a.executable.read_bytes()).hexdigest(),
              'bundle_sha256':hashlib.sha256(a.bundle.read_bytes()).hexdigest()}
    (a.directory/'acceptance.json').write_text(json.dumps(result,indent=2))
    print(json.dumps(result))


if __name__ == '__main__':
    main()
