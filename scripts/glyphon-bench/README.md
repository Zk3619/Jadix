# Glyphon A/B benchmark

This standalone harness compares the cached upstream glyphon 0.12.0 source with
the repository's `vendor/glyphon` source. It does not change the workspace's
dependency resolution or production renderer. `run.ps1` copies both implementations
under ignored `target/glyphon-ab`, renames their package identities, and builds
four optimized binaries: baseline/patched, each with and without instrumentation.
Both use the same dependency versions from the workspace lockfile.

Run from the repository root on Windows, with glyphon 0.12.0 already cached:

```powershell
./scripts/glyphon-bench/run.ps1 -Backend gl -Rounds 3 -Samples 60
./scripts/glyphon-bench/run.ps1 -Backend dx12 -Adapter Intel -Label dx12-intel -Rounds 3 -Samples 60 -SkipBuild
./scripts/glyphon-bench/run.ps1 -Backend dx12 -Adapter NVIDIA -Label dx12-nvidia -Rounds 3 -Samples 60 -SkipBuild
./scripts/glyphon-bench/run.ps1 -Backend gl -Label cache-gl -Rounds 3 -Samples 300 -Scenarios 'cjk_cached,color_cached,color_single' -SkipBuild
```

The actual adapter is recorded in each CSV's `.adapter.txt` companion. `gl` is
the framework's default Windows backend; `dx12` is an optional comparison.
Each process initializes its own adapter, device and font system, and renders
to a 1024 x 1024 RGBA target. The font is the repository's NotoSansCJK.ttc.

## Measurements

- `prepare_us`: wall time inside the real `prepare_with_custom` API, including
  glyph cache lookup, rasterization, packing, growth and upload enqueueing.
- `complete_us`: wall time from prepare through render, submit and GPU completion.
  This includes CPU encoding and driver synchronization; it is not a GPU timestamp
  and is not a complete Gallery frame time.
- Upload calls and logical upload bytes are measured in separate instrumented
  runs at each actual atlas `Queue::write_texture` call, including growth uploads.
  Logical bytes exclude row padding and backend staging allocations.
- CPU mirror memory is the actual pixel vector length; GPU atlas bytes are
  dimensions multiplied by bytes per pixel. Neither is process RSS or peak
  transient memory. The patch retains mirrors after `trim`.
- Custom rasterizer invocations include repeated callbacks caused by growth.
  The fixture returns deterministic pixels dependent on glyph id; it performs
  no SVG parsing or I/O.
- Each scenario's final render is read back. Nonblack RGB is required, and
  FNV-1a hashes must match between variants in every timing round and the metrics
  run. This is a fixture-specific correctness check, not a full visual regression
  suite.

Shaping, font discovery, buffer construction, atlas/renderer construction, and
final pixel readback are outside the timed interval. Each scenario discards ten
warmup frames. A cold sample creates a fresh atlas and SwashCache, while retaining
the warmed font system and pipeline cache. This measures cold glyph preparation,
not application startup. The renderer uses normal wgpu validation in both builds.
There is no VSync; every sample drains the queue before timing and waits after
submission. Runs alternate baseline/patched order to reduce clock/order bias.

## Scenarios

| Scenario | Input |
| --- | --- |
| `latin_small_cold` | Dialog/Save/Cancel text, fresh glyph cache |
| `latin_cold` | 94 printable ASCII characters, fresh glyph cache |
| `cjk_cold` | 1024 distinct CJK characters at 20 px, fresh glyph cache |
| `cjk_cached` | Same 1024 CJK characters, warmed glyph cache |
| `cjk_scroll` | 32 pages of 128 distinct CJK characters, one page per frame |
| `mask_cold` | 1024 unique 24 x 24 mask glyphs, fresh glyph cache |
| `color_cold` | 1024 unique 24 x 24 RGBA glyphs, fresh glyph cache |
| `color_cached` | Same 1024 RGBA glyphs, warmed glyph cache |
| `color_scroll` | 64 entirely new RGBA glyphs per frame |
| `color_single` | Change one displayed glyph among 1024; restored original ids remain cached, one miss per frame |
| `color_sparse` | Replace 64 glyph ids spread among 1024 resident RGBA glyphs |
| `color_growth` | Grow a resident set by 64 RGBA glyphs per frame; reset every 16 frames |

Results, raw CSVs, metadata and summaries are saved to `target/glyphon-ab`.
`summarize.mjs` reports pooled median/p95 timings, each run's prepare median,
mean upload traffic and maximum retained mirror size. Instrumented measurements
use eight recorded samples and are not included in timing statistics. For the
growth scenario the metrics sample window includes a reset and the largest atlas.
Use a new `-Label` for runs with different settings; it determines filenames and
summary grouping. `-Adapter` selects a case-insensitive substring of the adapter
name. `-Scenarios` takes a quoted comma-separated list and leaves other scenarios
out of the process. `-SkipBuild` reuses existing binaries; omit it after edits to
the harness or either glyphon source. Historical measurements and analysis are
saved in [RESULTS.md](RESULTS.md) and its adjacent results directory.

These scenarios isolate the modified glyph atlas code. They do not time Jadix's
Bake/MSDF, text-shadow bitmap cache, layout, composition, event handling or
full application scrolling. Other GPUs, drivers, backend choices and glyph
distributions can change the result.
