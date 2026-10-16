# Gallery CPU benchmark (Windows)

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

For shorter diagnostics or stage timings:

```powershell
python scripts/cpu-bench.py --sections forms,layout --seconds 6 --rounds 1 --profile --output .tmp/cpu-profile.json
```

Do not use profiling runs as final acceptance measurements. Other options include
`--exe`, `--warmup`, `--width` and `--height`; run `--help` for defaults.

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
