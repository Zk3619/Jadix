# Sidebar clipped-edge validation — 2026-10-17

The reported sequence is: expose a partially clipped Sidebar item above the
footer, scroll down and back to the top, then change the hovered item or the
scrollbar visibility. The exposed text/icon edge must keep the same color.

## Fix

`scroll_strip_copy_physical` previously used a damage scissor, whose outward
rounding included the footer separator's antialiased boundary row in the copy.
Copying that row translated fixed chrome into scrolling content. Scroll copies
now use the same integer overflow clip as glyphon.

The first correction exposed a second mismatch at a 1.333333 logical-pixel
offset on the 150% display: textured Lucide icons still used fractional fragment
clips and could draw one more row on a full frame. `scale_textured_draws` now
integerizes these clips after DPI scaling, using the shared text clip helper.
A clip that snaps empty remains empty, rather than disabling overflow clipping.

No boundary pins or additional redraw bands were added for this fix. The
retained scroll-copy optimization remains enabled.

## Checks

- All regular `jadix` library tests: **3799 passed, 63 ignored, zero failures**.
- Sidebar GPU regression on **GL and DX12**: 9 offsets, each with 3 hover changes
  and scrollbar show/hide. Compare the clipped text/icon column before and after
  each action, then against a full composite. The scrollbar gutter is excluded
  because its own pixels change on show/hide. The fixture asserts that clipped
  ink is present and that GPU scroll copies actually execute.
- Full Gallery scroll differential on **GL and DX12**: **180 scrolling frames,
  135 GPU copies** per backend; incremental/full comparisons passed.
- `rustfmt` check on the changed files and `git diff --check` passed.

The original native release still reproduced the issue with the same Win32
wheel/mouse replay: 150 dark pixels in the clipped item fixture, then 323
changed pixels after a hover switch. This provides the before-fix signal.

The GPU tests use an actual hidden native window and GPU readback. Run each
ignored native-window test in its own process, since winit does not allow
recreating the event loop within a single test process:

```powershell
$env:JADIX_GPU_BACKEND = 'gl' # repeat with 'dx12'
cargo test -p jadix --lib --features svg,bundled-lucide --config 'profile.test.package.jadix.opt-level=0' --config 'profile.test.package.jadix.debug=false' sidebar_clipped_edge -- --ignored --nocapture --test-threads=1
cargo test -p jadix --lib --features svg,bundled-lucide --config 'profile.test.package.jadix.opt-level=0' --config 'profile.test.package.jadix.debug=false' gallery_layout_and_scroll_damage_matches_full -- --ignored --nocapture --test-threads=1
```

Raw local test logs are under `.tmp/sidebar-final-gl.log`,
`.tmp/sidebar-final-dx12.log`, `.tmp/gallery-scroll-final-gl.log`,
`.tmp/gallery-scroll-final-dx12.log`, and `.tmp/sidebar-continue-unit.log`.

## Deliverables

The ARM64 package targets Ubuntu 24.04 / glibc 2.39 and includes the executable,
theme, styles, SVG asset, license, notice, and desktop run instructions. It
includes the earlier startup, HUD, IME, modal/Drawer, and Portal fixes.
Raspberry Pi graphical behavior and performance still require device validation.

## Restart/maximize regression found during validation

The old persistence script sampled geometry only after a 0.5-second delay. It
missed an initially visible normal-size window followed by maximization 10–20ms
later. The Win32 flag transition called `SW_SHOW` before `SW_MAXIMIZE`.

The backend now chooses the requested show state in its first `ShowWindow`
call. The native test observes `WM_WINDOWPOSCHANGED`: the old code exposed one
visible normal-size state, and the fix exposes none. Hidden-window visibility,
maximized work-area coverage and caption behavior pass; 20 persistence unit
tests pass. `scripts/window-restore-check.py` now samples from the first visible
HWND for one second and checks every observed startup geometry.

The side-by-side native Sidebar replay also passed on the first fixed Windows
release: 124 dark pixels confirmed the clipped fixture, and all 3 hover changes
had zero changed pixels. The final Windows release additionally includes the
atomic maximize-show correction.

## Final artifacts and native replay

- Windows: `target/release/gallery-sidebar-fixed.exe` (the old running process
  still locks `gallery.exe`). The final executable passed the 3-launch
  persistence replay: both restart launches covered the work area from the first
  visible sample, and restoring retained the original normal-size placement.
- The final Windows Sidebar replay again had 124 dark fixture pixels and zero
  changed pixels for all 3 hover switches. Startup-to-content median was
  **384.8ms** across 3 runs; the visible blank interval was 29–34ms.
- ARM64: `target/artifacts/gallery-ubuntu24.04-arm64-sidebar-fix-20261017.tar.gz`.
  Checked AArch64 ELF, glibc maximum 2.39, included resources and executable mode
  0755. QEMU reached event-loop creation, then exited with the expected missing
  graphical-display error. Actual Raspberry Pi graphics remain untested.
- Additional shared-texture checks passed: modal animation on GL and all four
  Drawer directions, resize, scroll and style changes on DX12.

Executable/archive SHA-256 identities, complete startup and restart results,
and test summaries are saved in `scripts/sidebar-validation.json`.
