#!/usr/bin/env python3
"""Own one XTerm window and verify the current CPU character app's real IO."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess as sp
import tempfile
import time


def run(*args):
    return sp.check_output(list(map(str,args)),stderr=sp.STDOUT,timeout=15)


def until(fn):
    deadline=time.monotonic()+20
    while time.monotonic()<deadline:
        result=fn()
        if result:return result
        time.sleep(.1)
    raise TimeoutError('owned XTerm condition')


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--terminal',type=Path,required=True)
    p.add_argument('--executable',type=Path,required=True)
    p.add_argument('--directory',type=Path,required=True)
    p.add_argument('--colors',choices=['rgb','256','16','mono'],default='rgb')
    p.add_argument('--label',default='XTerm',help='Observed emulator identity, e.g. rxvt-unicode')
    p.add_argument('--version-argument',default='-version')
    p.add_argument('--version-exit-code',type=int,default=0)
    a=p.parse_args();a.directory=a.directory.resolve();a.directory.mkdir(parents=True,exist_ok=False)
    terminal=a.terminal.resolve();executable=a.executable.resolve();folder=a.directory
    report={'status':'incomplete','colors':a.colors,'scope':f'real {a.label}/WSLg, CPU CellSurface; no Kitty/GPU/SSH/IME claim'}
    proc=None
    try:
        with tempfile.TemporaryDirectory(prefix='jadix-xterm-') as tmp, (folder/'runtime.log').open('wb') as log:
            script=Path(tmp)/'run.sh';receipt=folder/'receipt.json'
            script.write_text('stty -g > '+shlex.quote(str(folder/'stty-before.txt'))+'\n'+
                shlex.join([str(executable),'--auto-layout','--colors',a.colors,'--receipt',str(receipt)])+'\nstatus=$?\n'+
                'stty -g > '+shlex.quote(str(folder/'stty-after.txt'))+'\n'+
                'printf "%s" "$status" > '+shlex.quote(str(folder/'exit-code.txt'))+'\n')
            proc=sp.Popen([str(terminal),'-geometry','80x24','-title','Jadix owned XTerm','-e','/bin/sh',str(script)],stdout=log,stderr=log)
            def window():
                value=sp.run(['xdotool','search','--pid',str(proc.pid)],capture_output=True,timeout=3)
                return value.stdout.strip().splitlines()[0].decode() if value.returncode==0 else None
            xid=until(window);run('xdotool','windowfocus','--sync',xid);time.sleep(.7)
            run('xwd','-silent','-id',xid,'-out',folder/'initial.xwd')
            until(lambda:(folder/'stty-before.txt').exists())
            run('xdotool','key','--window',xid,'ctrl+a');time.sleep(.2)
            run('xdotool','type','--window',xid,'--clearmodifiers','--delay','20','xterm draft')
            run('xdotool','key','--window',xid,'Tab','Return');time.sleep(.3)
            run('xdotool','key','--window',xid,'F3');time.sleep(.3)
            run('xdotool','key','--window',xid,'Tab','Escape');time.sleep(.3)
            run('xdotool','key','--window',xid,'Return');time.sleep(.3)
            run('xdotool','windowsize',xid,'760','560');time.sleep(.5)
            run('xwd','-silent','-id',xid,'-out',folder/'updated.xwd')
            try:
                run('xdotool','key','--window',xid,'ctrl+q')
            except sp.CalledProcessError as error:
                # Some emulators destroy their window on key-down before
                # xdotool sends key-up. Require the app's final receipt and
                # clean child exit below; no missing target is accepted alone.
                if not receipt.exists():
                    raise
                report['exit_key_delivery_note']=error.output.decode(errors='replace')
            until(receipt.exists);proc.wait(timeout=15)
            value=json.loads(receipt.read_text())
            assert proc.returncode==0 and (folder/'exit-code.txt').read_text()=='0'
            assert value['count']==2 and value['value']=='xterm draft',value
            assert value['resizes']>=1 and value['raw_restored'] and value['output_cleanup_complete'],value
            assert (folder/'stty-before.txt').read_bytes()==(folder/'stty-after.txt').read_bytes()
            version=sp.run([str(terminal),a.version_argument],stdout=sp.PIPE,stderr=sp.STDOUT,timeout=15)
            assert version.returncode==a.version_exit_code and version.stdout
            report.update(status='pass',receipt=value,terminal_version=version.stdout.decode().splitlines()[0],version_exit_code=version.returncode,
                executable_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
                terminal_sha256=hashlib.sha256(terminal.read_bytes()).hexdigest())
    finally:
        if proc and proc.poll() is None:
            proc.terminate()
            try:proc.wait(timeout=5)
            except sp.TimeoutExpired:proc.kill();proc.wait()
        (folder/'acceptance.json').write_text(json.dumps(report,indent=2))
    print(json.dumps(report))


if __name__=='__main__':main()
