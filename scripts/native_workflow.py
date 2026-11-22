"""Local native delivery commands; package integrity never implies signing or OS acceptance."""

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import uuid
import zipfile

import native


def register(subcommands):
    import native_updater
    native_updater.register(subcommands)
    create = subcommands.add_parser("create", help="Create an independent Windows, Linux, macOS, Android or OHOS native Rust application")
    create.add_argument("destination", type=Path)
    create.add_argument("--name", default="native-app")
    create.add_argument("--platform", choices=["windows", "android", "ohos", "macos", "linux"], default="windows")
    build = subcommands.add_parser("build", help="Build and retain an identified native artifact")
    build.add_argument("--platform", choices=["windows", "android", "ohos", "macos", "linux"], default="windows")
    build.add_argument("--project", type=Path)
    build.add_argument("--bin", default="native-windows")
    build.add_argument("--workbench", action="store_true")
    build.add_argument("--offline", action="store_true")
    build.add_argument("--release", action="store_true", help="Use the platform builder's optimized native profile")
    build.add_argument("--features", help="Comma-separated Cargo features for the selected application")
    build.add_argument("--sdk", type=Path, help="Explicit OHOS SDK directory")
    build.add_argument("--android-sdk", type=Path, help="Explicit Android SDK directory")
    build.add_argument("--ndk", type=Path, help="Explicit Android NDK directory")
    build.add_argument("--java-home", type=Path, help="Explicit JDK directory")
    build.add_argument("--production", action="store_true", help="Build Android with an existing publisher signature and debuggable=false")
    build.add_argument("--keystore", type=Path)
    build.add_argument("--key-alias")
    build.add_argument("--ks-pass-env")
    build.add_argument("--key-pass-env")
    build.add_argument("--compile-sdk", type=int)
    build.add_argument("--build-tools")
    build.add_argument("--android-target", choices=["aarch64-linux-android", "x86_64-linux-android"], default="aarch64-linux-android")
    build.add_argument("--extended", action="store_true", help="Include Android Controls dialogs, switches and editor probes")
    build.add_argument("--large-list", action="store_true", help="Build Android Controls with 1000 retained rows")
    build.add_argument("--architecture", choices=["x86_64", "arm64"], default="x86_64")
    build.add_argument("--native-only", action="store_true", help="Build the OHOS native library without a HAP")
    run = subcommands.add_parser("run", help="Run a retained Windows or Android artifact with fresh probe evidence")
    run_source = run.add_mutually_exclusive_group(required=True)
    run_source.add_argument("--artifact", type=Path)
    run_source.add_argument("--runtime", help="Install and run a version-matched runtime component for a source project")
    run.add_argument("--project", type=Path)
    run.add_argument("--release", action="store_true")
    run.add_argument("--offline", action="store_true")
    run.add_argument("--home", type=Path, default=Path(os.environ.get("JADIX_DISTRIBUTION_HOME", Path.home() / ".jadix/distribution")))
    run.add_argument("--manifest", default=os.environ.get("JADIX_DISTRIBUTION_MANIFEST"))
    run.add_argument("--manifest-sha256", default=os.environ.get("JADIX_DISTRIBUTION_MANIFEST_SHA256"))
    run.add_argument("--channel", choices=("stable", "nightly"), default=os.environ.get("JADIX_DISTRIBUTION_CHANNEL", "stable"))
    run.add_argument("--runtime-argument", action="append", default=[], help="Additional runtime argument; use = for leading dashes")
    run.add_argument("--adb", type=Path, help="Explicit Android adb executable")
    run.add_argument("--serial", help="Exact Android device serial; no default device selection")
    run.add_argument("--process-recovery", action="store_true", help="Also probe Android background-process recovery")
    package = subcommands.add_parser("package", help="Package a verified Windows, macOS, Linux, Android or OHOS build as an unsigned local ZIP")
    package.add_argument("--artifact", type=Path, required=True)
    inspect = subcommands.add_parser("inspect", help="Check retained artifact identities without execution")
    inspect.add_argument("--artifact", type=Path, required=True)
    verify = subcommands.add_parser("verify", help="Verify all ZIP members against its retained build manifest")
    verify.add_argument("--package", type=Path, required=True)
    for parser in [create, build, run, package, inspect, verify]:
        parser.add_argument("--output", type=Path, required=True)


def capture(arguments, cwd, log):
    environment = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0")
    result = subprocess.run([str(value) for value in arguments], cwd=cwd, env=environment,
                            capture_output=True, text=True, encoding="utf-8", errors="replace",
                            timeout=600, creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    log.write_text(result.stdout + result.stderr, encoding="utf-8")
    if result.returncode:
        raise ValueError(f"command failed ({result.returncode}); see {log}")
    return result.stdout


def new_directory(path):
    path = validate_new_directory(path)
    path.mkdir(parents=True)
    return path


def validate_new_directory(path):
    path = path.resolve()
    if path.exists() or any((parent / "snapshot.json").exists() for parent in [path, *path.parents]):
        raise ValueError(f"directory must be new and outside archived snapshots: {path}")
    return path


def ohos_artifact(destination, architecture, native_only):
    build_report = destination / "build.json"
    report = json.loads(build_report.read_text(encoding="utf-8"))
    expected_target = {"x86_64": "x86_64-unknown-linux-ohos",
                       "arm64": "aarch64-unknown-linux-ohos"}[architecture]
    abi, machine = {"x86_64": ("x86_64", 62), "arm64": ("arm64-v8a", 183)}[architecture]
    library = destination / "libjadix_native_controls.so"
    if (report.get("schema") != 1 or "error" in report or report.get("target") != expected_target
            or report.get("abi") != abi or report.get("source_unchanged") is not True
            or report.get("native_built") is not True or report.get("signed") is not False
            or report.get("runtime_acceptance") is not False or not library.is_file()
            or not report.get("commands")
            or any(command.get("exit_code") != 0 for command in report["commands"])
            or native.file_identity(library) != report.get("library")):
        raise ValueError("OHOS builder report or native library identity is incomplete")
    with library.open("rb") as stream:
        header = stream.read(20)
    if (len(header) != 20 or header[:7] != b"\x7fELF\x02\x01\x01"
            or int.from_bytes(header[16:18], "little") != 3
            or int.from_bytes(header[18:20], "little") != machine):
        raise ValueError("OHOS native library ELF ABI mismatch")
    if not native_only and (report.get("hap_built") is not True or not isinstance(report.get("hap"), dict)):
        raise ValueError("OHOS HAP build was requested but no identified HAP was produced")
    if native_only and (report.get("hap_built") is not False or report.get("hap") is not None):
        raise ValueError("OHOS native-only report unexpectedly contains a HAP")
    config = report.get("configuration") if report.get("application") == "project" else None
    if report.get("application") == "project" and (
            not isinstance(config, dict) or config.get("schema") != 1
            or not isinstance(config.get("application_id"), str)
            or not re.fullmatch(r"dev\.jadix\.created\.[a-z][a-z0-9_]{0,63}", config["application_id"])
            or config.get("module") != "entry" or config.get("ability") != "EntryAbility"):
        raise ValueError("OHOS retained project configuration is invalid")
    if not native_only:
        hap = destination / f"jadix-native-controls-{architecture}-unsigned.hap"
        if native.file_identity(hap) != report["hap"]:
            raise ValueError("retained OHOS HAP identity mismatch")
        member = f"libs/{abi}/libjadix_native_controls.so"
        packaged = report.get("packaged_library", {})
        if packaged.get("member") != member:
            raise ValueError("OHOS HAP has no identified packaged library")
        with zipfile.ZipFile(hap) as archive:
            if config:
                if archive.namelist().count("module.json") != 1 or archive.getinfo("module.json").file_size > 1024 * 1024:
                    raise ValueError("OHOS project HAP must contain one bounded module manifest")
                module = json.loads(archive.read("module.json"))
                if (module.get("app", {}).get("bundleName") != config["application_id"]
                        or module.get("module", {}).get("name") != config["module"]
                        or module.get("module", {}).get("mainElement") != config["ability"]):
                    raise ValueError("OHOS project HAP bundle or ability mismatch")
            if archive.namelist().count(member) != 1:
                raise ValueError("OHOS HAP library is missing or duplicated")
            if member_identity(archive, archive.getinfo(member)) != {
                key: packaged.get(key) for key in ["bytes", "sha256"]
            }:
                raise ValueError("OHOS HAP packaged library identity mismatch")
    result = {"kind": "ohos-native-build", "architecture": architecture,
            "native_only": native_only, "build_directory": str(destination),
            "build_report": str(build_report),
            "build_report_identity": native.file_identity(build_report),
            "library": report["library"], "hap": report.get("hap"), "sdk": report["sdk"],
            "signed": False, "runtime_acceptance": False,
            "limits": ["Build and hashes do not establish HAP installation, ArkUI behavior, accessibility or device acceptance."]}
    if config:
        result.update({"application": "project", "configuration": config})
    if "features" in report:
        features = report["features"]
        if ((config and features is not None)
                or (not config and features not in [["ohos"], ["ohos", "popover"], ["ohos", "selection"], ["ohos", "field-workflow"]])):
            raise ValueError("OHOS retained sample features are invalid")
        result["features"] = features
    return result


def android_artifact(destination):
    destination = destination.resolve()
    build_report = destination / "build.json"
    report = json.loads(build_report.read_text(encoding="utf-8"))
    targets = {"aarch64-linux-android": "arm64-v8a", "x86_64-linux-android": "x86_64"}
    target = report.get("target")
    if (report.get("schema") != 1 or target not in targets or report.get("abi") != targets[target]
            or report.get("min_sdk") != 26
            or report.get("source_unchanged") is not True or report.get("signed") is not True
            or report.get("authenticity_verified") is not False or report.get("runtime_acceptance") is not False
            or type(report.get("debuggable")) is not bool or not report.get("build_inputs")
            or not report.get("toolchain")):
        raise ValueError("Android build report is incomplete or inconsistent")
    production = report.get("production", False)
    if (type(production) is not bool or report["debuggable"] != (not production)
            or (production and (report.get("rust_profile") != "release"
                                or not report.get("key_alias") or not report.get("signing_key_identity")))):
        raise ValueError("Android production signing report is inconsistent")
    apk = destination / "native-controls.apk"
    apk_identity = native.file_identity(apk)
    if (apk_identity != report.get("apk_identity") or apk_identity["bytes"] != report.get("apk_bytes")
            or apk_identity["sha256"] != report.get("apk_sha256")):
        raise ValueError("Android APK identity mismatch")
    member = f"lib/{targets[target]}/libjadix_native_controls.so"
    with zipfile.ZipFile(apk) as archive:
        names = [info.filename for info in archive.infolist()]
        if ([name for name in names if name.endswith("/libjadix_native_controls.so")] != [member]
                or any(names.count(name) != 1 for name in ["AndroidManifest.xml", "classes.dex"])):
            raise ValueError("Android APK must contain exactly one target native library")
        info = archive.getinfo(member)
        if member_identity(archive, info) != {"bytes": report.get("library_bytes"), "sha256": report.get("library_sha256")}:
            raise ValueError("Android packaged native library identity mismatch")
        with archive.open(info) as stream:
            header = stream.read(20)
        if (len(header) != 20 or header[:7] != b"\x7fELF\x02\x01\x01"
                or int.from_bytes(header[16:18], "little") != 3
                or int.from_bytes(header[18:20], "little") != (62 if targets[target] == "x86_64" else 183)):
            raise ValueError("Android native library ELF ABI mismatch")
    return {"kind": "android-native-build", "target": target, "abi": targets[target], "min_sdk": 26,
            "build_directory": str(destination), "build_report": str(build_report),
            "build_report_identity": native.file_identity(build_report), "apk": str(apk),
            "apk_identity": apk_identity, "signed": True, "authenticity_verified": False,
            "runtime_acceptance": False, "production": production, "debuggable": report["debuggable"],
            "limits": ["APK signature and archive hashes do not establish publisher authenticity, device runtime or accessibility acceptance."]}


def create(arguments, output):
    if not re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", arguments.name):
        raise ValueError("application name must be a lowercase Cargo package name")
    platform = getattr(arguments, "platform", "windows")
    if platform not in ("windows", "android", "ohos", "macos", "linux"):
        raise ValueError("native create supports Windows, macOS, Android and OHOS templates")
    destination = new_directory(arguments.destination)
    sources = native.ROOT / f"scripts/templates/native-{platform}"
    application_id = "dev.jadix.created." + arguments.name.replace("-", "_")
    replacements = {"@NAME@": arguments.name, "@ROOT@": native.ROOT.as_posix(), "@APPLICATION_ID@": application_id}
    for source in sorted(sources.rglob("*")):
        if source.is_file():
            target = destination / source.relative_to(sources)
            target.parent.mkdir(parents=True, exist_ok=True)
            text = source.read_text(encoding="utf-8")
            for key, value in replacements.items():
                text = text.replace(key, value)
            target.write_text(text, encoding="utf-8", newline="\n")
    shutil.copyfile(native.ROOT / "Cargo.lock", destination / "Cargo.lock")
    capture(["cargo", "update", "--workspace", "--offline"], destination, output.with_suffix(".log"))
    return {"project": str(destination), "name": arguments.name, "platform": platform,
            "source": native.tree_identity(destination), "repository_path_dependencies": True,
            "limits": [f"{platform} native template; other platforms need their own application entry and packaging.",
                       "Path dependencies target this checkout; registry publication is not verified."]}


def build(arguments, output):
    if getattr(arguments, "platform", "windows") == "linux":
        import native_linux
        return native_linux.build(arguments, output, capture)
    if getattr(arguments, "platform", "windows") == "macos":
        import native_macos
        return native_macos.build(arguments, output, capture)
    selected_platform = getattr(arguments, "platform", "windows")
    if selected_platform == "windows" and os.name != "nt":
        raise ValueError("Windows native builds require a Windows host")
    if selected_platform != "android" and (any(getattr(arguments, name, None) for name in ["android_sdk", "ndk", "java_home", "extended", "large_list", "production", "keystore", "key_alias", "ks_pass_env", "key_pass_env", "compile_sdk", "build_tools"])
                                          or getattr(arguments, "android_target", "aarch64-linux-android") != "aarch64-linux-android"):
        raise ValueError("Android SDK, NDK, JDK and target options apply only to --platform android")
    if selected_platform == "android":
        if (getattr(arguments, "workbench", False)
                or getattr(arguments, "features", None) not in (None, "field-workflow") or getattr(arguments, "sdk", None)
                or getattr(arguments, "native_only", False)
                or getattr(arguments, "architecture", "x86_64") != "x86_64"
                or getattr(arguments, "bin", "native-windows") != "native-windows"):
            raise ValueError("Android supports native-controls or a generated --project; SDK, binary and feature overrides are unsupported")
        field = getattr(arguments, "features", None) == "field-workflow"
        if field and any(getattr(arguments, option, None) for option in ["project", "extended", "large_list"]):
            raise ValueError("Android field-workflow requires its own application without project or controls overrides")
        if getattr(arguments, "project", None) and (getattr(arguments, "extended", False) or getattr(arguments, "large_list", False)):
            raise ValueError("Android --extended and --large-list apply only to the controls product")
        if not all([getattr(arguments, "android_sdk", None), getattr(arguments, "ndk", None), getattr(arguments, "java_home", None)]):
            raise ValueError("Android builds require explicit --android-sdk, --ndk and --java-home paths")
        destination = validate_new_directory(output.parent / f"{output.stem}-files")
        builder = native.ROOT / "examples/native-controls/build-android.py"
        command = [sys.executable, str(builder), "--target", arguments.android_target,
                   "--sdk", str(arguments.android_sdk.resolve()), "--ndk", str(arguments.ndk.resolve()),
                   "--java-home", str(arguments.java_home.resolve()), "--output", str(destination)]
        if getattr(arguments, "project", None):
            command += ["--project", str(arguments.project.resolve())]
        if getattr(arguments, "offline", False):
            command.append("--offline")
        if getattr(arguments, "release", False):
            command.append("--release")
        if getattr(arguments, "production", False):
            command.append("--production")
        for option in ["keystore", "key_alias", "ks_pass_env", "key_pass_env", "compile_sdk", "build_tools"]:
            value = getattr(arguments, option, None)
            if value is not None:
                command += ["--" + option.replace("_", "-"), str(value.resolve() if isinstance(value, Path) else value)]
        if field:
            command.append("--field")
        if getattr(arguments, "extended", False):
            command.append("--extended")
        if getattr(arguments, "large_list", False):
            command.append("--large-list")
        result = subprocess.run(command, cwd=native.ROOT, capture_output=True, text=True,
                                encoding="utf-8", errors="replace", timeout=1800,
                                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        workflow_log = output.with_suffix(".log")
        workflow_log.write_text(result.stdout + result.stderr, encoding="utf-8")
        if result.returncode:
            raise ValueError(f"Android build failed ({result.returncode}); see {workflow_log} and {destination / 'build.json'} when available")
        observation = android_artifact(destination)
        if field:
            retained = json.loads(Path(observation["build_report"]).read_text(encoding="utf-8"))
            if retained.get("application") != "field" or retained.get("package_name") != "dev.jadix.field":
                raise ValueError("Android builder did not retain the requested field application")
        return observation
    if getattr(arguments, "platform", "windows") == "ohos":
        if (getattr(arguments, "workbench", False)
                or getattr(arguments, "bin", "native-windows") != "native-windows"):
            raise ValueError("OHOS supports the fixed native-controls product or a generated --project; workbench and binary overrides are unsupported")
        features = getattr(arguments, "features", None)
        if features and (features not in ["popover", "selection", "field-workflow"] or getattr(arguments, "project", None)):
            raise ValueError("OHOS sample features support only --features popover, selection or field-workflow without --project")
        if not getattr(arguments, "sdk", None):
            raise ValueError("OHOS builds require an explicit --sdk directory")
        destination = validate_new_directory(output.parent / f"{output.stem}-files")
        builder = native.ROOT / "examples/native-controls/build-ohos.py"
        command = [sys.executable, str(builder), "--sdk", str(arguments.sdk.resolve()),
                   "--architecture", arguments.architecture, "--output", str(destination)]
        if getattr(arguments, "project", None):
            command += ["--project", str(arguments.project.resolve())]
        if features:
            command += ["--features", features]
        if getattr(arguments, "native_only", False):
            command.append("--native-only")
        if getattr(arguments, "release", False):
            command.append("--release")
        if getattr(arguments, "offline", False):
            command.append("--offline")
        result = subprocess.run(command, cwd=native.ROOT, capture_output=True, text=True,
                                encoding="utf-8", errors="replace", timeout=1800,
                                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        workflow_log = output.with_suffix(".log")
        workflow_log.write_text(result.stdout + result.stderr, encoding="utf-8")
        if result.returncode:
            raise ValueError(f"OHOS build failed ({result.returncode}); see {workflow_log} and {destination / 'build.json'} when available")
        observation = ohos_artifact(destination, arguments.architecture, getattr(arguments, "native_only", False))
        expected_features = None if getattr(arguments, "project", None) else ["ohos", *([features] if features else [])]
        if observation.get("features") != expected_features:
            raise ValueError("OHOS builder did not retain the requested sample features")
        return observation
    if (getattr(arguments, "sdk", None) or getattr(arguments, "native_only", False)
            or getattr(arguments, "architecture", "x86_64") != "x86_64"):
        raise ValueError("SDK, native-only and ARM64 options apply only to --platform ohos")
    destination = new_directory(output.parent / f"{output.stem}-files")
    project = arguments.project.resolve() if arguments.project else native.ROOT / "examples/native-controls"
    binary = "native-workbench" if arguments.workbench else arguments.bin
    if not re.fullmatch(r"[a-zA-Z0-9_-]+", binary):
        raise ValueError("invalid Cargo binary name")
    before = native.tree_identity(project)
    workspace = {name: native.file_identity(native.ROOT / name) for name in ["Cargo.toml", "Cargo.lock"]}
    dependencies = [native.tree_identity(native.ROOT / "crates" / name)
                    for name in ["jadix", "jadix_core", "jadix_components", "jadix_native", "jadix_macros",
                                 "jadix_tasks", "jadix_layout", "jadix_style_tokens", "jadix_taffy",
                                 "jadix_theme", "jadix_animation", "jadix_document", "jadix_hot_reload", "jadix_wgpu", "jadix_shell"]]
    command = ["cargo", "build", "--locked", "--manifest-path", project / "Cargo.toml", "--bin", binary,
               "--message-format=json-render-diagnostics"]
    if getattr(arguments, "release", False):
        command.append("--release")
    features = getattr(arguments, "features", None)
    if features:
        if not re.fullmatch(r"[a-zA-Z0-9_/-]+(?:,[a-zA-Z0-9_/-]+)*", features):
            raise ValueError("invalid Cargo feature list")
        command += ["--features", features]
    elif not arguments.project:
        command += ["--features", "windows"]
    if arguments.offline:
        command += ["--offline"]
    stdout = capture(command, project, destination / "cargo.log")
    if before != native.tree_identity(project) or dependencies != [
        native.tree_identity(Path(row["directory"])) for row in dependencies
    ] or workspace != {name: native.file_identity(native.ROOT / name) for name in workspace}:
        raise ValueError("application or framework source changed during compilation")
    executables = []
    for line in stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            executables.append(Path(message["executable"]))
    if len(executables) != 1:
        raise ValueError("build did not identify exactly one executable")
    executable = destination / f"{binary}.exe"
    shutil.copyfile(executables[0], executable)
    return {"kind": "windows-native-build", "executable": str(executable),
            "identity": native.file_identity(executable), "project": before, "framework": dependencies,
            "workspace": workspace, "source_unchanged": True,
            "command": [str(value) for value in command], "signed": False,
            "limits": ["Unsigned development executable; installer, Authenticode, runtime redistribution and OS acceptance remain separate."]}


def artifact(path):
    report = json.loads(path.read_text(encoding="utf-8"))
    if report.get("schema") != 1 or report.get("operation") != "build":
        raise ValueError("expected a native build report")
    result = report["observations"]
    if result.get("kind") == "linux-native-build":
        import native_linux
        return native_linux.validate_artifact(result), report
    if result.get("kind") == "android-native-build":
        directory = Path(result["build_directory"]).resolve()
        build_report = Path(result["build_report"]).resolve()
        if (build_report != directory / "build.json"
                or native.file_identity(build_report) != result.get("build_report_identity")):
            raise ValueError("retained Android build report path mismatch")
        current = android_artifact(directory)
        if result != current or Path(result["apk"]).resolve() != Path(current["apk"]).resolve():
            raise ValueError("retained Android artifact identity mismatch")
        return Path(result["apk"]).resolve(), report
    if result.get("kind") == "ohos-native-build":
        directory = Path(result["build_directory"]).resolve()
        build_report = Path(result["build_report"]).resolve()
        if (build_report != directory / "build.json"
                or native.file_identity(build_report) != result.get("build_report_identity")):
            raise ValueError("retained OHOS build report path mismatch")
        library = directory / "libjadix_native_controls.so"
        if result != ohos_artifact(directory, result["architecture"], result["native_only"]):
            raise ValueError("retained OHOS artifact identity mismatch")
        return library, report
    if result.get("kind") == "macos-native-build":
        import native_macos
        bundle = Path(result["bundle"]).resolve()
        if native_macos.bundle_files(bundle) != native_macos.package_files(result):
            raise ValueError("retained macOS bundle identity mismatch")
        executable = Path(result["executable"]).resolve()
        if executable.parent != bundle / "Contents/MacOS" or native.file_identity(executable) != result["identity"]:
            raise ValueError("retained macOS executable path or identity mismatch")
        return executable, report
    executable = Path(result["executable"]).resolve()
    if result.get("kind") != "windows-native-build" or executable.suffix.lower() != ".exe":
        raise ValueError("unsupported retained artifact kind")
    if native.file_identity(executable) != result["identity"]:
        raise ValueError("retained executable identity mismatch")
    return executable, report


def run_android(arguments, output, apk, report):
    if not getattr(arguments, "adb", None) or not getattr(arguments, "serial", None):
        raise ValueError("Android run requires explicit --adb and --serial")
    adb = arguments.adb.resolve()
    serial = arguments.serial
    if not adb.is_file() or not re.fullmatch(r"[A-Za-z0-9_.:\[\]%-]+", serial) or serial.startswith("-"):
        raise ValueError("Android run requires an existing adb executable and an exact device serial")
    build = report["observations"]
    retained = json.loads(Path(build["build_report"]).read_text(encoding="utf-8"))
    project = retained.get("application") == "project"
    if project:
        package = retained.get("package_name", "")
        if not isinstance(package, str) or not re.fullmatch(r"dev\.jadix\.created\.[a-z][a-z0-9_]{0,63}", package):
            raise ValueError("Android project run requires its generated package ID")
    elif (retained.get("application") != "controls" or retained.get("package_name") != "dev.jadix.nativeui"
            or type(retained.get("extended")) is not bool
            or type(retained.get("retained_rows")) is not int or retained["retained_rows"] not in (100, 1000)):
        raise ValueError("Android run requires the identified controls application and supported row count")
    directory = new_directory(output.parent / f"{output.stem}-files")
    nonce = uuid.uuid4().hex
    runner = native.ROOT / ("scripts/acceptance/native-android-project-check.py" if project else "scripts/acceptance/native-android-check.py")
    ownership_runner = native.ROOT / "scripts/acceptance/native-android-check.py"
    ownership_identity = native.file_identity(ownership_runner)
    runner_identity = native.file_identity(runner)
    adb_identity = native.file_identity(adb)
    prefix = [adb, "-s", serial]

    def query(name, command):
        return capture([*prefix, *command], directory, directory / f"device-{name}.log").strip()

    state = query("state", ["get-state"])
    if state != "device" or query("serial", ["get-serialno"]) != serial:
        raise ValueError("selected Android device is unavailable or its serial differs")
    abis = query("abis", ["shell", "getprop", "ro.product.cpu.abilist"]).split(",")
    sdk = query("sdk", ["shell", "getprop", "ro.build.version.sdk"])
    emulator = query("emulator", ["shell", "getprop", "ro.kernel.qemu"])
    device = {"serial": serial, "state": state, "abis": abis, "sdk": sdk,
              "emulator_property": emulator,
              "kind": "reported-emulator" if emulator == "1" else "unconfirmed",
              "adb": str(adb), "adb_identity": adb_identity}
    (directory / "device.json").write_text(json.dumps(device, indent=2) + "\n", encoding="utf-8")
    if build["abi"] not in abis or not sdk.isdecimal() or int(sdk) < 26:
        raise ValueError("selected Android device ABI or SDK does not support the retained APK")
    if native.file_identity(adb) != adb_identity:
        raise ValueError("adb changed during device discovery")
    checks_directory = directory / "checks"
    command = [sys.executable, runner, "--apk", apk, "--adb", adb, "--serial", serial,
               "--nonce", nonce, "--output", checks_directory]
    if not project and retained["extended"]:
        command.append("--extended")
    if not project and retained["retained_rows"] == 1000:
        command.append("--large-list")
    if getattr(arguments, "process_recovery", False):
        command.append("--process-recovery")
    capture(command, native.ROOT, directory / "process.log")
    checks = checks_directory / "checks.json"
    observed = json.loads(checks.read_text(encoding="utf-8"))
    if not isinstance(observed, dict):
        raise ValueError("Android probe receipt must be an object")
    cleanup = observed.get("cleanup", {})
    recovery = observed.get("process_recovery") or {}
    if not isinstance(cleanup, dict) or not isinstance(recovery, dict):
        raise ValueError("Android probe cleanup or process recovery receipt is malformed")
    if project and getattr(arguments, "process_recovery", False):
        if (recovery.get("nonce") != nonce or recovery.get("package_name") != retained["package_name"]
                or not re.fullmatch(r"[1-9][0-9]*", str(recovery.get("before_pid", "")))
                or not re.fullmatch(r"[1-9][0-9]*", str(recovery.get("after_pid", "")))
                or recovery.get("before_pid") == recovery.get("after_pid")
                or recovery.get("value") != "recovery_" + nonce or recovery.get("checked") is not True
                or type(recovery.get("counter")) is not int or recovery["counter"] < 1):
            raise ValueError("Android project process recovery receipt is not bound to this run")
    expected_artifact = {"build_report_sha256": build["build_report_identity"]["sha256"],
                         "apk_sha256": build["apk_identity"]["sha256"], "package_name": retained["package_name"]}
    if (observed.get("passed") is not True or observed.get("nonce") != nonce
            or observed.get("serial") != serial or observed.get("artifact") != expected_artifact
            or observed.get("runner_sha256") != runner_identity["sha256"]
            or (not project and observed.get("extended") is not retained["extended"])
            or (not project and observed.get("retained_rows") != retained["retained_rows"])
            or (project and observed.get("ownership_runner_identity") != ownership_identity)
            or observed.get("apk_sha256") != build["apk_identity"]["sha256"]
            or observed.get("apk_bytes") != build["apk_identity"]["bytes"]
            or observed.get("sdk") != int(sdk)
            or (not project and observed.get("retained_editing_checked") is not True)
            or cleanup.get("completed") is not True or cleanup.get("installed_by_this_run") is not True
            or cleanup.get("kept_installed") is not False or cleanup.get("package_registry_restored") is not True
            or (getattr(arguments, "process_recovery", False)
                and recovery.get("passed") is not True)):
        raise ValueError("Android probe receipt, artifact identity or owned-install cleanup is incomplete")
    artifact(arguments.artifact)
    if (native.file_identity(runner) != runner_identity or native.file_identity(adb) != adb_identity
            or native.file_identity(ownership_runner) != ownership_identity):
        raise ValueError("Android runner or adb changed during the probe")
    return {"artifact": str(arguments.artifact.resolve()), "apk_identity": build["apk_identity"],
            "nonce": nonce, "device": device, "runner_identity": runner_identity,
            "ownership_runner_identity": ownership_identity,
            "probe": str(checks), "probe_identity": native.file_identity(checks),
            "native_probe_observed": True, "scenario": observed,
            "limits": ["Explicit-device programmatic probe with an owned install and cleanup; physical input, TalkBack, other devices and release authenticity require separate acceptance."]}


def run(arguments, output):
    if getattr(arguments, "runtime", None):
        import native_distribution
        name = native_distribution.identifier(arguments.runtime)
        project = arguments.project.resolve() if arguments.project else None
        if project is not None and project.is_file():
            project = project.parent
        if project is None or not any((candidate / "Cargo.toml").is_file() for candidate in (project, *project.parents)):
            raise ValueError("source runtime runs require --project with a Cargo.toml")
        if getattr(arguments, "process_recovery", False):
            raise ValueError("process recovery requires a retained Android artifact")
        home = arguments.home.resolve()
        native_distribution.ensure(home, name, arguments.manifest, arguments.channel,
                                   arguments.manifest_sha256, arguments.offline)
        command = ["--project", str(arguments.project.resolve())]
        if arguments.release:
            command.append("--release")
        if arguments.offline:
            command.append("--offline")
        for option in ("adb", "serial"):
            if getattr(arguments, option, None):
                command += ["--" + option, str(getattr(arguments, option))]
        command += arguments.runtime_argument
        return native_distribution.launch(home, name, command)
    if any(getattr(arguments, option, None) for option in ("project", "release", "runtime_argument")):
        raise ValueError("project, release and runtime arguments require --runtime")
    executable, report = artifact(arguments.artifact)
    if report["observations"].get("kind") == "linux-native-build":
        import native_linux
        return native_linux.run(executable, report)
    if report["observations"].get("kind") == "android-native-build":
        return run_android(arguments, output, executable, report)
    if report["observations"].get("kind") == "macos-native-build":
        if sys.platform != "darwin":
            raise ValueError("macOS applications require a macOS host")
        process = subprocess.Popen([str(executable)], cwd=executable.parent)
        return {"executable": str(executable), "pid": process.pid, "launched": True,
                "runtime_acceptance": False, "limits": ["Application launched; interactive acceptance was not requested."]}
    if report["observations"].get("kind") != "windows-native-build":
        raise ValueError("native runtime probe is currently implemented only for Windows builds")
    if any(getattr(arguments, name, None) for name in ["adb", "serial", "process_recovery"]):
        raise ValueError("adb, serial and process recovery options apply only to Android artifacts")
    directory = new_directory(output.parent / f"{output.stem}-files")
    nonce = uuid.uuid4().hex
    probe = directory / "probe.json"
    capture([executable, "--probe", nonce, probe], directory, directory / "process.log")
    observed = json.loads(probe.read_text(encoding="utf-8"))
    if observed.get("nonce") != nonce or observed.get("native_controls") is not True:
        raise ValueError("native probe did not return this run's nonce and native-control observation")
    return {"artifact": str(arguments.artifact.resolve()), "executable": report["observations"]["identity"],
            "nonce": nonce, "probe": str(probe), "probe_identity": native.file_identity(probe),
            "native_probe_observed": True, "scenario": observed,
            "limits": ["Programmatic native messages; physical input, IME, screen readers and other OS environments are separate acceptance."]}


def package(arguments, output):
    executable, report = artifact(arguments.artifact)
    observation = report["observations"]
    platform = observation["kind"].removesuffix("-native-build")
    files = {executable.name: executable}
    identities = {executable.name: observation.get("identity")}
    if platform in ["android", "ohos"]:
        identities = mobile_package_files(observation)
        files["builder.json"] = Path(observation["build_report"])
        if platform == "ohos" and not observation["native_only"]:
            name = f"jadix-native-controls-{observation['architecture']}-unsigned.hap"
            files[name] = Path(observation["build_directory"]) / name
    if platform == "macos":
        import native_macos
        identities = native_macos.package_files(observation)
        parent = Path(observation["bundle"]).parent
        files = {name: parent / name for name in identities}
    if platform == "linux":
        import native_linux
        identities = native_linux.package_files(observation)
        parent = Path(observation["bundle"])
        files = {name: parent / name for name in identities}
    directory = new_directory(output.parent / f"{output.stem}-files")
    package_path = directory / f"native-{platform}.zip"
    manifest = {"schema": 1, "kind": f"{platform}-native-package", "signed": False,
                "files": identities,
                "build_report": native.file_identity(arguments.artifact),
                "limits": observation["limits"]}
    with zipfile.ZipFile(package_path, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, source in files.items():
            archive.write(source, name)
        archive.writestr("manifest.json", json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        archive.write(arguments.artifact, "build.json")
    verified = verify_package(package_path)
    return {"package": str(package_path), "identity": native.file_identity(package_path), **verified}


def retained_package_files(observation):
    if observation.get("kind") == "linux-native-build":
        import native_linux
        return native_linux.package_files(observation)
    if observation.get("kind") == "macos-native-build":
        import native_macos
        return native_macos.package_files(observation)
    return mobile_package_files(observation)


def mobile_package_files(observation):
    files = {"builder.json": observation.get("build_report_identity")}
    if observation["kind"] == "android-native-build":
        files["native-controls.apk"] = observation.get("apk_identity")
    elif observation["kind"] == "ohos-native-build":
        if observation.get("architecture") not in ["x86_64", "arm64"] or type(observation.get("native_only")) is not bool:
            raise ValueError("unsupported OHOS package target")
        files["libjadix_native_controls.so"] = observation.get("library")
        if not observation["native_only"]:
            files[f"jadix-native-controls-{observation['architecture']}-unsigned.hap"] = observation.get("hap")
    else:
        raise ValueError("unsupported mobile package build")
    return files


def member_identity(archive, info):
    if info.file_size > 128 * 1024 * 1024:
        raise ValueError("package member exceeds the 128 MiB local verification limit")
    digest = hashlib.sha256()
    count = 0
    with archive.open(info) as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            count += len(block)
            digest.update(block)
    if count != info.file_size:
        raise ValueError("package member length mismatch")
    return {"bytes": count, "sha256": digest.hexdigest()}


def verify_package(path):
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        if len(names) != len(set(names)) or len(names) > 514:
            raise ValueError("duplicate or excessive package members")
        for name in names:
            parts = PurePosixPath(name).parts
            if not parts or PurePosixPath(name).is_absolute() or ".." in parts or "\\" in name or ":" in name:
                raise ValueError("invalid package member path")
        if "manifest.json" not in names or archive.getinfo("manifest.json").file_size > 1024 * 1024:
            raise ValueError("missing or excessive package manifest")
        manifest = json.loads(archive.read("manifest.json"))
        kinds = [f"{platform}-native-package" for platform in ["windows", "android", "ohos", "macos", "linux"]]
        if manifest.get("schema") != 1 or manifest.get("kind") not in kinds or manifest.get("signed") is not False:
            raise ValueError("unsupported package manifest")
        files = manifest.get("files", {})
        if (not isinstance(files, dict) or not files or set(files) & {"manifest.json", "build.json"}
                or set(names) != set(files) | {"manifest.json", "build.json"}):
            raise ValueError("package contains missing or unlisted files")
        for name, identity in files.items():
            if member_identity(archive, archive.getinfo(name)) != identity:
                raise ValueError(f"package identity mismatch: {name}")
        if member_identity(archive, archive.getinfo("build.json")) != manifest.get("build_report"):
            raise ValueError("packaged build report identity mismatch")
        if manifest["kind"] != "windows-native-package":
            if archive.getinfo("build.json").file_size > 1024 * 1024:
                raise ValueError("excessive packaged build report")
            build = json.loads(archive.read("build.json"))
            observation = build.get("observations", {})
            if (build.get("schema") != 1 or build.get("operation") != "build"
                    or observation.get("kind") != manifest["kind"].replace("-package", "-build")
                    or retained_package_files(observation) != files):
                raise ValueError("mobile package files disagree with retained build identity")
            if manifest["kind"] == "linux-native-package":
                for name in files:
                    mode = archive.getinfo(name).external_attr >> 16
                    if mode & 0o170000 != 0o100000:
                        raise ValueError("packaged Linux files must have regular file modes")
                for name in files:
                    mode = archive.getinfo(name).external_attr >> 16
                    if mode & 0o170000 != 0o100000:
                        raise ValueError("packaged Linux files must have regular file modes")
                executable = observation["application"]["executable"]
                mode = archive.getinfo(executable).external_attr >> 16
                if not mode & 0o100 or mode & 0o170000 != 0o100000:
                    raise ValueError("packaged Linux executable lacks regular executable mode")
    return {"integrity_verified": True, "signed": manifest.get("signed") is True,
            "authenticity_verified": False, "kind": manifest["kind"], "files": files,
            "limits": ["Hashes check package consistency; they do not authenticate its publisher or accept its platform behavior."]}


def execute(arguments, output):
    operation = arguments.operation
    if operation in ("update-keygen", "update-release"):
        import native_updater
        return (native_updater.keygen if operation == "update-keygen" else native_updater.release)(arguments, output)
    if operation == "inspect":
        retained, report = artifact(arguments.artifact)
        result = {"integrity_verified": True, "artifact": str(retained), "build": report["observations"]}
        if report["observations"].get("kind") == "windows-native-build":
            result["executable"] = str(retained)
        return result
    if operation == "verify":
        return verify_package(arguments.package)
    return {"create": create, "build": build, "run": run, "package": package}[operation](arguments, output)
