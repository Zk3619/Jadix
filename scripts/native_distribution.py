"""Version-bound CLI/runtime distributions with atomic activation and an offline cache."""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from urllib.parse import urljoin, urlparse
from urllib.request import urlopen
import uuid
import zipfile

import native

MAX_ARCHIVE = 512 * 1024**2
MAX_EXPANDED = 2 * 1024**3
OPERATIONS = {"component-list", "component-add", "component-remove", "component-run", "self-update", "distribution-pack"}


def safe_segment(value):
    return (value and not any(ord(char) < 32 or char in '<>:"/\\|?*' for char in value)
            and not value.endswith((".", " "))
            and value.split(".")[0].upper() not in {"CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10))})


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_.-]{0,95}", value) or not safe_segment(value):
        raise ValueError("invalid distribution identifier")
    return value


def relative(value):
    if not isinstance(value, str):
        raise ValueError("distribution path must be a string")
    path = PurePosixPath(value)
    if (not value or "\\" in value or ":" in value or path.is_absolute()
            or any(part in (".", "..", "") or not safe_segment(part) for part in value.split("/"))):
        raise ValueError("distribution paths must be relative regular-file paths")
    return path


def encoded(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def read_json(path, maximum=4 * 1024**2):
    if path.stat().st_size > maximum:
        raise ValueError("distribution metadata exceeds limit")
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate distribution metadata key")
            result[key] = value
        return result
    return json.loads(path.read_bytes(), object_pairs_hook=pairs)


def transfer(source, destination, maximum):
    parsed = urlparse(str(source))
    if parsed.scheme in ("http", "https"):
        if parsed.scheme != "https" or parsed.username or parsed.password:
            raise ValueError("remote distributions require HTTPS without credentials")
        stream = urlopen(str(source), timeout=30)
        if urlparse(stream.geturl()).scheme != "https":
            stream.close()
            raise ValueError("distribution redirect left HTTPS")
    else:
        stream = Path(source).open("rb")
    try:
        with destination.open("xb") as target:
            total = 0
            while block := stream.read(1024 * 1024):
                total += len(block)
                if total > maximum:
                    raise ValueError("distribution transfer exceeds limit")
                target.write(block)
    finally:
        stream.close()


def catalog(source, channel, digest=None, home=None, offline=False):
    if channel not in ("stable", "nightly"):
        raise ValueError("invalid distribution channel")
    parsed = urlparse(str(source))
    if parsed.scheme in ("http", "https") and (parsed.scheme != "https" or parsed.username or parsed.password):
        raise ValueError("remote distributions require HTTPS without credentials")
    source = str(source)
    cache = home / "catalogs" / (hashlib.sha256(source.encode("utf-8")).hexdigest() + ".json") if home else None
    with tempfile.TemporaryDirectory(prefix="jadix-catalog-") as temporary:
        path = Path(temporary) / "manifest.json"
        if offline and cache and cache.is_file():
            transfer(cache, path, 4 * 1024**2)
        elif offline and urlparse(source).scheme in ("http", "https"):
            raise ValueError("distribution manifest missing from offline cache")
        else:
            transfer(source, path, 4 * 1024**2)
        identity = native.file_identity(path)
        if digest and identity["sha256"] != digest.lower():
            raise ValueError("distribution manifest SHA-256 mismatch")
        manifest = read_json(path)
        content = path.read_bytes()
    if not isinstance(manifest, dict) or manifest.get("schema") != 1 or manifest.get("channel") != channel:
        raise ValueError("distribution schema/channel mismatch")
    identifier(manifest["version"])
    if not isinstance(manifest["commit"], str) or not re.fullmatch(r"[a-f0-9]{40,64}|unknown", manifest["commit"]):
        raise ValueError("distribution commit must be a full hash or unknown")
    components = manifest["components"]
    if not isinstance(components, dict) or len(components) > 64:
        raise ValueError("invalid distribution component inventory")
    for name, item in components.items():
        identifier(name)
        if (not isinstance(item, dict) or not isinstance(item.get("sha256"), str) or not re.fullmatch(r"[a-f0-9]{64}", item["sha256"])
                or type(item.get("bytes")) is not int or not 0 < item["bytes"] <= MAX_ARCHIVE):
            raise ValueError("invalid distribution component identity")
        location = item["url"]
        if not isinstance(location, str) or not location or "\0" in location:
            raise ValueError("invalid component URL")
        if urlparse(str(source)).scheme == "https":
            item["url"] = urljoin(str(source), location)
            if urlparse(item["url"]).scheme != "https":
                raise ValueError("remote catalog cannot load local components")
        elif not urlparse(location).scheme:
            item["url"] = str((Path(source).resolve().parent / location).resolve())
    # Cache only admitted catalogs, preserving their original bytes for digest pins
    # and their original source for relative component URL resolution.
    if cache and not offline:
        cache.parent.mkdir(parents=True, exist_ok=True)
        temporary = cache.with_name(uuid.uuid4().hex + ".part")
        try:
            temporary.write_bytes(content)
            os.replace(temporary, cache)
        finally:
            temporary.unlink(missing_ok=True)
    manifest["_source"] = source
    manifest["_sha256"] = identity["sha256"]
    return manifest, identity


def state(home):
    path = home / "active.json"
    value = read_json(path) if path.exists() else {"schema": 1, "components": {}}
    if not isinstance(value, dict) or value.get("schema") != 1 or not isinstance(value.get("components"), dict) or len(value["components"]) > 64:
        raise ValueError("invalid installed distribution state")
    for name, item in value["components"].items():
        identifier(name)
        if not isinstance(item, dict):
            raise ValueError("invalid installed component")
        identifier(item["directory"])
        identifier(item["version"])
        if not isinstance(item["commit"], str) or not re.fullmatch(r"[a-f0-9]{40,64}|unknown", item["commit"]):
            raise ValueError("invalid installed commit")
    return value


@contextmanager
def lock(home):
    home.mkdir(parents=True, exist_ok=True)
    path = home / ".update-lock"
    try:
        descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as error:
        raise ValueError("distribution update is already locked; inspect .update-lock before recovering an interrupted update") from error
    try:
        with os.fdopen(descriptor, "w") as stream:
            stream.write(str(os.getpid()))
        yield
    finally:
        path.unlink()


def activate(home, value):
    home.mkdir(parents=True, exist_ok=True)
    temporary = home / (".active-" + uuid.uuid4().hex)
    try:
        temporary.write_bytes(encoded(value))
        os.replace(temporary, home / "active.json")
    finally:
        temporary.unlink(missing_ok=True)


def validate_bundle(directory, name, version, commit):
    if directory.is_symlink() or directory.parent.is_symlink() or (directory / "runtime.json").is_symlink():
        raise ValueError("installed component must not use symbolic links")
    record = read_json(directory / "runtime.json")
    if (not isinstance(record, dict) or record.get("schema") != 1 or record.get("name") != name or record.get("version") != version
            or record.get("commit") != commit):
        raise ValueError("component version/commit record mismatch")
    files = record["files"]
    command = record["command"]
    if not isinstance(files, dict) or not files or len(files) > 20000:
        raise ValueError("invalid component file inventory")
    if not isinstance(command, list) or not command or not all(isinstance(arg, str) and "\0" not in arg for arg in command):
        raise ValueError("invalid component entry command")
    if command[0] not in files:
        raise ValueError("component command must refer to an inventoried file")
    for name, identity in files.items():
        path = directory.joinpath(*relative(name).parts)
        if any(parent.is_symlink() for parent in [path, *path.parents] if parent.is_relative_to(directory)) or not path.is_file() or native.file_identity(path) != identity:
            raise ValueError("installed component file identity mismatch: " + name)
    return record


def prepare(home, manifest, name, offline=False):
    identifier(name)
    item = manifest["components"].get(name)
    if item is None:
        raise ValueError("component is unavailable in this release: " + name)
    cache = home / "cache"
    versions = home / "versions"
    cache.mkdir(parents=True, exist_ok=True)
    versions.mkdir(parents=True, exist_ok=True)
    archive = cache / (item["sha256"] + ".zip")
    if not archive.exists():
        if offline:
            raise ValueError("component missing from offline cache: " + name)
        temporary = cache / (uuid.uuid4().hex + ".part")
        try:
            transfer(item["url"], temporary, MAX_ARCHIVE)
            if native.file_identity(temporary) != {key: item[key] for key in ("bytes", "sha256")}:
                raise ValueError("component download SHA-256/size mismatch")
            os.replace(temporary, archive)
        finally:
            temporary.unlink(missing_ok=True)
    if native.file_identity(archive) != {key: item[key] for key in ("bytes", "sha256")}:
        raise ValueError("cached component SHA-256/size mismatch")
    # New generations preserve the currently running CLI on Windows. Activation
    # changes one small record, never the loaded executable or a live directory.
    directory = versions / (name + "-" + uuid.uuid4().hex)
    directory.mkdir()
    try:
        with zipfile.ZipFile(archive) as bundle:
            members = bundle.infolist()
            names = set()
            total = 0
            if len(members) > 20001:
                raise ValueError("component archive has too many members")
            for member in members:
                path = relative(member.filename)
                canonical = member.filename.casefold()
                mode = member.external_attr >> 16
                if canonical in names or member.is_dir() or stat.S_ISLNK(mode) or (stat.S_IFMT(mode) not in (0, stat.S_IFREG)):
                    raise ValueError("duplicate or nonregular component archive member")
                names.add(canonical)
                total += member.file_size
                if total > MAX_EXPANDED:
                    raise ValueError("expanded component exceeds limit")
                destination = directory.joinpath(*path.parts)
                destination.parent.mkdir(parents=True, exist_ok=True)
                with bundle.open(member) as source, destination.open("xb") as target:
                    shutil.copyfileobj(source, target, 1024 * 1024)
                if mode & 0o111:
                    destination.chmod(0o755)
            record = validate_bundle(directory, name, manifest["version"], manifest["commit"])
            if names != {"runtime.json", *(file.casefold() for file in record["files"])}:
                raise ValueError("component archive contains unrecorded files")
        return {"directory": directory.name, "version": manifest["version"], "commit": manifest["commit"],
                "channel": manifest["channel"], "sha256": item["sha256"],
                "manifest": manifest.get("_source"), "manifest_sha256": manifest.get("_sha256")}
    except Exception:
        # Both parent and leaf are fixed here; never delete a path from metadata.
        if directory.parent == home / "versions":
            shutil.rmtree(directory)
        raise


def install(home, manifest, names, offline=False):
    with lock(home):
        if names is None:
            names = sorted({"cli", *state(home)["components"]})
        return install_locked(home, manifest, names, offline)


def install_locked(home, manifest, names, offline=False):
    current = state(home)
    prepared = {}
    try:
        for name in names:
            prepared[name] = prepare(home, manifest, name, offline)
        activate(home, {"schema": 1, "components": {**current["components"], **prepared}})
    except Exception:
        for item in prepared.values():
            directory = home / "versions" / item["directory"]
            if directory.parent == home / "versions":
                shutil.rmtree(directory)
        raise
    return prepared


def launch(home, name, arguments):
    item = state(home)["components"].get(name)
    if item is None:
        raise ValueError("component is not installed: " + name)
    directory = home / "versions" / item["directory"]
    record = validate_bundle(directory, name, item["version"], item["commit"])
    command = [str(directory / record["command"][0]), *record["command"][1:], *arguments]
    if command[0].endswith(".py"):
        command.insert(0, sys.executable)
    environment = dict(os.environ, JADIX_DISTRIBUTION_HOME=str(home),
                       JADIX_DISTRIBUTION_VERSION=item["version"], JADIX_DISTRIBUTION_COMMIT=item["commit"],
                       JADIX_DISTRIBUTION_CHANNEL=item["channel"])
    for key, field in (("JADIX_DISTRIBUTION_MANIFEST", "manifest"), ("JADIX_DISTRIBUTION_MANIFEST_SHA256", "manifest_sha256")):
        environment.pop(key, None)
        if item.get(field):
            environment[key] = item[field]
    result = subprocess.run(command, cwd=directory, env=environment,
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    return {"component": name, "exit_code": result.returncode, "installed": item}


def ensure(home, name, source=None, channel="stable", digest=None, offline=False):
    """Resolve the requested release before deciding whether an installed runtime can run."""
    item = state(home)["components"].get(name)
    source = source or os.environ.get("JADIX_DISTRIBUTION_MANIFEST")
    expected_version = os.environ.get("JADIX_DISTRIBUTION_VERSION")
    expected_commit = os.environ.get("JADIX_DISTRIBUTION_COMMIT")
    if source:
        manifest, _ = catalog(source, channel, digest, home, offline)
        if ((expected_version and expected_version != manifest["version"])
                or (expected_commit and expected_commit != manifest["commit"])):
            raise ValueError("runtime catalog does not match the running CLI version/commit")
        component = manifest["components"].get(name)
        if component is None:
            raise ValueError("component is unavailable in this release: " + name)
        if item is None or any(item.get(key) != value for key, value in (
                ("version", manifest["version"]), ("commit", manifest["commit"]),
                ("channel", channel), ("sha256", component["sha256"]))):
            install(home, manifest, [name], offline)
    elif item is None:
        raise ValueError("missing runtime component; supply --manifest for on-demand installation")
    elif ((expected_version and item["version"] != expected_version)
            or (expected_commit and item["commit"] != expected_commit)):
        raise ValueError("installed runtime does not match the running CLI; supply its release manifest")
    return state(home)["components"][name]


def pack(arguments, output):
    source = arguments.source.resolve()
    version, name = identifier(arguments.version), identifier(arguments.name)
    if not re.fullmatch(r"[a-f0-9]{40,64}|unknown", arguments.commit):
        raise ValueError("invalid distribution commit")
    files = native.tree_identity(source)["files"]
    entry = relative(arguments.entry).as_posix()
    records = {item["path"]: {key: item[key] for key in ("bytes", "sha256")} for item in files}
    for path in records:
        relative(path)
    if len({path.casefold() for path in records}) != len(records):
        raise ValueError("component source contains case-insensitive duplicate paths")
    if entry not in records or "runtime.json" in records or len(records) > 20000:
        raise ValueError("bundle needs an inventoried entry and no reserved runtime.json")
    destination = output.with_suffix(".zip")
    if destination.exists() or destination.is_relative_to(source):
        raise ValueError("bundle output must be new and outside its source")
    record = {"schema": 1, "name": name, "version": version, "commit": arguments.commit,
              "command": [entry], "files": records}
    try:
        with zipfile.ZipFile(destination, "x", zipfile.ZIP_DEFLATED) as bundle:
            bundle.writestr("runtime.json", encoded(record))
            for path in records:
                bundle.write(source / path, path)
        if any(native.file_identity(source / path) != identity for path, identity in records.items()):
            raise ValueError("component source changed while packaging")
        identity = native.file_identity(destination)
        if identity["bytes"] > MAX_ARCHIVE or sum(file["bytes"] for file in records.values()) > MAX_EXPANDED:
            raise ValueError("component bundle exceeds limit")
    except Exception:
        destination.unlink(missing_ok=True)
        raise
    return {"archive": str(destination), "component": {"url": destination.name, **identity}, "record": record}


def register(subcommands):
    for operation in ("component-list", "component-add", "component-remove", "component-run", "self-update"):
        parser = subcommands.add_parser(operation)
        parser.add_argument("--home", type=Path, default=Path(os.environ.get("JADIX_DISTRIBUTION_HOME", Path.home() / ".jadix/distribution")))
        parser.add_argument("--output", type=Path, required=True)
        if operation in ("component-add", "component-run", "self-update"):
            parser.add_argument("--manifest", required=operation != "component-run", default=os.environ.get("JADIX_DISTRIBUTION_MANIFEST"))
            parser.add_argument("--manifest-sha256", default=os.environ.get("JADIX_DISTRIBUTION_MANIFEST_SHA256"))
            parser.add_argument("--channel", choices=("stable", "nightly"), default=os.environ.get("JADIX_DISTRIBUTION_CHANNEL", "stable"))
            parser.add_argument("--offline", action="store_true")
        if operation.startswith("component-") and operation != "component-list":
            parser.add_argument("name", type=identifier)
        if operation == "component-run":
            parser.add_argument("arguments", nargs="*")
    parser = subcommands.add_parser("distribution-pack")
    for name in ("name", "version", "commit", "entry"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)


def execute(arguments, output):
    operation = arguments.operation
    if operation == "distribution-pack":
        return pack(arguments, output)
    home = arguments.home.resolve()
    current = state(home)
    if operation == "component-list":
        return current
    if operation == "component-remove":
        # Deactivate only; retained immutable generations may still be running.
        with lock(home):
            current = state(home)
            removed = current["components"].pop(arguments.name, None)
            activate(home, current)
        return {"removed": removed, "component": arguments.name}
    if operation == "component-run":
        ensure(home, arguments.name, arguments.manifest, arguments.channel, arguments.manifest_sha256, arguments.offline)
    else:
        if not arguments.manifest:
            raise ValueError("missing runtime component; supply --manifest for on-demand installation")
        manifest, identity = catalog(arguments.manifest, arguments.channel, arguments.manifest_sha256, home, arguments.offline)
        names = None if operation == "self-update" else [arguments.name]
        installed = install(home, manifest, names, arguments.offline)
        if operation != "component-run":
            return {"installed": installed, "manifest": identity, "channel": arguments.channel,
                    "launcher": str(Path(__file__).with_name("native.py")), "launcher_command": "component-run cli", "atomic_activation": True}
    return launch(home, arguments.name, arguments.arguments)
