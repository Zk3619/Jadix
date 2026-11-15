import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys


TARGETS = {
    "aarch64-apple-ios-sim": ("iphonesimulator", "arm64-apple-ios14.0-simulator"),
    "aarch64-apple-ios": ("iphoneos", "arm64-apple-ios14.0"),
}


def validate_output(output):
    if any((directory / "snapshot.json").exists() for directory in (output, *output.parents)):
        raise ValueError("output contains an archived snapshot; select a new output directory")
    if any((output / name).exists() for name in ["JadixNative.app", "JadixNotes.app", "JadixSelect.app", "JadixPopover.app", "JadixField.app"]):
        raise ValueError("application bundle already exists; select a new output directory")


def swift_command(root, app, library, target, sdk_path, notes, checks=False, selection=False, popover=False, consistent_ui=False):
    sdk, swift_target = TARGETS[target]
    command = ["xcrun", "--sdk", sdk, "swiftc", "-swift-version", "5", "-target", swift_target,
               "-sdk", sdk_path, "-import-objc-header", str(root / "crates/jadix_native/platforms/ios/JadixNative.h")]
    if notes:
        command += ["-D", "JADIX_NOTES"]
    if selection:
        command += ["-D", "JADIX_SELECT"]
    if popover:
        command += ["-D", "JADIX_POPOVER"]
    if consistent_ui:
        command += ["-D", "JADIX_NATIVE_CONSISTENT_UI"]
    if checks:
        command += ["-D", "JADIX_NATIVE_CHECKS", str(root / "examples/native-controls/ios" / ("NativePopoverChecks.swift" if popover else "NativeSelectChecks.swift" if selection else "NativeChecks.swift"))]
    command += [str(root / "crates/jadix_native/platforms/ios/JadixNativeView.swift"),
                str(root / "crates/jadix_native/platforms/ios/NativePopover.swift")]
    if notes:
        command += [str(root / "examples/native-controls/ios/NativeNotes.swift")]
    command += [str(root / "examples/native-controls/ios/main.swift"), str(library)]
    for framework in ["UIKit", "Foundation", "Security", "SystemConfiguration", "ImageIO"]:
        command += ["-framework", framework]
    return command + ["-lresolv", "-lc++", "-o", str(app / "JadixNative")]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", choices=TARGETS, default="aarch64-apple-ios-sim")
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/native-controls/ios"))
    parser.add_argument("--notes", action="store_true")
    parser.add_argument("--selection", action="store_true", help="Build the separate native Select application")
    parser.add_argument("--popover", action="store_true", help="Build the separate native Popover application")
    parser.add_argument("--checks", action="store_true", help="Include opt-in simulator acceptance probes, never for device distribution")
    parser.add_argument("--consistent-ui", action="store_true", help="Enable Jadix consistent-ui projection and the native iOS keyboard accessory")
    parser.add_argument("--provisioning-profile", type=Path)
    parser.add_argument("--field", action="store_true", help="Build the shared Field workflow without a simulator acceptance probe")
    args = parser.parse_args()
    if args.field and any([args.notes, args.selection, args.popover, args.checks]):
        parser.error("--field requires its own application; simulator acceptance checks are not implemented")
    if args.selection and args.notes:
        parser.error("--selection cannot be combined with --notes")
    if args.popover and (args.selection or args.notes):
        parser.error("--popover cannot be combined with --selection or --notes")
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    try:
        validate_output(output)
    except ValueError as error:
        parser.error(str(error))
    if args.provisioning_profile is not None and not args.provisioning_profile.is_file():
        parser.error("--provisioning-profile must name an existing file")
    if args.checks and not args.target.endswith("-sim"):
        parser.error("--checks requires an iOS simulator target")
    if sys.platform != "darwin":
        parser.error("iOS packaging requires macOS, Xcode and the selected Apple SDK")
    sdk, _ = TARGETS[args.target]
    environment = os.environ.copy()
    environment["IPHONEOS_DEPLOYMENT_TARGET"] = "14.0"

    def run(command):
        subprocess.run([str(item) for item in command], cwd=root, env=environment, check=True)

    def capture(command):
        return subprocess.check_output(command, cwd=root, env=environment, text=True, encoding="utf8").strip()

    features = "uikit,field-workflow" if args.field else "uikit,popover" if args.popover else "uikit,selection" if args.selection else "uikit,notes" if args.notes else "uikit,extended"
    if args.consistent_ui:
        features += ",consistent-ui"
    sdk_path = capture(["xcrun", "--sdk", sdk, "--show-sdk-path"])
    sdk_version = capture(["xcrun", "--sdk", sdk, "--show-sdk-version"])
    swift_version = capture(["xcrun", "--sdk", sdk, "swiftc", "--version"])
    run(["cargo", "build", "--locked", "-p", "jadix_native_controls", "--features", features, "--target", args.target, "--release"])
    metadata = json.loads(capture(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"]))
    library = Path(metadata["target_directory"]) / args.target / "release/libjadix_native_controls.a"
    app = output / ("JadixField.app" if args.field else "JadixPopover.app" if args.popover else "JadixSelect.app" if args.selection else "JadixNotes.app" if args.notes else "JadixNative.app")
    app.mkdir(parents=True)
    run(swift_command(root, app, library, args.target, sdk_path, args.notes, args.checks, args.selection, args.popover, args.consistent_ui))
    with (root / "examples/native-controls/ios/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if args.field:
        info.update(CFBundleIdentifier="dev.jadix.field", CFBundleName="Jadix Field Workflow")
    if args.notes:
        info.update(CFBundleIdentifier="dev.jadix.notebook", CFBundleName="Jadix Notes")
    if args.selection:
        info.update(CFBundleIdentifier="dev.jadix.selection", CFBundleName="Jadix Select")
    if args.popover:
        info.update(CFBundleIdentifier="dev.jadix.popover", CFBundleName="Jadix Popover")
    info.update(CFBundleSupportedPlatforms=["iPhoneSimulator" if sdk == "iphonesimulator" else "iPhoneOS"], UIDeviceFamily=[1, 2])
    with (app / "Info.plist").open("wb") as stream:
        plistlib.dump(info, stream)
    if args.provisioning_profile is not None:
        shutil.copyfile(args.provisioning_profile, app / "embedded.mobileprovision")
    identity = "-" if sdk == "iphonesimulator" else environment.get("CODE_SIGN_IDENTITY")
    if identity:
        run(["codesign", "--force", "--sign", identity, app])
        run(["codesign", "--verify", "--deep", "--strict", app])
    report = {"target": args.target, "rust_profile": "release", "application": "field" if args.field else "popover" if args.popover else "selection" if args.selection else "notes" if args.notes else "controls",
              "selection_authoring": "compound-primary-options-disabled" if args.selection else None,
              "features": features, "checks_enabled": args.checks, "sdk": sdk, "sdk_version": sdk_version, "swift_version": swift_version,
              "cargo_lock_sha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(),
              "library_bytes": library.stat().st_size, "library_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
              "bundle_identifier": info["CFBundleIdentifier"],
              "signing": "ad-hoc simulator" if sdk == "iphonesimulator" else "configured identity" if identity else "unsigned device bundle",
              "provisioning_profile_copied": args.provisioning_profile is not None, "distribution_acceptance": False,
              "files": [{"path": path.relative_to(output).as_posix(), "bytes": path.stat().st_size,
                         "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in sorted(app.rglob("*")) if path.is_file()]}
    (output / "build.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print(app)


if __name__ == "__main__":
    main()
