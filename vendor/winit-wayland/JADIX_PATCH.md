# Jadix Wayland configure and menu APIs

Source: rust-windowing/winit `758a50e35a928dbf72bf05ae91f12fee7a540a83`,
`winit-wayland/` (2026-10-16). Apache-2.0 retained in LICENSE.
Manifest dependencies replace the upstream workspace inheritance and keep core,
DPI and common on the same exact source as jadix_winit. Package is unpublished.

Local changes:

- `window/chrome.rs`: Window::frame_state projects actual SCTK WindowConfigure
  into server decoration, per-edge tiling, minimize/maximize and menu capability.
  Returns None before configure or for popups, rather than inventing support.
- `Window::try_show_window_menu`: validate coordinates and submit only with a
  configured xdg_toplevel, WINDOW_MENU capability and a pointer/touch input serial.
  The existing void setter delegates to it. At most one seat receives the request.
  Result means request submitted, not that the compositor displayed the menu.
- `window_state` is now read for the snapshot, so the old dead-code expectation
  on the retained Arc is removed.

Reference: GPUI Kit TitleBar/WindowBorder at
`2963d6d880b9dffe59b48b0533bdc83c03201209`; fixed-size regression #3525.
Jadix's facade uses these facts to suppress duplicate/server decorations,
unsupported controls and resizing attached edges. Pure frame geometry excludes
the shadow's extension lines; it does not itself set xdg window geometry.

Validation: Linux target compile (default Wayland+X11); configure projection unit
test is runnable with `cargo test -p winit-wayland --lib projects_real_configure`
on Linux. A real compositor/device session is still required for input serials,
tiled restore, CSD/SSD transitions and system-menu visual acceptance. Xdg client
inset geometry, layer-shell and global menus are not added by this patch.

On refresh, preserve this small delta, update the shared upstream revision and
rerun scripts/check-window-deps.ps1; do not update only one core/DPI dependency.
