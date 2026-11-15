# Pinned Vello compute reference

This independent workspace pins `vello=0.11.0` and upstream `wgpu=30.0.0`.
It is distinct from `../vello-reference`, which uses vello_gpu 0.3 strip rendering.
Both lockfiles remain independent from the framework's forked wgpu types.

```powershell
cargo build --locked --manifest-path benchmarks/vello-compute/Cargo.toml
benchmarks/vello-compute/target/debug/jadix-vello-compute.exe vulkan OUTPUT 40 include-blur area
python crates/jadix_2d/evidence/round15/run-comparison.py
python crates/jadix_2d/evidence/round15/run-large-scene.py
```

The reference rebuilds a scene each frame, uses five warmups, and waits for GPU
completion before stopping each frame timer. This is serialized frame latency,
not timestamp-query GPU time, overlapping throughput or application FPS.
P50/P95, preparation and encode/submit/wait are retained separately.
Dev opt-level=1 matches Jadix's optimization level; features, limits, AA,
architecture, compiler and debug settings are not identical.

Vello uses Area AA; Jadix runs Balanced and Reference sampled coverage. Same
compound arc geometry is flattened independently. Sweep-grid comparison uses
513 explicit sRGB stops representing linear red/blue interpolation: the pinned
Vello encoder does not consume Peniko interpolation_cs. This extra stop payload
is a semantic adaptation with a real preparation cost, not an equal-API benchmark.
See comparison-api-probe.json and the complete API report for that distinction.
Vello fine.wgsl writes straight sRGB output; Jadix's selected output is sRGB
encoding of linear-premultiplied RGB. The Python comparison normalizes both.

The bounded 30k micro-rectangle probe preserves failures. On this Windows Intel
host Vello DX12/FXC reports device lost, and Jadix exceeds its CPU comparison
threshold on all three backends. Neither failure is counted as a speed win.
The separate 100k hardware-instancing resource test is a different workload.

Source/license provenance: ../vello-api/*-source.json, original source trees,
and the exact Cargo.lock. No upstream source is patched to produce the reference.

Round 16 adds the explicit `glyph-controls` selector (argument 4), for unhinted,
hinted and expanded Inter H glyph scenes. Glyph ID161 is bound to the checked-in
Inter font, whose source SHA is retained in round16. Hinted Jadix preparation
uses device ppem, matching Vello's uniform-transform scale adjustment. Other
scenes keep their existing selectors; default acceptance does not silently grow.
Both comparison builders retain a font resource; Vello internally caches glyph
encodings while Jadix explicitly rebuilds prepared runs per measured frame.
The reported latency therefore includes different retained-work policies.


Round23 `matched` selector (argument 4) reads the same retained RGBA files as
Jadix and runs solid, bilinear, high, 24-frame NPOT image lifecycle, 3200-rectangle
compound and oblique phases, twice with a new renderer on one device. It records
constructor, per-frame CPU preparation, render/submit/wait and final readback
separately. No five-frame warmup is used for this selector: first use is retained.
Area AA and one initialization thread are explicit. The Python runner alternates
three fresh processes per renderer/backend and observes process CPU/peak RSS.
High is Vello's high filter versus Jadix EWA, not equal filter algorithms. Device
limits follow engine requirements. No driver cache clearing or VRAM measurement
is claimed. See `crates/jadix_2d/evidence/round23/matched-comparison.py` and
`docs/2d-round23.md`; older selectors and historical results retain their scope.
