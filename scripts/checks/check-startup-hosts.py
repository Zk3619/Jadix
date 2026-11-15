"""Validate packaged launch resources; optionally compile the Android resource package."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as XML


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--android-sdk", type=Path, default=os.environ.get("ANDROID_HOME"))
    parser.add_argument("--output", type=Path, default=Path(".theme-evidence/splash-startup/hosts"))
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    mobile = root / "examples/gallery_mobile"
    arguments.output.mkdir(parents=True, exist_ok=True)
    android = "{http://schemas.android.com/apk/res/android}"
    manifest = XML.parse(mobile / "android/AndroidManifest.xml").getroot()
    application = manifest.find("application")
    activity = application.find("activity")
    assert activity.get(android + "name") == "android.app.NativeActivity"
    assert application.get(android + "theme") == "@style/JadixLaunchTheme"
    for directory in ("values", "values-v31"):
        styles = XML.parse(mobile / f"android/res/{directory}/styles.xml").getroot()
        style = next(style for style in styles if style.get("name") == "JadixLaunchTheme")
        entries = {entry.get("name"): entry.text for entry in style}
        assert "android:windowBackground" in entries
        if directory == "values-v31":
            assert entries["android:windowSplashScreenAnimatedIcon"] == "@drawable/app_icon"
            assert entries["android:windowSplashScreenBackground"] == "@color/jadix_launch_background"
    for directory in ("values", "values-night"):
        colors = XML.parse(mobile / f"android/res/{directory}/colors.xml").getroot()
        assert any(color.get("name") == "jadix_launch_background" for color in colors)
    with (mobile / "ios/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    storyboard = XML.parse(mobile / f"ios/{info['UILaunchStoryboardName']}.storyboard").getroot()
    assert storyboard.get("launchScreen") == "YES"
    assert storyboard.find(".//viewLayoutGuide[@key='safeArea']") is not None
    assert storyboard.find(".//imageView[@image='LaunchLogo']") is not None
    assert not storyboard.findall(".//*[@customClass]")
    assets = json.loads((mobile / "ios/Assets.xcassets/LaunchLogo.imageset/Contents.json").read_text(encoding="utf-8"))
    assert assets["images"][0]["filename"] == "logo.png"
    module = json.loads((mobile / "ohos/entry/src/main/module.json5").read_text(encoding="utf-8"))["module"]
    for ability in module["abilities"]:
        assert ability["startWindowIcon"] == "$media:app_icon"
        assert (mobile / "ohos/entry/src/main/resources/base/media/app_icon.png").is_file()
        assert ability["startWindowBackground"] == "$color:start_window_background"
    for directory in ("base", "dark"):
        colors = json.loads((mobile / f"ohos/entry/src/main/resources/{directory}/element/color.json").read_text(encoding="utf-8"))
        assert any(color["name"] == "start_window_background" for color in colors["color"])
    result = {"resource_contracts": "passed", "android_resources": "not compiled: supply --android-sdk",
              "ios": "static resources only; Xcode, signing and devices required",
              "ohos": "static resources only; HAP and device verification required"}
    if arguments.android_sdk:
        sdk = arguments.android_sdk
        executable = "aapt2.exe" if os.name == "nt" else "aapt2"
        tools = max((sdk / "build-tools").iterdir(), key=lambda path: tuple(int(part) for part in path.name.split(".") if part.isdigit()))
        platform = max((path for path in (sdk / "platforms").iterdir() if path.name.removeprefix("android-").isdigit()), key=lambda path: int(path.name.removeprefix("android-")))
        with tempfile.TemporaryDirectory(dir=arguments.output) as temporary:
            staging = Path(temporary)
            shutil.copytree(mobile / "android/res", staging / "res")
            shutil.copyfile(root / "assets/icons/desktop/window.png", staging / "res/drawable/app_icon.png")
            subprocess.run([str(tools / executable), "compile", "--dir", str(staging / "res"), "-o", str(staging / "resources.zip")], check=True)
            package = arguments.output / "launch-resources.apk"
            subprocess.run([str(tools / executable), "link", "--manifest", str(mobile / "android/AndroidManifest.xml"), "-I", str(platform / "android.jar"), "-o", str(package), str(staging / "resources.zip")], check=True)
            badging = subprocess.check_output([str(tools / executable), "dump", "badging", str(package)], text=True, encoding="utf-8")
            assert "launchable-activity: name='android.app.NativeActivity'" in badging, badging
            (arguments.output / "android-badging.txt").write_text(badging, encoding="utf-8")
            result["android_resources"] = {"compiled": True, "platform": platform.name, "sha256": hashlib.sha256(package.read_bytes()).hexdigest(), "native_library_included": False}
    (arguments.output / "results.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
