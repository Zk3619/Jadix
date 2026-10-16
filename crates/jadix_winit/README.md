# jadix_winit

Jadix-owned source fork of the Winit **facade**, with small Win32 and Wayland
backend patches. Other backends and `winit-core` come from one pinned upstream Git
revision. Local backend provenance is recorded in `vendor/winit-{win32,wayland}/JADIX_PATCH.md`. This crate is unpublished; workspace
members keep the dependency alias `winit` to avoid mass import changes.

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
  revision above. The workspace patches its Git source to that same official
  source so identical version numbers cannot hide distinct Rust type identities.
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


## Verification and updates

Run `cargo test -p jadix_winit --lib`, Jadix's window/proxy contracts and the
`desktop_native_smoke` / `desktop_proxy_wake_batches` ignored tests in separate
interactive processes. Compile the OHOS backend and Ability entry with the OHOS
SDK; compilation alone does not verify focus, IME, surface recreation or teardown
on a device. Consult `development-docs/ops/native_window_and_menu.md` for results.

Keep the core and every unchanged backend on the same exact revision; rebase the
two documented native backend patches onto it. Update
the source copy and provenance together, reapply the small OHOS wiring delta,
then check Cargo metadata for **one** `winit-core` and **one** `dpi` package identity
across all targets. Recheck Windows, macOS, Linux, Web and OHOS compilation after
updates. A standalone consumer must reproduce the root Git patches from
`Cargo.toml`; otherwise OHOS's core/DPI source would differ. Publishing requires
an independently resolved dependency strategy and is intentionally disabled.

This fork adds tested native API extensions. Wider OS integration,
upstream merge costs and cross-platform acceptance remain engineering work; a
new package name alone does not increase Jadix's comparison score.

### Native API validation: 2026-10-16

- Windows: three facade geometry/policy tests pass. The separate native caption
  test passes with actual WM_NCHITTEST, runtime fixed-size changes, native caption
  double-click/system maximize, invalid replacement, worker clear and HWND release.
  Facade, local Win32 backend and Linux-target Wayland tests pass strict Clippy.
- Jadix: 3701 library tests pass, 44 ignored. Native window/menu/lifetime and
  proxy batching tests pass in separate Windows processes. The first full run had
  an existing style-matching performance-budget timeout (35.557 ms vs 25 ms);
  its isolated retry and the complete rerun passed. See the local ops log.
- Cross compile: aarch64 macOS (including private-apple-apis), aarch64 Linux
  (Wayland+X11), wasm32 facade and all-feature aarch64 OHOS pass. Wayland's
  configure-projection test compiles for Linux and is registered in Linux CI;
  it has not been executed on this Windows host.

Workspace all-target checks, dependency identity and license/source checks pass.
Jadix strict Clippy retains 568 existing errors, with no primary diagnostic in
files changed in this round. macOS/Linux/OHOS real-system interaction is still
required for acceptance.
Traffic-light placement must be reapplied after native layout transitions;
Windows regions must be published by the caller after layout (Jadix TitleBar does
not yet publish them automatically). Client frame geometry, layer-shell and
application-global menus on Linux remain work. Full Jadix Web compilation still
has the pre-existing clipboard gap. These are not certified by facade checks.
