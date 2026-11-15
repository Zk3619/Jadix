#!/usr/bin/env python3
"""Private TLS service exercises real certificate verification and signed OTA."""
import argparse,functools,gzip,http.server,json,os,ssl,subprocess,threading,time
from pathlib import Path
class Handler(http.server.SimpleHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def log_message(self,*args):pass
    def do_GET(self):
        if self.path=='/redirect':
            self.send_response(302);self.send_header('Location','https://example.com/');self.send_header('Content-Length','0');self.end_headers();return
        if self.path=='/slow':time.sleep(1)
        if self.path in ['/oversized','/compressed','/truncated']:
            body=b'x'*8192
            if self.path=='/compressed':body=gzip.compress(body)
            self.send_response(200)
            if self.path=='/compressed':self.send_header('Content-Encoding','gzip')
            if self.path=='/oversized':self.send_header('Transfer-Encoding','chunked')
            else:self.send_header('Content-Length',str(len(body)+(100 if self.path=='/truncated' else 0)))
            self.end_headers()
            try:
                if self.path=='/oversized':self.wfile.write(('%x\r\n'%len(body)).encode()+body+b'\r\n0\r\n\r\n')
                else:self.wfile.write(body)
            except (BrokenPipeError,ConnectionResetError):pass
            if self.path=='/truncated':self.close_connection=True
            return
        return super().do_GET()
def certificate_pair(directory):
 cert=directory/'certificate.pem';key=directory/'key.pem'
 from cryptography import x509
 from cryptography.x509.oid import NameOID,ExtendedKeyUsageOID
 from cryptography.hazmat.primitives import hashes,serialization
 from cryptography.hazmat.primitives.asymmetric import rsa
 import datetime
 private=rsa.generate_private_key(public_exponent=65537,key_size=2048);name=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,'localhost')]);now=datetime.datetime.now(datetime.timezone.utc)
 certificate=x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(private.public_key()).serial_number(x509.random_serial_number()).not_valid_before(now-datetime.timedelta(minutes=1)).not_valid_after(now+datetime.timedelta(days=1)).add_extension(x509.SubjectAlternativeName([x509.DNSName('localhost')]),critical=False).add_extension(x509.BasicConstraints(ca=True,path_length=None),critical=True).add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]),critical=False).sign(private,hashes.SHA256())
 cert.write_bytes(certificate.public_bytes(serialization.Encoding.PEM));key.write_bytes(private.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.TraditionalOpenSSL,serialization.NoEncryption()))
 return cert,key

def main():
 p=argparse.ArgumentParser();p.add_argument('--test-executable',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--openssl',default='openssl');p.add_argument('--curl',default='curl');a=p.parse_args();a.directory.mkdir(parents=True,exist_ok=False);a.directory=a.directory.resolve()
 cert,key=certificate_pair(a.directory)
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(a.directory)));server.daemon_threads=True
 context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.minimum_version=ssl.TLSVersion.TLSv1_2;context.load_cert_chain(cert,key);server.socket=context.wrap_socket(server.socket,server_side=True)
 thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
 env=dict(os.environ,JADIX_HTTPS_TEST_URL=f'https://localhost:{server.server_port}',JADIX_HTTPS_TEST_DIRECTORY=str(a.directory),JADIX_HTTPS_TEST_CA=str(cert),JADIX_HTTPS_TEST_CURL=a.curl)
 try:
  with (a.directory/'tests.log').open('wb') as log:subprocess.run([str(a.test_executable.resolve()),'--ignored','--exact','native_https_signed_ota_trial_and_transport_boundaries','--nocapture'],env=env,stdout=log,stderr=log,check=True,timeout=40)
  value=json.loads((a.directory/'acceptance.json').read_text());assert value['status']=='pass';print(json.dumps(value))
 finally:server.shutdown();server.server_close();thread.join(timeout=3);key.unlink(missing_ok=True)
if __name__=='__main__':main()
