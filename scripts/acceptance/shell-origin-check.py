#!/usr/bin/env python3
"""Run the production Caddy configuration on owned loopback with temporary TLS."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import ssl
import subprocess
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--caddy', type=Path, required=True)
    parser.add_argument('--test-executable', type=Path, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=False)
    directory = args.directory.resolve()
    helper = importlib.util.spec_from_file_location('tls_fixture', Path(__file__).with_name('shell-https-check.py'))
    module = importlib.util.module_from_spec(helper)
    helper.loader.exec_module(module)
    cert, key = module.certificate_pair(directory)
    with socket.socket() as port:
        port.bind(('127.0.0.1', 0))
        number = port.getsockname()[1]
    source = Path(__file__).parents[1] / 'release' / 'shell-ota.Caddyfile'
    config = '{\n admin off\n auto_https disable_redirects\n}\n' + source.read_text()
    config = config.replace('    file_server {', f'    tls "{cert}" "{key}"\n    file_server {{')
    configured = directory / 'Caddyfile'
    configured.write_text(config)
    base = f'https://localhost:{number}'
    env = dict(os.environ, JADIX_OTA_DOMAIN=base, JADIX_OTA_ROOT=str(directory),
               JADIX_HTTPS_TEST_URL=base, JADIX_HTTPS_TEST_DIRECTORY=str(directory),
               JADIX_HTTPS_TEST_CA=str(cert), JADIX_HTTPS_TEST_CURL='curl')
    process = None
    try:
        with (directory / 'server.log').open('wb') as server_log:
            subprocess.run([str(args.caddy), 'adapt', '--config', str(configured), '--adapter', 'caddyfile', '--validate'], env=env, stdout=server_log, stderr=server_log, check=True, timeout=20)
            process = subprocess.Popen([str(args.caddy), 'run', '--config', str(configured), '--adapter', 'caddyfile'], env=env, stdout=server_log, stderr=server_log)
            context = ssl.create_default_context(cafile=str(cert))
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context))
            deadline = time.monotonic() + 10
            while True:
                assert process.poll() is None, 'Caddy exited before readiness'
                try:
                    opener.open(base + '/not-found', timeout=1)
                except urllib.error.HTTPError:
                    break
                except urllib.error.URLError:
                    if time.monotonic() > deadline:
                        raise
                    time.sleep(.1)
            with (directory / 'tests.log').open('wb') as log:
                subprocess.run([str(args.test_executable), '--ignored', '--exact', 'published_https_channel_with_static_server', '--nocapture'], env=env, stdout=log, stderr=log, check=True, timeout=30)
            for path, status, method in [('/.publication.lock', 404, 'GET'), ('/key.pem', 404, 'GET'), ('/certificate.pem', 404, 'GET'), ('/Caddyfile', 404, 'GET'), ('/releases/.staging-9/panel.js', 404, 'GET'), ('/channel.json', 405, 'POST')]:
                try:
                    opener.open(urllib.request.Request(base + path, method=method), timeout=3)
                    raise AssertionError('private path or write method accepted')
                except urllib.error.HTTPError as error:
                    assert error.code == status, (path, error.code)
            for path, expected in [('/channel.json', 'no-store'), ('/releases/2/panel.js', 'immutable')]:
                with opener.open(base + path, timeout=3) as response:
                    assert expected in response.headers['Cache-Control']
            receipt = json.loads((directory / 'acceptance.json').read_text())
            receipt.update(server='Caddy', production_template_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), private_paths_rejected=True, writes_rejected=True, cache_headers=True, scope='owned loopback TLS; public DNS/ACME issuance not exercised')
            (directory / 'acceptance.json').write_text(json.dumps(receipt, indent=2))
            print(json.dumps(receipt))
    finally:
        if process and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        key.unlink(missing_ok=True)


if __name__ == '__main__':
    main()
