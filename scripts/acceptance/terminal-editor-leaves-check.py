#!/usr/bin/env python3
"""Two owned Console SDK sessions: controlled editor patches versus full mounts."""
import argparse,importlib.util,hashlib,json,time
from pathlib import Path
spec=importlib.util.spec_from_file_location('conpty',Path(__file__).with_name('terminal-conpty-check.py'));conpty=importlib.util.module_from_spec(spec);spec.loader.exec_module(conpty)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--executable',type=Path,required=True);p.add_argument('--bundle',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);a=p.parse_args();a.directory.mkdir(parents=True,exist_ok=False)
 cases=[];stages=[]
 for partial in [False,True]:
  folder=a.directory/('partial' if partial else 'full');folder.mkdir();bundle=folder/'controlled-leaves.js';bundle.write_text(f'globalThis.scaleRows=256;globalThis.leafPatches={str(partial).lower()};\n'+a.bundle.read_text(encoding='utf8'),encoding='utf8');receipt=(folder/'receipt.json').resolve();console=conpty.Console([a.executable.resolve(),'--bundle',bundle.resolve(),'--receipt',receipt,'--receipt-cells']);case=dict(status='incomplete',partial=partial,bundle_sha256=sha(bundle));captures=[]
  def state():
   try:return json.loads(receipt.read_text(encoding='utf8'))
   except(FileNotFoundError,json.JSONDecodeError):return {}
  def until(test):
   end=time.monotonic()+10
   while time.monotonic()<end:
    s=state()
    if test(s):return s
    time.sleep(.02)
   raise AssertionError({k:v for k,v in state().items() if k!='cells'})
  def save(name,s):
   (folder/(name+'.json')).write_text(json.dumps(s,ensure_ascii=False,indent=2)+'\n',encoding='utf8');captures.append(dict(name=name,cells=s['cells'],content=s['content'],revision=s['revision']))
  def contains(s,index,value):return ('value: '+json.dumps(value,ensure_ascii=False)) in s.get('content',['','',''])[index]
  try:
   s=until(lambda s:s.get('phase')=='active' and s.get('cells'));save('start',s)
   console.send('中文 🦀 '.encode());s=until(lambda s:contains(s,0,'中文 🦀 draft'));save('edit',s)
   # Console emits five scalar key events here, so undo each actual edit.
   undos=0
   while not contains(s,0,'draft'):
    revision=s['revision'];console.send(b'\x1a');s=until(lambda s:s.get('revision',0)>revision);undos+=1;assert undos<=16
   save('undo',s)
   for _ in range(undos):
    revision=s['revision'];console.send(b'\x19');s=until(lambda s:s.get('revision',0)>revision)
   assert contains(s,0,'中文 🦀 draft');save('redo',s)
   console.send(b'\t');console.send('多行\r🙂'.encode());s=until(lambda s:contains(s,1,'多行\n🙂notes'));save('textarea',s)
   console.send(b'\x1b[<0;3;5M\x1b[<0;3;5m');s=until(lambda s:contains(s,0,'SERVER 中文 🦀'));save('server',s)
   revision=s['revision'];frames=s['frames'];assert console.resize(console.pc,conpty.Coord(100,30))==0
   s=until(lambda s:s.get('viewport')==[100,30] and s.get('frames',0)>frames);assert s['revision']==revision;assert s['layout_work']['sync_nodes']==1;save('resize',s)
   work=s['application_work'];assert work['full_mounts']==1 if partial else work['full_mounts']>10
   assert work['leaf_patches']>10 if partial else work['leaf_patches']==0
   end=time.monotonic()+3;quiet=time.monotonic();last=b''
   while time.monotonic()-quiet<.1:
    assert time.monotonic()<end
    time.sleep(.01)
    with console.lock:raw=bytes(console.output)
    if raw!=last:last=raw;quiet=time.monotonic()
   (folder/'application.ansi').write_bytes(last);console.send(b'\x11');console.finish();s=state();assert s['phase']=='closed' and s['restored'] and s['error'] is None
   case.update(status='pass',undo_events=undos,work=work,closed=s,application_bytes=len(last),application_sha256=hashlib.sha256(last).hexdigest());stages.append(captures)
  finally:
   console.close();raw=bytes(console.output);(folder/'output.ansi').write_bytes(raw);case.update(raw_bytes=len(raw),raw_sha256=hashlib.sha256(raw).hexdigest());(folder/'acceptance.json').write_text(json.dumps(case,ensure_ascii=False,indent=2)+'\n',encoding='utf8');cases.append(case)
 assert stages[0]==stages[1],'complete visible cells, descriptors and revision differ'
 full=(a.directory/'full/application.ansi').read_bytes();partial=(a.directory/'partial/application.ansi').read_bytes();title=b'\x1b]0;Jadix JS TSX Shell\x07'
 report=dict(status='pass',runtime='owned Windows ConPTY / existing QuickJS SDK',executable_sha256=sha(a.executable),source_bundle_sha256=sha(a.bundle),cases=cases,exact_stages_equal=True,exact_application_wire_equal=full==partial,except_exact_title_equal=full.replace(title,b'')==partial.replace(title,b''),title_counts=[full.count(title),partial.count(title)],scope='same 256-row business, real Console Unicode keys and undo/redo, multiline, server replacement and resize; cells are captured application Buffer, not OS raster; raw title/cleanup ordering reported independently')
 (a.directory/'acceptance.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps({k:v for k,v in report.items() if k not in ['cases']},ensure_ascii=False))
if __name__=='__main__':main()
