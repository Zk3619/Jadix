#!/bin/sh
set -eu
export JADIX_SPLASH_MODE=${JADIX_SPLASH_MODE:-component}
case "$JADIX_SPLASH_MODE" in
    system|component|off) ;;
    *) echo "JADIX_SPLASH_MODE must be system, component or off" >&2; exit 2 ;;
esac
mobile=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
workspace=$(CDPATH= cd -- "$mobile/../.." && pwd)
target=${1:-aarch64-apple-ios-sim}
case "$target" in
    aarch64-apple-ios) sdk=iphoneos; destination='generic/platform=iOS' ;;
    aarch64-apple-ios-sim) sdk=iphonesimulator; destination='generic/platform=iOS Simulator'; architecture=arm64 ;;
    x86_64-apple-ios) sdk=iphonesimulator; destination='generic/platform=iOS Simulator'; architecture=x86_64 ;;
    *) echo "Unsupported iOS target: $target" >&2; exit 2 ;;
esac
rustup target add "$target"
cd "$workspace"
cargo build -p gallery_mobile --release --locked --target "$target"
cp "$workspace/assets/icons/desktop/window.png" "$mobile/ios/Assets.xcassets/LaunchLogo.imageset/logo.png"
output="$mobile/dist/ios-$target"
if [ "$sdk" = iphoneos ]; then
    : "${JADIX_APPLE_TEAM:?Set JADIX_APPLE_TEAM to your Apple development team identifier}"
    xcodebuild -project "$mobile/ios/Gallery.xcodeproj" -target Gallery -configuration Release \
        -sdk "$sdk" -destination "$destination" \
        "LIBRARY_SEARCH_PATHS=$workspace/target/$target/release" \
        "CONFIGURATION_BUILD_DIR=$output" "DEVELOPMENT_TEAM=$JADIX_APPLE_TEAM" build
else
    xcodebuild -project "$mobile/ios/Gallery.xcodeproj" -target Gallery -configuration Release \
        -sdk "$sdk" -destination "$destination" "ARCHS=$architecture" \
        "LIBRARY_SEARCH_PATHS=$workspace/target/$target/release" \
        "CONFIGURATION_BUILD_DIR=$output" CODE_SIGNING_ALLOWED=NO build
fi
printf '%s\n' "$output/Gallery.app"
