#!/usr/bin/env python3
"""Owned Jadix window/QuickJS integration and an optional real system IME probe.
Only this process's window receives messages. SendInput requires confirmed foreground.
Unicode SendInput verification is reported separately from system IME preedit.
"""
import argparse, ctypes as C, hashlib, json, os, subprocess as sp, tempfile, time
from ctypes import wintypes as W
from pathlib import Path
from PIL import ImageGrab

def until(fn, seconds=20):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        value=fn()
        if value:return value
        time.sleep(.05)
    raise TimeoutError('owned window acceptance condition')

def main():
    p=argparse.ArgumentParser();p.add_argument('--executable',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--bundle',type=Path);p.add_argument('--capabilities',action='store_true');p.add_argument('--retained',action='store_true');p.add_argument('--input-limits',action='store_true');p.add_argument('--unicode-probe',action='store_true');p.add_argument('--snapshot-only',action='store_true');a=p.parse_args();a.directory.mkdir(parents=True,exist_ok=False)
    u=C.WinDLL('user32',use_last_error=True);imm=C.WinDLL('imm32',use_last_error=True);u.SetProcessDPIAware()
    cb=C.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
    for name,types in {'EnumWindows':[cb,W.LPARAM],'GetWindowThreadProcessId':[W.HWND,C.POINTER(W.DWORD)],'GetClientRect':[W.HWND,C.POINTER(W.RECT)],'ClientToScreen':[W.HWND,C.POINTER(W.POINT)],'IsWindowVisible':[W.HWND],'GetDpiForWindow':[W.HWND],'PostMessageW':[W.HWND,W.UINT,W.WPARAM,W.LPARAM],'SetForegroundWindow':[W.HWND]}.items():getattr(u,name).argtypes=types
    u.GetForegroundWindow.restype=W.HWND;u.GetKeyboardLayout.argtypes=[W.DWORD];u.GetKeyboardLayout.restype=C.c_void_p
    u.GetKeyboardLayoutList.argtypes=[C.c_int,C.POINTER(C.c_void_p)];u.GetKeyboardLayoutList.restype=C.c_int
    imm.ImmGetContext.argtypes=[W.HWND];imm.ImmGetContext.restype=C.c_void_p;imm.ImmReleaseContext.argtypes=[W.HWND,C.c_void_p];imm.ImmSetOpenStatus.argtypes=[C.c_void_p,W.BOOL];imm.ImmGetOpenStatus.argtypes=[C.c_void_p]
    # INPUT has the platform-sized union, including the largest mouse member.
    class K(C.Structure):_fields_=[('vk',W.WORD),('scan',W.WORD),('flags',W.DWORD),('time',W.DWORD),('extra',C.c_size_t)]
    class M(C.Structure):_fields_=[('dx',W.LONG),('dy',W.LONG),('data',W.DWORD),('flags',W.DWORD),('time',W.DWORD),('extra',C.c_size_t)]
    class U(C.Union):_fields_=[('ki',K),('mi',M)]
    class I(C.Structure):_fields_=[('type',W.DWORD),('value',U)]
    u.SendInput.argtypes=[W.UINT,C.POINTER(I),C.c_int];u.SendInput.restype=W.UINT
    prior=u.GetForegroundWindow();result={'status':'incomplete'}
    prior_cursor=W.POINT();u.GetCursorPos(C.byref(prior_cursor));last_cursor=None
    with tempfile.TemporaryDirectory(prefix='jadix-window-sdk-') as folder:
        env=dict(os.environ,JADIX_WINDOW_STATE_PATH=str(Path(folder)/'state.toml'));receipt=a.directory/'receipt.json'
        with (a.directory/'runtime.log').open('wb') as log:
            command=[str(a.executable.resolve()),'--receipt',str(receipt.resolve())]
            if a.bundle:command += ['--bundle',str(a.bundle.resolve())]
            if a.retained:command += ['--retained']
            if a.capabilities:command += ['--width','640','--height','480']
            proc=sp.Popen(command,env=env,stdout=log,stderr=log);hwnd=None
            try:
                def find():
                    found=[]
                    @cb
                    def visit(h,_):
                        pid=W.DWORD();u.GetWindowThreadProcessId(h,C.byref(pid));r=W.RECT();u.GetClientRect(h,C.byref(r))
                        if pid.value==proc.pid and u.IsWindowVisible(h) and r.right>200 and r.bottom>100:found.append(h)
                        return True
                    u.EnumWindows(visit,0);return found[0] if found else None
                hwnd=until(find,45);until(receipt.exists,30);scale=u.GetDpiForWindow(hwnd)/96
                u.SetForegroundWindow(hwnd);until(lambda:u.GetForegroundWindow()==hwnd)
                def state():
                    try:return json.loads(receipt.read_text(encoding='utf-8'))
                    except (FileNotFoundError,json.JSONDecodeError):return {}
                def click(x,y):
                    point=round(x*scale)|(round(y*scale)<<16)
                    for msg,keys in [(0x200,0),(0x201,1),(0x202,0)]:assert u.PostMessageW(hwnd,msg,keys,point)
                    time.sleep(.25)
                def capture(name):
                    origin=W.POINT();r=W.RECT();u.ClientToScreen(hwnd,C.byref(origin));u.GetClientRect(hwnd,C.byref(r));ImageGrab.grab(bbox=(origin.x,origin.y,origin.x+r.right,origin.y+r.bottom)).save(a.directory/name)
                def stroke(vk,down=True):
                    assert u.GetForegroundWindow()==hwnd,'foreground ownership changed'
                    value=I(type=1,value=U(ki=K(vk,0,0 if down else 2,0,0)));assert u.SendInput(1,C.byref(value),C.sizeof(I))==1
                def key(vk):stroke(vk);stroke(vk,False)
                if a.snapshot_only:
                    until(lambda:bool(state().get('content')));time.sleep(.6)
                    assert u.GetForegroundWindow()==hwnd,'foreground ownership changed before capture'
                    capture('snapshot.png')
                    result.update(runtime='real Windows Jadix App / DX12 / SDK TSX / existing QuickJS',
                        snapshot_only=True,dpi_scale=scale,final=state(),
                        executable_sha256=hashlib.sha256(a.executable.read_bytes()).hexdigest(),
                        bundle_sha256=hashlib.sha256(a.bundle.read_bytes()).hexdigest())
                    assert u.PostMessageW(hwnd,0x10,0,0);proc.wait(timeout=15);assert proc.returncode==0
                    result['status']='pass'
                elif a.input_limits:
                    def has_value(value):return any(item.get('value')==value for item in state().get('observations',[]))
                    def unicode_input(text):
                        for scalar in text:
                            assert u.GetForegroundWindow()==hwnd,'foreground ownership changed'
                            data=scalar.encode('utf-16-le')
                            units=[int.from_bytes(data[at:at+2],'little') for at in range(0,len(data),2)]
                            events=(I*(2*len(units)))(*[I(type=1,value=U(ki=K(0,unit,flags,0,0))) for flags in (4,6) for unit in units])
                            assert u.SendInput(len(events),events,C.sizeof(I))==len(events)
                            time.sleep(.08)
                    time.sleep(1);capture('initial.png');click(40,18)
                    if a.unicode_probe:
                        unicode_input('🦀');until(lambda:has_value('🦀'),3)
                        result.update(unicode_probe=True,final=state())
                        assert u.PostMessageW(hwnd,0x10,0,0);proc.wait(timeout=15);assert proc.returncode==0
                        result['status']='pass';return
                    unicode_input('ab中🦀x');until(lambda:has_value('ab中🦀'))
                    first=state();capture('input-limit.png')
                    click(40,58);unicode_input('中🦀xy');until(lambda:has_value('中🦀x'))
                    notes=state();capture('textarea-limit.png')
                    click(40,164);until(lambda:any('Limit:2' in item for item in state().get('content',[])))
                    click(40,18);key(0x23);unicode_input('Z');time.sleep(.25)
                    assert has_value('ab中🦀'),'tightening must preserve over-limit draft and reject growth'
                    stroke(0x11);key(0x41);stroke(0x11,False);key(0x08)
                    until(lambda:has_value(''));unicode_input('🦀x');until(lambda:has_value('🦀'))
                    final=state();capture('dynamic-limit.png')
                    result.update(runtime='real Windows Jadix App / DX12 / SDK TSX / existing QuickJS',
                        input_utf16_limit=True,textarea_utf16_limit=True,dynamic_limit=True,
                        external_draft_preserved=True,input=first,textarea=notes,final=final,
                        keyboard='foreground-owned SendInput Unicode; no clipboard or system IME claim',
                        executable_sha256=hashlib.sha256(a.executable.read_bytes()).hexdigest(),
                        bundle_sha256=hashlib.sha256(a.bundle.read_bytes()).hexdigest())
                    assert u.PostMessageW(hwnd,0x10,0,0);proc.wait(timeout=15);assert proc.returncode==0
                    result['status']='pass'
                elif a.capabilities:
                    def has(value):return any(value in text for text in state().get('content',[]))
                    until(lambda:has('Ready'));time.sleep(1);capture('initial.png')
                    click(60,108);capture('select-open.png');key(0x28);key(0x0d);until(lambda:has('Selected:b'));capture('selected.png')
                    click(45,38);until(lambda:any(text.startswith('Popover') and 'open: true' in text for text in state().get('content',[])));capture('popover.png');key(0x1b);until(lambda:has('Popover:false'))
                    click(45,70);until(lambda:any(text.startswith('Dialog') and 'open: true' in text for text in state().get('content',[])));capture('dialog.png')
                    click(600,440);until(lambda:has('Dialog:true'))
                    click(45,70);until(lambda:any(text.startswith('Dialog') and 'open: true' in text for text in state().get('content',[])));key(0x1b);until(lambda:has('Dialog:false'))
                    click(30,143);until(lambda:any(text.startswith('Checkbox') and 'checked: true' in text for text in state().get('content',[])))
                    time.sleep(.6)
                    point=W.POINT(round(60*scale),round(235*scale));u.ClientToScreen(hwnd,C.byref(point))
                    assert u.GetForegroundWindow()==hwnd,'foreground ownership changed'
                    assert u.SetCursorPos(point.x,point.y);last_cursor=(point.x,point.y);time.sleep(.15)
                    value=I(type=0,value=U(mi=M(0,0,(-120)&0xffffffff,0x800,0,0)))
                    assert u.SendInput(1,C.byref(value),C.sizeof(I))==1
                    until(lambda:has('Scroll:0,') and not has('Scroll:0,0'),3);capture('scrolled.png')
                    result.update(runtime='real Windows Jadix App / DX12 / SDK TSX / existing QuickJS',selection=True,popover_escape=True,dialog_backdrop=True,dialog_escape=True,checkbox=True,scroll=True,final=state(),executable_sha256=hashlib.sha256(a.executable.read_bytes()).hexdigest(),bundle_sha256=hashlib.sha256(a.bundle.read_bytes()).hexdigest())
                    assert u.PostMessageW(hwnd,0x10,0,0);proc.wait(timeout=15);assert proc.returncode==0;result['status']='pass'
                else:
                    time.sleep(1);capture('initial.png');click(45,26);until(lambda:any('Count: 1' in s for s in state().get('content',[])))
                    capture('activated.png');click(65,57)
                    # Unicode packets enter the real keyboard queue; this is NOT IME.
                    # Unpaired synthetic WM_CHAR is not a complete winit keyboard event.
                    sample='Window 中文'
                    for scalar in sample:
                        assert u.GetForegroundWindow()==hwnd,'foreground ownership changed'
                        data=scalar.encode('utf-16-le')
                        units=[int.from_bytes(data[at:at+2],'little') for at in range(0,len(data),2)]
                        events=(I*(2*len(units)))(*[I(type=1,value=U(ki=K(0,unit,flags,0,0))) for flags in (4,6) for unit in units])
                        assert u.SendInput(len(events),events,C.sizeof(I))==len(events)
                        time.sleep(.02)
                    until(lambda:any(sample in s for s in state().get('content',[])))
                    unicode_state=state();capture('unicode.png')
                    stroke(0x11);key(0x41);stroke(0x11,False);key(0x08);time.sleep(.3)
                    n=u.GetKeyboardLayoutList(0,None);layouts=(C.c_void_p*n)();u.GetKeyboardLayoutList(n,layouts);chinese=next((h for h in layouts if h&0xffff==0x804),None)
                    ime={'installed_layouts':[hex(h) for h in layouts],'preedit_seen':False,'committed':False}
                    if chinese:
                        assert u.PostMessageW(hwnd,0x50,0,chinese);time.sleep(.4)
                        context=imm.ImmGetContext(hwnd)
                        if context:
                            ime['imm_open_requested']=bool(imm.ImmSetOpenStatus(context,True));ime['imm_open']=bool(imm.ImmGetOpenStatus(context));imm.ImmReleaseContext(hwnd,context)
                        for attempt in range(2):
                            if attempt:key(0x10);time.sleep(.2)
                            for ch in 'NIHAO':key(ord(ch));time.sleep(.2)
                            time.sleep(.4)
                            if state().get('preedit_seen'):
                                ime['preedit_seen']=True;capture('system-ime-preedit.png');key(0x20);time.sleep(.7);break
                            key(0x20)
                        final=state();ime['committed']=ime['preedit_seen'] and any(any(ord(c)>127 for c in x.get('value','')) and x.get('composing') is None for x in final.get('observations',[]));capture('system-ime-final.png')
                    result.update(runtime='real Windows Jadix App / DX12 / existing shell QuickJS',unicode=sample,unicode_receipt=unicode_state,system_ime=ime,final=state(),executable_sha256=hashlib.sha256(a.executable.read_bytes()).hexdigest())
                    assert u.PostMessageW(hwnd,0x10,0,0);proc.wait(timeout=15);assert proc.returncode==0,proc.returncode
                    result['status']='pass'
            finally:
                if hwnd and receipt.exists():
                    result['last_state']=state()
                    if result['status']!='pass' and u.GetForegroundWindow()==hwnd:capture('failure.png')
                if proc.poll() is None:
                    if hwnd:u.PostMessageW(hwnd,0x10,0,0)
                    try:proc.wait(timeout=5)
                    except sp.TimeoutExpired:proc.terminate();proc.wait(timeout=5)
                if prior:u.SetForegroundWindow(prior)
                current=W.POINT();u.GetCursorPos(C.byref(current))
                if last_cursor==(current.x,current.y):u.SetCursorPos(prior_cursor.x,prior_cursor.y)
                (a.directory/'acceptance.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps({'status':result['status'],'system_ime':result.get('system_ime')},ensure_ascii=False))
if __name__=='__main__':main()
