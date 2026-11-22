"""Build independent AppKit applications using the shared native runtime."""
import json
import os
from pathlib import Path, PurePosixPath
import platform
import plistlib
import re
import subprocess
import tempfile

import native


def bundle_files(bundle):
    bundle = Path(bundle).resolve()
    if bundle.suffix != ".app" or not bundle.is_dir():
        raise ValueError("expected a macOS application bundle")
    result = {}
    for path in sorted(bundle.rglob("*")):
        if path.is_symlink():
            raise ValueError("application bundle symlinks are unsupported")
        if path.is_file():
            result[path.relative_to(bundle.parent).as_posix()] = native.file_identity(path)
    if not result or len(result) > 512:
        raise ValueError("empty or excessive application bundle")
    return result


def package_files(observation):
    files = observation.get("files")
    bundle = Path(observation.get("bundle", "")).name
    if not isinstance(files, dict) or not files or len(files) > 512 or not bundle.endswith(".app"):
        raise ValueError("invalid macOS bundle inventory")
    for name in files:
        if not isinstance(name, str) or name != PurePosixPath(name).as_posix():
            raise ValueError("invalid macOS bundle member")
        parts = PurePosixPath(name).parts
        if (not parts or parts[0] != bundle or len(parts) < 3 or parts[1] != "Contents"
                or ".." in parts or "\\" in name or ":" in name):
            raise ValueError("invalid macOS bundle member")
    executable = bundle + "/Contents/MacOS/" + Path(observation["executable"]).name
    if files.get(executable) != observation.get("identity") or bundle + "/Contents/Info.plist" not in files:
        raise ValueError("macOS executable identity disagrees with bundle inventory")
    return files


def build(arguments, output, capture):
    if platform.system() != "Darwin":
        raise ValueError("macOS native builds require macOS and Xcode command line tools")
    if not arguments.project:
        raise ValueError("macOS builds require --project from native create --platform macos")
    project = arguments.project.resolve()
    config = json.loads((project / "jadix-macos.json").read_text(encoding="utf-8"))
    name = config.get("name", "")
    crate = config.get("crate", "")
    identifier = config.get("bundle_identifier", "")
    if (config.get("schema") != 1 or not re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", name)
            or not re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", crate)
            or not re.fullmatch(r"[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+", identifier)):
        raise ValueError("invalid macOS project identity")
    entry = (project / config.get("swift_entry", "App.swift")).resolve()
    if not entry.is_relative_to(project) or not entry.is_file():
        raise ValueError("Swift entry must be a project file")
    schemes = config.get("url_schemes", [])
    if (not isinstance(schemes, list) or len(schemes) > 16 or len(set(schemes)) != len(schemes)
            or any(not isinstance(s, str) or not re.fullmatch(r"[a-z][a-z0-9+.-]{0,63}", s) for s in schemes)):
        raise ValueError("invalid URL scheme whitelist")
    repository = Path(config["repository"]).resolve()
    metadata = subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=project,
                              check=True, text=True, capture_output=True)
    metadata = json.loads(metadata.stdout)
    packages = [p for p in metadata["packages"] if p["name"] == crate]
    if len(packages) != 1:
        raise ValueError("macOS crate must identify one application package")
    version = config.get("version", packages[0]["version"])
    if not isinstance(version, str) or not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version):
        raise ValueError("macOS application version must be a semantic version")
    import native_updater
    updater = native_updater.configuration(config.get("updater"), identifier, version)
    source = repository / "crates/jadix_native/platforms/macos"
    architecture = "arm64" if platform.machine() == "arm64" else "x86_64"
    target = "aarch64-apple-darwin" if architecture == "arm64" else "x86_64-apple-darwin"
    destination = output.parent / (output.stem + "-files")
    if destination.exists():
        raise ValueError("build output already exists")
    destination.mkdir(parents=True)
    bundle = destination / (name + ".app")
    executable = bundle / "Contents/MacOS" / name
    executable.parent.mkdir(parents=True)
    (bundle / "Contents/Resources").mkdir()
    if updater is not None:
        (bundle / "Contents/Resources/jadix-updater.json").write_text(json.dumps(updater), encoding="utf-8")
    info = {"CFBundleExecutable": name, "CFBundleIdentifier": identifier, "CFBundleName": name,
            "CFBundlePackageType": "APPL", "CFBundleVersion": version.split("-")[0].split("+")[0], "CFBundleShortVersionString": version,
            "LSMinimumSystemVersion": "14.0", "NSHighResolutionCapable": True,
            "LSMultipleInstancesProhibited": config.get("single_instance", True),
            "NSMicrophoneUsageDescription": config.get("microphone_usage", "Record application audio."),
            "NSSpeechRecognitionUsageDescription": config.get("speech_usage", "Transcribe application speech.")}
    if schemes:
        info["CFBundleURLTypes"] = [{"CFBundleURLName": identifier, "CFBundleURLSchemes": schemes}]
    (bundle / "Contents/Info.plist").write_bytes(plistlib.dumps(info))
    profile = "release" if arguments.release else "debug"
    command = ["cargo", "build", "--locked", "--lib", "--package", crate, "--target", target]
    if arguments.release: command.append("--release")
    if arguments.offline: command.append("--offline")
    if arguments.features: command += ["--features", arguments.features]
    # Both Rust and Swift must agree on the minimum OS supported by the bundle.
    previous = os.environ.get("MACOSX_DEPLOYMENT_TARGET")
    os.environ["MACOSX_DEPLOYMENT_TARGET"] = "14.0"
    try:
        capture(command, project, destination / "cargo.log")
    finally:
        if previous is None: os.environ.pop("MACOSX_DEPLOYMENT_TARGET", None)
        else: os.environ["MACOSX_DEPLOYMENT_TARGET"] = previous
    library = Path(metadata["target_directory"]) / target / profile / ("lib" + crate.replace("-", "_") + ".a")
    with tempfile.TemporaryDirectory(prefix="jadix-appkit-") as temporary:
        obj = Path(temporary) / "JadixTextField.o"
        capture(["xcrun", "--sdk", "macosx", "clang", "-arch", architecture, "-mmacosx-version-min=14.0",
                 "-fobjc-arc", "-c", source / "JadixTextField.m", "-o", obj], project, destination / "clang.log")
        swift = ["xcrun", "--sdk", "macosx", "swiftc", "-swift-version", "5", "-target", architecture + "-apple-macosx14.0",
                 "-import-objc-header", source / "JadixNative.h", *sorted(source.glob("*.swift")), *sorted((source.parent / "apple").glob("*.swift")), entry, obj, library]
        for framework in ["AppKit", "CoreFoundation", "CoreText", "ImageIO", "QuartzCore", "WebKit", "Speech",
                          "AVFoundation", "AVKit", "Metal", "MetalKit", "IOSurface", "Security", "SystemConfiguration",
                          "UniformTypeIdentifiers", "UserNotifications", "AuthenticationServices", "Carbon"]:
            swift += ["-framework", framework]
        swift += ["-lresolv", "-lc++", "-o", executable]
        capture(swift, project, destination / "swift.log")
    capture(["codesign", "--force", "--sign", "-", bundle], project, destination / "codesign.log")
    capture(["codesign", "--verify", "--strict", "--deep", bundle], project, destination / "signature-check.log")
    return {"kind": "macos-native-build", "bundle": str(bundle), "executable": str(executable),
            "identity": native.file_identity(executable), "files": bundle_files(bundle), "architecture": architecture,
            "url_schemes": schemes, "version": version, "updater": updater, "signing": "ad-hoc", "signed": True,
            "limits": ["Ad-hoc signature verifies local bundle integrity; publisher signing, notarization and macOS runtime acceptance are separate."]}
