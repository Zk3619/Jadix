#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "$0")/../.." && pwd)"
target="${1:-x86_64-unknown-linux-gnu.2.39}"
rust_target="${target%%.*}"
output="${GALLERY_OUTPUT_DIR:-$workspace/examples/gallery_mobile/dist/linux}"
export PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
pkg-config --exists gtk+-3.0 webkit2gtk-4.1
cd "$workspace"
cargo zigbuild -p gallery --bin gallery --release --all-features --locked --target "$target" -j "${CARGO_BUILD_JOBS:-4}"
bundle="$output/$rust_target"
mkdir -p "$bundle"
cp "${CARGO_TARGET_DIR:-$workspace/target}/$rust_target/release/gallery" "$bundle/gallery"
cp examples/gallery/theme.toml examples/gallery/styles.toml "$bundle/"
cp examples/gallery/LINUX.md "$bundle/README.md"
cp -R examples/gallery/assets "$bundle/"
tar -czf "$output/jadix-gallery-$target-release.tar.gz" -C "$bundle" gallery theme.toml styles.toml assets README.md
sha256sum "$bundle/gallery" "$output/jadix-gallery-$target-release.tar.gz"
