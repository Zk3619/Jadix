# jadix_2d

Independent 2D drawing extracted from Jadix. No Jadix application, Element tree,
window or layout is required. Uses `jadix_core` path data and `jadix_theme` color
parsing so existing path and theme types retain their identity.

- `path`: geometry, fill/stroke styles, hit tests, CPU RGBA rasterization, GPU meshes.
- `gradient`: CSS gradient parsing, sampling and CPU rasterization.
- `canvas`: `Canvas2D`, `RasterCanvas`, and `GpuCanvas` operation recording.
- `gpu` feature: `gpu::PathGpu` renders prepared operations to a caller-owned
  `Rgba8UnormSrgb` texture view, or `Rgba16Float` via `try_new_with_format`.
  Select native backends on the workspace `wgpu` dependency; the GPU API uses
  `jadix_wgpu`, the same fork as Jadix.

```rust
use jadix_2d::{Canvas2D, RasterCanvas};
let mut canvas = RasterCanvas::new(32, 32);
canvas.set_fill("#ff0000");
canvas.fill_rect(4.0, 4.0, 16.0, 16.0);
let rgba = canvas.get_image_data(0, 0, 32, 32).unwrap();
```

For GPU drawing, record with `GpuCanvas`, call `into_ops()`, then
`gpu::PathGpu::render(device, queue, target_view, size, &ops)`. The caller owns
device, queue, target allocation and presentation. Deferred readback request
storage is exposed by `path`; the host performs the actual GPU copy.

Jadix retains its existing public APIs via re-exports. It owns paint callback
scheduling, Element attributes, texture caching, uploads and render-graph nodes.

See [Vello feasibility](../../docs/vello-feasibility.md).

## Analytic pixel area and automatic compact layers

`gpu::AaQuality::Area` integrates triangle/pixel intersection on the GPU before
per-primitive composition. `RasterCanvas::set_area_antialiasing` supplies an
independent f64 clipping reference. Both canvases expose an explicit device-pixel
curve tolerance; defaults retain legacy sampled rendering. Rectangle helpers now
preserve positive subpixel dimensions and small corner radii.

`PathGpu::set_tight_layer_bounds(true)` infers conservative isolated source/filter
domains, translates geometry and brush mappings, and reinserts source origins.
It is opt-in; masks, content-generating filters and convolution retain full domains.
Area assumes non-overlapping tessellations; stencil-only expert fills reject.
See [round 17 contracts and evidence](../../docs/2d-round17.md).

## GPU texture interop, outline controls and context colors

`PathGpu::{register_texture,override_image,mark_override_image_dirty,unregister_texture}`
imports caller GPU textures (TEXTURE_BINDING or COPY_SRC) using explicit color/alpha
contracts; immutable CPU fallbacks remain separate. `render_to_image` and
`SceneRegion::render_gpu` feed GPU scene/filter results back into Canvas images.
Unchanged revisions reuse conversion caches; dirty updates currently invalidate
image/layer caches and can reallocate. Same-device, format and budget rules apply.

`GlyphRun::with_options` adds Skrifa hinting, Kurbo X/Y contour expansion, exact
normalized axes and shared/per-glyph transforms. `ColorContext2D::resolve` adds
caller-context variables/currentColor/light-dark, two-color mix, relative colors
and bounded scalar color math. These are explicit APIs; existing glyph and paint
constructors keep their behavior. Context evaluation is not a full stylesheet
cascade or complete typed CSS Values implementation.

See [round 16 contracts and evidence](../../docs/2d-round16.md) and the
[updated complete Vello API checklist](../../docs/vello-jadix-api-comparison.md).

## Float color, incremental resources and prepared rendering

Round 15 adds `Color2D` (Color 4 literals and extended linear RGB), explicit
`GamutMapping`, optional `icc` profile import/export, `Image2D` f32 input,
`RasterCanvas::new_hdr` and float output, and `PathGpu` RGBA16F targets and layers.
Use `Color2D::parse(css)?.brush()` through `set_fill_brush`; legacy string paint
setters retain their existing parser. Normal Porter–Duff composition retains HDR;
non-Normal blending and bounded imaging filters retain documented LDR limits.
This is float rendering and export, without OS HDR display/swapchain management.

`AtlasCache<K>` adds bounded pages and LRU entries with immutable snapshots;
`PreparedText2D::new_with_atlas_cache` shares these pages between prepared texts.
GPU uploads use dirty regions only when the predecessor texture is safe to reuse.
`SceneRegion` records and filters an explicit padded region, including negative
coordinates; `PreparedSceneGpu` retains tessellation, while `gpu::GpuMesh` retains
GPU geometry and draws `MeshInstance` batches with real hardware instancing.
These are explicit resource APIs, without automatic global scene/mesh eviction.

See [round 15 contracts and verified limits](../../docs/2d-round15.md),
[complete Vello 0.11 API comparison](../../docs/vello-jadix-api-comparison.md),
and [machine-readable validation](evidence/round15/summary.json).
Older numbered sections below describe the progression; current text layout,
COLR, four-corner/inset shadows and group filters extend the round 8 baseline.

## Font instances, polar gradients and image effects

`text-layout` now exports `FontVariations` (also available from the re-exported
Cosmic Text engine). Apply it to `Attrs` **before shaping**, so glyph selection,
advances, font metrics, Basic/Advanced shaping, decorations, outlines, Swash
bitmaps and native COLR paints use the same normalized instance:

```rust
# #[cfg(feature="text-layout")] {
use jadix_2d::FontVariations;
use jadix_2d::text_layout::cosmic_text::{Attrs, Family};
let axes = FontVariations::new(&[(*b"wdth", 125.), (*b"wght", 650.)]).unwrap();
let attrs = Attrs::new().family(Family::Name("My Variable Font"))
    .font_variations(axes);
# }
```

Up to 16 finite explicit axes, canonical tag ordering, duplicate rejection and
signed-zero normalization make cache keys independent of input order. Known
axes clamp to each font's ranges; fallback fonts ignore unknown tags. An explicit
wght overrides the weight setting for instance construction, while family/style/
weight still select the face. `Font2D::with_variations` retains its stricter
unknown-tag rejection. Rich spans with different features or axes split **before**
shaping; shape-run and glyph caches include axes. `TextGlyphInfo.font_variations`
retains the requested settings. Reprepare after changing the buffer attributes.

`GradientSpace::{Oklch,Hsl}` extends the existing three spaces.
`with_hue_direction(HueDirection::{Shorter,Longer,Increasing,Decreasing})`
selects the polar hue path; default is Shorter. Achromatic endpoints borrow the
other hue. Premultiplied interpolation weights the non-hue components only;
hue uses the interpolation parameter directly. Input stops remain straight
sRGB RGBA, and output is clamped linear-sRGB LDR. These are interpolation spaces,
not new HDR/image/output formats or a complete CSS Color parser.

`LayerFilter::ComponentTransfer { channels, color_space }` transforms straight
RGBA independently with `TransferFunction::{Identity,Linear,Gamma,Table,Discrete}`.
Tables contain 1..=16 samples, gamma exponents 0..=128. Choose
`FilterColorSpace::{LinearSrgb,Srgb}` explicitly. Filters unpremultiply, convert
RGB to the selected domain, transform/clamp, convert back and premultiply.
Changing alpha can create content in an initially empty group.

`LayerFilter::ConvolveMatrix(Convolution::new([3,3], &kernel)?)` supports 1..=7
kernels per axis (49 weights), adjustable target, divisor, bias, fractional
positive unit lengths, Transparent/Duplicate/Wrap edges, optional original-alpha
preservation and either color domain. Kernel weights rotate 180 degrees, as in
SVG convolution. Divisor zero uses the kernel sum (or 1 for a zero sum). With
preserve_alpha the color convolution uses straight components; otherwise it
uses premultiplied components and clamps RGB to output alpha. Fractional offsets
sample bilinearly in the selected representation; they do not rescale to an
intermediate SVG kernel grid. Invalid filters are removed atomically, matching
existing LayerFilter behavior. The entire output canvas is the filter region;
final group clip/mask and opacity apply after the filter chain.

No required Canvas method was added. Exhaustive `LayerFilter`/`GradientSpace`
matches need the new variants. Raw vector gradient payloads (kinds 10..12) use
`stop_t[4]` for hue direction 0..3; other payloads, including kind 8, are unchanged.
Exhaustive Cosmic Text `Attrs`/`AttrsOwned`/`LayoutGlyph`/`ShapeGlyph`/`CacheKey`
literals need `font_variations: FontVariations::empty()` for old behavior;
`CacheKey::new` still defaults to empty axes.

FontVariations uses a fixed 16-slot value to keep physical/glyph keys Copy; this
increases per-key memory. FontSystem/Swash cache retention remains caller-owned
and is not an automatic LRU. New convolution/transfer kernels are bounded, but
full-canvas scratch surfaces and parser/driver memory remain outside those
bounds. Reusing the same GPU renderer and prepared operations preserves existing
resource reuse, while changing image/atlas identities can still upload again.

Run `cargo run -p jadix_2d --features text-layout --example imaging_capabilities`.
See [round 14 validation and limits](../../docs/2d-round14.md).

## Scalable color fonts and gradient interpolation

`ColorGlyphRun` prepares COLRv0/v1 paint graphs through the same vector scenes,
brushes, clips and isolated groups as other 2D content. It supports CPAL palette
selection, foreground index 0xFFFF, font variation axes, nested glyph references,
transforms, linear/radial/sweep paints and all 28 COLR composition operators.
There is no glyph bitmap allocation for COLR. No shaping is performed by this
low-level API; use caller-positioned glyphs or the optional text-layout bridge.

```rust
use jadix_2d::{Font2D, PositionedGlyph, ColorGlyphRun, ColorGlyphOptions,
    RasterCanvas};
# fn demo(bytes: &[u8], glyph_id: u16) -> Result<(), Box<dyn std::error::Error>> {
let font = Font2D::new(bytes, 0)?;
let run = ColorGlyphRun::new(&font, 36.,
    &[PositionedGlyph { id: glyph_id, x: 8., y: 48. }],
    ColorGlyphOptions { palette: 0, foreground: [0.2, 0.6, 1., 1.],
        ..Default::default() })?;
let mut canvas = RasterCanvas::new_linear(160, 96);
run.draw(&mut canvas)?;
# Ok(()) }
```

`ColorGlyphRun::scene()` can be nested in a reusable scene and filtered like any
other content. `bounds()` returns a conservative baseline `[x,y,width,height]`
with Y down. Caller global alpha applies once to each completed color glyph.
Cycles, invalid palettes, unsupported paints, unbounded fills and exceeded scene
budgets return explicit errors. Empty glyphs remain valid. Font outlines are
unhinted; this is not an LCD/hinting implementation.

Optional `svg-fonts` enables static SVG/SVGZ fonts, including CPAL/currentColor,
root viewBox/em viewport, glyph-ID selection via SVG use semantics and embedded
raster images. They are rasterized once at `ColorGlyphOptions.raster_scale`,
then retained as immutable images. Text, animation and external image/file
references return `UnsupportedSvg`; no file/network/system font resolver is used.
Compressed and expanded document bytes, element/depth counts, image dimensions
and copied bitmap bytes are bounded, but do not describe total parser/scratch
memory. This is a supported static subset, not full SVG conformance. Jadix's
existing `svg` feature enables `jadix_2d/svg-fonts` too.

`VectorGradient::with_color_space(GradientSpace::{LinearSrgb,Srgb,Oklab})`
selects interpolation, independently of Straight/Premultiplied alpha and extend
mode. Premultiplication happens in the selected space, followed by conversion to
linear-sRGB for compositing. Output remains clamped LDR/sRGB, not HDR or wide
gamut. `sweep_range(center,start,end,stops)` uses a fixed +X branch cut and permits
ranges beyond one turn; ordinary `sweep` keeps its existing wrapped semantics.
`radial_signed` preserves signed circle anchors and only paints nonnegative
interpolated radii; existing `radial` still requires nonnegative anchors.

Migration: no new required Canvas method; `fill_color_glyph_run` has a default.
Explicit `TextPrepareOptions` literals need `palette` (or `..Default::default()`).
`TextGlyphInfo.color_vector` distinguishes scalable color ink; test it together
with `color_bitmap` when detecting all color glyphs. New `PreparedText2D::try_draw`
reports scene preflight errors; the existing unit-returning `draw` remains a best
effort wrapper. Vector color playback on custom canvases needs truthful
`current_transform`/`current_global_alpha`, as reusable scenes do. `Scene2D::validate`
checks replay requirements without changing the destination.

Manual gradient payloads: kind 8 remains legacy Gaussian and kind 9 remains the
independent-corner shadow. For vector kinds 10..12 only, `stop_t[2]` is the
interpolation-space tag (0/1/2) and `stop_t[3]` marks absolute sweep angles (0/1).
Old CSS gradient payloads are unchanged. Legacy conic centers use a 2e-7
normalized-UV tolerance for the +X convention, avoiding backend-dependent
quadrants from interpolation roundoff at the singularity.

Run `cargo run -p jadix_2d --example color_fonts -- target/color-fonts.png` for
Nabla variations/palettes and the three interpolation spaces. See
[round 13 implementation and measured limits](../../docs/2d-round13.md).

## Reusable scenes and nested instances

`Scene2D::record([width, height], |canvas| ...)` runs the callback once against a
high-level recorder, without creating a bitmap or tessellating. The immutable
snapshot is `Send + Sync`; clones and nested scenes share `Arc` resources.
Images, source rectangles, prepared glyph runs, prepared text/color atlas output,
gradients, strokes, clips, masks, rounded shadows and isolated filter groups can
be retained together. It works independently of the optional GPU/text features.

```rust
use jadix_2d::{Canvas2D, Scene2D, SceneOptions, LayerOptions, BlendMode,
    LayerFilter, RasterCanvas};
let icon = Scene2D::record([32, 32], |c| {
    c.set_fill("#20a0ff");
    c.fill_rect(2., 2., 28., 28.);
}).unwrap();
let mut canvas = RasterCanvas::new_linear(160, 64);
for x in [8., 60., 112.] {
    icon.draw(&mut canvas, SceneOptions {
        transform: [1., 0., 0., 1., x, 8.],
        layer: Some(LayerOptions {
            filters: vec![LayerFilter::Gaussian { sigma: [1., 1.] }],
            ..LayerOptions::new(0.8, BlendMode::Normal)
        }),
        ..Default::default()
    }).unwrap();
}
```

Scene style defaults are black fill/stroke, alpha 1, width 1, round cap/join,
nonzero fill and nearest image sampling. Caller styles are restored after drawing.
The instance matrix composes with the caller CTM. All scene `set_transform` and
`reset_transform` calls remain **relative to that instance base**, including in
nested scenes; they cannot escape into the caller's coordinate system.

With `layer: None`, caller alpha multiplies each draw. With `Some(LayerOptions)`,
caller alpha times layer opacity applies **once** to the finished instance; blend,
Porter–Duff/luminance composition and the ordered filter chain apply at that group.
Caller clip/mask constrains the input content. `SceneOptions.layer` clip/mask is
in target device pixels and limits the final output. Masks and final layer clips
**inside a recorded scene** are in its declared scene-device coordinates and move
with its instance base, independently of later local CTM changes. Filter radii
and offsets remain target device pixels. `clip_to_bounds` adds a transformed input
clip; filters can spread beyond it. The default leaves the declared viewport
informational, allowing content outside its rectangle.

`record_with_limits` bounds commands (default 65,536), paths/dash entries
(1,000,000), copied borrowed RGBA (64 MiB), expanded commands (1,000,000), combined
nesting (32) and remapped image/final-clip masks (64 MiB per replay). Frozen children
cannot refer back to a future parent. Preflight checks transform composition,
child budgets and conservative expanded mask bytes before painting. `stats()`
separates directly retained data from expanded work; remap bytes count final RGBA
payloads and exclude staging copies/GPU conversion. These are not total CPU/VRAM
measurements. Shared image/glyph data, ordinary path clips and renderer scratch
are excluded from these byte budgets. Singular instance bases, overflowing
matrices, unbalanced save/layer stacks and exceeded limits return `SceneError`.

Recording `clear`, pixel IO, hit queries or host `TextureKey` sources returns
`UnsupportedOperation`; paint a background rect or perform those operations on
the destination instead. Custom `Canvas2D` implementations remain source compatible:
to opt into scene replay, implement `current_transform` and `current_global_alpha`.
Their defaults return `None`, causing `UnsupportedCanvas` before any mutation.

GPU recording retains a scene and captured caller state. `into_ops` expands into
the existing pipelines; CPU readback/software continuation replay the same snapshot.
This does not provide hardware instancing, transform-independent mesh caching,
incremental editing or an O(1) draw operation. Legacy CSS gradients that cannot
fit the original eight-stop payload still select software rendering; new vector
gradients keep the existing shared stop-table route. Gradient payload kind 8 is
unchanged. Transformed scene masks currently use a CPU 2×2 coverage remap and may
allocate/upload again when rebuilding operations; warm reuse of **prepared ops**
is a separate guarantee. See [round 12 validation](../../docs/2d-round12.md).

## Shaped text, color glyphs and immutable atlas pages

Enable `text-layout` to reuse the workspace Cosmic Text + Swash stack, independently
of a Jadix window or GPU. The Jadix `gpu` feature enables this adapter and re-exports
the prepared types through its prelude. Default `jadix_2d` builds retain no shaping
dependency. `text_layout::cosmic_text` re-exports the exact compatible engine types.

```rust
use jadix_2d::{Canvas2D, RasterCanvas, PreparedText2D, TextPrepareOptions};
use jadix_2d::text_layout::cosmic_text::{Attrs, Buffer, FontSystem, Metrics,
    Shaping, SwashCache};
let mut fonts = FontSystem::new(); // Or load explicit font bytes into a custom database.
let mut buffer = Buffer::new(&mut fonts, Metrics::new(24., 32.));
buffer.set_size(Some(320.), Some(128.));
buffer.set_text("office العربية אבג", &Attrs::new(), Shaping::Advanced, None);
let text = PreparedText2D::new(&mut fonts, &mut buffer, &mut SwashCache::new(),
    TextPrepareOptions::default()).unwrap();
let mut canvas = RasterCanvas::new_linear(360, 160);
canvas.draw_text_layout(&text, [16., 12.]);
```

`PreparedText2D` snapshots the buffer's **visible viewport** after resolving pending
shaping/layout. It preserves font fallback, glyph/cluster order, bidi, wrapping,
alignment, scrolling, rich span colors, variable weight, synthetic italic and text
decorations supported by the existing engine. Fonts must actually be supplied;
no font download or platform fallback is silently installed by this crate.

`TextRenderMode::Outline` (default) retains monochrome outlines and native
COLRv0/v1 vector paint graphs. `Atlas` rasterizes all ink into shared immutable
pages. SVG/SVGZ and embedded bitmap emoji use atlas entries in either mode;
bitmap emoji continue through the existing Swash stack. Unsupported or malformed
color formats return an error instead of silently losing color. Low-level
`GlyphRun` remains monochrome; `ColorGlyphRun` is its separate color counterpart.

`raster_scale` (0.25..=8) selects pixels per logical unit for atlas/SVG/bitmap ink;
font size times raster scale must not exceed 1024. Reprepare when text, attributes,
viewport, palette or desired raster resolution changes. COLR stays vector in
Outline mode under transforms; atlas, SVG and bitmap ink have fixed resolution.
Low-level ColorGlyphRun supports declared Font2D axes; prepared layouts now
carry explicit FontVariations through the shaping engine and rendering caches. Clones share pages, vector scenes and paths without touching fonts.
Repreparing creates a new snapshot; this is not a mutable global glyph cache.

`draw_text_layout` uses prepared span colors; CTM, caller global alpha, clip/mask,
layer context and filters still apply, and drawing state is restored. In Outline
mode, `draw_text_layout_with_brush` replaces monochrome span colors and decorations
with the current fill brush (including gradient/image brushes). Color glyphs retain
their prepared colors/foreground. Atlas mode returns `BrushRequiresOutlines`
before painting. Neither method introduces a required custom Canvas trait method.
Their defaults delegate to existing glyph/image/scene methods; custom canvases
need image drawing for bitmaps and truthful state introspection for COLR scenes.

`glyphs()` exposes line-relative UTF-8 clusters, baseline, advance, bidi level,
font/glyph IDs and span metadata. `hit`, `selection_rects` and `caret_rect` use
the same frozen engine layout. Coordinates are in its logical viewport before
draw origin/CTM; callers invert those mappings for pointer interaction.
This provides drawing/interaction integration, not an IME/editor widget.

`ImageAtlas::pack` is also available without `text-layout`. It freezes input
bitmaps into shelf-packed pages with transparent one-texel gutters. `AtlasLimits`
bounds page size (4..=2048), pages (1..=64) and unique entries (1..=65536).
Default pages are 512², at most 16 pages / 16,384 entries (16 MiB RGBA page bytes).
Packing validates the page budget before allocating page pixels. The text adapter
also bounds visible glyph instances (default 65,536) and staging bitmap bytes.
The font rasterizer and frozen Buffer snapshot have additional memory costs;
atlas byte counts are not process peak memory or VRAM measurements.

`CanvasImageSource::Region { image, rect:[x,y,w,h] }` paints source texels from
the same immutable image, without extracting/reuploading each glyph. Source rects
must be nonempty and completely in bounds; invalid ones paint nothing. Filtering
may sample adjacent source texels, so atlas gutters isolate neighboring entries.
Linear CPU region painting integrates 4x4 subpixels; ordinary full-image behavior
is unchanged. GPU regions reuse the image pipeline and can batch across different
source rectangles on one page.

Migration: exhaustive `CanvasImageSource` matches must handle `Region`; raw
`PathGpuImage` literals need `source_rect: None` for old full-image drawing. There
is no change to Gaussian kind=8 or vector-gradient payload meanings. See
[round 11 validation and remaining capability gaps](../../docs/2d-round11.md).

## Group filters and independent inset shadows

`draw_rounded_rect_shadow(rect, radii, sigma, color, inset)` supports independent
circular TL/TR/BR/BL radii, proportionally normalized to fit adjacent sides.
Inset draws the Gaussian complement inside the rounded shape; sigma below 0.001
paints nothing for inset and an ordinary rounded fill for outer shadows. The
existing `draw_blurred_rounded_rect` and its GPU kind=8 payload retain their
semantics; independent corners/inset use kind=9. The new shadow method is required
on custom `Canvas2D` implementations. Glyph methods and `push_luminance_layer`
still have defaults; exhaustive `CompositeMode` matches include LuminanceIn/Out.

`LayerOptions::filters` is an ordered `Vec<LayerFilter>`: anisotropic Gaussian,
drop shadow, shadow-only, fractional offset, rectangular RGBA dilation/erosion,
and a 4x5 straight-RGBA color matrix. Effects process the entire isolated group's paths, text, images and nested
groups before group opacity, mixing and final clip/mask. Effects use device pixels
independently of the current CTM. Gaussian convolves premultiplied working-space
pixels with integrated pixel weights, transparent outside the canvas, truncated
at four sigma. It is a discrete image filter; the rounded shadow primitive instead
convolves continuous geometry in user space. Matrix coefficients are finite and
at most 1e6 in magnitude; invalid filters are ignored atomically.

```rust
use jadix_2d::{Canvas2D, RasterCanvas, LayerOptions, LayerFilter};
let mut canvas = RasterCanvas::new_linear(128, 96);
canvas.push_layer_with_options(LayerOptions {
    filters: vec![LayerFilter::DropShadow {
        sigma: [3., 5.], offset: [2.5, 4.], color: [0., 0., 0., 0.6],
    }],
    ..Default::default()
});
canvas.draw_rounded_rect_shadow([12., 12., 96., 64.], [0., 20., 8., 12.],
    3., [0.2, 0.5, 1., 1.], true);
canvas.pop_layer();
```

GPU filter stages share one frame encoder and three FP16 scratch textures among
sequential siblings, retaining independent parameter buffers. Nested groups have
their own scratch set. Final group/target storage remains LDR sRGB8: FP16 scratch
does not provide HDR/wide-gamut output. Groups are canvas-sized; content beyond
the canvas is clipped before filtering. Large sigma and many full-size groups
remain costly; these are explicit current limits. Struct literals that list all
`LayerOptions`/`PathGpuLayer` fields must now initialize `filters` (or use Default
for options). See [round 9 validation and measured limits](../../docs/2d-round9.md).

## Exact vector gradients

`GradientStops` validates 1..=4096 sorted offsets in [0,1], retaining duplicate
offsets for hard transitions. Input colors are finite straight display-sRGB,
clamped to LDR and prepared in linear-sRGB once. Clones share immutable stop data.
Optional per-interval midpoints bias interpolation with a power curve; each
midpoint must be strictly between zero and one. Updating hints uses copy on write.

`VectorGradient` supports explicit linear endpoints, general moving-circle
two-point radial geometry (including equal/decreasing radii), and signed sweep
angle ranges. The radial solver picks the greatest parameter with nonnegative
radius; no valid root paints transparent. Sweep angles run clockwise from +X,
wrap one signed turn from the start and permit a span up to 360 degrees.
Geometry must be finite, bounded to +/-1e6, and nondegenerate. Near-tangent
radial quadratics use a relative 1e-7 linear threshold; GPU math uses f32.

`with_extend` selects Pad/Repeat/Reflect/Decal. `with_alpha_mode` selects
premultiplied (default) or straight **linear-light** color interpolation.
`sample` returns premultiplied linear RGBA. Brushes inherit CTM/clip/mask/alpha,
work for fill/stroke/glyph outlines, isolated groups and software continuation.
CPU vector painting integrates colors and coverage at 4x4 subpixels.

```rust
use jadix_2d::{Canvas2D, RasterCanvas, GradientStops, VectorGradient, Brush2D,
    ExtendMode, LayerOptions, LayerFilter, Morphology};
let stops = GradientStops::new(&[
    (0., [1., 0., 0., 0.]), (0.5, [1., 0.8, 0., 1.]), (1., [0., 0., 1., 1.]),
]).unwrap().with_midpoints(&[Some(0.3), None]).unwrap();
let gradient = VectorGradient::radial([24., 32.], 2., [48., 32.], 40., stops)
    .unwrap().with_extend(ExtendMode::Reflect);
let mut canvas = RasterCanvas::new_linear(128, 96);
canvas.push_layer_with_options(LayerOptions {
    filters: vec![LayerFilter::Morphology { radius: [2, 3], operator: Morphology::Dilate },
        LayerFilter::Offset { offset: [0.5, -1.25] }],
    ..Default::default()
});
canvas.set_fill_brush(Brush2D::vector(gradient));
canvas.fill_rect(8., 8., 112., 80.);
canvas.pop_layer();
```

GPU uses exact stop texels and binary search, sharing cloned stop arrays within
each renderer submission. The finite 16-texel-wide brush table is bounded by
`device.limits().max_texture_dimension_2d`; oversized tables return false before
submission. Warm GPU resources are reused. `gradient_records`, `gradient_stops`
and `gradient_table_bytes` report headers, uploaded stops (shared by Arc identity)
and actual upload size; `gradient_slots` measures retained 256-byte table rows.
This is not a VRAM counter. Legacy CSS gradient parsing/payloads and its >8-stop
software fallback are unchanged; opt into `Brush2D::vector` for this exact GPU API.

Integration: exhaustive `Brush2D` matches need `Vector`; exhaustive `LayerFilter`
matches need Offset/Morphology/DropShadowOnly. Raw `PathGpuFill` literals need
`paint: None` for existing payloads. Kinds 10..12 and their linked stop table are
internal vector encodings; prefer Canvas recording. **Kind 8 retains its Gaussian
meaning, kind 9 retains independent-corner/inset semantics.** No additional
required `Canvas2D` method is introduced in round 10.

See [round 10 results and comparison limits](../../docs/2d-round10.md). Morphology
uses transparent exterior pixels and per-channel premultiplied min/max. CPU uses
O(width*height) monotone windows; GPU uses two direct radius-dependent passes.

## Extended Canvas

`Canvas2D` supports `set_fill_gradient`, `set_global_alpha`, `set_fill_rule`,
`transform` / `set_transform` / `reset_transform`, arbitrary `clip_path`, and
isolated `push_layer(opacity, BlendMode)` / `pop_layer` groups. All 16 W3C color
mix functions are available, including ColorDodge/Burn, Hard/SoftLight and the
nonseparable Hue/Saturation/Color/Luminosity modes. Linear canvases and GPU groups
apply those functions in linear-sRGB; legacy CPU canvases use display RGB.
Save/restore includes paint, alpha, fill rule, transform and clip state; layer
push/pop has its own stack. Finish CPU canvases with `into_pixels()` to close
remaining groups. `RasterCanvas::new_linear` and `GpuCanvas::new_linear` make CPU
compositing, recorded readback and software continuation use linear-sRGB, matching
GPU groups. Pixel I/O and final exports still use display-sRGB. The framework's
Canvas uses these constructors; standalone `new` keeps legacy display-RGB CPU
compositing. `RasterCanvas::buf` contains premultiplied values in the selected
working space; use `into_pixels()` for a display-sRGB premultiplied export.
Linear CPU images integrate 2x2 subpixel samples before compositing, including
rotated edges and nearest-neighbor texel boundaries.

`push_layer_with_options(LayerOptions)` adds independent `CompositeMode`, final
device-space rectangle bounds and an optional canvas-sized alpha mask. Composition
includes Clear/Copy/Destination, all Porter–Duff source/destination operators,
Xor, and LDR-clamped Plus/PlusLighter. Empty or zero-opacity groups still execute
destructive operators; fractional clip/mask coverage interpolates the composed
result with the original backdrop. Invalid bounds/mask dimensions define an
empty final clip. These options are explicit group effects, independent of the
current drawing CTM/clip and save/restore stack.

```rust
use jadix_2d::{Canvas2D, RasterCanvas, LayerOptions, CompositeMode};
let mut canvas = RasterCanvas::new_linear(64, 64);
canvas.clear("#4080c0");
canvas.push_layer_with_options(LayerOptions {
    compose: CompositeMode::DestinationOut,
    clip: Some([8., 8., 48., 48.]),
    ..Default::default()
});
canvas.set_fill("white");
canvas.fill_rect(16., 16., 32., 32.);
canvas.pop_layer();
```

`set_stroke_gradient` uses the existing gradient brush path on a transformed
stroke outline, including caps/joins/dashes/alpha and save/restore. Coordinates
use the transformed outline's bounding box. Use `set_stroke_brush` /
`set_fill_brush` for explicit coordinates and independent affine transforms.
Gradients outside the existing GPU stop/geometry
support use the same software continuation as fill gradients.

GPU path fills support up to eight resolved stops for linear, radial, conic,
and repeating gradients, with linear-sRGB color interpolation. Pixel stops,
color hints and larger stop lists use the CPU sampler. Legacy gradient setters
use object-bounding-box coordinates, including after a Canvas transform.

`GpuCanvas.needs_cpu` becomes true for raw pixel writes and unsupported
GPU gradients. Path masks, isolated groups and RGBA bitmaps preserve GPU drawing. A clear inside an active group currently uses software continuation. In that case call `into_raster()` rather than `into_ops()`.
Recorded drawing continues in software, with the user's paint callback invoked
only once. Image drawing defaults to nearest-neighbor; optional bilinear filtering
uses linear-sRGB premultiplied texels on CPU/GPU. Images honor CTM, alpha
and clips; `put_image_data` writes raw pixels independently of those settings.
GPU presentation itself remains the host's responsibility.

Run `cargo run -p jadix_2d --example advanced_scene -- scene.ppm` for a standalone
rendering example. `fill_bench` provides a repeatable CPU fill smoke benchmark.
See [enhancement results and remaining Vello gaps](../../docs/2d-enhancements.md).

## Portable acceptance and dynamic rendering

Select explicit backends with `backend-dx12`, `backend-vulkan`, `backend-gles`,
or `backend-metal`. Browser consumers select `webgpu` / `webgl` and currently
need `RUSTFLAGS="--cfg web_sys_unstable_apis"`, as does Gallery Web.

```powershell
cargo run --locked -p jadix_2d --example gpu_acceptance --features backend-dx12 -- dx12 target/2d-acceptance 40
```

Replace the feature and first argument with `vulkan`, `gles`, or `metal` on a
host with that backend. The tool rejects missing backends/adapters, writes a JSON
report and CPU/GPU PNG pairs, and checks finite scene error envelopes and warm
resource reuse. Set `JADIX_FORCE_SOFTWARE=1` to explicitly request a software
adapter. Times include CPU preparation, encoding, submission and a GPU wait;
they do not report isolated GPU execution time or window FPS.

`gpu::PathGpu::last_stats()` reports per-submission uploads, draws and allocation
counts, plus unique vertices, indices, queue submissions, retained vertex/index
capacity and gradient slots. Exact vertex attributes share an index; draw order,
gradient indices and image UVs remain intact. Buffers grow geometrically; packed
uniforms, brush-table textures and GPU targets are reused across dynamic frames.
Caches retain their high-water size until the renderer is dropped. These values
do not measure total GPU/driver memory.

Canvas arcs now flatten using the CTM's conservative maximum scale bound and
the shared `flatten_path_with_tolerance` API. High magnification no longer
amplifies the default user-space chord error; subdivision remains bounded.
See [validation procedure and measured results](../../docs/2d-validation.md).

## Shared GPU images, masks and ordered batches

`Image2D::from_rgba8(width, height, rgba)` validates/copies an immutable image;
clones share identity. Keep the resource across frames and pass
`CanvasImageSource::Image(&image)` to avoid repeated copies/uploads. Borrowed
`Rgba` sources remain supported. `set_image_sampling(ImageSampling::Bilinear)`
filters premultiplied linear-light texels; save/restore includes this setting.
`CanvasImageSource::TextureKey` is still reserved and does not resolve host assets.

`clip_image_mask(&image)` intersects a device-pixel alpha mask matching the canvas
size. `clip_path` generates CPU coverage then keeps drawing on the GPU through
an R8 texture; repeated static clips should retain a mask resource. GPU caches
hold weak CPU ownership, recycle expired same-size textures on later frames and
can be explicitly emptied with `PathGpu::clear_image_cache()`.

Single convex fills avoid stencil/cover; compound and self-intersecting paths
retain winding semantics. Adjacent compatible gradients share a float brush
table, meshes combine contiguous draw ranges, and same-image/sampling sprites
batch without reordering. Mask/scissor/paint-kind changes split batches. Manual
`PathGpuFill`/`PathGpuMesh` construction now includes `mask`; fill also includes
`direct`. Prefer Canvas recording or the geometry builders.

Hosts accepting **linear-light premultiplied** RGB can call
`PathGpu::render_premultiplied` into an RGBA8 sRGB target and composite it with
premultiplied blending after sRGB decoding. At 4x MSAA on DX12/Vulkan/Metal it eliminates the
straight-alpha export pass. OpenGL uses the explicit coverage export to avoid a
reproduced stale-color issue when recreating mixed-sample renderers. The default `render` contract and Jadix framework
integration still export straight alpha. Do not interchange the two conventions.

See [round-three implementation and measured Vello comparison](../../docs/2d-round3.md)
and the [isolated pinned reference runner](../../benchmarks/vello-reference/README.md).

## Stroke geometry and quality

Stroke outlines use Kurbo 0.13.1; Lyon 1.0.23 resolves winding into disjoint
triangles. Compound/self-intersecting fills can now batch directly too. Canvas
strokes expand in user space before the affine transform, preserving anisotropic
width, caps and dash lengths. CPU and GPU paint intersections once per stroke.
`set_line_dash(&[7.0, 3.0, 2.0])`, `set_line_dash_offset(offset)` and
`set_miter_limit(limit)` participate in save/restore and hit testing. Odd arrays
repeat; invalid setters preserve the previous value; all-zero arrays mean solid.
`PathStyle::dashes` / `miter_limit` provide the corresponding standalone controls.
`PathStyle` literals now require `stroke_options: Default::default()` (or struct
update syntax). Framework path attributes preserve and hash these options.
Legacy `tessellate_path` quads remain a compatibility approximation; use
`tessellate_path_mesh` or Canvas for the complete stroke controls.

`gpu::PathGpu::set_aa_quality(gpu::AaQuality::High)` combines 2x spatial
supersampling with available MSAA. `Reference` uses 4x spatial supersampling.
The default `Balanced` retains the original sampling budget. Requested quality
that exceeds device texture limits returns false; it is not silently lowered.
The framework accepts `JADIX_GPU_2D_AA=high` or `reference` at renderer creation.
Higher quality costs more texture memory and fill bandwidth.

GPU isolated groups support all 16 `BlendMode` variants, 14 LDR composition modes, nested ordering,
straight/premultiplied export and cached targets/bindings. The compositor retains
peak slots for reuse; `clear_image_cache()` also releases its group resources.
The whole canvas, including nested groups, uses one command submission. A sole
group on a transparent backdrop folds its opacity into export, including chains
of sole nested groups, only with SourceOver and no final bounds/mask. Empty or
zero-opacity groups skip geometry only when the operator preserves the backdrop; general
groups retain full-canvas intermediates. No sparse layer or filter graph is
claimed. Core GPU nesting is limited to 32 levels.

See [round 4 evidence and limits](../../docs/2d-round4.md), including an independent
numeric cubic-distance coverage oracle. Vello pixels are a comparison, not an
unconditional correctness oracle.

See [round 5 optimizations and linear compositing](../../docs/2d-round5.md) for
indexed upload measurements, repeated Vello comparisons and portability checks.

See [round 6 feature coverage](../../docs/2d-round6.md) for blend/composition
matrices and remaining Vello capability gaps. `Canvas2D` implementors must add
`set_stroke_gradient` and `push_layer_with_options`; manual `PathGpuLayer`
literals need the new compose/clip/mask fields. Prefer `PathGpuLayer::new`.


## Explicit gradient and image brushes

`Brush2D` gives fills and strokes a brush-local domain and affine transform.
The mapping is `device = canvas_CTM * brush_transform * brush_position`, captured
at each draw. Multiple shapes can share one continuous gradient; moving a shape
need not rescale its paint to the new bounding box. Invalid/singular mappings or
invalid gradient rectangles paint nothing. Save/restore includes both brushes.

```rust
use jadix_2d::{Canvas2D, RasterCanvas, Brush2D, ExtendMode, Image2D, ImageSampling};
let image = Image2D::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
let brush = Brush2D::image(image)
    .with_transform([8., 0., 0., 8., 4., 4.])
    .with_extend(ExtendMode::Repeat, ExtendMode::Decal)
    .with_sampling(ImageSampling::Bilinear);
let mut canvas = RasterCanvas::new_linear(64, 64);
canvas.set_fill_brush(brush.clone());
canvas.fill_rect(0., 0., 64., 64.);
canvas.set_stroke_brush(brush);
canvas.set_line_width(4.);
canvas.stroke_rect(8., 8., 48., 48.);
```

`Brush2D::gradient(gradient, [x,y,width,height])` resolves the existing CSS gradient
inside an explicit local rectangle. Image pixels define an image brush's domain.
`ExtendMode::{Pad,Repeat,Reflect,Decal}` are independent per image axis; bilinear
filtering extends every tap across seams in linear premultiplied color. Image
extension/filter setters leave gradient variants unchanged. Pattern filtering is
independent of `set_image_sampling`, which controls `draw_image`.

Adjacent direct pattern fills/strokes using the same image, clip and mask batch
in painter order despite different transforms, extension modes or filtering.
Immutable images upload once and reuse the existing weak GPU texture cache.
CPU pattern geometry uses 2x2 coverage and center brush samples; it does not
integrate arbitrary transformed texel discontinuities jointly with geometry.
Scale-adaptive mip/anisotropic/EWA paths are described in the later round19/20 sections below.

See [round 7 implementation, evidence and remaining Vello gaps](../../docs/2d-round7.md).
Custom `Canvas2D` implementors must implement `set_fill_brush` and
`set_stroke_brush`. Exhaustive `PathGpuOp` matches need the `Pattern` variant;
manual `GpuFillGrad` literals need `device_to_brush` (identity for legacy behavior).


## Vector glyphs, Gaussian rectangles and luminance groups

`Font2D::new(bytes, face_index)` and `GlyphRun::new(&font, size, &positions)`
prepare monochrome vector outlines. `PositionedGlyph { id, x, y }` uses baseline
coordinates with Y down. Glyph IDs/positions may come from the caller's shaper;
`glyph_id(char)` is only cmap lookup. Variable axes use
`font.with_variations(&[(*b"wght", 700.)])`. TrueType and CFF outlines use the
workspace's existing `ttf-parser`; TTF, variable TTF and CFF OTF have real-font tests.

`fill_glyph_run` and `stroke_glyph_run` apply current brushes, alpha, CTM, clip,
mask and layers. Runs retain independent glyph painter order and are cheap to
clone; GPU recording retains a shared run, generating transformed meshes at
`into_ops()`. Reusing a run avoids font parsing/outline extraction during each
recording. Default CPU glyph coverage uses 4x4 samples; whole-scene sampling supplies its
own point grid. GlyphRunOptions supports explicit hinting and contour expansion.
Optional text-layout integrates shaping/bidi/fallback, while ColorGlyphRun and
prepared text handle supported COLR/bitmap/SVG color glyphs. LCD subpixel output
is not provided. Select suitable GPU AA for small text.

`draw_blurred_rounded_rect([x,y,w,h], radius, sigma, color)` draws a local-space
Gaussian convolution under the current transform/alpha/clip/mask, using explicit
display-sRGB color and preserving fill/stroke paint. Radius clamps to half the
smaller dimension. Sigma below 0.001 is an ordinary rounded fill; negative,
nonfinite, singular or overflowing inputs paint nothing. GPU blur uses the
existing brush table/mesh batching, with no per-rectangle bitmap upload. Support
is truncated at 4 sigma. This legacy convenience keeps uniform outer blur. Use
`draw_rounded_rect_shadow` for independent corners/inset, and LayerFilter chains
for arbitrary isolated group effects.

`push_luminance_layer(opacity, invert)` makes group content a luminance mask on
the prior backdrop. It maps to `CompositeMode::LuminanceIn/Out`. In linear space,
coverage is `opacity * dot(source_premul.rgb, [0.2126,0.7152,0.0722])`; invert uses
one minus coverage. Clip/mask in `LayerOptions` limits the final effect. Empty
LuminanceIn groups clear their final region; LuminanceOut preserves it. These
modes ignore BlendMode. Legacy display-space canvases keep their working space.

See [round 8 features, API examples, validation and Vello comparison](../../docs/2d-round8.md).
Custom Canvas implementors must add `draw_blurred_rounded_rect`; exhaustive
CompositeMode matches need the two luminance variants. Gaussian GPU payload uses
kind 8 in `GpuFillGrad`; its bbox is the local rect, center is radius/std_dev.

## Boolean geometry and image reconstruction

`PathGeometry2D::from_path` resolves a fill region once. `boolean` supports Union,
Intersection, Difference and Xor, with explicit path/fragment/work budgets. Use
`geometry.fill(canvas)` to compute geometric intersections before antialiasing;
clones share the immutable geometry, and ordinary Scene2D recording works.
`to_path()` contains filled triangle contours, not a strokeable boundary outline.

`ImageSampling::{Bicubic,Lanczos3}` provide real CPU/GPU reconstruction for images
and image brushes, in premultiplied linear light with HDR/signed color preserved.
Every crop tap is restricted to its source region; image domain tables preserve
atlas batching. Exhaustive ImageSampling matches need the two new variants.
Bicubic/Lanczos3 are reconstruction kernels; explicit minification filters are described below.
No required Canvas2D method was added; gradient payload kind 8 is unchanged.

See [round18 APIs, limits and validation](../../docs/2d-round18.md) and the
[complete Vello API comparison](../../docs/vello-jadix-api-comparison.md).


## Prefiltered minification and whole-scene samples

`Image2D::with_mipmaps(MipmapLimits::default())` prepares shared, immutable NPOT
mips in premultiplied linear light. `ImageSampling::Trilinear` and `Anisotropic`
work in CPU images/brushes and all GPU paths; anisotropy is bounded to 16 taps.
Use `mipmapped_region()` before drawing an atlas crop to isolate coarse levels.
A raw source rectangle deliberately uses level zero; updates invalidate mip chains.

`scene.sampled(size, SceneOptions::default(), SceneSampling::default())` retains
all painter order, clips and groups at 4x4 spatial samples, then resolves once.
Use `.rasterize()` for CPU, or `.prepare_gpu(device, format)?.render(...)` for GPU.
The GPU path uses FP16 intermediates and one submission, without CPU readback.
Aliased sample membership uses explicit f32 triangle edge tests over the existing
Area data, avoiding hardware fixed-point edge quantization. Area work/triangle
budgets also apply to this point mode.
Gaussian/shadow/offset/morphology/convolution distances scale with the sample grid.
The explicit surface budget excludes geometry, immutable inputs and driver memory.

Exhaustive matches need the new ImageSampling variants and `AaQuality::Aliased`.
Canvas2D has no new required method; legacy Gaussian payload kind 8 is unchanged.
See [round19 APIs, cross-platform runtime evidence and limits](../../docs/2d-round19.md).


## GPU-only regional mip chains and elliptical sampling (round20)

`PathGpu::register_texture_region(texture, [x,y,width,height], color, alpha,
Some(MipmapLimits::default()))` isolates an atlas domain before GPU conversion
and NPOT area prefiltering. `override_image_region` supplies a real CPU fallback;
`set_registered_texture_mipmaps` also configures existing whole-texture imports.
GPU limits count converted FP16 base plus mips; source/staging/driver resources
and copies held by nested renderers are separate. No source readback occurs.
Unchanged frames reuse the chain; notify content changes with
`mark_override_image_dirty`. Masks remain one level.

`ImageSampling::Ewa` (new enum value 6) adds Gaussian ellipse reconstruction to
CPU/GPU images and patterns, with actual NPOT level coordinates and a bounded
17x17 support per selected level. Explicit mips retain minification footprints;
missing/truncated chains clamp support to 8 texels. This is a different kernel
from box integration or Vello High's encoded bicubic. Extend modes apply to each
tap. Raw `CanvasImageSource::Region` retains base bilinear isolation; prepare an
independent mip domain for coarse sampling.

`PathGpu::prepare_sampled_scene` freezes the renderer's external registry into a
sampled scene; the prepared object exposes its own dirty/mip configuration.
Area/Aliased spatial lists now admit distributed compound paths beyond the old
4096-triangle primitive cap. Lists are 16x16 pixels with <=4096 triangles each;
input/reference/work budgets still fail explicitly. The 6400-triangle regression
checks independent coverage and sampled-scene output on three native backends.

See [round20 contracts, complete validation and finite Vello comparison](../../docs/2d-round20.md).
Exhaustive `ImageSampling` matches must handle `Ewa`; no new required Canvas2D
method or dependency is introduced. Gradient payload kind=8 remains unchanged.


## Execution controls and optional compute (round21)

`gpu::RendererOptions` adds actual CPU scene execution, bounded native pipeline
initialization threads, same-device wgpu pipeline cache and initial AA quality.
Use `SceneRenderer2D` to select complete CPU rasterization plus FP16 export or
GPU drawing. `PathGpu::try_new_with_options` remains GPU-only; `warm_up` prepares
only needed leaf materials. Area pipelines are lazy; ordinary image/pattern
shaders omit EWA code until first EWA use, then retain the upgraded pipeline.

`Scene2D::bump_estimate(size, options)` and `PreparedSceneGpu::bump_estimate()`
return checked `SceneMemoryEstimate` payload counters and conservative planning
bounds. They are not Vello compute bump-buffer ABI or process/VRAM guarantees.

Enable `compute` explicitly on `jadix_2d` / `jadix` / `jadix_renderer` for safe
wgpu compute creation and compute GPU-only mip generation. Backend selection,
including DX12, does not enable it implicitly. Without compute, mip generation
retains the render-pass path. WebGL always uses that fallback; WebGPU and native
backends additionally require actual requested-device compute/storage limits.
The standalone wgpu package defaults include compute for compatibility; Cargo
features unify across the dependency graph. Handle/backend internals remain.

No new required Canvas2D method or ImageSampling variant is added in round21;
Gaussian gradient kind=8 remains reserved. See [round21 APIs, migration, runtime
checks and limits](../../docs/2d-round21.md).


## Typed compute/render recordings (round22)

With `compute`, `gpu::low_level::{Recording, Engine}` executes ordered buffer /
uniform / partial image uploads, clears, direct and indirect dispatch, vertex-index
render draws, downloads and explicit releases. Typed proxies include checked
BufferRange bindings. Shaders compile lazily; payload/resource/staging/command
budgets are explicit. Native validation failures do not submit; browsers need an
outer awaited error scope. Readbacks retain padded image row strides.

`gpu::low_level::Render` retains CPU-prepared Scene2D geometry and FP16 output,
with one-time capture and caller-encoder `record_fine`. Submit each encoded frame
before encoding another on that retained renderer. Output textures interoperate
with existing Canvas GPU image imports. These are native Jadix schemas, not Vello
Encoding/Resolver/FullShaders/bump-buffer compatibility.

Nearest/Bilinear now have a small shader tier excluding higher-order convolution
and mip/EWA loops. Materials upgrade once when stronger sampling is actually used.
See [round22 execution contracts, exact API differences and validation](../../docs/2d-round22.md).


## Scene updates, external compute resources and EWA specialization (round23)

`Scene2D::reset/reset_and_resize/rerecord` updates a recording handle while
preserving cloned/retained child snapshots. Invalid replacement leaves the old
scene intact. Unique reset retains command capacity; rerecord builds a new scene.

`gpu::low_level::Engine::import_buffer/import_image` adopts same-device external
handles without copying, with exact proxy descriptors and command-specific usage
preflight. Images have one mip/layer/sample. Exported handles can outlive engine
release; budgets exclude driver allocations and external ownership.

Image/pattern materials now select simple, non-EWA high, EWA-only or mixed shader
families using monotonic capability bits. EWA-only excludes unrelated higher-order
reconstruction loops. Sampling equations, public sampling enums and gradient
kind=8 are unchanged. Mixed high/EWA still requires the complete shader.

[Round23 contracts and latest Vello comparison](../../docs/2d-round23.md) includes
18 fresh-process observations, independent box-reference pixels and their limits.


## Command packets, rectangular sampling and DX12 compilation (round24)

`Engine::warm_up` compiles only shaders used by a valid Recording, without
resource uploads/submissions. `Engine::record` returns `RecordedExecution` with
owned commands, downloads and stats for caller-managed submissions. Submit in
recording order; discard requires resetting affected resource state before reuse.
`execute` remains a record-and-submit convenience method.

`Scene2D::sampled_grid(size, options, [nx,ny], max_surface_bytes)` accepts each
axis1/2/4/8, including actual8-sample2x4/4x2 grids. Per-axis filter scaling,
per-sample painter order/clip and final linear resolve apply on CPU/GPU. Existing
uniform sampled scenes keep their API. `sample_grid/sample_count` expose the exact
configuration; legacy `axis_samples` returns the horizontal count.

DX12 shader compilation keeps the same compiler/flags. Texel wrapping uses one
unsigned remainder with negative-coordinate Euclidean restoration; mip bilinear
uses one path across levels, and solid Area avoids generic gradient/blur shading.
See [round24 contracts, pixel gates and matched measurements](../../docs/2d-round24.md).

Single-pixel and strip targets now accept nonzero dimensions on PathGpu and
sampled GPU output; zero dimensions and device-limit overflows still fail.
Native validation covers72 size/AA/layer/HDR/alpha/sample combinations.
