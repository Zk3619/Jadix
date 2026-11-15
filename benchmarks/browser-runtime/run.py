"""Owned localhost/Edge run, collecting actual WebGPU/WebGL readbacks."""
import functools,http.server,json,os,subprocess,tempfile,threading,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
OUT=ROOT/'crates/jadix_2d/evidence/round24'
OUT.mkdir(parents=True,exist_ok=True)
bindgen=HERE/'tools/wasm-bindgen-0.2.114-x86_64-pc-windows-msvc/wasm-bindgen.exe'
commands=[]
for feature in ['jadix','vello-reference']:
    env=os.environ.copy();env['RUSTFLAGS']='--cfg web_sys_unstable_apis';env['CARGO_HTTP_PROXY']=''
    build=['cargo','build','--locked','--manifest-path',str(HERE/'Cargo.toml'),'--target','wasm32-unknown-unknown','--no-default-features','--features',('jadix,compute' if feature=='jadix' else feature)]
    with (OUT/('browser-'+feature+'-build.log')).open('w',encoding='utf-8') as f:subprocess.run(build,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
    cmd=[str(bindgen),'--target','web','--out-dir',str(HERE/('pkg-'+feature)),str(HERE/'target/wasm32-unknown-unknown/debug/jadix_browser_acceptance.wasm')]
    subprocess.run(cmd,check=True);commands.append({'build':build,'bindgen':cmd})
finished=threading.Event()
class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_POST(self):
        if self.path!='/result':self.send_error(404);return
        value=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        value['captured_at']=time.strftime('%Y-%m-%dT%H:%M:%S%z')
        (OUT/'browser-runtime.json').write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
        self.send_response(200);self.end_headers();self.wfile.write(b'ok');finished.set()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(HERE)))
thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
profile=Path(tempfile.mkdtemp(prefix='jadix-browser-'))
browser=Path('C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe')
args=[str(browser),'--headless=new','--no-first-run','--no-default-browser-check','--enable-unsafe-webgpu',
      '--user-data-dir='+str(profile),'http://127.0.0.1:'+str(server.server_port)+'/index.html']
(OUT/'browser-command.json').write_text(json.dumps({'command':args,'build_commands':commands,'profile':str(profile)},indent=2),encoding='utf-8')
with (OUT/'browser-stderr.log').open('w',encoding='utf-8') as log:
    child=subprocess.Popen(args,stdout=log,stderr=subprocess.STDOUT,creationflags=0x08000000 if os.name=='nt' else 0)
    try:
        if not finished.wait(150):raise TimeoutError('browser did not report readback results')
        print((OUT/'browser-runtime.json').read_text(encoding='utf-8'),flush=True)
        result=json.loads((OUT/'browser-runtime.json').read_text(encoding='utf-8'))
        if not all(row.get('status')=='pass' for row in result['results']):raise RuntimeError('browser runtime reported failed cases; see browser-runtime.json')
    finally:
        if child.poll() is None:
            if os.name=='nt':subprocess.run(['taskkill','/PID',str(child.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            else:child.terminate()
        server.shutdown();server.server_close()
