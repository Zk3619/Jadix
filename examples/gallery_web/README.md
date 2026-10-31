# Jadix Web Gallery

This browser host reuses `../gallery` with native-only default features disabled. Pages retain their Rust Builder and `ui!` implementations; `scene3d` and `image-stream` are enabled. Native clipboard and window-state crates are excluded from the browser graph. System tray, OS windows/processes, filesystem dialogs, WebView hosting and threaded adapters are not browser implementations.

See the [English](../../docs/BUILD.md#web--webassembly) / [Chinese](../../docs/BUILD.zh.md#web--webassembly) build guide for prerequisites and deployment:

```sh
python examples/gallery_web/build.py --serve --port 8765
```

The script validates the wasm-bindgen CLI against `Cargo.lock`, builds release Wasm and serves the static bundle. `--no-build --serve` reuses it; `--port` chooses the local port, and `--bind` defaults to loopback. The browser needs a working GPU. The WebGL fallback is not a promise that every renderer capability is available. `/#forms`, `/#scene`, `/#documents` and `/#code` select initial pages.

## Fonts

The host registers Inter from the workspace font assets and `assets/gallery-sans.otf` before initializing the framework. The latter is a renamed Noto Sans CJK SC Regular subset containing the characters in Gallery and component source strings, not every Chinese character. Additional input coverage requires registering an appropriate font with `jadix::utils::font::register_font_data`.

`gallery-sans.otf` derives from Noto Sans CJK, copyright the Noto Project Authors / Adobe and Google, under the [SIL Open Font License 1.1](assets/OFL.txt). The font's original copyright and licensing name records are retained. Inter's license is included with its [source asset](../../crates/jadix_cosmic_text/fonts/Inter-LICENSE).

To regenerate the subset after adding text, install `fonttools` in a Python environment and run `python examples/gallery_web/subset_font.py`; this is not required for an ordinary build. The source TTC must be present at `crates/jadix/assets/NotoSansCJK.ttc`.
