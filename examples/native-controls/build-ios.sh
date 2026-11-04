#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
arguments=(--target "${1:-aarch64-apple-ios-sim}" --output "${2:-$root/.theme-evidence/native-controls/ios}")
if (( $# > 2 )); then arguments+=("${@:3}"); fi
exec python3 "$root/examples/native-controls/build-ios.py" "${arguments[@]}"
