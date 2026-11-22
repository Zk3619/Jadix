"""Application update artifacts, validated and signed by the shared Rust core."""
import base64
import json
import os
from pathlib import Path
import re
import secrets
import subprocess

import native


def tool(request):
    # Secret seed travels only over stdin; neither argv nor logs retain it.
    result = subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-j", "1",
                             "-p", "jadix_core", "--features", "updater", "--example", "updater-tool"],
                            input=json.dumps(request), cwd=native.ROOT, capture_output=True,
                            text=True, encoding="utf-8", timeout=600,
                            env=dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0"),
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    if result.returncode:
        raise ValueError("shared updater validation failed: " + result.stderr[-2048:].strip())
    return json.loads(result.stdout)


def configuration(config, identifier, version):
    if config is None:
        return None
    if not isinstance(config, dict) or set(config) - {"github", "url", "channels", "publicKey"}:
        raise ValueError("updater configuration accepts only packaged sources, channels and publicKey")
    return tool({"action": "config", "config": {**config, "id": identifier, "version": version}})["config"]


def register(subcommands):
    keygen = subcommands.add_parser("update-keygen", help="Create a private Ed25519 update key and its public key")
    keygen.add_argument("--key", type=Path, required=True, help="New private key file; keep outside published artifacts")
    release = subcommands.add_parser("update-release", help="Generate a validated latest.json and optional detached signature")
    release.add_argument("--id", required=True)
    release.add_argument("--version", required=True)
    release.add_argument("--manifest-url", required=True, help="HTTPS publication URL, including a named channel path")
    release.add_argument("--asset", nargs=3, action="append", required=True, metavar=("PLATFORM", "FILE", "URL"))
    release.add_argument("--localized-notes", nargs=3, action="append", default=[], metavar=("LOCALE", "FILE", "URL"))
    release.add_argument("--notes", type=Path)
    release.add_argument("--key", type=Path, help="Private seed from update-keygen; required for custom update sources")
    for parser in (keygen, release):
        parser.add_argument("--output", type=Path, required=True)


def keygen(arguments, output):
    key = arguments.key.resolve()
    if key == output or key.exists():
        raise ValueError("private update key must be a new file distinct from the report")
    value = base64.b64encode(secrets.token_bytes(32)).decode("ascii")
    public = tool({"action": "key", "seed": value})["publicKey"]
    # O_EXCL prevents replacing an existing key. Unix permissions are private at creation.
    with os.fdopen(os.open(key, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w", encoding="ascii") as stream:
        stream.write(value + "\n")
    return {"publicKey": public, "private_key": str(key)}


def release(arguments, output):
    destination = output.parent / (output.stem + "-files")
    if destination.exists():
        raise ValueError("update release directory already exists")
    manifest = {"id": arguments.id, "version": arguments.version, "platforms": {}, "localizedNotes": {}}
    files = {}
    for field, rows in [("platforms", arguments.asset), ("localizedNotes", arguments.localized_notes)]:
        if len(rows) > (64 if field == "platforms" else 128):
            raise ValueError("excessive update assets or localized notes")
        for name, source, url in rows:
            if not re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", name) or name in manifest[field]:
                raise ValueError("invalid or duplicate update platform/locale")
            path = Path(source).resolve()
            identity = native.file_identity(path)
            maximum = 2 * 1024**3 if field == "platforms" else 256 * 1024
            if not 0 < identity["bytes"] <= maximum:
                raise ValueError("update asset or notes size exceeds limit")
            item = {"url": url, "sha256": identity["sha256"]}
            if field == "platforms":
                item["size"] = identity["bytes"]
            else:
                path.read_text(encoding="utf-8")
            manifest[field][name] = item
            files[field + "/" + name] = {"path": str(path), **identity}
    if arguments.notes:
        if arguments.notes.stat().st_size > 256 * 1024:
            raise ValueError("release notes exceed limit")
        manifest["notes"] = arguments.notes.read_text(encoding="utf-8")
    seed = None
    if arguments.key:
        key = arguments.key.resolve()
        if key.is_relative_to(destination.resolve()) or key.stat().st_size > 256:
            raise ValueError("private key cannot be inside update artifacts or exceed the key limit")
        seed = key.read_text(encoding="ascii").strip()
    generated = tool({"action": "release", "seed": seed, "manifest_url": arguments.manifest_url, "manifest": manifest})
    # Publish the exact bytes signed by Rust, with no newline or platform translation.
    payload = generated["manifest"].encode("utf-8")
    destination.mkdir()
    try:
        path = destination / "latest.json"
        path.write_bytes(payload)
        if generated["signature"]:
            (destination / "latest.json.sig").write_text(generated["signature"], encoding="ascii")
    except Exception:
        # Only files created here may be cleaned up.
        for name in ("latest.json", "latest.json.sig"):
            (destination / name).unlink(missing_ok=True)
        destination.rmdir()
        raise
    return {"manifest": str(path), "identity": native.file_identity(path), "files": files,
            "publicKey": generated["publicKey"], "signed": generated["signature"] is not None,
            "manifest_url": arguments.manifest_url}
