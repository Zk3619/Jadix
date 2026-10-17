# Gallery startup check (Windows)

```powershell
cargo build -p gallery
python scripts/startup-bench.py
```

The script measures process launch, first visible window, and first painted
content on the normal persisted-window path. It samples the foreground client
interior through the composed desktop, excludes title-bar/show-animation edges,
and closes only its own process. Keep the Gallery in the foreground during capture.
Compilation is excluded. The default gates are content within 3 seconds and a
visible blank interval below 200 ms. These are local regression budgets, not a
cross-machine startup guarantee. Run `--help` to change budgets or executable.

## Checkpoint — 2026-10-16

Windows 11, Intel UHD Graphics, D3D12, development build with the existing package
optimization settings. Three fresh processes per version; OS/driver disk caches
were warm. Both executables use `runtime-logging,cpu-bench`, with logging disabled.

| Median | Before | After |
|---|---:|---:|
| Launch to content | 2.948 s | 2.215 s |
| Visible blank interval | 2.885 s | 0.057 s |

The earlier maximization check inspected only the state flag. It did not verify
actual monitor coverage and therefore missed the restart geometry bug; use the
process round-trip regression below.

The state-persistence hook previously restored `visible=true` before GPU setup.
It now leaves first show/focus to the engine. Windows primes the real UI before
showing the primary window and retains the normal redraw retry for surfaces
that cannot present while hidden.

Restoring maximization also used Win32's show command briefly before hiding the
window again. The final correction retains hidden maximize intent without
staging `WS_MAXIMIZE`: setting that style prematurely made the later show command
leave the window at its restored size. Native regression checks both visibility
during messages and actual work-area coverage after showing.

The perimeter and general rectangle pipelines previously compiled identical
fragment functions under different entry names. Both now use `fs_main`, retaining
the distinct vertex geometry, opaque-center/solid fast paths, validation, custom
hot-reload ring shaders, and all existing damage/text/layout caches.

`startup-checkpoint.json` records executable hashes, raw startup samples and
the paired CPU smoke measurements. CPU smoke is a short development-build
comparison; it does not replace the release gates in [cpu-bench.md](cpu-bench.md).

The six-second paired scroll checks (two rounds, six-second warmup) retained
58.16–58.50 presented FPS. Single-core CPU medians were ordinary 15.75% → 15.23%,
Forms 23.04% → 23.31%, Layout 31.51% → 32.16%. These short, noisy development
samples show similar performance; ordinary/Layout still exceed the existing
absolute CPU targets. Three separate six-second About idle rounds had zero
redraws, zero presents, and no charged CPU tick. Rows losing foreground are
marked invalid in the raw data and excluded from conclusions.
CPU captures precede the final startup-only hidden-maximize backend correction;
all executable hashes are recorded separately.

## Release verification — 2026-10-16, 21:43

Run from the executable directory, matching Explorer's working directory:

```powershell
cargo build -p gallery --release --features cpu-bench,runtime-logging
python scripts/startup-bench.py --exe target/release/gallery.exe --cwd target/release --state .tmp/startup-isolated.toml --rounds 5 --max-content-seconds 1
python scripts/window-restore-check.py
```

The scripts use an isolated state file; the round-trip check maximizes the real
Gallery, closes it normally, reads the saved TOML, then restarts twice and checks
`IsZoomed` **and** `GetWindowRect` against the monitor work area. The old release
failed on both restarts (flag true, normal 1000×700 content bounds). The corrected
release passed all three launches and preserved the normal geometry.

Opaque desktop windows now prefer GL. On this Windows 11 / Intel UHD machine,
three old DX12 release starts had a 1.965 s median. Five new GL release starts
measured 0.393 / 0.423 / 0.418 / 0.428 / 0.454 s (median **0.423 s**). Three starts
from a copied maximized state measured 0.455 / 0.448 / 0.476 s. OS/driver caches
were warm; these numbers do not represent a reboot-cold launch or Raspberry Pi.
Transparent compositor windows retain their native backend order, and
`JADIX_GPU_BACKEND=dx12` remains available for backend comparisons.

The final viewport-HUD release was measured again: 0.411 / 0.449 / 0.475 s
(median **0.449 s**, visible blank 31–39 ms). Maximize/close/restart/restore passed
again on this executable. Exact executable hash and native measurements are in
[gallery-local-validation.json](gallery-local-validation.json).

Gallery shows a 500 ms performance HUD by default: presented FPS, frame interval,
CPU frame mean/max, layout and text preparation. CPU times include acquire/submit
waits; they are not GPU execution timings. The button collapses the panel;
`JADIX_PERF_OVERLAY=0` disables sampling. Idle does not force continuous redraws.
The HUD uses the existing viewport portal so it does not enter the retained
scroll layer. Native capture after 100 wheel messages found six ghost stripe
rows with the original absolute sibling; the portal version had zero differing
pixels against a forced full redraw in the same checked region.

Local scrim regression: `gallery_layout_and_scroll_damage_matches_full` with
`JADIX_DAMAGE_SECTION=overlays` compares retained scrolling against full redraw
over 180 scroll frames. Before: 12,624 differing pixels and horizontal text
stripes. After: passed, retaining all 180 optimized strip copies. The blur source
footprint is repainted and its glyphs prepared before the prefix capture.

`cargo test -p jadix --test backdrop_blur -- --ignored` checks actual GL pixels
for blur + modal blit in a shared encoder versus separate submissions. The blit
now reserves a uniform slot instead of overwriting the downsample parameters.

The five-second native scroll smoke run delivered 299 presents per 300 wheel
ticks in ordinary/forms/layout (59.79 FPS, p95 intervals 17.17–17.21 ms). Settled
windows had zero redraws. CPU use was 19.69% / 30.31% / 39.68% of one logical CPU,
so the existing stricter CPU budgets (10% / 25% / 25%) remain unmet; this is a
frame-cadence check, not a claim that all CPU performance gates passed.


## Modal fades — 2026-10-16

Dialog, Sheet, Drawer and ViewportModal share the same modal backdrop path.
Unchanged page draws/text reuse their captured background throughout fade/slide
frames. Page geometry, text, texture replacement, dynamic texture updates, size,
DPI and compositor changes invalidate the snapshot. Dynamic canvas/shader/effect
sources remain conservative and recapture. Completed blur output is reused when
both source and radius are unchanged.

Modal blur keeps a fixed work resolution chosen from its target radius (at least
half width/height), while sigma still follows the fading underlay opacity. This
avoids full-resolution small-sigma passes and texture reallocation at sigma
thresholds. Local frost and modal frost have separate targets/uniform buffers,
sharing the compiled program so first modal open does not compile it again.
Existing animation durations and opacity/translation curves are preserved.

The Windows GPU regression runs real Dialog/Sheet/Drawer enter and exit frames,
checks zero additional captures over four unchanged animation frames, fixed work
target sizes, page text invalidation, and no new blur pass after settling. At
mid-enter and mid-exit it compares retained pixels with a forced fresh capture
at the **same pose** (frame_dt=0); zero pixels exceed the 3-channel-value tolerance.
Both GL and DX12 passed. Test build optimization is disabled only for this crate
to keep the large native test executable within local compiler memory limits:

```powershell
cargo test -p jadix --lib --features svg,bundled-lucide --config 'profile.test.package.jadix.opt-level=0' --config 'profile.test.package.jadix.debug=false' modal_animation_reuses_background_and_matches_fresh_capture -- --ignored --nocapture --test-threads=1
```

The modal/drawer/sheet unit filters passed 105 tests; the native local-scrim scroll
regression still passed all 180 frames with 180 GPU strip copies. Raspberry Pi
Ubuntu 24.04 was offline: these checks establish local correctness and reduced
render work, not a measured Pi frame rate.


Final release native pair (1400×1000 physical window, eight cycles with first
cycle excluded; no concurrent build; all samples foreground): whole-process CPU
time over each 600 ms observation window had medians Dialog enter 109→109 ms,
Dialog exit 234→234 ms, Sheet enter 234→203 ms (13% lower), Sheet exit 266→266 ms.
This desktop CPU sample does not establish a broad animation-speed improvement;
the deterministic GPU tests establish less capture work and smaller blur targets.
The final executable SHA-256 is
`a300153cf2b93037b62deee386ee87d4f0433cc77b52f8f7ca80e42825f74dfb`.
Startup recheck: 0.386 / 0.401 / 0.483 s (median 0.401 s); maximize/save/restart
round-trip and normal geometry restoration passed again. Raw samples are retained
in [gallery-local-validation.json](gallery-local-validation.json).
