#!/usr/bin/env python3
"""Pinned same-business Counter through both shipped JS runtimes, in ABBAAB order.

Runs on one Linux host after all builds stop. Source compiler setup and captures
are outside update timing; input through committed TestBackend output is inside.
"""
import argparse, hashlib, json, math, os, platform, re, signal, statistics, subprocess, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def load(path):return json.loads(path.read_text(encoding='utf8'))
def validate(data,rows):
 spec=data['spec'];warmup=spec['warmup'];actions=spec['actions']
 assert data['profile']=='release' and spec['business_rows']==rows
 assert len(data['samples_ns'])==actions and all(v>0 for v in data['samples_ns'])
 assert len(data['frames'])==warmup+actions+1
 for n,frame in enumerate(data['frames']):
  assert len(frame)==24 and frame[0].rstrip()==f'Counter {n}' and 'Increment' in frame[1]
  at=0
  for fragment in ['Unicode:','中','文','🦀','é']:
   at=frame[2].index(fragment,at)+len(fragment)
  for i in range(min(rows,21)):assert frame[i+3].rstrip()==f'Row {i}: business'
 assert len(data['snapshots'])==2 and all(len(s)==80*24 for s in data['snapshots'])
 if data['host']=='jadix-sdk':
  assert data['nodes']==rows+4
  assert data['work']['full_mounts']==1 and data['work']['leaf_patches']==warmup+actions
  assert data['layout_work']['sync_nodes']==1 and data['layout_work']['geometry_reused']
 else:assert data['idle_memory']['nodes']>=rows+4 and data['idle_memory']['js_heap']>0
 return dict(mount_ns=data['mount_ns'],median_ns=statistics.median(data['samples_ns']),p95_ns=sorted(data['samples_ns'])[math.ceil(actions*.95)-1],max_ns=max(data['samples_ns']))
def quiet_host():
 active=[]
 for proc in Path('/proc').iterdir():
  if not proc.name.isdigit():continue
  try:
   name=(proc/'comm').read_text().strip()
   if name in ('cargo','rustc'):active.append(dict(pid=int(proc.name),name=name))
  except (FileNotFoundError,PermissionError,ProcessLookupError):continue
 assert not active, ('build active during measurement',active)
 return dict(loadavg=Path('/proc/loadavg').read_text().strip(),boot=Path('/proc/sys/kernel/random/boot_id').read_text().strip())
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--nsh',type=Path,required=True);p.add_argument('--jadix',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--rows',type=int,nargs='+',default=[0,256,1024,3072]);p.add_argument('--order',default='ABBAAB',choices=['AB','ABBAAB']);a=p.parse_args()
 assert a.rows and all(0<=r<=4096 for r in a.rows) and len(set(a.rows))==len(a.rows), 'business rows must be unique, in 0..4096'
 a.directory.mkdir(parents=True,exist_ok=False)
 spec=load(ROOT/'benchmarks/terminal-sdk-counter/scene.json');source=(ROOT/'benchmarks/terminal-sdk-counter/App.tsx').read_text(encoding='utf8');assert source.count('const businessRows = 0;')==1
 result=dict(status='running',limits={'jadix':{'heap_bytes':128*1024*1024,'interpreter_stack_bytes':512*1024,'turn_milliseconds':1000},'nsh':'AppOptions::default limits: heap 256 MiB, interpreter stack 4 MiB, load 10 s, call 2 s; all limits remain active'},platform=platform.uname()._asdict(),order=a.order,nsh_sha256=sha(a.nsh),jadix_sha256=sha(a.jadix),runs=[],scope='same Counter/Unicode/static-row business, 80x24, JS runtimes on both sides; input to observed committed TestBackend output; no ANSI/emulator/scanout; process max RSS includes harness and post-timing nsh idle sample')
 for rows in a.rows:
  scene=a.directory/str(rows);scene.mkdir();task=dict(spec,business_rows=rows);(scene/'scene.json').write_text(json.dumps(task,ensure_ascii=False,indent=2)+'\n',encoding='utf8');(scene/'App.tsx').write_text(source.replace('const businessRows = 0;',f'const businessRows = {rows};'),encoding='utf8')
  for index,kind in enumerate(a.order):
   output=scene/f'{index}-{kind}.json';resources=scene/f'{index}-{kind}-resources.txt';raw=scene/f'{index}-{kind}.log'
   command=[str(a.nsh.resolve()),'--exact','canonical_sdk::sdk_counter','--nocapture','--test-threads=1'] if kind=='A' else [str(a.jadix.resolve()),str((scene/'scene.json').resolve()),str(output.resolve())]
   env=dict(os.environ,LC_ALL='C',NSH_FPS='0',JADIX_EQUIVALENT=str(scene.resolve()),JADIX_EQUIVALENT_REPORT=str(output.resolve()))
   host_before=quiet_host()
   start=time.monotonic()
   with raw.open('wb') as out:
    child=subprocess.Popen(['/usr/bin/time','-v','-o',str(resources),*command],env=env,stdout=out,stderr=subprocess.STDOUT,start_new_session=True)
    try:code=child.wait(timeout=120)
    except subprocess.TimeoutExpired:
     os.killpg(child.pid,signal.SIGTERM)
     try:child.wait(timeout=5)
     except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
     raise
   assert code==0,(command,code,raw)
   data=load(output);metrics=validate(data,rows)
   rss=re.search(r'Maximum resident set size \(kbytes\): (\d+)',resources.read_text());assert rss
   result['runs'].append(dict(rows=rows,index=index,project='nsh' if kind=='A' else 'jadix-sdk',command=command,exit=code,elapsed_s=time.monotonic()-start,host_before=host_before,host_after=quiet_host(),max_rss_kib=int(rss.group(1)),report=str(output.relative_to(a.directory)),report_sha256=sha(output),log_sha256=sha(raw),**metrics))
   (a.directory/'comparison.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(rows,index,kind,metrics,flush=True)
 result['summary']=[]
 for rows in a.rows:
  for project in ['nsh','jadix-sdk']:
   runs=[r for r in result['runs'] if r['rows']==rows and r['project']==project]
   result['summary'].append(dict(rows=rows,project=project,runs=len(runs),**{k:statistics.median(r[k] for r in runs) for k in ['mount_ns','median_ns','p95_ns','max_rss_kib']}))
 result['visible_frames_equal']=[]
 for rows in a.rows:
  runs=[r for r in result['runs'] if r['rows']==rows];left=load(a.directory/runs[0]['report']);right=load(a.directory/runs[1]['report'])
  result['visible_frames_equal'].append(dict(rows=rows,equal=left['frames']==right['frames'],scope='raw symbol rows only; equality diagnostic, not the acceptance gate'))
 result['status']='pass'
 (a.directory/'comparison.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
