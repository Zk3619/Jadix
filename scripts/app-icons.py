#!/usr/bin/env python3
"""Generate install-time icon assets from a TOML configuration (Python 3.11+, Pillow)."""
import argparse
import json
from pathlib import Path
import plistlib
import tomllib
from PIL import Image, ImageColor, ImageOps


def generate(config_path, output, platforms):
    config_path = Path(config_path).resolve()
    config = tomllib.loads(config_path.read_text(encoding="utf-8"))
    root = config_path.parent
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    files = []

    def source(key=None):
        path = root / (key or config["source"])
        with Image.open(path) as original:
            image = ImageOps.exif_transpose(original).convert("RGBA")
        if image.width != image.height:
            raise ValueError(f"icon must be square: {path}")
        return image

    def square(image, size, background=None, content_size=None):
        color = (0, 0, 0, 0) if background is None else ImageColor.getrgb(background)
        if background is not None and len(color) == 4 and color[3] != 255:
            raise ValueError("icon background must be opaque")
        canvas = Image.new("RGBA", (size, size), color)
        content = ImageOps.contain(image, (content_size or size, content_size or size), Image.Resampling.LANCZOS)
        canvas.alpha_composite(content, ((size-content.width)//2, (size-content.height)//2))
        return canvas

    def save(image, name, **kwargs):
        path = output / name
        path.parent.mkdir(parents=True, exist_ok=True)
        image.save(path, **kwargs)
        files.append(name)

    def text(name, contents):
        path = output / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")
        files.append(name)

    base = source()
    if "desktop" in platforms:
        desktop = config.get("desktop", {})
        save(square(base, 256), "desktop/window.png")
        save(square(source(desktop.get("windows")), 256), "desktop/app.ico", sizes=[(n,n) for n in (16,24,32,48,64,128,256)])
        save(square(source(desktop.get("macos")), 1024), "desktop/app.icns")
        for size in (16,24,32,48,64,128,256,512):
            save(square(source(desktop.get("linux")), size), f"desktop/hicolor/{size}x{size}/apps/app.png")
        save(square(source(desktop.get("tray")), 32), "desktop/tray.png")
    if "android" in platforms:
        android = config.get("android", {})
        background = android.get("background", "#ffffff")
        # Android adaptive layers are 108dp, central 66dp is the safe artwork region.
        for density, px, layer in (("mdpi",48,108),("hdpi",72,162),("xhdpi",96,216),("xxhdpi",144,324),("xxxhdpi",192,432)):
            save(square(base, px, background), f"android/res/mipmap-{density}/ic_launcher.png")
            save(square(source(android.get("foreground")), layer, content_size=round(layer*66/108)), f"android/res/drawable-{density}/ic_launcher_foreground.png")
            if android.get("monochrome"):
                save(square(source(android["monochrome"]), layer, content_size=round(layer*66/108)), f"android/res/drawable-{density}/ic_launcher_monochrome.png")
        mono = '\n    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />' if android.get("monochrome") else ""
        adaptive = '<?xml version="1.0" encoding="utf-8"?>\n<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n    <background android:drawable="@color/ic_launcher_background" />\n    <foreground android:drawable="@drawable/ic_launcher_foreground" />'
        text("android/res/mipmap-anydpi-v26/ic_launcher.xml", adaptive+'\n</adaptive-icon>\n')
        if mono:
            text("android/res/mipmap-anydpi-v33/ic_launcher.xml", adaptive+mono+'\n</adaptive-icon>\n')
        # Validate color using Pillow before writing Android's opaque hex representation.
        rgb = ImageColor.getrgb(background)
        if len(rgb) == 4 and rgb[3] != 255:
            raise ValueError("Android icon background must be opaque")
        text("android/res/values/icon_colors.xml", '<resources><color name="ic_launcher_background">#%02x%02x%02x</color></resources>\n' % rgb[:3])
        text("android/manifest-snippet.xml", '<!-- Merge this attribute into your existing application element. -->\n<application xmlns:android="http://schemas.android.com/apk/res/android" android:icon="@mipmap/ic_launcher" />\n')
    if "ios" in platforms:
        ios = config.get("ios", {})
        def iconset(image, name):
            entries = []
            for idiom, points, scales in (("iphone",(20,29,40,60),(2,3)), ("ipad",(20,29,40,76),(1,2))):
                for point in points:
                    for scale in scales:
                        filename=f"{idiom}-{point}@{scale}x.png"
                        save(square(image, point*scale, ios.get("background", "#ffffff")).convert("RGB"), f"ios/Assets.xcassets/{name}.appiconset/{filename}")
                        entries.append(dict(idiom=idiom, size=f"{point}x{point}", scale=f"{scale}x", filename=filename))
            for idiom,point,scale in (("ipad",83.5,2),("ios-marketing",1024,1)):
                filename=f"{idiom}-{point}@{scale}x.png"
                save(square(image, int(point*scale), ios.get("background", "#ffffff")).convert("RGB"), f"ios/Assets.xcassets/{name}.appiconset/{filename}")
                entries.append(dict(idiom=idiom, size=f"{point}x{point}", scale=f"{scale}x", filename=filename))
            text(f"ios/Assets.xcassets/{name}.appiconset/Contents.json", json.dumps(dict(images=entries,info=dict(version=1,author="jadix")),indent=2))
        iconset(source(ios.get("primary")), "AppIcon")
        alternates = {}
        for name,path in ios.get("alternates", {}).items():
            if not name.isidentifier() or name == "AppIcon":
                raise ValueError("alternate icon names must be identifiers other than AppIcon")
            iconset(source(path), name)
            alternates[name] = {"CFBundleIconName":name}
        text("ios/Assets.xcassets/Contents.json", json.dumps(dict(info=dict(version=1,author="jadix"))))
        plist = {"CFBundleIcons":{"CFBundlePrimaryIcon":{"CFBundleIconName":"AppIcon"}, "CFBundleAlternateIcons":alternates}}
        plist["CFBundleIcons~ipad"] = plist["CFBundleIcons"].copy()
        text("ios/Info-icons.plist", plistlib.dumps(plist).decode())
    text("icons-manifest.json", json.dumps(dict(source=config["source"], platforms=platforms, files=files), indent=2))
    return files


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--platform", choices=("desktop","android","ios"), action="append")
    args=parser.parse_args()
    files=generate(args.config,args.output,args.platform or ["desktop","android","ios"])
    print(f"Generated {len(files)} assets in {args.output}")
