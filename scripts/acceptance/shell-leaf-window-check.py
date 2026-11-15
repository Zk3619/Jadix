#!/usr/bin/env python3
"""Verify sparse Checkbox/Switch SDK patches in the real retained Windows host."""
import argparse, ctypes as C, hashlib, json, os, subprocess as sp, tempfile, time
from ctypes import wintypes as W
from pathlib import Path
from PIL import ImageGrab

def until(fn, seconds=30):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        value=fn()
        if value:return value
        time.sleep(.05)
    raise TimeoutError('owned retained window condition')

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--executable',type=Path,required=True);p.add_argument('--bundle',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--benchmark-rows',type=int,default=32);p.add_argument('--full',action='store_true');a=p.parse_args();a.directory.mkdir(parents=True,exist_ok=False)
    u=C.WinDLL('user32',use_last_error=True);u.SetProcessDPIAware();cb=C.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
    for name,types in {'EnumWindows':[cb,W.LPARAM],'GetWindowThreadProcessId':[W.HWND,C.POINTER(W.DWORD)],'IsWindowVisible':[W.HWND],'GetClientRect':[W.HWND,C.POINTER(W.RECT)],'ClientToScreen':[W.HWND,C.POINTER(W.POINT)],'PostMessageW':[W.HWND,W.UINT,W.WPARAM,W.LPARAM],'SetForegroundWindow':[W.HWND]}.items():getattr(u,name).argtypes=types
    u.GetForegroundWindow.restype=W.HWND;receipt=a.directory/'receipt.json';result={'status':'incomplete'};prior=u.GetForegroundWindow()
    def state():
        try:return json.loads(receipt.read_text(encoding='utf-8'))
        except (FileNotFoundError,json.JSONDecodeError):return {}
    with tempfile.TemporaryDirectory(prefix='jadix-leaf-window-') as temp, (a.directory/'runtime.log').open('wb') as log:
        command=[str(a.executable.resolve()),'--bundle',str(a.bundle.resolve()),'--receipt',str(receipt.resolve()),'--benchmark-rows',str(a.benchmark_rows)]
        if not a.full:command += ['--retained','--sparse']
        (a.directory/'command.json').write_text(json.dumps(command,indent=2),encoding='utf-8')
        proc=sp.Popen(command,env=dict(os.environ,JADIX_WINDOW_STATE_PATH=str(Path(temp)/'state.toml')),stdout=log,stderr=log)
        hwnd=None
        try:
            def find():
                found=[]
                @cb
                def visit(h,_):
                    pid=W.DWORD();rect=W.RECT();u.GetWindowThreadProcessId(h,C.byref(pid));u.GetClientRect(h,C.byref(rect))
                    if pid.value==proc.pid and u.IsWindowVisible(h) and rect.right>200:found.append(h)
                    return True
                u.EnumWindows(visit,0);return found[0] if found else None
            hwnd=until(find)
            dwm=C.WinDLL('dwmapi');dwm.DwmSetWindowAttribute.argtypes=[W.HWND,W.DWORD,C.c_void_p,W.DWORD];square=C.c_int(1)
            assert dwm.DwmSetWindowAttribute(hwnd,33,C.byref(square),C.sizeof(square))==0
            until(lambda:state().get('content'));u.SetForegroundWindow(hwnd);until(lambda:u.GetForegroundWindow()==hwnd);time.sleep(.8)
            def capture(name):
                origin=W.POINT();rect=W.RECT();u.ClientToScreen(hwnd,C.byref(origin));u.GetClientRect(hwnd,C.byref(rect))
                ImageGrab.grab(bbox=(origin.x,origin.y,origin.x+rect.right,origin.y+rect.bottom)).save(a.directory/name)
            capture('initial.png')
            scale=u.GetDpiForWindow(hwnd)/96
            def click(y):
                point=round(40*scale)|(round(y*scale)<<16)
                for msg,key in [(0x200,0),(0x201,1),(0x202,0)]:assert u.PostMessageW(hwnd,msg,key,point)
            def has(value):return any(value in item for item in state().get('content',[]))
            initial_builds=sum(item['full_build'] for item in state()['updates'])
            (a.directory/'initial-receipt.json').write_text(json.dumps(state(),ensure_ascii=False,indent=2),encoding='utf-8')
            for value in [True,False,True]:
                click(8);until(lambda:has('Checkbox:'+str(value).lower()))
                click(40);until(lambda:has('Switch:'+str(value).lower()))
            current=state();updates=current['updates'];leaf=[item for item in updates if not item['full_build']]
            if not a.full:
                assert len(leaf)==6 and all(item['patched_nodes']==1 for item in leaf),updates
                assert sum(item['full_build'] for item in updates)==initial_builds,updates
                assert current['work']['leaf_patches']==6 and current['work']['full_mounts']==1,current
            assert current['work']['event_lookups']==6,current
            time.sleep(.5)
            capture('final.png')
            result={'status':'incomplete','rows':a.benchmark_rows,'full':a.full,'updates':updates,'work':current['work'],'content':current['content'],
                    'executable_sha256':hashlib.sha256(a.executable.read_bytes()).hexdigest(),'bundle_sha256':hashlib.sha256(a.bundle.read_bytes()).hexdigest(),
                    'scope':'real Windows window host CPU update; layout/collect/output/presentation timing excluded'}
            assert u.PostMessageW(hwnd,0x10,0,0);proc.wait(timeout=15);assert proc.returncode==0
            result['status']='pass'
        finally:
            if proc.poll() is None:
                if hwnd:u.PostMessageW(hwnd,0x10,0,0)
                try:proc.wait(timeout=5)
                except sp.TimeoutExpired:proc.terminate();proc.wait(timeout=5)
            if prior:u.SetForegroundWindow(prior)
            (a.directory/'acceptance.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps(result,ensure_ascii=False))

if __name__=='__main__':main()
