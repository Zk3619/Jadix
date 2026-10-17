# Jadix Win32 window chrome

Source: rust-windowing/winit `758a50e35a928dbf72bf05ae91f12fee7a540a83`,
`winit-win32/` (2026-10-16). Apache-2.0 retained in LICENSE.

The facade needed access to native non-client hit testing. Keep this backend's
core, DPI and common dependencies on exactly the same revision as jadix_winit.
The manifest is standalone, unpublished, and the upstream facade dev dependency
is omitted to avoid resolving a second window/core family.

Local changes:

- `caption.rs`: logical caption rectangles, last rectangle wins (Client masks
  interactive children out of Drag). Current RESIZABLE/MAXIMIZABLE/MINIMIZABLE/
  CLOSABLE/fullscreen/decoration flags determine each result.
- `WindowState` owns the rectangle vector; the existing mutex serializes updates
  from any thread. No global registry, extra HWND hook or strong window reference.
- `WindowExtWindows::set_caption_regions` validates the whole replacement before
  changing state. Empty removes the map. Coordinates must be republished on layout.
- Existing WndProc handles WM_NCHITTEST: preserve OS frame hits; translate signed
  screen coordinates with current DPI; return HTCAPTION/HTMAXBUTTON/HTMINBUTTON/
  HTCLOSE/HTCLIENT. WM_CLOSE continues through the ordinary Winit close event.
- Native caption double-click and SC_MAXIMIZE recheck fixed/fullscreen/button
  policy, covering Snap and system-menu routes. Programmatic set_maximized remains
  available through the existing ShowWindow path.
- Hidden maximize changes retain the intent in `WindowFlags` without setting
  `WS_MAXIMIZE` or calling `ShowWindow`. Staging that native style on normal
  bounds makes the later maximize command skip resizing. Explicit show applies
  maximization and actual monitor geometry, preserving the first-show boundary.
  Regression: `cargo test -p jadix_winit --lib native_hidden_maximize -- --ignored
  --test-threads=1` observes visibility and actual work-area coverage. The Gallery
  process/persistence regression is `python scripts/window-restore-check.py`.
  Explicit show uses `SW_MAXIMIZE` directly when restoring maximization; it must
  never expose normal geometry via an earlier `SW_SHOW`. The native observer
  rejects visible normal-size `WM_WINDOWPOSCHANGED` events, and the persistence
  script samples from the first visible HWND rather than waiting for startup.

Reference: GPUI Kit TitleBar at `2963d6d880b9dffe59b48b0533bdc83c03201209`
uses GPUI WindowControlArea; GPUI's Windows implementation maps those areas to
native hit codes. This implementation uses the same public Win32 mechanism in
Winit's existing message process. Snap flyout display is controlled by Windows;
the passing WM_NCHITTEST test does not certify the visual flyout or pointer UX.

Validation: `cargo test -p jadix_winit --lib native_caption_regions -- --ignored
--test-threads=1` in a separate interactive Windows process. Checks actual
WM_NCHITTEST, client masks, runtime fixed-size changes, invalid replacement,
worker update/clear, HWND lifetime. No GPU is needed.

On upstream refresh, compare these changes against this revision, update both
local backends/provenance, and rerun scripts/check-window-deps.ps1. The other
platform sources remain pinned upstream; this is an explicit maintenance delta.
