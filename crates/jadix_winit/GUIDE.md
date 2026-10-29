# jadix_winit

Jadix-owned source fork of the Winit **facade**, with small Win32 and Wayland
backend patches. The facade uses pinned backend/common/core
sources as explicit `jadix_winit_*` packages at `0.31.0-beta.3-jadix.1`.
Normal/build dependencies use path plus registry versions, with no root patch
or Git dependency required by consumers. Every backend and window helper shares
registry `dpi = 0.1.2`. Workspace members retain the dependency alias `winit`.

These packages are prepared for publication; this does not mean they have been
uploaded or the registry names are owned. Source identities are recorded in
`vendor/RELEASE_FORKS.json`, each `JADIX_FORK.md`, and the original
`vendor/winit-{win32,wayland}/JADIX_PATCH.md`. Provenance identifies the imported source, not platform runtime validation.

## Source provenance (2026-10-16)

- Facade and the base of both local backend patches, plus all unchanged backends: [rust-windowing/winit
  `758a50e35a928dbf72bf05ae91f12fee7a540a83`](https://github.com/rust-windowing/winit/commit/758a50e35a928dbf72bf05ae91f12fee7a540a83).
  This was official `master` HEAD when imported; the version still says
  `0.31.0-beta.3`, but contains changes after that release.
- OHOS backend: [ohos-rs/winit-ohos
  `af4063b4e75b62b152a315d2606d799ca65bda84`](https://github.com/ohos-rs/winit-ohos/commit/af4063b4e75b62b152a315d2606d799ca65bda84).
- OHOS facade wiring adapted from [richerfu/winit
  `c9ee2bff8a79aacb8068546e7150197693d35a5a`](https://github.com/richerfu/winit/commit/c9ee2bff8a79aacb8068546e7150197693d35a5a).
  `winit-core` and `dpi` trees at that revision exactly matched the official
  revision above. Explicit package dependencies select the maintained core and
  a single registry DPI identity.
- Original Apache-2.0 license is retained in `LICENSE`; copied facade sources and
  changelogs retain upstream documentation (crate paths adjusted for this name).

## Local changes

1. Package renamed to `jadix_winit`, with explicit pinned backend dependencies.
2. `target_env = "ohos"` is excluded from Unix/X11/Wayland selection even though
   OHOS triples use `target_os = "linux"`. It selects `winit-ohos` instead.
3. `platform::ohos::EventLoopBuilderExtOpenHarmony::with_openharmony_app` configures
   the ordinary `EventLoop::builder()`. `run_app` registers Ability callbacks and
   returns. Desktop, Android and iOS retain their upstream lifecycle semantics.
4. Optional `ohos-window-plugin`, `ohos-native-vsync`, `ohos-extended-plugins`
   forward backend features. The matching Ability/ArkTS plugins must be installed
   by the application. These switches do not certify device support.
5. `chrome::WindowExtJadix` exposes a live `WindowChrome` snapshot, guarded native
   resize, fallible Windows/Wayland system menus and native title-bar double click.
   `TiledEdges` and `resize_direction_at_frame` handle logical shadow/frame geometry;
   these helpers do not claim to configure compositor window geometry.
6. Windows `platform::windows::set_window_control_regions(&Arc<dyn Window>, Vec<...>)`
   publishes logical native caption rectangles. Existing Winit WindowState/WndProc
   owns them, checks live flags/DPI, and returns HTCAPTION/HTMAXBUTTON/HTMINBUTTON/
   HTCLOSE. Last containing rectangle wins; Client masks interactive children.
   Empty clears; invalid replacement preserves the old map; any thread can update.
   Do not also run client click handlers on these regions. Snap flyout visibility
   remains Windows policy. No subclass helper imports or global HWND registry.
7. Wayland `Window::frame_state()` reports actual configure decoration, per-edge
   tiling and compositor capabilities; None means not configured. Menus require a
   configured toplevel, advertised capability and an input serial, and submit once.
8. macOS `platform::macos::titlebar_double_click` reads AppleActionOnDoubleClick.
   None/Minimize/zoom respect native window policy and main-thread affinity. Fill
   falls back to public zoom unless private-apple-apis explicitly enables _zoomFill:.
   `set_traffic_light_position` places real AppKit buttons; reapply after native
   title-bar layout/fullscreen transitions and reserve content space for controls.
   Existing AppKit tab, shadow, material and fullscreen-auxiliary APIs remain exposed.
9. No Element, Signal, renderer or product-specific scheduling enters this crate.

The native API reference was [GPUI Kit
`2963d6d880b9dffe59b48b0533bdc83c03201209`](https://github.com/longbridge/gpui-kit/tree/2963d6d880b9dffe59b48b0533bdc83c03201209):
TitleBar (GPUI WindowControlArea and macOS double-click preference), Linux
WindowBorder (configure/tiled edges, shadow geometry) and fixed-size checks #3525.
Kit consumes GPUI native backends; it does not have a separate OS event loop.


## Integration and updates

Run `cargo test -p jadix_winit --lib` and the consuming application's window/proxy contracts. GPU/native window tests need separate interactive processes. Compile OHOS with its SDK, then verify focus, IME, surface recreation and teardown on a device.

Keep the maintained core and all backends on compatible pinned versions. Reapply documented native patches and update source provenance together. A consumer resolves versioned packages without copying repository patches. Check Cargo metadata for one maintained Winit core and one registry DPI identity across all targets, then validate each supported platform.

Traffic-light placement must be reapplied after native layout transitions. Callers must publish Windows caption regions after layout. Client frame geometry, layer-shell and application-global Linux menus are outside these extensions. Host compilation alone does not certify native interaction or accessibility.
