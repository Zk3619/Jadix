# Jadix Web Gallery

This browser host reuses `../gallery` with native-only default features disabled. Pages retain their Rust Builder and `ui!` implementations; `scene3d` and `image-stream` are enabled. Native clipboard and window-state crates are excluded from the browser graph. System tray, OS windows/processes, filesystem dialogs, WebView hosting and threaded adapters are not browser implementations.

See the [English](../../docs/BUILD.md#web--webassembly) / [Chinese](../../docs/BUILD.zh.md#web--webassembly) build guide for prerequisites and deployment:

```sh
python examples/gallery_web/build.py --serve --port 8765
```

The script validates the wasm-bindgen CLI against `Cargo.lock`, builds release Wasm and serves the static bundle. `--no-build --serve` reuses it; `--port` chooses the local port, and `--bind` defaults to loopback. The browser needs a working GPU. The WebGL fallback is not a promise that every renderer capability is available. `/#forms`, `/#scene`, `/#documents` and `/#code` select initial pages.

## Fonts

The host registers Inter, Fira Mono, Arabic, Hebrew, Thai, Devanagari, math/symbol fonts and Noto Color Emoji before initializing the framework. Color fonts use the isolated emoji rasterizer rather than the ordinary UI font database. `assets/gallery-sans.otf` contains the complete Noto Sans CJK SC Regular face, not a subset of example strings, so Chinese input is not limited to text already present in Gallery. Extracting one face avoids embedding the entire font collection, but increases the Wasm download compared with a source-string subset. Characters outside these fonts still require registering an appropriate font with `jadix::utils::font::register_font_data`.

`gallery-sans.otf` derives from Noto Sans CJK, copyright the Noto Project Authors / Adobe and Google, under the [SIL Open Font License 1.1](assets/OFL.txt). The font's original copyright and licensing name records are retained. Inter's license is included with its [source asset](../../crates/jadix_cosmic_text/fonts/Inter-LICENSE).

Thai and Devanagari assets come from Google Fonts' Noto Sans families and retain their [Thai](assets/notosansthai-OFL.txt) and [Devanagari](assets/notosansdevanagari-OFL.txt) licenses. Other Noto faces use the [Noto license](../../crates/jadix_cosmic_text/fonts/NotoSans-LICENSE); Fira Mono retains its [license](../../crates/jadix_cosmic_text/fonts/FiraMono-LICENSE).

## Text Input And Colors

The window backend focuses a transparent textarea at the caret while a text field owns IME focus. It retains a nonzero font size so browser text services can report valid native character bounds; zero-sized glyphs can send Windows candidate windows to the page origin even when the element itself is correctly positioned. A fixed 10,000 CSS-pixel text layout is horizontally compressed to one visible CSS pixel, retaining the framework caret height and preventing ordinary preedit text from wrapping, scrolling or resizing the native host. The framework paints the visible preedit text. Browser composition events drive the framework's preedit/commit events, and ordinary keyboard commands remain in the existing input pipeline. The editable element must remain in the document and must not be hidden with `display:none` or `visibility:hidden`; those prevent browser IME and virtual keyboards from working. Candidate-window appearance is controlled by the browser and OS. Headless DOM geometry checks alone cannot validate native candidate positioning.

WebGPU configures a non-sRGB canvas texture with an sRGB render view so the renderer's linear output is encoded once, including offscreen captures and translucent modal overlays. A browser's preferred light/dark color scheme still selects the theme; it is not a color-conversion substitute.

To regenerate the complete CJK face, install `fonttools` in a Python environment and run `python examples/gallery_web/prepare_cjk_font.py`; this is not required for an ordinary build or after adding example strings. The source TTC must be present at `crates/jadix/assets/NotoSansCJK.ttc`. The extraction preserves all glyphs, character mappings and original font metadata.
