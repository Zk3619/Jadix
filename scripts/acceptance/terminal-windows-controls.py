#!/usr/bin/env python3
"""Generate Ctrl-C/Break and close only a fixture-owned hidden Win32 console."""
import argparse,ctypes as C,json,subprocess as sp,sys,time,hashlib
from ctypes import wintypes as W
from pathlib import Path

def sender(pid,signal):
 k=C.WinDLL('kernel32',use_last_error=True)
 k.FreeConsole();assert k.AttachConsole(pid),C.WinError(C.get_last_error())
 callback_type=C.WINFUNCTYPE(W.BOOL,W.DWORD)
 @callback_type
 def ignore(_):return True
 k.SetConsoleCtrlHandler.argtypes=[callback_type,W.BOOL];assert k.SetConsoleCtrlHandler(ignore,True)
 assert k.GenerateConsoleCtrlEvent(signal,0),C.WinError(C.get_last_error());time.sleep(.1);k.FreeConsole()
def main():
 p=argparse.ArgumentParser();p.add_argument('--sender',type=int);p.add_argument('--signal',type=int);p.add_argument('--executable',type=Path);p.add_argument('--directory',type=Path);a=p.parse_args()
 if a.sender:sender(a.sender,a.signal);return
 a.directory.mkdir(parents=True,exist_ok=False);results=[]
 for signal in [0,1,2,5,6]:
  path=(a.directory/f'control-{signal}.json').resolve();startup=sp.STARTUPINFO();startup.dwFlags=sp.STARTF_USESHOWWINDOW;startup.wShowWindow=0
  proc=sp.Popen([str(a.executable.resolve()),str(path)],creationflags=sp.CREATE_NEW_CONSOLE,startupinfo=startup)
  try:
   ready=Path(str(path)+'.ready');deadline=time.monotonic()+15
   while not ready.exists():
    assert time.monotonic()<deadline and proc.poll() is None,'owned console not ready';time.sleep(.03)
   identity=json.loads(ready.read_text());assert identity['pid']==proc.pid
   if signal in [5,6]:
    user=C.WinDLL('user32',use_last_error=True);callback_type=C.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
    user.EnumWindows.argtypes=[callback_type,W.LPARAM];user.GetWindowThreadProcessId.argtypes=[W.HWND,C.POINTER(W.DWORD)];user.GetClassNameW.argtypes=[W.HWND,W.LPWSTR,C.c_int];user.PostMessageW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM]
    found=[]
    @callback_type
    def visit(hwnd,_):
     pid=W.DWORD();user.GetWindowThreadProcessId(hwnd,C.byref(pid));name=C.create_unicode_buffer(256);user.GetClassNameW(hwnd,name,256)
     if pid.value==proc.pid and name.value=='JadixTerminalSessionMonitor':found.append(hwnd)
     return True
    user.EnumWindows(visit,0);assert len(found)==1,'hidden session-monitor window missing'
    # No OS shutdown/logoff is requested. Exercise only the owned fixture's
    # real Win32 message queue (also covers hosts that load user32).
    assert user.PostMessageW(found[0],0x16,1,0x80000000 if signal==5 else 0)
   elif signal==2:
    user=C.WinDLL('user32',use_last_error=True);user.PostMessageW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM];assert user.PostMessageW(identity['window'],0x10,0,0)
   else:sp.run([sys.executable,__file__,'--sender',str(proc.pid),'--signal',str(signal)],check=True,timeout=5)
   deadline=time.monotonic()+8
   while not path.exists():
    assert time.monotonic()<deadline,'UI restoration receipt missing';time.sleep(.02)
   value=json.loads(path.read_text());assert value['signal']==signal and value['restored'] and value['exact_modes'] and value['exact_codepage'],value
   proc.wait(timeout=6);results.append(dict(status='pass',signal=signal,receipt=value,exit_code=proc.returncode,delivery='owned monitor WM_ENDSESSION (simulated session message)' if signal in [5,6] else 'owned Console WM_CLOSE' if signal==2 else 'OS GenerateConsoleCtrlEvent to isolated attached console'))
  finally:
   if proc.poll() is None:proc.terminate();proc.wait(timeout=5)
   (a.directory/'acceptance.json').write_text(json.dumps({'scenarios':results,'untested':['OS logoff','OS shutdown'],'reason':'Logoff/shutdown tested through owned native window messages; no actual OS session termination requested','executable_sha256':hashlib.sha256(a.executable.read_bytes()).hexdigest()},indent=2))
 print(json.dumps({'status':'pass','real_console_events':3,'owned_session_messages':2,'cases':len(results)}))
if __name__=='__main__':main()
