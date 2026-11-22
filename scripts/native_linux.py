"""Linux native build and relocatable bundle support (stdlib only)."""
import json
import os
import re
import shutil
import stat
import subprocess
import sys
from pathlib import Path, PurePosixPath

import native


def doctor():
    packages = {"gtk4": "4.14", "pangocairo": "1.50", "fontconfig": "2.13",
                "webkitgtk-6.0": "2.44", "gstreamer-app-1.0": "1.22", "openssl": "3.0"}
    observations = {}
    for package, minimum in packages.items():
        available = False
        version = None
        if sys.platform == "linux" and shutil.which("pkg-config"):
            check = subprocess.run(["pkg-config", f"--atleast-version={minimum}", package],
                                   capture_output=True, timeout=15)
            available = check.returncode == 0
            probe = subprocess.run(["pkg-config", "--modversion", package],
                                   capture_output=True, text=True, timeout=15)
            version = probe.stdout.strip() if probe.returncode == 0 else None
        observations[package] = {"minimum": minimum, "version": version, "available": available}
    curl = shutil.which("curl")
    return {"available": sys.platform == "linux" and bool(curl) and all(x["available"] for x in observations.values()),
            "packages": observations, "curl": curl,
            "desktop_session": bool(os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY")),
            "runtime_acceptance": False}


def relative(value):
    if not isinstance(value, str) or not value or "\\" in value or ":" in value:
        raise ValueError("invalid Linux bundle path")
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in ("", ".", "..") for part in value.split("/")):
        raise ValueError("Linux bundle paths must be normalized and relative")
    return path


def configuration(value):
    if not isinstance(value, dict):
        raise ValueError("Linux application configuration must be an object")
    identifier = value.get("id", "")
    name = value.get("name", "")
    version = value.get("version", "")
    executable = str(relative(value.get("executable")))
    schemes = value.get("schemes", [])
    assets = value.get("assets", [])
    if (not isinstance(identifier, str) or len(identifier) > 200
            or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)+", identifier)
            or not isinstance(name, str) or not name or len(name) > 128
            or any(character in name for character in "\r\n\x00")
            or not isinstance(version, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?", version)
            or not isinstance(schemes, list) or len(schemes) > 16
            or any(not isinstance(scheme, str) or not re.fullmatch(r"[a-z][a-z0-9+.-]{0,63}", scheme) for scheme in schemes)
            or len(schemes) != len(set(schemes))
            or not isinstance(assets, list) or len(assets) > 256):
        raise ValueError("invalid Linux application identity, desktop name, version, schemes or assets")
    assets = [str(relative(asset)) for asset in assets]
    reserved = {executable, "jadix-native-linux.json", "install-desktop.py", f"share/applications/{identifier}.desktop.in"}
    if len(assets) != len(set(assets)) or any(asset in reserved for asset in assets):
        raise ValueError("duplicate or reserved Linux bundle asset")
    return {"id": identifier, "name": name, "version": version, "executable": executable,
            "schemes": schemes, "assets": assets}


def desktop_entry(config):
    name = config["name"].replace("\\", "\\\\").replace("\t", "\\t")
    mime = ";".join(f"x-scheme-handler/{scheme}" for scheme in config["schemes"])
    return (f"[Desktop Entry]\nType=Application\nName={name}\nExec=@EXECUTABLE@ %U\n"
            f"Terminal=false\nStartupNotify=true\nDBusActivatable=false\n"
            + (f"MimeType={mime};\n" if mime else ""))


# The installer is retained inside the package. Building/packaging does not register
# an application or change the user's desktop associations.
INSTALLER = '''#!/usr/bin/env python3
import json, os, pathlib, re, shutil, subprocess
root = pathlib.Path(__file__).resolve().parent
config = json.loads((root / "jadix-native-linux.json").read_text(encoding="utf-8"))
identifier = config["id"]
if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*(?:\\.[A-Za-z_][A-Za-z0-9_-]*)+", identifier):
    raise SystemExit("invalid desktop application ID")
relative = pathlib.PurePosixPath(config["executable"])
if relative.is_absolute() or ".." in relative.parts or "\\\\" in str(relative):
    raise SystemExit("invalid executable path")
executable = (root / relative).resolve(strict=True)
if not executable.is_relative_to(root) or not executable.is_file() or not os.access(executable, os.X_OK):
    raise SystemExit("invalid executable")
# Desktop Exec has its own quoting rules; percent fields must also be escaped.
quoted = str(executable).replace("%", "%%")
for character in ("\\\\", '"', "`", "$" ):
    quoted = quoted.replace(character, "\\\\" + character)
content = (root / "share/applications" / (identifier + ".desktop.in")).read_text(encoding="utf-8")
destination = pathlib.Path(os.environ.get("XDG_DATA_HOME", pathlib.Path.home() / ".local/share")) / "applications"
destination.mkdir(parents=True, exist_ok=True)
path = destination / (identifier + ".desktop")
with path.open("x", encoding="utf-8") as stream:
    stream.write(content.replace("@EXECUTABLE@", '"' + quoted + '"'))
path.chmod(0o644)
if shutil.which("update-desktop-database"):
    subprocess.run(["update-desktop-database", str(destination)], check=True, timeout=30)
print(path)
'''


def package_files(observation):
    files = observation.get("files")
    config = configuration(observation.get("application"))
    if not isinstance(files, dict) or not files or len(files) > 260:
        raise ValueError("invalid Linux bundle inventory")
    required = {config["executable"], "jadix-native-linux.json", "install-desktop.py",
                f"share/applications/{config['id']}.desktop.in", *config["assets"]}
    if set(files) != required:
        raise ValueError("Linux bundle inventory disagrees with application configuration")
    for name, identity in files.items():
        relative(name)
        if (not isinstance(identity, dict) or type(identity.get("bytes")) is not int
                or identity["bytes"] < 0 or identity["bytes"] > 128 * 1024 * 1024
                or not re.fullmatch(r"[0-9a-f]{64}", str(identity.get("sha256", "")))):
            raise ValueError("invalid Linux bundle file identity")
    if sum(identity["bytes"] for identity in files.values()) > 1024 * 1024 * 1024:
        raise ValueError("Linux bundle byte budget exceeded")
    return files


def bundle_files(bundle):
    bundle = Path(bundle)
    files = {}
    total = 0
    for path in bundle.rglob("*"):
        if path.is_symlink():
            raise ValueError("Linux bundles cannot contain symbolic links")
        if path.is_dir():
            continue
        if not path.is_file():
            raise ValueError("Linux bundles contain only regular files")
        if len(files) >= 260 or path.stat().st_size > 128 * 1024 * 1024:
            raise ValueError("Linux bundle file budget exceeded")
        total += path.stat().st_size
        if total > 1024 * 1024 * 1024:
            raise ValueError("Linux bundle byte budget exceeded")
        files[path.relative_to(bundle).as_posix()] = native.file_identity(path)
    return files


def validate_artifact(observation):
    expected = package_files(observation)
    bundle = Path(observation["bundle"]).resolve(strict=True)
    config = configuration(observation["application"])
    executable = bundle / config["executable"]
    metadata = bundle / "jadix-native-linux.json"
    if metadata.stat().st_size > 65536 or configuration(json.loads(metadata.read_text(encoding="utf-8"))) != config:
        raise ValueError("Linux application metadata disagrees with build report")
    if bundle_files(bundle) != expected or Path(observation["executable"]).resolve() != executable:
        raise ValueError("retained Linux artifact identity mismatch")
    if native.file_identity(executable) != observation.get("identity"):
        raise ValueError("retained Linux executable identity mismatch")
    with executable.open("rb") as stream:
        header = stream.read(20)
    if (len(header) != 20 or header[:6] != b"\x7fELF\x02\x01"
            or int.from_bytes(header[16:18], "little") not in (2, 3)
            or int.from_bytes(header[18:20], "little") not in (62, 183)):
        raise ValueError("expected a Linux x86-64 or AArch64 ELF executable")
    if sys.platform == "linux" and not executable.stat().st_mode & stat.S_IXUSR:
        raise ValueError("Linux application lacks executable permission")
    return executable


def build(arguments, output, capture):
    if sys.platform != "linux":
        raise ValueError("Linux native builds require a Linux host (WSL is supported)")
    project = arguments.project.resolve() if arguments.project else native.ROOT / "examples/native-controls"
    if project.is_file():
        project = project.parent
    manifest = project / "Cargo.toml"
    probe = capture(["cargo", "metadata", "--no-deps", "--format-version", "1", "--manifest-path", str(manifest)] + (["--offline"] if arguments.offline else []),
                    project, output.with_suffix(".metadata.log"))
    metadata = json.loads(probe)
    selected = next((package for package in metadata["packages"] if Path(package["manifest_path"]).resolve() == manifest), None)
    if selected is None:
        raise ValueError("project manifest must select one Cargo package")
    binary = arguments.bin
    if binary == "native-windows":
        binary = selected["name"] if arguments.project else "native-linux"
    if not any(target["name"] == binary and "bin" in target["kind"] for target in selected["targets"]):
        raise ValueError("selected Linux binary does not exist in the package")
    command = ["cargo", "build", "--manifest-path", str(manifest), "--bin", binary,
               "--message-format=json-render-diagnostics"]
    features = arguments.features or ("linux,consistent-ui" if not arguments.project else None)
    if features:
        command += ["--features", features]
    if arguments.release:
        command.append("--release")
    if arguments.offline:
        command.append("--offline")
    result = capture(command, project, output.with_suffix(".log"))
    products = [item for line in result.splitlines() if line.startswith("{")
                for item in [json.loads(line)] if item.get("reason") == "compiler-artifact"
                and item.get("executable") and item.get("target", {}).get("name") == binary]
    if len(products) != 1:
        raise ValueError("Cargo did not report exactly one Linux executable")
    config_path = project / "jadix-native-linux.json"
    config = configuration(json.loads(config_path.read_text(encoding="utf-8")) if config_path.exists() else {
        "id": "org.jadix.NativeControls", "name": "Jadix native controls", "version": selected["version"],
        "executable": f"bin/{binary}", "schemes": ["jadix"]})
    if config["version"] != selected["version"]:
        raise ValueError("Linux application version must match its Cargo package")
    from native_workflow import new_directory
    bundle = new_directory(output.parent / f"{output.stem}-files")
    executable = bundle / config["executable"]
    executable.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(products[0]["executable"], executable)
    executable.chmod(0o755)
    for name in config["assets"]:
        source = project / name
        if source.is_symlink() or not source.resolve(strict=True).is_relative_to(project) or not source.is_file():
            raise ValueError("Linux asset escapes project or is not a regular file")
        target = bundle / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        target.chmod(0o644)
    (bundle / "jadix-native-linux.json").write_text(json.dumps(config, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    desktop = bundle / "share/applications" / f"{config['id']}.desktop.in"
    desktop.parent.mkdir(parents=True, exist_ok=True)
    desktop.write_text(desktop_entry(config), encoding="utf-8")
    (bundle / "install-desktop.py").write_text(INSTALLER, encoding="utf-8")
    observation = {"kind": "linux-native-build", "bundle": str(bundle), "executable": str(executable),
                   "identity": native.file_identity(executable), "application": config,
                   "files": bundle_files(bundle), "runtime_acceptance": False, "signed": False,
                   "limits": ["Requires GTK 4.14, Pango 1.50, WebKitGTK 6 (2.44), GStreamer 1.22, Fontconfig 2.13, OpenSSL 3 and curl on the target Linux host.",
                              "Build and integrity verification do not prove desktop, accessibility, portal or GPU acceptance.",
                              "Unsigned relocatable application bundle; distribution signing is a separate release step."]}
    validate_artifact(observation)
    return observation


def run(executable, report):
    if sys.platform != "linux" or not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
        raise ValueError("running a Linux native application requires a Linux desktop session")
    process = subprocess.Popen([str(executable)], cwd=report["observations"]["bundle"],
                               start_new_session=True, stdin=subprocess.DEVNULL,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return {"pid": process.pid, "started": True, "runtime_acceptance": False}
