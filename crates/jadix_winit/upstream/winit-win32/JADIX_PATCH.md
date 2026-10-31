# Jadix Win32 window chrome

Source: rust-windowing/winit `758a50e35a928dbf72bf05ae91f12fee7a540a83`,
`winit-win32/` (2026-10-16). Apache-2.0 retained in LICENSE.

The facade needed access to native non-client hit testing. Keep this backend's
core, DPI and common dependencies on exactly the same revision as jadix_winit.
The manifest is standalone, unpublished, and the upstream facade dev dependency
is omitted to avoid resolving a second window/core family.

Local changes:

Reference: GPUI Kit TitleBar at `2963d6d880b9dffe59b48b0533bdc83c03201209`
uses GPUI WindowControlArea; GPUI's Windows implementation maps those areas to
native hit codes. This implementation uses the same public Win32 mechanism in
Winit's existing message process. Snap flyout display is controlled by Windows;
the passing WM_NCHITTEST test does not certify the visual flyout or pointer UX.
