#!/usr/bin/env python3
"""Private Linux PTY: real SDK TSX/QuickJS terminal keyboard/edit roundtrip."""
import argparse,fcntl,hashlib,json,os,pty,select,signal,struct,subprocess,termios,time
from pathlib import Path

def main():
    p=argparse.ArgumentParser();p.add_argument('--executable',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);a=p.parse_args();a.directory.mkdir(parents=True,exist_ok=False)
    master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,80,384,640));before=termios.tcgetattr(slave);receipt=a.directory/'receipt.json';output=bytearray();result={'status':'incomplete'}
    def setup():os.setsid();fcntl.ioctl(0,termios.TIOCSCTTY,0)
    proc=subprocess.Popen([str(a.executable.resolve()),'--receipt',str(receipt.resolve())],stdin=slave,stdout=slave,stderr=slave,preexec_fn=setup,env=dict(os.environ,TERM='xterm-256color',LANG='C.UTF-8'))
    def state():
        try:return json.loads(receipt.read_text())
        except (FileNotFoundError,json.JSONDecodeError):return {}
    def until(fn):
        end=time.monotonic()+12
        while time.monotonic()<end:
            if select.select([master],[],[],.05)[0]:
                try:output.extend(os.read(master,65536))
                except OSError:pass
            assert len(output)<=8*1024*1024,'PTY output budget'
            if fn():return
            if proc.poll() is not None:raise RuntimeError(('child exited',proc.returncode,output[-1024:]))
        raise TimeoutError('SDK PTY condition')
    try:
        until(lambda:state().get('phase')=='active')
        os.write(master,b'\x1b[I\x1b[<0;2;2M\x1b[<0;2;2m');until(lambda:any('Count: 1' in x for x in state().get('content',[])))
        os.write(master,b'\t');time.sleep(.1);sample='PTY 中文😀';os.write(master,b'\x1b[200~'+sample.encode()+b'\x1b[201~');until(lambda:any(sample in x for x in state().get('content',[])))
        os.write(master,b'\x11');until(lambda:state().get('phase')=='closed');proc.wait(timeout=5)
        value=state();assert proc.returncode==0 and value['restored'] and value['error'] is None,value
        assert termios.tcgetattr(slave)==before,'terminal flags not restored'
        result.update(status='pass',receipt=value,unicode=sample,controlling_pty=True,terminal_attributes_restored=True,executable_sha256=hashlib.sha256(a.executable.read_bytes()).hexdigest(),scope='real Linux controlling PTY, SDK QuickJS/Ratatui; emulator scanout excluded')
    finally:
        if proc.poll() is None:
            os.killpg(proc.pid,signal.SIGTERM)
            try:proc.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
        os.close(master);os.close(slave);(a.directory/'capture.ansi').write_bytes(output);(a.directory/'acceptance.json').write_text(json.dumps(result,ensure_ascii=False,indent=2))
    print(json.dumps({'status':result['status'],'controlling_pty':True}))
if __name__=='__main__':main()
