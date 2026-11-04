import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from tempfile import TemporaryDirectory
import xml.etree.ElementTree as ET
import zipfile


def build_inputs(root):
    sys.path.insert(0, str(root / "scripts"))
    import native

    directories = ["crates/jadix_native", "crates/jadix", "crates/jadix_core",
                   "crates/jadix_components", "crates/jadix_macros", "crates/jadix_tasks",
                   "crates/jadix_layout", "crates/jadix_style_tokens", "crates/jadix_taffy",
                   "crates/jadix_theme", "crates/jadix_animation", "crates/jadix_hot_reload",
                   "crates/jadix_document", "crates/jadix_3d", "examples/native-controls"]
    return native, {
        "workspace": {name: native.file_identity(root / name) for name in ["Cargo.toml", "Cargo.lock"]},
        "trees": [native.tree_identity(root / name) for name in directories],
    }


def assert_build_inputs_unchanged(root, native, identity):
    current = {
        "workspace": {name: native.file_identity(root / name) for name in identity["workspace"]},
        "trees": [native.tree_identity(Path(tree["directory"])) for tree in identity["trees"]],
    }
    if current != identity:
        raise RuntimeError("Android build inputs changed during compilation or packaging")


def project_configuration(project):
    project = project.resolve()
    config = json.loads((project / "native-android.json").read_text(encoding="utf-8"))
    if (not isinstance(config, dict) or config.get("schema") != 1
            or not isinstance(config.get("name"), str)
            or not isinstance(config.get("application_id"), str)
            or not re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", config.get("name", ""))
            or not re.fullmatch(r"dev\.jadix\.created\.[a-z][a-z0-9_]{0,63}", config.get("application_id", ""))):
        raise ValueError("Android project configuration is invalid")
    manifest = ET.parse(project / "android/AndroidManifest.xml").getroot()
    if manifest.get("package") != config["application_id"]:
        raise ValueError("Android project manifest package differs from its configuration")
    return config


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", choices=["aarch64-linux-android", "x86_64-linux-android"], default="aarch64-linux-android")
    parser.add_argument("--project", type=Path, help="Independent project created by native.py create --platform android")
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/native-controls/android"))
    parser.add_argument("--extended", action="store_true")
    parser.add_argument("--selection", action="store_true", help="Build the separate native Select application")
    parser.add_argument("--popover", action="store_true", help="Build the separate native Popover application")
    parser.add_argument("--notes", action="store_true", help="Build the separate persistent notebook application")
    parser.add_argument("--large-list", action="store_true", help="Build the retained 1000-row sample instead of 100 rows")
    parser.add_argument("--release", action="store_true", help="Optimize the Rust library; this sample still uses a debuggable development APK")
    parser.add_argument("--keystore", type=Path)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--sdk", type=Path, default=Path(os.environ["ANDROID_HOME"]) if os.environ.get("ANDROID_HOME") else None)
    parser.add_argument("--ndk", type=Path, default=Path(os.environ["NDK_HOME"]) if os.environ.get("NDK_HOME") else None)
    parser.add_argument("--java-home", type=Path, default=Path(os.environ["JAVA_HOME"]) if os.environ.get("JAVA_HOME") else None)
    args = parser.parse_args()
    if args.popover and (args.notes or args.selection or args.extended or args.large_list):
        parser.error("--popover cannot be combined with other sample features")
    if args.notes and (args.extended or args.large_list or args.selection):
        parser.error("--notes cannot be combined with the controls sample features")
    if args.selection and (args.extended or args.large_list):
        parser.error("--selection cannot be combined with --extended or --large-list")
    if args.project and any([args.extended, args.selection, args.popover, args.notes, args.large_list]):
        parser.error("independent projects cannot use controls product features")
    root = Path(__file__).resolve().parents[2]
    project = args.project.resolve() if args.project else root
    config = project_configuration(project) if args.project else None
    native, inputs = build_inputs(root)
    if config:
        inputs["trees"].append(native.tree_identity(project))
    output = args.output.resolve()
    if any((directory / "snapshot.json").exists() for directory in (output, *output.parents)):
        parser.error("output contains an archived snapshot; select a new output directory")
    if args.keystore is not None and not args.keystore.is_file():
        parser.error("--keystore must name an existing file; omit it to create a development key in the output directory")
    if not all([args.sdk, args.ndk, args.java_home]):
        parser.error("Android build requires --sdk, --ndk and --java-home or their environment variables")
    output.mkdir(parents=True, exist_ok=True)
    sdk = args.sdk.resolve()
    ndk = args.ndk.resolve()
    java_home = args.java_home.resolve()
    java = java_home / "bin"
    windows = sys.platform == "win32"
    executable_suffix = ".exe" if windows else ""
    command_suffix = ".bat" if windows else ""
    tools = sdk / "build-tools" / "36.1.0"
    platform = sdk / "platforms" / "android-36" / "android.jar"
    host_tag = "windows-x86_64" if windows else "darwin-x86_64" if sys.platform == "darwin" else "linux-x86_64"
    llvm = ndk / "toolchains/llvm/prebuilt" / host_tag / "bin"
    toolchain = {"sdk": str(sdk), "ndk": str(ndk), "java_home": str(java_home),
                 "files": {str(path): native.file_identity(path) for path in [
                     platform, tools / "source.properties", ndk / "source.properties", java_home / "release",
                     llvm / ("clang" + executable_suffix), llvm / ("llvm-ar" + executable_suffix),
                     java / ("javac" + executable_suffix), tools / ("aapt2" + executable_suffix),
                     tools / ("zipalign" + executable_suffix), tools / ("d8" + command_suffix),
                     tools / ("apksigner" + command_suffix)]}}
    clang_target, abi = {"aarch64-linux-android": ("aarch64-linux-android", "arm64-v8a"), "x86_64-linux-android": ("x86_64-linux-android", "x86_64")}[args.target]
    environment = os.environ.copy()
    target_key = args.target.replace("-", "_")
    clang = str(llvm / (f"{clang_target}28-clang" + (".cmd" if windows else "")))
    environment[f"CARGO_TARGET_{target_key.upper()}_LINKER"] = clang
    environment[f"CC_{target_key}"] = clang
    environment[f"AR_{target_key}"] = str(llvm / ("llvm-ar" + executable_suffix))
    environment["ANDROID_HOME"] = str(sdk)
    environment["NDK_HOME"] = str(ndk)
    environment["JAVA_HOME"] = str(java_home)
    environment["PATH"] = str(java) + os.pathsep + environment["PATH"]

    def run(command):
        subprocess.run([str(item) for item in command], cwd=project, env=environment, check=True)

    command = ["cargo", "build", "--locked", "-p", config["name"] if config else "jadix_native_controls", "--target", args.target]
    if args.extended:
        command += ["--features", "extended"]
    if args.selection:
        command += ["--features", "selection"]
    if args.popover:
        command += ["--features", "popover"]
    if args.notes:
        command += ["--features", "notes"]
    if args.large_list:
        command += ["--features", "large-list"]
    if args.release:
        command += ["--release"]
    if args.offline:
        command += ["--offline"]
    run(command)
    assert_build_inputs_unchanged(root, native, inputs)
    metadata_command = ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"]
    if args.offline:
        metadata_command.append("--offline")
    metadata = json.loads(subprocess.check_output(metadata_command, cwd=project, env=environment, text=True, encoding="utf8"))
    profile = "release" if args.release else "debug"
    library = Path(metadata["target_directory"]) / args.target / profile / "libjadix_native_controls.so"
    library_identity = native.file_identity(library)
    sources = list((root / "crates/jadix_native/platforms/android/src").rglob("*.java"))
    sources += list((project / "android/src" if config else root / "examples/native-controls/android/src").rglob("*.java"))
    jar = output / "classes.jar"
    with TemporaryDirectory(prefix="classes-", dir=output) as temporary:
        classes = Path(temporary).resolve()
        if not classes.is_relative_to(output):
            raise RuntimeError("Java build directory escaped output")
        run([java / ("javac" + executable_suffix), "-J-Dfile.encoding=UTF-8", "-encoding", "UTF-8", "--release", "8", "-classpath", platform, "-d", classes, *sources])
        with zipfile.ZipFile(jar, "w") as archive:
            for source in classes.rglob("*.class"):
                archive.write(source, source.relative_to(classes).as_posix())
    run([tools / ("d8" + command_suffix), "--min-api", "28", "--lib", platform, "--output", output, jar])
    apk = output / "native-controls-unsigned.apk"
    manifest = project / "android/AndroidManifest.xml" if config else root / ("examples/native-controls/android/popover/AndroidManifest.xml" if args.popover else "examples/native-controls/android/notes/AndroidManifest.xml" if args.notes else "examples/native-controls/android/selection/AndroidManifest.xml" if args.selection else "examples/native-controls/android/AndroidManifest.xml")
    run([tools / ("aapt2" + executable_suffix), "link", "-I", platform, "--manifest", manifest, "-o", apk])
    with zipfile.ZipFile(apk, "a", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.write(output / "classes.dex", "classes.dex")
        archive.write(library, f"lib/{abi}/libjadix_native_controls.so")
    aligned = output / "native-controls-aligned.apk"
    run([tools / ("zipalign" + executable_suffix), "-f", "-P", "16", "4", apk, aligned])
    key = args.keystore.resolve() if args.keystore else output / "debug.keystore"
    if not key.exists():
        if args.keystore is not None:
            raise RuntimeError("the supplied keystore disappeared; refusing to create a replacement")
        run([java / ("keytool" + executable_suffix), "-genkeypair", "-keystore", key, "-storepass", "android", "-keypass", "android", "-alias", "jadix-debug", "-dname", "CN=Jadix Debug", "-keyalg", "RSA", "-validity", "3650"])
    signed = output / "native-controls.apk"
    run([tools / ("apksigner" + command_suffix), "sign", "--ks", key, "--ks-pass", "pass:android", "--out", signed, aligned])
    run([tools / ("apksigner" + command_suffix), "verify", "--verbose", signed])
    assert_build_inputs_unchanged(root, native, inputs)
    if native.file_identity(library) != library_identity or any(
            native.file_identity(Path(path)) != identity for path, identity in toolchain["files"].items()):
        raise RuntimeError("Android native library or toolchain changed during packaging")
    report = {"schema": 1, "target": args.target, "abi": abi, "rust_profile": profile, "extended": args.extended,
              "selection": args.selection, "popover": args.popover, "package_name": config["application_id"] if config else "dev.jadix.popover" if args.popover else "dev.jadix.selection" if args.selection else "dev.jadix.notebook" if args.notes else "dev.jadix.nativeui",
              "selection_authoring": "compound-primary-options-disabled" if args.selection else None,
              "application": "project" if config else "notes" if args.notes else "controls",
              "project": str(project) if config else None,
              "retained_rows": 0 if config or args.notes or args.selection or args.popover else 1000 if args.large_list else 100,
              "debuggable": True, "signing": "development keystore; not a product release signature",
              "build_inputs": inputs, "toolchain": toolchain, "source_unchanged": True, "signed": True,
              "authenticity_verified": False, "runtime_acceptance": False,
              "cargo_lock_sha256": hashlib.sha256((project / "Cargo.lock").read_bytes()).hexdigest(),
              "library_bytes": library_identity["bytes"], "library_sha256": library_identity["sha256"],
              "apk_bytes": signed.stat().st_size, "apk_sha256": hashlib.sha256(signed.read_bytes()).hexdigest()}
    report["apk_identity"] = native.file_identity(signed)
    (output / "build.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print(signed)


if __name__ == "__main__":
    main()
