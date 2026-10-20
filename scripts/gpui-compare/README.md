# Fixed-content native frame comparison

Cache-policy comparison (both native D3D11 and ANGLE Jadix against Fast):

```powershell
python scripts/d3d11-compare.py --jadix <frozen-jadix.exe> --fast <frozen-fast.exe> --angle-library <libEGL.dll> --output <fresh-directory> --policy default
python scripts/d3d11-compare.py --jadix <frozen-jadix.exe> --fast <frozen-fast.exe> --angle-library <libEGL.dll> --output <another-fresh-directory> --policy full-frame
python scripts/d3d11-compare.py --jadix <frozen-jadix.exe> --fast <frozen-fast.exe> --angle-library <libEGL.dll> --output <paired-directory> --policy both
```

The driver explicitly sets `JADIX_PARTIAL_DAMAGE`, `JADIX_SCROLL_STRIP_BLIT`
and `GPUI_SCROLL_LAYERS` for each process. `full-frame` disables retained scroll
pixels on both sides; glyph, text, layout and other normal caches remain enabled.
Both modes reject a per-scene median scroll-distance ratio above 1.10.
`both` alternates policy order each round within each engine, and rotates engine
order across the three rounds, so the policy comparison is paired in time.
`--cache-diagnostics` is a separate profiled run, excluded from CPU acceptance.
The opt-in Windows Fast renderer trace records actual scene layer count,
composited tiles, dirty tiles and tiles scheduled for rasterization. A frame
with composited tiles and zero rasterized tiles demonstrates pixel reuse;
an empty layer list demonstrates that scroll-layer caching was not used.
These diagnostics require a build containing the benchmark identity feature.
The comparison driver defaults to 12 seconds of idle warmup, then 4 seconds of
unmeasured scrolling and a 1 second drain before timing. Warmup wheel events are
counted separately and included in exact delivery validation, but excluded from
the measured CPU/frame and displacement interval. Set `--scroll-warmup 0` for
the historical cold-scroll protocol; do not pool these two protocols.
With scrolling warmup enabled, the driver also compares client pixels before
and after the even-second round trip (threshold 6 per channel, excluding the
outer 32px scrollbar/pointer margins). Any changed interior pixels reject the
capture before CPU timing; the saved PNGs retain the evidence. This catches
fractional-DPI strip-copy drift that nonblank/palette checks cannot detect.
Jadix's opt-in trace records its actual partial-load target, committed scroll
copies, repair rectangles and collect replay/patch counts. Logical damage
classification alone is insufficient to establish that partial rendering ran.

The ANGLE transport and optional native D3D11 HAL are experimental. Both have
native-window and offscreen pixel checks and a fixed-content benchmark path.
No production performance winner is certified. Keep failed historical smoke
data separate; rotating fixed-content results remain calibration evidence.

Run from the repository root on Windows:

```powershell
./scripts/gpui-compare.ps1 -Mode Build
./scripts/gpui-compare.ps1 -Mode Run -AngleLibrary D:/path/to/angle/libEGL.dll
python scripts/gpui-compare/test_summary.py
```

The three executables are saved outside Cargo targets in
`.tmp/d3d11-compare-20261018/`. `cargo clean` does not remove that evidence.
GPUI Kit uses `Reference/gpui-kit-main/crates/kit` with default features disabled
(Kit + Base; no Component skin/assets). GPUI Fast uses **the identical adapter**
and Kit's official `gpui-fast` feature. The standalone Cargo.lock pins published
GPUI dependencies. `Reference/gpui-fast-main` is not silently substituted for the
published Fast engine. The two Windows crates have local, opt-in GPU-identity
patches only; no rendering or Fast cache logic is changed. Sources under
`Reference` are unchanged. Jadix uses a dedicated
workspace example, `jadix-frame-compare`, with optional ANGLE D3D11 and a11y.

The comparison requires D3D11 and the **same selected DXGI adapter/LUID** in every
capture. ANGLE queries EGL's actual D3D11 device and its DXGI adapter. Kit/Fast
tag GPU specs with the LUID read from their actual renderer's adapter. A device
name alone is insufficient (this machine has two same-name Intel adapters).
Missing identity, fallback, mixed DX12/DX11, or a
different device rejects the set as `incomparable-renderer`. Historical captures
without these fields remain preserved, but cannot pass the new backend check.
Jadix's `dx11-angle` request fails instead of falling back. Native Kit/Fast remain
native D3D11; translated Jadix is explicitly reported as `Gl/angle-d3d11`.

Native and translated bring-up oracle:

```powershell
./scripts/d3d11-verify.ps1 -AngleLibrary D:/path/to/angle/libEGL.dll
```

This checks a 16x16 RGBA8 render target, shader-generated triangle, top-left 8x8
scissor and staging readback on both paths. The native diagnostic calls D3D11
directly. The dedicated native Jadix benchmark additionally requires building
`jadix-frame-compare --features dx11,dx11-angle`. See
`vendor/wgpu-hal/JADIX_PATCH.md` for the optional patch and library requirements.

All binaries use release opt-level 3, one codegen unit, fat LTO, no debug info,
stripped symbols and panic abort. Shared `workload.rs` fixes the strings, colors,
Segoe UI 16px, 25px line height, 56px header and 48px rows. Ordinary has 200 rows /
600 labels, dense has 600 / 1800, nested has 200 / 600 plus eight wrappers per row.
No virtual list, canvas substitute, animated idle, or forced redraw loop is used.
The GPUI zero-size canvas only buffers telemetry and observes native wheel events.
The Jadix adapter uses the existing native lifecycle hook.

Client size is 1280 x 800 physical pixels. Both adapters use the same display/DPI;
logical sizes therefore depend on the recorded DPI. Wheel messages arrive at
60 Hz, reversing each second. GPUI's line height and Jadix's native wheel line
height are both 25 logical pixels. Each run warms up five seconds, measures ten
seconds scrolling, waits three seconds, then measures ten seconds settled.
Three rounds rotate Jadix/Kit/Fast, Kit/Fast/Jadix, Fast/Jadix/Kit. Build and
sampling must not overlap. Foreground visibility, delivered input, displacement
and >=55 observed frames/s are mandatory. Initial/scrolled/settled client PNGs
are captured outside CPU intervals for visual inspection.

Before acceptance, require identical client size/DPI, all valid samples,
unchanged binary hashes within each three-round set, and scroll-distance median
ratio <=1.10. Report every original CPU gate result; `--no-gate` suppresses only
the command's exit code. It never changes measurements or converts FAIL to PASS.
The 10% ordinary / 25% complex targets remain uncalibrated product targets.
Dense and nested do not represent the production Forms/Layout pages.

GPUI telemetry counts **paint callbacks**, not successful presentations. Jadix
counts presentations. Process CPU percentages and observed-frame CPU are useful
calibration evidence; this suite cannot certify a cross-engine presentation,
GPU, latency, IME, screen-reader or overall 5/5 winner. Paint counters are rejected
by the production driver unless `--calibration` is explicitly selected.
Ten-second settled results are diagnostics, not the >=60-second idle acceptance.
The numeric 25% field on dense/nested results is a calibration target, not a pass
for the production Forms/Layout gate.

For a runtime optimization, preserve an exact pre-change binary, use three
alternating A/B pairs on the same fixture and production Gallery, and reject a
>10% per-scene median CPU/frame regression. Declare a stable improvement only
when each pair improves and the median improvement exceeds 10%; smaller changes
remain inconclusive here. Correctness must pass the existing 80/96-frame tests,
full library suite and native pixel oracle. Keep diagnostics separate from CPU
acceptance. Do not loosen these criteria after seeing results.

When restoring a source backup on Windows, refresh its last-write time before
running Cargo. `Copy-Item` can preserve an old timestamp and leave a newer
candidate test binary apparently fresh. Confirm the final test inventory no
longer includes rejected candidate-only tests.
