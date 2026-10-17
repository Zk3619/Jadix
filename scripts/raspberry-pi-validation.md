# Raspberry Pi 5 / Ubuntu 24.04 follow-up

The supplied `gallery-vulkan.log` identifies the actual Gallery adapter as
`V3D 7.1.10.2` with GPU text rendering. It contains 1,644 symbol-font load attempts;
the database repeatedly stays at 18 faces. The OpenGL renderer reported by `inxi`
does not describe this Vulkan device.

## Changes

- Recognize the Unix fallback families already loaded into each font database.
  In particular, DejaVu Sans supplies the Unix symbol fallback. Repeated coverage
  probes must preserve cosmic-text's font-match cache and Jadix's measurement cache.
- Recognize both `File` and `SharedFile` sources when deduplicating font paths.
- Honor `Preedit(_, None)` by hiding the Input/Textarea caret while preserving
  inline candidate text and the candidate-window anchor. Preserve Windows' native
  missing-position fallback separately.
- Use the trailing endpoint of an IME cursor range, including reversed ranges
  such as Smart Pinyin's `(18, 6)`. Input, Textarea and extra carets share the fix;
  collapsed ranges still allow the user to move inside the composition.
- Skip portal membership scans on stable page frames with no overlay activity.
  Reuse that preparation for the first nonstructural layout pass; consume the
  hint after one pass so standalone layout still hoists newly introduced portals.
- Try remaining compiled graphics backends before accepting a CPU adapter.
  Log backend initialization failures, adapter/driver details and startup stages.
- Give Wayland windows an executable-name application ID by default and ship
  `gallery.desktop` for the desktop display name.

## Fixed-header artifact

The user could no longer reproduce this symptom in the latest report. Further
investigation is suspended; the earlier diagnostic results below are retained.

The remaining Pi report places intermittent black pixels to the left of the
fixed **Jadix Gallery** header. It persists with `JADIX_SCROLL_STRIP_BLIT=0`.
This differs from the earlier footer clipped-edge regression.

The native Gallery regression now checks the entire fixed header, including the
space left of its icon, on every scrolling frame. The small-sidebar regression
also checks it during sidebar scrolling, item hover changes and scrollbar changes.
Both tests support `JADIX_SCROLL_STRIP_BLIT=0`, asserting that no GPU strip copies
execute in that mode. A local pass does not establish a fix on the Pi's V3D driver.

```powershell
cargo test -p jadix --lib --features svg,bundled-lucide --config 'profile.test.package.jadix.opt-level=0' --config 'profile.test.package.jadix.debug=false' --no-run
# Run each native test separately: winit permits one event loop per process.
$env:JADIX_GPU_BACKEND = 'gl' # Repeat with dx12.
cargo test -p jadix --lib --features svg,bundled-lucide --config 'profile.test.package.jadix.opt-level=0' --config 'profile.test.package.jadix.debug=false' gallery_layout_and_scroll_damage_matches_full -- --ignored --nocapture --test-threads=1
cargo test -p jadix --lib --features svg,bundled-lucide --config 'profile.test.package.jadix.opt-level=0' --config 'profile.test.package.jadix.debug=false' sidebar_clipped_edge_keeps_color_after_hover -- --ignored --nocapture --test-threads=1
# Repeat both tests with this override, then remove it.
$env:JADIX_SCROLL_STRIP_BLIT = '0'
```

## Device checks

Use the newly built package so the font fix and startup diagnostics participate.
Run from the graphical desktop, scroll the basics page and close the app after
each run. Keep the first and second launch logs separately.

```sh
JADIX_GPU_BACKEND=vulkan JADIX_RENDER_PROFILE=1 RUST_LOG=info ./gallery 2>&1 | tee pi-vulkan-first.log
JADIX_GPU_BACKEND=vulkan JADIX_RENDER_PROFILE=1 RUST_LOG=info ./gallery 2>&1 | tee pi-vulkan-second.log
JADIX_GPU_BACKEND=gl JADIX_RENDER_PROFILE=1 RUST_LOG=info ./gallery 2>&1 | tee pi-gl.log
```

If the header artifact persists, compare full redraw on the same Vulkan driver:

```sh
JADIX_GPU_BACKEND=vulkan JADIX_PARTIAL_DAMAGE=0 JADIX_RENDER_PROFILE=1 RUST_LOG=info ./gallery 2>&1 | tee pi-vulkan-full.log
```

If full redraw removes it, the remaining investigation is the retained/partial
paint path. If it persists, a partial-damage-only explanation is insufficient.
Neither result alone proves a driver defect. This is a diagnostic override and
can increase frame cost.

Inspect `backend gl: ...` if the GL attempt selects Vulkan again. The setting
changes backend priority; initialization failure or a CPU GL adapter can cause
fallback. Compare `startup stage=...`, `startup primary_ui_mounted_ms`, adapter
and `render profile` lines. UI mounting is not the first visible display frame.

60 FPS and first-launch latency remain device measurements; local build and
regression results do not establish those outcomes.

## Local results (2026-10-17)

- Regular library suite after the cursor-range and portal changes:
  **3,805 passed, 63 ignored, zero failures**.
- Font-cache checks: **5 passed**; caret-related checks: **17 passed**;
  IME support checks: **26 passed** (also included in the full library suite).
- Native GPU matrix: **8 passed** (GL/DX12 × strip-copy on/off × main-page
  scroll/sidebar scroll-and-hover). Main-page cases check 180 scrolling frames
  each; optimized cases execute 135 GPU strip copies, disabled cases execute zero.
- `git diff --check` passes. The IME test/font files pass rustfmt. Full-file
  rustfmt still reports existing formatting differences in `state.rs`,
  `lifecycle.rs` and `portal_hoist.rs`; unrelated regions were left as-is.
- Raw logs: `.tmp/pi-continue-library-tests.log` and `.tmp/pi-header-*.log`.
- Latest library log: `.tmp/pi3-library-tests.log`. The updated native GL/DX12
  main-page and sidebar cases pass (four runs; `.tmp/pi3-gpu-*.log`).
- The original reversed-range regression failed with byte position 6 instead of
  12 (`.tmp/pi2-ime-red.log`), then passed after using the trailing endpoint.
  Component coverage also checks a six-Han `(18, 6)` range and a subsequent
  collapsed `(3, 3)` cursor move in both Input and Textarea.

The fixed-header symptom remains unconfirmed on the Pi after these changes.

## Automated Pi measurements (2026-10-17)

SSH was used to launch the app in the existing Wayland session. The monitor was
1600×900 at 60 Hz, CPU governor `ondemand`, and observed device temperatures were
about 47–50°C. Every run used the basics page, the same maximized window state,
three seconds of startup, and four alternating groups of 65 discrete wheel
events at 80 ms intervals. The driver uses the existing Mutter RemoteDesktop
session API, stops the temporary session on exit, and asserts that the Gallery
log contains `scroll=true` before accepting a run. No text is typed.

An initial uinput attempt produced zero scrolling samples and was rejected. It
is excluded from these measurements. The driver is retained for debugging in
`.tmp/pi3_device_bench.py`; no credentials are stored in it.

The first three render-profile windows are excluded from the means below.
These are means of one-second application profiling windows during this fixed
wheel sequence; they are not monitor scanout measurements or a broad guarantee.

| Original package, same binary | Mean FPS | Pre (ms) | Layout (ms) | Present (ms) |
| --- | ---: | ---: | ---: | ---: |
| Default mode, run 1 | 46.15 | 4.86 | 4.77 | 2.25 |
| Default mode, run 2 | 46.26 | 4.81 | 4.73 | 2.23 |
| Mesa Mailbox override, run 1 | 48.93 | 4.86 | 4.60 | 0.20 |
| Mesa Mailbox override, run 2 | 49.45 | 4.80 | 4.61 | 0.18 |

The original binary SHA-256 is
`15d013165915a66a395f0adbbb251411de67490de9bfc92dc4c3e9f571953ac8`.
Raw logs: `.tmp/pi3-dbus-{fifo,mailbox}.log` and
`.tmp/pi3-repeat-{fifo,mailbox}.log`.

Mailbox reduced presentation blocking but did not reach 60 FPS by itself. The
override is scoped to each test launch; the application's presentation policy
is unchanged.

### New package results

The new executable SHA-256 is
`ef3818c54d324c89e653606832107981a7c87eef3fbfdb989e473c716c723889`.

| New package | Mean FPS | Pre (ms) | Layout (ms) | Present (ms) |
| --- | ---: | ---: | ---: | ---: |
| Default mode, run 1 | 45.87 | 4.84 | 4.18 | 2.63 |
| Default mode, run 2 | 45.66 | 4.89 | 4.23 | 2.42 |
| Mesa Mailbox override, run 1 | 49.68 | 4.86 | 4.14 | 0.18 |
| Mesa Mailbox override, run 2 | 49.08 | 4.94 | 4.14 | 0.19 |
| Mailbox, performance HUD disabled, one run | 53.33 | 4.28 | 4.16 | 0.22 |

The duplicate-hoist change saves roughly 0.5–0.6 ms in layout, but the repeated
device results do **not** establish an overall FPS improvement from the code
change. Stable-overlay presence still takes the conservative membership scan;
the performance HUD is such an overlay. Disabling the HUD is a diagnostic
comparison, not a substitute for fixing its overhead. 60 FPS remains unresolved.

The first two new-package runs reported `primary_ui_mounted_ms=247.7` and
`237.8`; the GPU and shared-font initialization stage completed in about 146 ms
on the first run. These are warm-system measurements after repeated test
launches. Neither measures reboot-cold startup or the first visible frame.

Raw new-package logs: `.tmp/pi3-new-{fifo,mailbox}.log`,
`.tmp/pi3-new-repeat-{fifo,mailbox}.log`, and `.tmp/pi3-no-hud-mailbox.log`.
The summary is `.tmp/pi3-device-summary.json`.

The cursor-range bug is covered by replay and component regressions. The Pi
performance driver deliberately types no text, so a physical Smart Pinyin
typing confirmation remains a separate device check.
