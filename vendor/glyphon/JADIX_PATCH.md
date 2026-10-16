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
upstream. No other upstream changes are intended.
