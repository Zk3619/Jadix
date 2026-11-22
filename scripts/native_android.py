"""Android SDK selection and signing policy shared by the build and doctor commands."""

import os
import hashlib
from pathlib import Path
import re
import subprocess


def sdk_inventory(directory):
    directory = Path(directory).resolve()
    platforms = sorted(int(path.name[8:]) for path in (directory / "platforms").glob("android-*")
                       if path.name[8:].isdigit() and (path / "android.jar").is_file())
    versions = sorted((path.name for path in (directory / "build-tools").glob("*")
                       if path.is_dir() and re.fullmatch(r"\d+(?:\.\d+){1,3}", path.name)),
                      key=lambda version: tuple(map(int, version.split("."))))
    ndks = sorted(path.name for path in (directory / "ndk").glob("*")
                  if (path / "source.properties").is_file())
    return {"directory": str(directory), "platforms": platforms, "build_tools": versions, "ndks": ndks}


def select_sdk(directory, compile_sdk=None, build_tools=None):
    inventory = sdk_inventory(directory)
    compatible = [api for api in inventory["platforms"] if api >= 36]
    api = compile_sdk if compile_sdk is not None else max(compatible, default=0)
    version = build_tools if build_tools is not None else next(reversed(inventory["build_tools"]), None)
    if api < 36 or api not in inventory["platforms"]:
        raise ValueError("Android compilation requires an installed SDK platform >= 36")
    if version not in inventory["build_tools"]:
        raise ValueError("Select an installed stable Android build-tools version")
    return api, version


def signing_options(keystore, alias, store_env, key_env, production, environment=None):
    environment = os.environ if environment is None else environment
    if production and (keystore is None or not alias or not store_env):
        raise ValueError("--production requires --keystore, --key-alias and --ks-pass-env")
    if keystore is None and any((alias, store_env, key_env)):
        raise ValueError("Custom signing options require --keystore")
    if keystore is not None and not Path(keystore).is_file():
        raise ValueError("--keystore must name an existing file")
    if key_env and not store_env:
        raise ValueError("--key-pass-env requires --ks-pass-env")
    for name in (store_env, key_env):
        if name and (not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) or not environment.get(name)):
            raise ValueError("Signing password environment variable is invalid or empty")
    result = ["--ks-pass", f"env:{store_env}" if store_env else "pass:android"]
    if alias:
        result += ["--ks-key-alias", alias]
    if key_env:
        result += ["--key-pass", f"env:{key_env}"]
    return result


def signing_identity(java_home, keystore, alias, store_env=None):
    """Resolve the selected certificate before compilation, without exposing passwords."""
    keytool = Path(java_home) / "bin" / ("keytool.exe" if os.name == "nt" else "keytool")
    password = ["-storepass:env", store_env] if store_env else ["-storepass", "android"]
    command = [str(keytool), "-exportcert", "-keystore", str(Path(keystore).resolve()),
               "-alias", alias or "jadix-debug", *password]
    result = subprocess.run(command, capture_output=True, timeout=30,
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    if result.returncode or not result.stdout or len(result.stdout) > 1024 * 1024:
        # Tool diagnostics can echo environment values; keep them out of logs.
        raise ValueError("Cannot read the signing certificate; check keystore, alias and password environment")
    return {"alias": alias or "jadix-debug", "certificate_sha256": hashlib.sha256(result.stdout).hexdigest()}


def development_keystore(java_home, home):
    """Retain one development certificate across runtime upgrades and APK installs."""
    import native_distribution
    import tempfile
    home = Path(home).resolve()
    key = home / "keys" / "android-debug.jks"
    with native_distribution.lock(home):
        if not key.exists():
            key.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.TemporaryDirectory(prefix="android-key-", dir=key.parent) as temporary:
                pending = Path(temporary) / key.name
                tool = Path(java_home) / "bin" / ("keytool.exe" if os.name == "nt" else "keytool")
                result = subprocess.run([str(tool), "-genkeypair", "-keystore", str(pending),
                    "-storepass", "android", "-keypass", "android", "-alias", "jadix-debug",
                    "-dname", "CN=Jadix Debug", "-keyalg", "RSA", "-validity", "3650"],
                    capture_output=True, timeout=60,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                if result.returncode or not pending.is_file():
                    raise ValueError("Cannot create the Android development signing certificate")
                os.replace(pending, key)
        signing_identity(java_home, key, "jadix-debug")
    return key
