#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
case "$(uname -m)" in
  arm64) rust_target=aarch64-apple-darwin; swift_arch=arm64 ;;
  x86_64) rust_target=x86_64-apple-darwin; swift_arch=x86_64 ;;
  *) printf 'Unsupported macOS CPU architecture.\n' >&2; exit 1 ;;
esac

out="${1:-$root/target/native-controls-macos}"
features=appkit
if [[ -n "${2:-}" ]]; then features+=",$2"; fi
export MACOSX_DEPLOYMENT_TARGET=14.0
target_dir="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
cargo build --locked --release --lib --package jadix_native_controls --target "$rust_target" --features "$features"

platform="$root/crates/jadix_native/platforms/macos"
library="$target_dir/$rust_target/release/libjadix_native_controls.a"
mkdir -p "$out"
build_tmp="$(mktemp -d)"
trap 'rm -f "$build_tmp/JadixTextField.o"; rmdir "$build_tmp"' EXIT
xcrun --sdk macosx clang -arch "$swift_arch" -mmacosx-version-min=14.0 -fobjc-arc \
  -c "$platform/JadixTextField.m" -o "$build_tmp/JadixTextField.o"
xcrun --sdk macosx swiftc -swift-version 5 -target "$swift_arch-apple-macosx14.0" \
  -import-objc-header "$platform/JadixNative.h" \
  "$platform/"*.swift "$platform/../apple/"*.swift "$root/examples/native-controls/macos/NativeControlsApp.swift" \
  "$build_tmp/JadixTextField.o" "$library" \
  -framework AppKit -framework CoreFoundation -framework CoreText -framework CoreImage -framework ImageIO -framework QuartzCore \
  -framework WebKit -framework Speech -framework AVFoundation -framework AVKit -framework Metal -framework MetalKit -framework IOSurface \
  -framework Security -framework SystemConfiguration -framework UniformTypeIdentifiers -framework UserNotifications -framework AuthenticationServices -framework Carbon \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker "$root/examples/native-controls/macos/Info.plist" \
  -lresolv -lc++ -o "$out/JadixNativeControls"
