# Glyphon 0.12.0 patch measurement

Date: 2026-10-13. Repository revision: `01d49174f2830d453b226f8732beb199220481f2`.
Production glyphon and Jadix sources were not changed. The only retained additions
are the standalone benchmark, documentation and measurement artifacts.

## Decision

Retain the batched-upload/pixel-preserving-growth patch. All three tested
adapter/backend combinations benefited substantially when new glyphs entered
the atlas. The independent cache-only measurements show no stable cache-hit
speedup. This is an upload/cache-growth optimization, not a general text-rendering
acceleration and not evidence for moving rotation or shadows into glyphon.

Further work should prioritize bounded mirror memory and upload amplification.
Renaming the maintained dependency to `jadix_glyphon` can clarify ownership;
renaming alone has no performance benefit.

## Environment And Method

- Windows; Intel Core i9-13980HX; rustc 1.95.0.
- Intel UHD/Raptor Lake Mobile Graphics, driver 32.0.101.6790: GL and DX12.
- NVIDIA RTX 4060 Laptop GPU, driver 32.0.15.7283: DX12.
- Optimized standalone build: opt-level 2, no LTO, normal wgpu validation.
  This is not the workspace's size-optimized release profile or default dev build.
- Upstream 0.12.0 from the local registry cache versus current vendor source;
  same dependency versions, adapter, font and deterministic inputs per comparison.
- Repository NotoSansCJK.ttc; 20 px text; 1024 x 1024 RGBA output target.
  Custom glyphs are deterministic 24 x 24 mask/RGBA fixtures, not emoji decoding.
- Three alternating-order process rounds; 60 recorded samples per scenario
  per variant, after ten discarded warmup samples. Statistics pool 180 samples.
- Cache-only follow-up: three rounds, 300 samples each, pooling 900 per scenario
  per variant. These processes skip the repeated cold-load scenarios.
- Upload calls/bytes and retained pixel-vector lengths are measured separately
  in instrumented binaries, with eight recorded samples, excluded from timing stats.
- Each scenario's final output is read back in every round and the metrics run.
  All baseline/patched hash comparisons matched and all outputs were nonblank.
- Both existing patch regression tests passed in an optimized standalone copy:
  batched pixel writes/dirty bounds and color-pixel preservation across growth.
  Rust formatting, JavaScript syntax and PowerShell parsing checks also passed.
- `prepare` excludes shaping, font discovery, renderer/atlas construction and
  output readback. Cold means a fresh atlas and SwashCache, not app startup.
- `complete` includes prepare, render encoding, submit and GPU completion.
  It is synchronized wall time, not a GPU timestamp or full application frame.

## New-Glyph Workloads

Median `prepare` time in milliseconds, shown as upstream -> patched.
The table includes growth/re-rasterization where the workload triggers it.

| Workload | Intel GL | Intel DX12 | NVIDIA DX12 |
| --- | ---: | ---: | ---: |
| Small Latin label, fresh cache | 1.164 -> 0.290 | 0.246 -> 0.150 | 0.194 -> 0.135 |
| 94 printable ASCII characters, fresh cache | 5.441 -> 0.517 | 1.612 -> 0.580 | 1.818 -> 0.503 |
| 1024 CJK characters, fresh cache | 157.983 -> 9.452 | 103.633 -> 8.703 | 34.900 -> 9.122 |
| CJK paging, 128 characters/page | 10.050 -> 5.950 | 11.256 -> 1.382 | 11.485 -> 1.296 |
| 1024 mask glyphs, fresh cache | 102.064 -> 5.767 | 88.699 -> 1.835 | 81.588 -> 0.881 |
| 1024 RGBA glyphs, fresh cache | 117.390 -> 7.992 | 94.187 -> 3.272 | 82.718 -> 1.845 |
| 64 new RGBA glyphs/frame | 27.852 -> 0.890 | 2.869 -> 0.247 | 2.864 -> 0.090 |
| Replace 64 dispersed RGBA ids among 1024 residents | 24.247 -> 1.266 | 3.278 -> 0.420 | 3.286 -> 0.190 |
| Add 64 resident RGBA glyphs/frame, periodic reset | 23.687 -> 1.139 | 3.103 -> 0.606 | 3.047 -> 0.148 |

Median synchronized `complete` time in milliseconds for selected workloads:

| Workload | Intel GL | Intel DX12 | NVIDIA DX12 |
| --- | ---: | ---: | ---: |
| 1024 CJK characters, fresh cache | 219.726 -> 11.273 | 131.163 -> 10.281 | 44.179 -> 9.805 |
| CJK paging | 13.708 -> 8.361 | 14.130 -> 2.120 | 14.813 -> 1.768 |
| 64 new RGBA glyphs/frame | 38.194 -> 2.619 | 5.381 -> 0.909 | 5.251 -> 0.270 |

These cold bursts deliberately stress the changed code; they do not represent
typical per-frame counts in Gallery. The benefit is nevertheless reproduced
with real CJK glyph rasterization and with controlled custom glyphs.

GL has substantial driver/host variability. For CJK cold preparation, the
upstream per-round medians were 125.0 / 913.0 / 127.3 ms; patched medians were
9.95 / 8.94 / 9.74 ms. The pooled P95 was 1150.3 -> 18.53 ms. Even the fastest
upstream round was much slower here, but the pooled 16.7x speedup is not a
portable promise. Every summary includes per-round medians and P95 for scrutiny.
We did not isolate the cause of the outlier driver/host behavior.

## Cache-Only Follow-Up

Median `prepare` in microseconds, upstream -> patched. Use these results,
not the cache rows following repeated cold bursts in the initial suite,
when judging warm steady-state behavior.

| Workload | Intel GL | Intel DX12 | NVIDIA DX12 |
| --- | ---: | ---: | ---: |
| 1024 cached CJK glyphs | 218.4 -> 243.7 | 217.3 -> 193.2 | 105.8 -> 97.3 |
| 1024 cached RGBA glyphs | 238.6 -> 235.4 | 168.9 -> 153.6 | 70.6 -> 72.5 |
| One new RGBA id among 1024 residents | 807.5 -> 294.2 | 158.9 -> 173.1 | 82.7 -> 77.5 |

There is no consistent cache-hit win: cached RGBA GL/NVIDIA timings are
essentially equal, and CJK results change direction across backends. Individual
rounds overlap and contain noise. Intel DX12 complete times for cached CJK
were 1021.6 -> 1021.7 us; cached RGBA were 1007.9 -> 1013.4 us. The initial
suite's much larger cached speedups are sensitive to preceding workloads and
must not be attributed directly to the patch. The isolated GL one-new-glyph
result is also variable (upstream run medians 199.8 / 832.2 / 943.9 us), so it
does not establish a portable benefit for single uploads.

## Uploads And Memory

These instrumented means are identical across the three adapter/backend
combinations because they describe logical calls and texel areas at the API
boundary. They are not measured physical PCIe/bus traffic or staging allocations.

| Workload | Upload calls/frame | Logical KiB/frame | Additional retained CPU mirror |
| --- | ---: | ---: | ---: |
| Small Latin cold | 15 -> 1 | 1.7 -> 4.1 | 64 KiB |
| CJK cold | 1658 -> 1 | 608.9 -> 523.0 | 1 MiB |
| CJK paging | 128 -> 1 | 46.0 -> 179.0 | 256 KiB |
| 1024 RGBA cold | 1544 -> 1 | 3474.0 -> 2476.5 | 4 MiB |
| 64 new RGBA/frame | 64 -> 1 | 144.0 -> 206.3 | 256 KiB |
| 64 dispersed RGBA replacements | 64 -> 1 | 144.0 -> 389.8 | 4 MiB |
| Growing RGBA resident set | 76.5 -> 1 | 172.1 -> 248.7 | up to 4 MiB |
| Cached RGBA | 0 -> 0 | 0 -> 0 | 4 MiB |

The cold RGBA fixture made 1544 custom-rasterizer calls upstream versus 1024
patched: 520 growth-triggered re-rasterizations were removed. The mask fixture
had the same counts. This complements fewer driver upload calls.

The patch does not shrink GPU atlas dimensions in these fixtures. Its extra CPU
mirror equals `mask_size^2 + 4 * color_size^2` bytes for initialized atlases.
`trim` clears the in-use set but does not release this mirror. Different renderers
and overlay atlases multiply the cost. A mask and color atlas both at 1024 square
add 5 MiB; both at 4096 square add 80 MiB if device limits allow those dimensions.
Growth temporarily holds old and new CPU vectors together, so peak memory is
higher. The measured columns are retained vector lengths, not process RSS.

Unioning dirty rectangles trades call overhead for extra pixels. In the paging
fixture, logical upload size rose 3.89x; dispersed RGBA replacements rose 2.71x.
The tested drivers still benefited, but low-bandwidth devices or widely separated
small updates can change this tradeoff. Upload amplification remains unresolved.

## Next Work

1. Keep the current patch, its pixel-preserving growth behavior, and its existing
   regression tests. Follow real Gallery traces to quantify full-frame gains;
   these isolated measurements do not establish application FPS improvements.
2. Bound/observe per-renderer mirror memory and dirty-upload amplification before
   expanding the maintained fork. Explore a small set of dirty regions or an
   adaptive batching threshold against these same fixtures; avoid an always-one-
   rectangle policy when it unnecessarily transfers most of the atlas.
3. Consider GPU-to-GPU atlas copying on growth as an alternative to repeated
   rasterization and permanent CPU mirrors. Measure added encoder/submission
   cost and backend compatibility before adopting it.
4. Benchmark rotation's current Bake/MSDF path and shadow bitmap generation
   separately before moving either into the glyphon fork. These measurements
   only exercise Coverage text and custom glyph atlas operations.

## Artifacts And Reproduction

See [README.md](README.md) for the complete method and scenario definitions.
Saved summaries and metadata are under [results/2026-10-13](results/2026-10-13).
Raw per-sample CSVs and adapter logs are in `raw-measurements.zip` there.
The original uncompressed measurements remain under `target/glyphon-ab`.
The first GL run preceded automatic metadata/adapter-log capture; its adapter
information was recorded in the execution console and is noted in `gl-metadata.json`.

```powershell
./scripts/glyphon-bench/run.ps1 -Backend gl -Rounds 3 -Samples 60
./scripts/glyphon-bench/run.ps1 -Backend dx12 -Adapter Intel -Label dx12-intel -Rounds 3 -Samples 60 -SkipBuild
./scripts/glyphon-bench/run.ps1 -Backend dx12 -Adapter NVIDIA -Label dx12-nvidia -Rounds 3 -Samples 60 -SkipBuild
./scripts/glyphon-bench/run.ps1 -Backend gl -Label cache-gl -Rounds 3 -Samples 300 -Scenarios 'cjk_cached,color_cached,color_single' -SkipBuild
./scripts/glyphon-bench/run.ps1 -Backend dx12 -Adapter Intel -Label cache-intel -Rounds 3 -Samples 300 -Scenarios 'cjk_cached,color_cached,color_single' -SkipBuild
./scripts/glyphon-bench/run.ps1 -Backend dx12 -Adapter NVIDIA -Label cache-nvidia -Rounds 3 -Samples 300 -Scenarios 'cjk_cached,color_cached,color_single' -SkipBuild
```
