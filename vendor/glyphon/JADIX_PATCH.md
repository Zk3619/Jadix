# Jadix glyphon 0.12.0 patch

Source: glyphon 0.12.0 crate, https://github.com/grovesNL/glyphon.
Upstream license files are included unchanged.

The public API, shaping, glyph cache keys, packing, clipping, and draw order are
unchanged. `InnerAtlas` retains a CPU pixel mirror, allocated on its first glyph,
and unions writes into one dirty rectangle. `prepare` flushes at most one upload
per atlas. Growing an atlas copies existing pixels with their original stride;
it does not call the font or custom glyph rasterizer again. No fonts or glyphs
are preloaded. The mirror uses one byte per mask pixel and four per color pixel.

This patch targets the many small driver uploads observed in the native Gallery
scroll profiler. Keep the pixel packing/growth regression tests when updating
upstream.

Glyph residency now stores an epoch in each cache entry instead of inserting
every prepared glyph into a second HashSet. Both cache hits and new glyphs mark
the current atlas epoch; only `trim` advances it. Multiple renderers preparing
against one atlas therefore share protection until the same public trim boundary.
LRU access and the eviction stop rule (including zero-size glyphs) are preserved.
Epoch rollover clears old marks without changing LRU order. CPU allocator tests
cover retention across prepares, trim, zero-size entries, eviction and rollover.

The D3D11/ANGLE bring-up also separates fragment inputs from `VertexOutput`.
Fragments consume only color/UV/content type; the vertex position retains
`@invariant`. This avoids emitting the invalid GLES `invariant gl_FragCoord`
declaration without changing the vertex transform or fragment calculation.
