"""Android source runtime entry point, usable from checkout or a release component."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import uuid

import native
import native_android
import native_workflow


def project_root(path):
    path = path.resolve()
    if path.is_file():
        path = path.parent
    for candidate in (path, *path.parents):
        if (candidate / "native-android.json").is_file() and (candidate / "Cargo.toml").is_file():
            return candidate
    raise ValueError("project must be inside an Android project with native-android.json and Cargo.toml")


def executable(path, name):
    path = path.resolve()
    if not path.is_file():
        raise ValueError(f"{name} executable is unavailable: {path}")
    return path


def adb_command(adb, serial, *command):
    result = subprocess.run([str(adb), "-s", serial, *command], check=True,
                            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120,
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    return result.stdout.strip()


def run(arguments):
    project = project_root(arguments.project)
    sdk = arguments.android_sdk
    if sdk is None:
        raise ValueError("supply --android-sdk or ANDROID_HOME")
    sdk = sdk.resolve()
    java = arguments.java_home
    if java is None:
        raise ValueError("supply --java-home or JAVA_HOME")
    java = java.resolve()
    executable(java / "bin" / ("javac.exe" if os.name == "nt" else "javac"), "javac")
    native_android.select_sdk(sdk, arguments.compile_sdk, arguments.build_tools)
    ndk = arguments.ndk
    if ndk is None:
        installed = native_android.sdk_inventory(sdk)["ndks"]
        if len(installed) != 1:
            raise ValueError("select --ndk or NDK_HOME when the SDK does not contain exactly one NDK")
        ndk = sdk / "ndk" / installed[0]
    ndk = ndk.resolve()
    if not (ndk / "source.properties").is_file():
        raise ValueError("selected NDK has no source.properties")
    if not re.fullmatch(r"[A-Za-z0-9_.:-]+", arguments.serial):
        raise ValueError("an exact Android device serial is required")
    adb = executable(arguments.adb or sdk / "platform-tools" / ("adb.exe" if os.name == "nt" else "adb"), "adb")
    if adb_command(adb, arguments.serial, "get-state") != "device":
        raise ValueError("selected Android device is not ready")
    api = adb_command(adb, arguments.serial, "shell", "getprop", "ro.build.version.sdk")
    if not api.isdigit() or int(api) < 26:
        raise ValueError("Android source runtime requires API 26 or newer")
    abis = adb_command(adb, arguments.serial, "shell", "getprop", "ro.product.cpu.abilist").split(",")
    targets = {"arm64-v8a": "aarch64-linux-android", "x86_64": "x86_64-linux-android"}
    target = arguments.target or next((targets[abi] for abi in abis if abi in targets), None)
    if target is None or not any(targets.get(abi) == target for abi in abis):
        raise ValueError("selected Android target ABI is unavailable on the device")
    directory = arguments.output.resolve() if arguments.output else (
        Path(os.environ.get("JADIX_DISTRIBUTION_HOME", Path.home() / ".jadix/distribution")) / "runs" / uuid.uuid4().hex)
    if directory.is_relative_to(project) or directory.is_relative_to(native.ROOT):
        raise ValueError("runtime output must be outside application and runtime source trees")
    directory = native_workflow.new_directory(directory)
    home = Path(os.environ.get("JADIX_DISTRIBUTION_HOME", Path.home() / ".jadix/distribution"))
    key = native_android.development_keystore(java, home)
    parser = argparse.ArgumentParser()
    native_workflow.register(parser.add_subparsers(dest="operation", required=True))
    command = ["build", "--platform", "android", "--project", str(project), "--android-sdk", str(sdk),
               "--ndk", str(ndk), "--java-home", str(java), "--android-target", target,
               "--keystore", str(key), "--key-alias", "jadix-debug",
               "--output", str(directory / "build.json")]
    for flag in ("release", "offline"):
        if getattr(arguments, flag):
            command.append("--" + flag)
    for option in ("compile_sdk", "build_tools"):
        value = getattr(arguments, option)
        if value is not None:
            command += ["--" + option.replace("_", "-"), str(value)]
    build = native_workflow.build(parser.parse_args(command), directory / "build.json")
    (directory / "build.json").write_text(json.dumps({"schema": 1, "operation": "build", "observations": build,
        "runtime_acceptance": False}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    report = json.loads(Path(build["build_report"]).read_text(encoding="utf-8"))
    package = report["package_name"]
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)+", package):
        raise ValueError("invalid retained Android package name")
    if arguments.release and report.get("rust_profile") != "release":
        raise ValueError("runtime builder did not produce the requested optimized profile")
    if native.file_identity(Path(build["apk"])) != build["apk_identity"]:
        raise ValueError("Android artifact changed before installation")
    installed = adb_command(adb, arguments.serial, "install", "-r", build["apk"])
    if not any(line.strip() == "Success" for line in installed.splitlines()):
        raise ValueError("adb did not confirm installation")
    launched = adb_command(adb, arguments.serial, "shell", "am", "start", "-W", "-n", package + "/dev.jadix.nativeui.MainActivity")
    if "Status: ok" not in launched or "Error:" in launched:
        raise ValueError("Android activity manager did not confirm launch")
    receipt = {"schema": 1, "project": str(project), "serial": arguments.serial, "api": int(api),
               "target": target, "package_name": package, "build": build,
               "installed": True, "launched": True, "runtime_acceptance": False}
    (directory / "run.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return receipt


def main():
    def environment_path(name):
        return Path(os.environ[name]) if os.environ.get(name) else None
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", type=Path)
    parser.add_argument("--android-sdk", type=Path, default=environment_path("ANDROID_HOME") or environment_path("ANDROID_SDK_ROOT"))
    parser.add_argument("--ndk", type=Path, default=environment_path("NDK_HOME"))
    parser.add_argument("--java-home", type=Path, default=environment_path("JAVA_HOME"))
    parser.add_argument("--compile-sdk", type=int)
    parser.add_argument("--build-tools")
    parser.add_argument("--target", choices=["aarch64-linux-android", "x86_64-linux-android"])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--offline", action="store_true")
    try:
        print(json.dumps(run(parser.parse_args()), ensure_ascii=False, indent=2))
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Android runtime failed: {error}\n")


if __name__ == "__main__":
    main()
