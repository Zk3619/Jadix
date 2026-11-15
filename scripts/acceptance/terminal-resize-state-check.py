#!/usr/bin/env python3
"""Owned ConPTY: resize wrapped Scroll while keeping Unicode editor history."""
import argparse,hashlib,importlib.util,json,time
from pathlib import Path
spec=importlib.util.spec_from_file_location('conpty',Path(__file__).with_name('terminal-conpty-check.py'))
conpty=importlib.util.module_from_spec(spec);spec.loader.exec_module(conpty)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--executable',type=Path,required=True);p.add_argument('--bundle',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--expect-root-sync',action='store_true',help='require retained viewport synchronization in actual resize receipts');args=p.parse_args()
 args.directory.mkdir(parents=True,exist_ok=False);cases=[]
 for run in range(2):
  folder=args.directory/str(run);folder.mkdir();receipt=(folder/'receipt.json').resolve()
  console=conpty.Console([args.executable.resolve(),'--bundle',args.bundle.resolve(),'--receipt',receipt,'--receipt-cells'])
  steps=[];case=dict(status='incomplete',run=run)
  def state():
   try:return json.loads(receipt.read_text(encoding='utf-8'))
   except(FileNotFoundError,json.JSONDecodeError):return {}
  def until(test):
   end=time.monotonic()+10
   while time.monotonic()<end:
    s=state()
    if test(s):return s
    time.sleep(.03)
   raise AssertionError({k:v for k,v in state().items() if k!='cells'})
  def save(name,s):
   (folder/(name+'.json')).write_text(json.dumps(s,ensure_ascii=False,indent=2),encoding='utf-8');steps.append(dict(name=name,frames=s['frames'],revision=s['revision'],viewport=s['viewport']))
  def resize(width):
   before=state()['frames'];assert console.resize(console.pc,conpty.Coord(width,12))==0
   s=until(lambda s:s.get('frames',0)>before and s.get('viewport')==[width,12] and s.get('cells') is not None)
   if args.expect_root_sync:assert s['layout_work']['sync_nodes']==1 and s['layout_work']['incremental_sync'],s['layout_work']
   return s
  def has_draft(s,unicode=True):return any(('value: "中文 🦀 draft"' if unicode else 'value: "draft"') in c for c in s.get('content',[]))
  def wheel():
   before=state();console.send(b'\x1b[<65;2;1M'*5)
   s=until(lambda s:s.get('frames',0)>before['frames'] and s.get('cells',[])[:3]!=before.get('cells',[])[:3]);assert has_draft(s);return s
  try:
   s=until(lambda s:s.get('phase')=='active' and s.get('cells') is not None);save('start',s)
   s=resize(12);save('narrow',s)
   console.send('中文 🦀 '.encode())
   s=until(lambda s:has_draft(s));save('edit',s);revision=s['revision']
   for round in range(2):
    s=wheel();save(f'scrolled-{round}',s);assert s['revision']==revision
    s=resize(100);save(f'wide-{round}',s)
    assert s['revision']==revision and has_draft(s)
    assert s['cells'][0][:4]==list('wrap'),('content shifted out after wide resize',s)
    assert all(c==' ' for c in s['cells'][2]),'Scroll blank third row at wide width'
    if round==0:
     s=resize(12);save('narrow-again',s);assert s['revision']==revision and has_draft(s)
   undos=0
   while not has_draft(s,False):
    before=s['revision'];console.send(b'\x1a')
    s=until(lambda s:s.get('revision',0)>before);undos+=1
    save(f'undo-{undos}',s)
    assert undos<=16,'undo chain did not reach original draft'
   case['undo_events']=undos
   save('undo',s);assert s['revision']>revision
   # Receipt follows draw, while ConPTY's reader drains on another thread.
   end=time.monotonic()+3;stable=time.monotonic();last=b''
   while time.monotonic()-stable<.1:
    assert time.monotonic()<end,'output failed to settle'
    time.sleep(.01)
    with console.lock:raw=bytes(console.output)
    if raw!=last:last=raw;stable=time.monotonic()
   (folder/'application.ansi').write_bytes(last)
   console.send(b'\x11');console.finish();s=state();save('closed',s)
   assert s['phase']=='closed' and s['restored'] and s['error'] is None
   case.update(status='pass',application_bytes=len(last),application_sha256=hashlib.sha256(last).hexdigest(),final_cells=s['cells'])
  finally:
   console.close();(folder/'output.ansi').write_bytes(bytes(console.output));case.update(steps=steps,raw_bytes=len(console.output),raw_sha256=hashlib.sha256(console.output).hexdigest())
   (folder/'acceptance.json').write_text(json.dumps(case,ensure_ascii=False,indent=2),encoding='utf-8');cases.append(case)
 report=dict(status='pass',expect_root_sync=args.expect_root_sync,runtime='owned Windows ConPTY / formal TSX QuickJS SDK',executable_sha256=sha(args.executable),source_bundle_sha256=sha(args.bundle),cases=cases,exact_final_cells_equal=cases[0]['final_cells']==cases[1]['final_cells'],scope='Two genuine Console sessions, viewport reflow, wheel, controlled Unicode input and undo; captured glyph cells are not device raster or latency proof')
 assert report['exact_final_cells_equal']
 (args.directory/'acceptance.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps({k:v for k,v in report.items() if k!='cases'}))
if __name__=='__main__':main()
