# Vello GPU reference

This isolated workspace pins upstream `vello_gpu =0.3.0` and `wgpu =30.0.0` in
its own Cargo.lock. It adds no Vello runtime dependency to Jadix. The reference
uses Vello's public Scene/Renderer interfaces without patching its implementation.

```powershell
cargo build --locked -p jadix_2d --example gpu_acceptance --features backend-dx12,backend-vulkan,backend-gles
cargo build --locked --manifest-path benchmarks/vello-reference/Cargo.toml
python benchmarks/vello-reference/compare.py --frames 120 --repeats 3
python benchmarks/vello-reference/compare.py --frames 120 --repeats 3 --premultiplied --output crates/jadix_2d/evidence/vello-premultiplied-comparison
```

Python requires Pillow. The runner rejects mismatched adapter/driver/platform
identity, alternates process order, preserves all timing reports, and compares
frame-zero images. Identical repeat PNGs reference run zero through per-run
SHA-256 manifests; differing images are retained. Default builds use opt-level=1
for both renderers. Each case has five warmups followed by measured frames.

The finite scene mapping covers compound circular holes, 64 conic gradient
rectangles, and 32 moving cubic strokes at 256/512px. Each renderer creates a
new scene per frame while reusing its renderer/resources. Vello receives explicit
linear-sRGB gradient interpolation, a positive full sweep rotated through the
paint transform, round stroke caps/joins, and the same opaque colors/control
points. Vello's SVG arc conversion and analytic AA differ from Jadix flattening
and MSAA/SSAA. This benchmark does not make the algorithms pixel-identical.

The output conventions also differ. Vello GPU composites in display sRGB and
exports premultiplied display pixels to `Rgba8Unorm`. Jadix normally exports
straight display pixels through `Rgba8UnormSrgb`; its optional direct output
exports linear-light premultiplied color through that sRGB target. Comparisons
therefore use RGB only where both alpha values are at least 254, and compare
alpha separately across the full image. Fully translucent color equality is
outside this benchmark. Finite regression envelopes are opaque RGB MAE <=2 and
alpha MAE <=8 (byte units); failures remain in the report and do not become
performance wins.

Report preparation + encoding/submission + serialized GPU completion, cold frame,
and pipeline initialization separately. This measures neither isolated GPU
timestamps nor window FPS, release performance, memory, browser execution, or
the compute-centric `vello` renderer. Small differences on one integrated GPU
are exploratory observations, not universal performance guarantees.

See [implementation/results](../../docs/2d-round3.md) and primary references:
[Vello architecture](https://github.com/linebender/vello/blob/main/ARCHITECTURE.md),
[Vello GPU 0.3.0](https://docs.rs/vello_gpu/0.3.0/vello_gpu/).

Round 4 adds `compare.py --quality balanced|high|reference` for the Jadix side;
quality and output mode are recorded in comparison.json. Keep Vello-difference
envelopes unchanged when investigating an accuracy discrepancy. The standalone
`stroke_oracle` example independently integrates exact cubic-distance coverage
at 16x16 or 32x32 subpixels (it does not call either renderer's geometry code).
See docs/2d-round4.md for the distinction between output similarity and geometric
accuracy, and the current narrow comparison with vello_gpu 0.3.0.


The optional `compare.py --include-blur` mode adds 16 Gaussian rounded rectangles
with radius 8 and dynamic local sigma 2..4, at 256/512. Existing three-scene runs
and thresholds remain unchanged. Jadix uses 4-sigma support; fixed vello_gpu 0.3.0
uses 2.5-sigma bounds and a different approximation. The comparison still checks
only common opaque RGB and all alpha; it does not prove blur-kernel equality.
No glyph, color-font or luminance-mask performance parity is claimed.

Round 10 corrects a methodology error in historical Jadix acceptance timings:
submission was asynchronous despite the old `encode_submit_wait` label. The
acceptance loop now explicitly waits for GPU completion just like this reference.
Historical speed ratios (including round 9) must not be used as fair serialized
comparisons; image/quality evidence remains useful. See [corrected measurements](../../docs/2d-round10.md).
