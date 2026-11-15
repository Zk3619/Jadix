#!/usr/bin/env python3
"""Exercise the actual offline release CLI with a disposable test signing key."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--executable', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True, help='Existing manifest structure; never its signing key')
    parser.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=False)
    directory = args.directory.resolve()
    resource_root = directory / 'input'
    resource_root.mkdir()
    body = b'globalThis.publicationAccepted=true;'
    (resource_root / 'main.js').write_bytes(body)
    template = json.loads(args.manifest.read_text(encoding='utf-8'))['manifest']
    template.update(release='1', entry='main.js', resources={'main.js': {'sha256': hashlib.sha256(body).hexdigest(), 'bytes': len(body)}})
    template_path = directory / 'template.json'
    output = directory / 'origin'
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    key = Ed25519PrivateKey.generate()  # Test only, not persisted or installed.
    secret = key.private_bytes(serialization.Encoding.Raw, serialization.PrivateFormat.Raw, serialization.NoEncryption()).hex()
    environment = dict(os.environ, JADIX_RELEASE_SIGNING_KEY=secret)

    def run(success):
        template_path.write_text(json.dumps(template), encoding='utf-8')
        result = subprocess.run([str(args.executable.resolve()), str(template_path), str(resource_root), str(output)], env=environment, capture_output=True, text=True, timeout=20)
        assert (result.returncode == 0) == success, result.stderr
        assert secret not in result.stdout + result.stderr
        return result

    def verify(release):
        signed = json.loads((output / 'channel.json').read_text())
        assert signed['manifest']['release'] == str(release)
        canonical = json.dumps(signed['manifest'], ensure_ascii=False, separators=(',', ':')).encode()
        key.public_key().verify(bytes.fromhex(signed['signature']), b'JADIX-SHELL-PACKAGE\x00\x01' + canonical)
        stored = output / 'releases' / str(release)
        assert json.loads((stored / 'manifest.json').read_text()) == signed
        assert (stored / 'main.js').read_bytes() == body
        return (output / 'channel.json').read_bytes()

    run(True)
    first = verify(1)
    run(False)  # Immutable release cannot be replaced, even by the signer.
    assert (output / 'channel.json').read_bytes() == first
    template['release'] = '2'
    run(True)
    second = verify(2)
    template['release'] = '3'
    template['entry'] = '../main.js'
    run(False)
    assert (output / 'channel.json').read_bytes() == second
    assert not (output / 'releases' / '3').exists()
    receipt = {'status': 'pass', 'actual_signer_cli': True, 'independent_ed25519_verification': True, 'immutable_releases': True, 'channel_last': True, 'path_rejected': True, 'key_persisted': False, 'executable_sha256': hashlib.sha256(args.executable.read_bytes()).hexdigest()}
    (directory / 'acceptance.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
    print(json.dumps(receipt))


if __name__ == '__main__':
    main()
