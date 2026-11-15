#!/usr/bin/env python3
"""Owned WSLg Kitty windows, private nested tmux servers and two live clients.
The fixture uses explicit 128px geometry; automatic nested metrics are separate.
"""
import argparse, hashlib, io, json, os, shlex, subprocess as sp, tempfile, time
from pathlib import Path
from PIL import Image
def run(*args,**kw):return sp.run(list(map(str,args)),check=True,stdout=sp.PIPE,stderr=sp.PIPE,**kw).stdout
def until(fn,seconds=20):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        value=fn()
        if value:return value
        time.sleep(.1)
    raise TimeoutError('owned mux acceptance condition')
def read_receipt(path):
    try:return json.loads(path.read_text())
    except (FileNotFoundError,json.JSONDecodeError):return None

def main():
    p=argparse.ArgumentParser();p.add_argument('--executable',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);a=p.parse_args();a.directory.mkdir(parents=True,exist_ok=False)
    with tempfile.TemporaryDirectory(prefix='jadix-mux-') as tmp:
        tmp=Path(tmp);conf=tmp/'tmux.conf';conf.write_text('set -g status off\nset -g allow-passthrough on\nset -g default-terminal tmux-256color\n')
        outer=tmp/'outer';inner=tmp/'inner';gate=tmp/'go';receipt=a.directory/'receipt.json';processes=[];logs=[];addresses=[];result={}
        try:
            probe=[str(a.executable.resolve()),'--depth','2','--receipt',str(receipt.resolve()),'--gate',str(gate)]
            nested=['env','-u','TMUX','-u','TMUX_PANE','tmux','-u','-S',str(inner),'-f',str(conf),'new-session','-s','inner',shlex.join(probe)]
            command=['tmux','-u','-S',str(outer),'-f',str(conf),'new-session','-s','outer',shlex.join(nested)]
            for index in range(2):
                if index:until(lambda:outer.exists());command=['tmux','-u','-S',str(outer),'attach-session','-t','outer']
                socket=tmp/f'kitty{index}';address=f'unix:{socket}';addresses.append(address);log=(a.directory/f'kitty{index}.log').open('wb');logs.append(log)
                env=dict(os.environ,LIBGL_ALWAYS_SOFTWARE='1')
                process=sp.Popen(['kitty','--config','NONE','--listen-on',address,'-o','allow_remote_control=socket-only','-o','linux_display_server=x11','-o','remember_window_size=no','-o',f'initial_window_width={760+index*80}','-o','initial_window_height=520','--title',f'Jadix mux client {index}',*command],stdout=log,stderr=log,env=env);processes.append(process);until(lambda:socket.exists());until(lambda:run('kitty','@','--to',address,'ls'))
            clients=run('tmux','-S',outer,'list-clients','-F','#{client_width} #{client_height}').decode().splitlines();assert len(clients)==2,clients
            gate.write_text('go');value=until(lambda:read_receipt(receipt),30);assert value['frames']>=1 and value['replies']>=3,value
            for index,(address,process) in enumerate(zip(addresses,processes)):
                xid=until(lambda:run('xdotool','search','--pid',process.pid).strip()).splitlines()[0].decode();xwd=a.directory/f'client{index}.xwd';run('xwd','-silent','-id',xid,'-out',xwd);image=Image.open(io.BytesIO(run('xwdtopnm',xwd))).convert('RGB');image.save(a.directory/f'client{index}.png');green=sum(1 for r,g,b in image.getdata() if abs(r-23)<12 and abs(g-170)<12 and abs(b-92)<12);assert green>1000,(index,green)
                result[f'client{index}']={'green_pixels':green,'window_id':xid,'size':image.size}
            run('kitty','@','--to',addresses[0],'send-text','--stdin',input=b'\x1b[Iq');until(lambda:(read_receipt(receipt) or {}).get('status')=='closed');value=until(lambda:read_receipt(receipt));assert value['restored'] and value['error'] is None,value
            result.update(status='pass',clients=clients,depth=2,receipt=value,executable_sha256=hashlib.sha256(a.executable.read_bytes()).hexdigest(),kitty=run('kitty','--version').decode().strip(),tmux=run('tmux','-V').decode().strip(),scope='CPU RGBA KGP fanout, explicit geometry, two simultaneous real Kitty clients; no GPU or public SSH claim')
        finally:
            for socket in (inner,outer):sp.run(['tmux','-S',str(socket),'kill-server'],stdout=sp.DEVNULL,stderr=sp.DEVNULL)
            for process in processes:
                process.terminate()
                try:process.wait(timeout=5)
                except sp.TimeoutExpired:process.kill();process.wait()
            for log in logs:log.close()
            (a.directory/'acceptance.json').write_text(json.dumps(result,indent=2))
    print(json.dumps({'status':result.get('status'),'clients':2,'depth':2}))
if __name__=='__main__':main()
