# Gallery CPU benchmark (Windows)

For the separate Jadix / GPUI Kit / GPUI Fast fixed-content calibration, see
[gpui-compare/README.md](gpui-compare/README.md) and
[the 2026-10-18 results](../development-docs/engine/gpui_same_content_20261018.md).
`--calibration` explicitly permits paint-callback telemetry; it never converts
that telemetry into successful presentation counts. `--client-size` fixes the
physical client dimensions, `--screenshots` captures outside CPU intervals, and
`--scroll-only` supports paired scrolling regressions without idle acceptance.
The default production Gallery mode and its numeric thresholds remain unchanged.

```powershell
cargo build -p gallery --release --features cpu-bench -j 1
python scripts/cpu-bench.py --output .tmp/cpu-bench.json
```

Close other CPU-intensive jobs before measuring. The script opens each Gallery
page in the foreground at 1280 × 800 outer pixels, warms up for five seconds,
then measures three ten-second rounds. Keep the test window in front throughout
the run. It closes its own windows and disables Gallery window-state persistence.

The gate uses the sum of **all process threads' user and kernel CPU time**, divided
by wall time. **100% means one logical CPU**, independent of the machine's CPU
count. This is not Task Manager's machine-wide normalized percentage. GPU and DWM
time are not included.

| Scene | Gate |
|---|---:|
| About page, stationary | < 0.5% |
| Ordinary list, 200 rows / 600 labels (no virtualization) | < 10% |
| Forms page, complex controls | < 25% |
| Layout page, complex scroll | < 25% |
| Each scroll scene, three seconds after scrolling stops | < 0.5% |

Scrolling sends native wheel messages at 60 Hz, reversing direction every second
to avoid measuring a page stuck at its boundary. Each round must have actual
scroll displacement > 20 px and at least 55 presented frames/s. The gate also
checks delivered wheel-event counts and rejects hidden, minimized or background
samples. Failure returns exit code 1; `--no-gate` only suppresses that exit code.
It does not change the measurements or their pass/fail fields.

`ordinary` is a fixed benchmark fixture in `examples/gallery/src/cpu_bench.rs`.
`forms` and `layout` use the complete production Gallery pages (including their
sidebar and controls). They are separate workloads: passing the ordinary-list
gate does not imply that the full Forms showcase consumes less than 10%.

The JSON includes per-round CPU time, wall time, scroll range, frame rate,
frame-interval p95, validity, executable SHA-256 and machine information. Companion
`*.frames.json` files retain timestamped scroll and presentation-counter samples.
Old baseline files without presentation counters report redraw rate instead.
Presentation counters are sampled immediately before each paint, so frame counts
at measurement boundaries have at most one frame of error. Windows CPU accounting
is quantized; a reported zero means no CPU tick was charged in that interval,
not a mathematical guarantee of zero work forever.

`cpu_ms_per_present` divides total process CPU time by actual presented frames
(null when none were observed). Phase summaries also include its median and
`cpu_weighted_percent` (total CPU / total wall time); these descriptive fields
do not change the original per-round gates or exit code. See the
[2026-10-18 gate review](../development-docs/engine/cpu_gate_review_20261018.md)
for budget calibration limits, long idle samples and the separate change
regression criteria. The absolute 10%/25% targets still need hardware/workload
calibration; passing a relative regression check does not satisfy them.

For shorter diagnostics or stage timings:

```powershell
python scripts/cpu-bench.py --sections forms,layout --seconds 6 --rounds 1 --profile --output .tmp/cpu-profile.json
```

Do not use profiling runs as final acceptance measurements. Other options include
`--exe`, `--warmup`, `--width` and `--height`; run `--help` for defaults.

`--thread-cpu` adds native `GetThreadTimes` measurements for the thread owning
the Gallery HWND. Each round reports UI CPU and the remaining process CPU
(process total minus UI); it leaves the process CPU gate unchanged. These are
diagnostics, with no stack or wakeup attribution. Windows accounting is
quantized and the two counters are read sequentially, so very short intervals
can even produce a negative remainder; values are preserved without clamping.
Use longer, repeated intervals for idle attribution, and keep this diagnostic
separate from uninstrumented acceptance runs.

`--cpu-timeline` includes that UI split and records process/UI CPU deltas every
half second in each result's `cpu_intervals`. Use consecutive rounds in the same
process to compare first scrolling with later scrolling. `--warmup` is idle
waiting; it does not warm scrolling. Preserve the first round when reporting
cold behavior. These diagnostic intervals retain Windows accounting granularity
and do not attribute CPU to a driver function or measure GPU execution.

```powershell
python scripts/cpu-bench.py --sections about,ordinary --seconds 15 --rounds 2 --thread-cpu --output .tmp/cpu-threads.json
```

## Complete application frame budget

`--frame-budget-ms 16.667` enables the UI-thread event-loop budget instrumentation
and saves `*.SECTION.budget.log` alongside the ordinary process-CPU results.
This includes input callbacks, shared UI tasks, animation/reconcile, render,
surface waits, and post-render IME/accessibility work, not only `AppState::render`.
IME and semantic timings are subsets of that work; do not add them again.
Shared event-loop work is charged once to the next presented window, so per-window
values describe accounting rather than causal attribution. GPU execution, DWM,
scanout, OS queue latency, and background workers are not UI-thread CPU timings.

```powershell
cargo build -p gallery --features cpu-bench,a11y
python scripts/cpu-bench.py --exe target/debug/gallery.exe --sections ordinary --seconds 6 --rounds 1 --frame-budget-ms 16.667 --output .tmp/app-budget.json
./scripts/application-frame-budget.ps1 -LogPath .tmp/app-budget.ordinary.budget.log -Scenario ordinary-wheel-60hz -ExecutablePath target/debug/gallery.exe -RequireInput
```

The additional gate rejects undersampled windows and requires p95 <= budget,
p99 <= twice budget, and <= 1% budget misses by default. It excludes the first
30 presented frames per window and requires 120 measured samples. Save cold
startup separately. `-RequireIme` and `-RequireSemantics` fail unless the sampled
history actually contains those events/active semantic factory calls. The wheel
runner does not exercise an OS IME or activate a screen reader and cannot satisfy
those gates by itself. Supply a separately captured real interaction history.

Trace output is diagnostic I/O; do not mix its CPU percentages with uninstrumented
release acceptance. Frame timing collection requires the opt-in `jadix/performance`
feature (Gallery `cpu-bench` enables it). For lower-overhead product measurement, enable
`jadix::app::frame_stats::enable_application_frame_timings(Some(16.667))` on the UI
thread and periodically drain `take_application_frame_timings()`. Each window
keeps at most 2048 recent samples and reports dropped samples plus all-frame
budget counters. Draining preserves pending input until a frame presents.
See [the five-point acceptance rubric](../development-docs/engine/frame5_acceptance.md).

Production rendering coalesces paint at a default maximum of 60 FPS and sleeps
between deadlines. All input events continue to be processed. Apps may call
`jadix::set_frame_rate_limit(std::num::NonZeroU32::new(120))` on the UI thread for
120 FPS, or `jadix::set_frame_rate_limit(None)` for display-paced rendering.
The CPU gate above applies to the default, not an arbitrary higher frame rate.
Finite animations and potentially visible motion continue running; bounded
infinite animations far outside the viewport no longer wake the whole window.

## Measurement checkpoint — 2026-10-16

These are **not accepted final results**: one six-second round per phase with a
four-second warmup, on an Intel Core i9-13980HX / Windows 11 machine. Background
Rust compilation was suspended throughout the capture. All samples passed the
foreground, input-delivery, scroll-displacement and presentation-rate checks.

| Workload | One-core CPU | Presented FPS | CPU gate |
|---|---:|---:|---|
| Ordinary list scrolling | 13.80% | 58.50 | **FAIL** (< 10%) |
| Full Forms page scrolling | 20.31% | 58.50 | PASS (< 25%) |
| Full Layout page scrolling | 26.56% | 58.33 | **FAIL** (< 25%) |
| Ordinary list after settling | 0.26% | 0 | PASS (< 0.5%) |
| Forms / Layout after settling | 0.00% | 0 | PASS (< 0.5%) |

See [the recorded checkpoint](cpu-bench-checkpoint.json) for executable identity,
CPU time, duration, displacement, frame intervals and validity checks.
The ordinary and complex scrolling targets remain open.

CSS-branch summaries reuse the framework's existing subtree paint-hash traversal
and invalidation paths. They avoid repeated style work in static branches; they
are a general runtime optimization, not a Gallery section special case. This is
separate from the 60 FPS cap, which reduces frame frequency and is an explicit
scheduling tradeoff. Old uncapped baseline CPU percentages must not be presented
as a same-frame-rate comparison of per-frame efficiency.

## Acceptance checkpoint — 2026-10-18

After `cargo clean`, Gallery dev/release builds and `cargo check --workspace
--all-targets` passed. The final full `a11y` library run passed 3767 tests (53
ignored), plus 48 macro/public-surface tests and two explicitly run native tests.
Full logs and build parameters are in
[the acceptance archive](../development-docs/engine/perf-20261018-acceptance/).

The stock release profile (`cpu-bench,a11y`, fat LTO), without trace or profiling,
completed three ten-second rounds per phase. All 21 phase samples were valid.
Scrolling CPU medians were **17.65% / 27.34% / 36.56%** for ordinary/forms/layout;
all three failed the unchanged **<10% / <25% / <25%** gates. About idle and Layout
settled each also had one round above 0.5%, despite zero presentations throughout
all idle/settled samples. The command returned 1; raw records are in
`release-cpu.json` and its companion telemetry files in that archive.

Separate dev and release traces passed the 16.667 ms application-frame gate for
all three pages. Release p95 was **3.84 / 5.86 / 6.98 ms**; Forms had one
over-budget frame (0.27%), within the unchanged 1% limit. These frame-work and
whole-process CPU results measure different costs;
neither substitutes for IME, active screen-reader, GPU or long-duration memory
acceptance. See [the updated rubric](../development-docs/engine/frame5_acceptance.md).
