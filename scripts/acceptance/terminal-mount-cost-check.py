#!/usr/bin/env python3
"""Owned Windows Console startup, actual click, resize and restore before/after."""
import argparse,hashlib,importlib.util,json,statistics,time
from pathlib import Path
R=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('conpty',R/'scripts/acceptance/terminal-conpty-check.py');conpty=importlib.util.module_from_spec(spec);spec.loader.exec_module(conpty)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--before',type=Path,required=True);p.add_argument('--after',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--rows',type=int,nargs='+',default=[0,256,1024,3072]);p.add_argument('--order',choices=['AB','ABBAAB'],default='ABBAAB');a=p.parse_args();a.directory.mkdir(parents=True,exist_ok=False)
 source=(R/'crates/jadix_shell/tests/fixtures/counter-leaves.js').read_text(encoding='utf8');runs=[]
 for rows in a.rows:
  root=a.directory/str(rows);root.mkdir();bundle=root/'counter.js';bundle.write_text(f'globalThis.businessRows={rows};\n'+source,encoding='utf8')
  for index,kind in enumerate(a.order):
   folder=root/f'{index}-{kind}';folder.mkdir();receipt=(folder/'receipt.json').resolve();executable=a.before if kind=='A' else a.after
   console=conpty.Console([executable.resolve(),'--bundle',bundle.resolve(),'--receipt',receipt,'--receipt-cells']);case=dict(rows=rows,index=index,kind=kind,status='incomplete',executable_sha256=sha(executable),bundle_sha256=sha(bundle));start=time.monotonic()
   def state():
    try:return json.loads(receipt.read_text(encoding='utf8'))
    except(FileNotFoundError,json.JSONDecodeError):return {}
   def until(test):
    end=time.monotonic()+15
    while time.monotonic()<end:
     value=state()
     if value.get('phase')=='failed':raise AssertionError(value)
     if test(value):return value
     assert console.wait(console.proc.process,0)==258,bytes(console.output)
     time.sleep(.01)
    raise AssertionError({k:v for k,v in state().items() if k!='cells'})
   def save(name,value):
    (folder/(name+'.json')).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
   def counter(value,n):return value.get('content',[''])[0]==f'Text("Counter {n}")'
   try:
    value=until(lambda v:v.get('phase')=='active' and counter(v,0) and v.get('cells'));save('start',value)
    assert value['layout_work']['nodes']==rows+4
    assert all(value['startup_work'][k]>0 for k in ['application_load_ns','first_publish_ns','first_draw_ns'])
    first=value['startup_work'];case['vm_limits']=value.get('vm_limits');cells=value['cells'];assert 'Counter 0' in ''.join(cells[0])
    for n in range(min(rows,len(cells)-3)):assert ''.join(cells[n+3]).rstrip()==f'Row {n}: business'
    console.send(b'\x1b[<0;5;2M\x1b[<0;5;2m');value=until(lambda v:counter(v,1));save('click',value)
    assert value['application_work']['full_mounts']==1 and value['application_work']['leaf_patches']==1
    assert console.resize(console.pc,conpty.Coord(100,30))==0
    value=until(lambda v:v.get('viewport')==[100,30] and counter(v,1));save('resize',value)
    assert value['startup_work']==first
    with console.lock:raw=bytes(console.output)
    (folder/'application.ansi').write_bytes(raw)
    console.send(b'\x11');console.finish();value=state();assert value['phase']=='closed' and value['restored'] and value['error'] is None
    case.update(status='pass',startup_work=first,work=value['application_work'],elapsed_s=time.monotonic()-start,restored=True)
   finally:
    console.close();raw=bytes(console.output);(folder/'output.ansi').write_bytes(raw);case.update(output_sha256=hashlib.sha256(raw).hexdigest(),output_bytes=len(raw));(folder/'acceptance.json').write_text(json.dumps(case,ensure_ascii=False,indent=2)+'\n',encoding='utf8');runs.append(case)
   print(rows,index,kind,first,flush=True)
 summaries=[]
 for rows in a.rows:
  selected=[v for v in runs if v['rows']==rows]
  for kind in ['A','B']:
   cases=[v for v in selected if v['kind']==kind];summaries.append(dict(rows=rows,kind=kind,runs=len(cases),**{k:statistics.median(v['startup_work'][k] for v in cases) for k in ['application_load_ns','first_publish_ns','first_draw_ns']}))
  for stage in ['start','click','resize']:
   states=[json.loads((a.directory/str(rows)/f"{v['index']}-{v['kind']}"/(stage+'.json')).read_text(encoding='utf8')) for v in selected]
   assert all({k:s[k] for k in ['cells','content','revision','viewport']}=={k:states[0][k] for k in ['cells','content','revision','viewport']} for s in states)
 result=dict(status='pass',order=a.order,runs=runs,summary=summaries,scope='real ConPTY/Console input and output; application startup/publish/first Terminal.draw CPU+write timing; application Buffer checkpoints, not emulator raster or OS scanout; startup and interaction policies belong to each supplied binary; inspect vm_limits receipts and frozen sources when policies differ')
 (a.directory/'acceptance.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
