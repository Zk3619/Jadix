# Vello 0.11 与 Jadix 2D：完整公开 API 清单与有限实测

本文固定 `vello = 0.11.0` compute 渲染器，另将既有 `vello_gpu = 0.3.0` strip 渲染器明确区分。扫描使用 docs.rs 官方 all-features rustdoc JSON；Jadix 使用当前源码、原生 GPU/compute/text/SVG/ICC/test-hooks feature 的本地 rustdoc JSON。原始 JSON、SHA-256、完整类型签名、文档、重导出和 trait implementation 记录保存在 [机器可读 inventory](../benchmarks/vello-api/inventory/)。

**完整的含义是公开 API 枚举完整，不是所有 API 已动态证明等价。** 本文列出 vello 自有可达公开 API、全部公开固有方法/字段/枚举变体和重导出；trait 自带/blanket 方法按 owner 的 trait 清单记录，不递归扩展 std；条件编译以所保存 docs.rs all-features target 为准。依赖重导出递归边界单独列出，不把 wgpu 的上千项 GPU 接口当成 Vello 独有绘制能力。

本次枚举 Vello **239** 条（含重导出别名）；Jadix 枚举 **1897** 条。机器分类：`different` 44, `equivalent` 135, `partial` 58, `shared` 2。这些数字含结构字段和内部专家 API，**不能作为能力百分比**。

状态：`equivalent` 有用途对应入口，不保证逐像素；`partial` 有显著语义/路径缺口；`missing` 当前无同等实现；`different` 架构/主机接口不直接映射；`shared` 共同依赖。所有对应入口必须结合下文限制与证据判断。

## 已增加的能力与仍缺少的能力

| 维度 | 本轮 Jadix 可调用入口 | 固定 Vello 0.11 公开入口/差异 |
|---|---|---|
| 浮点与广色域 | Color2D、Image2D::from_linear_rgba32f、RasterCanvas::new_hdr、PathGpu::try_new_with_format(Rgba16Float)、ICC 双向转换 | render_to_texture 文档规定 Rgba8Unorm；没有同类公开 HDR/ICC 输出管理入口。Jadix 仍无 OS HDR 呈现。 |
| 组图像效果 | Gaussian/drop-shadow/颜色矩阵/transfer/convolution/morphology 链；显式 SceneRegion halo | Scene 有隔离/混合与专用 blurred rounded rectangle；没有同形任意组滤镜链 API。算法质量/边界需逐用例验证。 |
| 图集管理 | AtlasCache/GlyphAtlasCache 公开预算/LRU/不可变快照/安全局部上传 | Vello 内部资源图集和 GPU texture registration/override；Jadix 本轮新增 GPU-only image registration，支持 binding 或 copy-source 输入及明确色彩/alpha 合同。 |
| 场景与网格 | Scene2D 子场景、PreparedSceneGpu、GpuMesh 实际硬件实例 | Scene::append 实例编码与内部缓存是不同架构；不把显式 GpuMesh 视作所有 Scene 自动优化。 |
| 几何布尔与高质量图像 | PathGeometry2D 的 Union/Intersection/Difference/Xor，先几何交集再 AA；Bicubic/Lanczos3 真实 CPU/GPU 采样，crop 逐 tap 限域 | Vello Scene 无几何布尔结果 API；Peniko High 提示及固定 Vello fine.wgsl 已有 Mitchell Bicubic，Jadix 本轮补齐并增加显式 Lanczos3。round19 另加入 Trilinear/Anisotropic mip 缩小采样及 SampledScene2D 整场景样本合成，round20 增加有界 Gaussian EWA 与独立区域 GPU-only mip 生成；仍不是解析全场景积分。 |
| 文字与 AA | shaping/layout/variable axes/COLR/bitmap/SVG 接入；采样 AA | Jadix 本轮新增低层 outline hint/embolden/归一化坐标/逐字形变换；新增 GPU/CPU Area 与显式曲线精度；与 Vello winding-integral 的边界和重复轮廓差异见独立数值参考。 |

## 绘制和资源接口逐项比对

| Vello API | 签名 | 状态 | Jadix 对应 | 实际差异 |
|---|---|---|---|---|
| `vello::AaSupport::all` | `() → Self` | partial | `gpu::AaQuality` | Jadix supports analytic per-primitive Area, Balanced/High/Reference and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene spatial samples with one final resolve; native MSAA8/16 identities still differ. |
| `vello::AaSupport::area_only` | `() → Self` | equivalent | `gpu::AaQuality::Area` | GPU analytic pixel/triangle intersection area; independently clipped CPU f64 reference. Curves are tessellated; raw meshes must be non-overlapping, stencil-only expert fills reject. Different from Vello signed winding integration. |
| `vello::DrawGlyphs::brush` | `(self: Self, brush: impl Into) → Self` | equivalent | `Canvas2D::set_fill_brush` | Inherited fill brushes for monochrome outlines; native color paints retain font colors. |
| `vello::DrawGlyphs::brush_alpha` | `(self: Self, alpha: f32) → Self` | equivalent | `Canvas2D::set_global_alpha` | Alpha affects complete draw/glyph according to prepared color semantics. |
| `vello::DrawGlyphs::brush_transform` | `(self: Self, transform: Option<Affine>) → Self` | equivalent | `Brush2D::with_transform` | Explicit brush-local affine, combined with current transform. |
| `vello::DrawGlyphs::draw` | `(self: Self, style: impl Into, glyphs: impl Iterator) → ()` | partial | `Canvas2D::fill_glyph_run; stroke_glyph_run; fill_color_glyph_run` | Prepared hinted/expanded outline drawing and native COLR paths. Native color glyph hint/embolden does not use monochrome options. |
| `vello::DrawGlyphs::font_embolden` | `(self: Self, embolden: FontEmbolden) → Self` | equivalent | `GlyphRun::with_options; FontEmbolden` | Same Kurbo expand_path engine, independent x/y amounts, join/miter/tolerance; explicit finite input limits. Prepared monochrome outlines, not native COLR paints or advance recalculation. |
| `vello::DrawGlyphs::font_size` | `(self: Self, size: f32) → Self` | equivalent | `GlyphRun::new; TextPrepareOptions` | Font size participates in preparation and cache identity. |
| `vello::DrawGlyphs::glyph_transform` | `(self: Self, transform: Option<Affine>) → Self` | equivalent | `GlyphRunOptions::glyph_transform; per_glyph_transforms` | Glyph-local affine before baseline translation, plus independent transforms per glyph. Canvas/run global transforms remain separate; public coordinate convention is Y-down. |
| `vello::DrawGlyphs::hint` | `(self: Self, hint: bool) → Self` | partial | `GlyphRunOptions::hint; GlyphRun::with_options` | Skrifa default HintingInstance on scalable outlines at preparation ppem, plus existing Swash text atlas. Canvas transforms do not rehint; native COLR paint graph remains unhinted. |
| `vello::DrawGlyphs::new` | `(scene: &mut Scene, font: &FontData) → Self` | partial | `Font2D::new; GlyphRun::new; PreparedText2D::new` | Jadix uses immutable prepared glyph runs rather than a borrowed scene builder. |
| `vello::DrawGlyphs::normalized_coords` | `(self: Self, coords: &[NormalizedCoord]) → Self` | equivalent | `Font2D::normalized_coords; GlyphRunOptions::normalized_coords` | Exact post-avar F2Dot14 axis-order coordinates for prepared monochrome outlines. Does not retroactively change external shaping/advances. |
| `vello::DrawGlyphs::transform` | `(self: Self, transform: Affine) → Self` | equivalent | `Canvas2D::transform; SceneOptions::transform` | Full affine path transform. |
| `vello::Renderer::mark_override_image_dirty` | `(self: &mut Self, image: &ImageData) → ()` | equivalent | `gpu::PathGpu::mark_override_image_dirty` | Explicit revision invalidation; refreshes image/mask/pattern caches on next encode. Current implementation clears image/layer caches and may reallocate on dirty. |
| `vello::Renderer::new` | `(device: &Device, options: RendererOptions) → std::result::Result<Self, Error>` | partial | `gpu::PathGpu::try_new_with_format` | Different pipeline architecture and configuration; supports RGBA8 sRGB and RGBA16F targets. |
| `vello::Renderer::override_image` | `(self: &mut Self, image: &ImageData, texture: Option<wgpu::TexelCopyTextureInfoBase<wgpu::Texture>>) → Option<wgpu::TexelCopyTextureInfoBase<wgpu::Texture>>` | equivalent | `gpu::PathGpu::override_image` | Per-renderer GPU texture overrides with immutable CPU fallback, shared by nested group renderers. Requires same device and TEXTURE_BINDING or COPY_SRC, explicit color/alpha; converts to cached linear FP16 on GPU. |
| `vello::Renderer::register_texture` | `(self: &mut Self, texture: wgpu::Texture) → ImageData` | equivalent | `gpu::PathGpu::register_texture; render_to_image` | GPU-only image handles with transparent CPU fallback. RGBA/BGRA8 or RGBA16F, single-sample D2; <=16M pixels and <=4096 registrations and <=64M total pixels. Optional GPU-only FP16 mip generation, independent cropped domains and sampled-scene registry snapshots. Different resource preconditions from Vello COPY_SRC. |
| `vello::Renderer::render_to_texture` | `(self: &mut Self, device: &Device, queue: &Queue, scene: &Scene, texture: &TextureView, params: &RenderParams) → std::result::Result<(), Error>` | partial | `gpu::PathGpu::render; render_premultiplied` | Jadix caller target is render attachment; Vello uses RGBA8 storage output; alpha/encoding must be normalized before comparisons. |
| `vello::Renderer::unregister_texture` | `(self: &mut Self, handle: ImageData) → ()` | equivalent | `gpu::PathGpu::unregister_texture; prune_registered_textures` | Restores original CPU fallback for subsequent GPU frames, invalidates nested caches, drops registration references. Submitted work keeps owned resources; submit pending encoders before registry mutations. |
| `vello::Scene::append` | `(self: &mut Self, other: &Self, transform: Option<Affine>) → ()` | equivalent | `Canvas2D::draw_scene; Scene2D::draw` | Nested reusable shared scenes with appended transform and optional isolation. |
| `vello::Scene::bump_estimate` | `(self: &Self, transform: Option<Affine>) → BumpAllocatorMemory` | partial | `Scene2D::bump_estimate; PreparedSceneGpu::bump_estimate; gpu::SceneMemoryEstimate` | Checked CPU replay/tessellation with explicit target and transform estimates geometry, vertex/Area payloads, unique images and conservative layer/target surfaces. Not Vello compute bump-buffer ABI or process/driver allocation guarantee. |
| `vello::Scene::draw_blurred_rounded_rect` | `(self: &mut Self, transform: Affine, rect: Rect, brush: Color, radius: f64, std_dev: f64) → ()` | partial | `Canvas2D::draw_blurred_rounded_rect; draw_rounded_rect_shadow` | Gaussian support differs: Jadix 4 sigma cutoff, independent corners/inset extension, not identical Vello 2.5 sigma clipping. |
| `vello::Scene::draw_blurred_rounded_rect_in` | `(self: &mut Self, shape: &impl Shape, transform: Affine, rect: Rect, brush: Color, radius: f64, std_dev: f64) → ()` | equivalent | `Canvas2D::save; clip_path; draw_blurred_rounded_rect; restore` | Arbitrary shape clip composition through existing Canvas state. |
| `vello::Scene::draw_glyphs` | `(self: &mut Self, font: &FontData) → DrawGlyphs<'_>` | partial | `GlyphRun::with_options; ColorGlyphRun; PreparedText2D` | Low-level scalable hint/embolden/normalized/local transforms now available; color glyph graph remains a distinct preparation path. |
| `vello::Scene::draw_image` | `(self: &mut Self, image: impl Into, transform: Affine) → ()` | equivalent | `Canvas2D::draw_image; Image2D; PathGpu::register_texture` | Nearest/Bilinear plus explicit 4x4 Mitchell Bicubic and 6x6 Lanczos3 on CPU/GPU; source crops clamp every tap, float/predefined-space and caller GPU textures. Explicit NPOT linear-premultiplied mip chains with Trilinear / bounded 16-tap Anisotropic / Gaussian elliptical Ewa sampling; GPU-only region imports can generate NPOT mip chains lazily without source readback; independently prefilter atlas crops with mipmapped_region; negative-lobe ringing clamp differs by source-domain contract. |
| `vello::Scene::encoding` | `(self: &Self) → &Encoding` | different | `PreparedSceneGpu::ops; GpuCanvas::into_ops` | Jadix triangle/brush/group ops are not Vello compute encoding. |
| `vello::Scene::encoding_mut` | `(self: &mut Self) → &mut Encoding` | different | `GpuCanvas::into_ops; PathGpuOp` | Editable owned prepared ops, not the same binary encoding. |
| `vello::Scene::fill` | `(self: &mut Self, style: Fill, transform: Affine, brush: impl Into, brush_transform: Option<Affine>, shape: &impl Shape) → ()` | equivalent | `Canvas2D::set_fill_brush; set_fill_rule; fill_path` | Solid/gradient/image brushes, explicit affine and fill rule. |
| `vello::Scene::new` | `() → Self` | equivalent | `Scene2D::record; GpuCanvas::new_linear` | Recorded callback or direct Canvas recording. |
| `vello::Scene::pop_layer` | `(self: &mut Self) → ()` | equivalent | `Canvas2D::pop_layer` | Closes an isolated group, applies ordered filters, then composition. |
| `vello::Scene::push_clip_layer` | `(self: &mut Self, clip_style: impl Into, transform: Affine, clip: &impl Shape) → ()` | equivalent | `Canvas2D::save; clip_path; restore` | Save/restore path mask clips, or final layer clips. Explicit PathGeometry2D Intersection resolves clip/subject geometry before AA and avoids correlated identical-edge coverage multiplication; existing mask clip semantics are unchanged. SampledScene2D additionally retains per-sample clips, painter order and group composition until one final resolve; finite ordered grid, not analytic exact scene integration. |
| `vello::Scene::push_layer` | `(self: &mut Self, clip_style: impl Into, blend: impl Into, alpha: f32, transform: Affine, clip: &impl Shape) → ()` | equivalent | `Canvas2D::push_layer_with_options` | Blend/composition/opacity plus arbitrary filters and final clip/mask. Rasterization/region semantics differ. |
| `vello::Scene::push_luminance_mask_layer` | `(self: &mut Self, clip_style: impl Into, alpha: f32, transform: Affine, clip: &impl Shape) → ()` | equivalent | `Canvas2D::push_luminance_layer` | Premultiplied luminance mask; invert extension available. |
| `vello::Scene::reset` | `(self: &mut Self) → ()` | equivalent | `Scene2D::reset; reset_and_resize; rerecord` | Clear/reset a handle with copy-on-write snapshot isolation. Unique command storage retains capacity, clones/subscenes remain unchanged. Rerecord validates atomically in a fresh command vector; not Vello mutable Encoding storage. |
| `vello::Scene::stroke` | `(self: &mut Self, style: &Stroke, transform: Affine, brush: impl Into, brush_transform: Option<Affine>, shape: &impl Shape) → ()` | equivalent | `Canvas2D::stroke_path; PathStyle; StrokeOptions` | User-space stroke geometry, caps/joins/miter/dashes and arbitrary brushes. |

## 全部 Vello 自有公开项（含字段、变体、专家和主机 API）

| 公开项 | 类别/源位置 | 状态 | Jadix 对应与说明 |
|---|---|---|---|
| `vello` | module; `src/lib.rs:4` | different | `jadix_2d; jadix` — Project boundary, not an individual operation. |
| `vello::AaConfig` | enum; `src/lib.rs:177` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::AaConfig::Area` | variant; `src/lib.rs:186` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::AaConfig::Msaa16` | variant; `src/lib.rs:194` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::AaConfig::Msaa8` | variant; `src/lib.rs:190` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::AaSupport` | struct; `src/lib.rs:205` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::AaSupport::all` | function; `src/lib.rs:218` | partial | `gpu::AaQuality` — Jadix supports analytic per-primitive Area, Balanced/High/Reference and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene spatial samples with one final resolve; native MSAA8/16 identities still differ. |
| `vello::AaSupport::area` | struct_field; `src/lib.rs:207` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::AaSupport::area_only` | function; `src/lib.rs:229` | equivalent | `gpu::AaQuality::Area` — GPU analytic pixel/triangle intersection area; independently clipped CPU f64 reference. Curves are tessellated; raw meshes must be non-overlapping, stencil-only expert fills reject. Different from Vello signed winding integration. |
| `vello::AaSupport::msaa16` | struct_field; `src/lib.rs:211` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::AaSupport::msaa8` | struct_field; `src/lib.rs:209` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::DrawGlyphs` | struct; `src/scene.rs:467` | partial | `Font2D; GlyphRun; PreparedText2D` — Prepared glyph API instead of borrowed builder. See individual methods for axes/hint/embolden differences. |
| `vello::DrawGlyphs` | use; `src/lib.rs:147` | partial | `Font2D; GlyphRun; PreparedText2D` — Prepared glyph API instead of borrowed builder. See individual methods for axes/hint/embolden differences. |
| `vello::DrawGlyphs::brush` | function; `src/scene.rs:581` | equivalent | `Canvas2D::set_fill_brush` — Inherited fill brushes for monochrome outlines; native color paints retain font colors. |
| `vello::DrawGlyphs::brush_alpha` | function; `src/scene.rs:590` | equivalent | `Canvas2D::set_global_alpha` — Alpha affects complete draw/glyph according to prepared color semantics. |
| `vello::DrawGlyphs::brush_transform` | function; `src/scene.rs:530` | equivalent | `Brush2D::with_transform` — Explicit brush-local affine, combined with current transform. |
| `vello::DrawGlyphs::draw` | function; `src/scene.rs:604` | partial | `Canvas2D::fill_glyph_run; stroke_glyph_run; fill_color_glyph_run` — Prepared hinted/expanded outline drawing and native COLR paths. Native color glyph hint/embolden does not use monochrome options. |
| `vello::DrawGlyphs::font_embolden` | function; `src/scene.rs:546` | equivalent | `GlyphRun::with_options; FontEmbolden` — Same Kurbo expand_path engine, independent x/y amounts, join/miter/tolerance; explicit finite input limits. Prepared monochrome outlines, not native COLR paints or advance recalculation. |
| `vello::DrawGlyphs::font_size` | function; `src/scene.rs:539` | equivalent | `GlyphRun::new; TextPrepareOptions` — Font size participates in preparation and cache identity. |
| `vello::DrawGlyphs::glyph_transform` | function; `src/scene.rs:517` | equivalent | `GlyphRunOptions::glyph_transform; per_glyph_transforms` — Glyph-local affine before baseline translation, plus independent transforms per glyph. Canvas/run global transforms remain separate; public coordinate convention is Y-down. |
| `vello::DrawGlyphs::hint` | function; `src/scene.rs:555` | partial | `GlyphRunOptions::hint; GlyphRun::with_options` — Skrifa default HintingInstance on scalable outlines at preparation ppem, plus existing Swash text atlas. Canvas transforms do not rehint; native COLR paint graph remains unhinted. |
| `vello::DrawGlyphs::new` | function; `src/scene.rs:477` | partial | `Font2D::new; GlyphRun::new; PreparedText2D::new` — Jadix uses immutable prepared glyph runs rather than a borrowed scene builder. |
| `vello::DrawGlyphs::normalized_coords` | function; `src/scene.rs:562` | equivalent | `Font2D::normalized_coords; GlyphRunOptions::normalized_coords` — Exact post-avar F2Dot14 axis-order coordinates for prepared monochrome outlines. Does not retroactively change external shaping/advances. |
| `vello::DrawGlyphs::transform` | function; `src/scene.rs:506` | equivalent | `Canvas2D::transform; SceneOptions::transform` — Full affine path transform. |
| `vello::Error` | enum; `src/lib.rs:259` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::BufferAsyncError` | variant; `src/lib.rs:287` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::BufferAsyncError::0` | struct_field; `src/lib.rs:287` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::DownloadError` | variant; `src/lib.rs:292` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::DownloadError::0` | struct_field; `src/lib.rs:292` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::NoCompatibleDevice` | variant; `src/lib.rs:263` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::UnavailableBufferUsed` | variant; `src/lib.rs:282` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::UnavailableBufferUsed::0` | struct_field; `src/lib.rs:282` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::UnavailableBufferUsed::1` | struct_field; `src/lib.rs:282` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::UnsupportedSurfaceFormat` | variant; `src/lib.rs:276` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::WgpuCreateSurfaceError` | variant; `src/lib.rs:268` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::WgpuCreateSurfaceError::0` | struct_field; `src/lib.rs:268` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::WgpuErrorFromScope` | variant; `src/lib.rs:296` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::Error::WgpuErrorFromScope::0` | struct_field; `src/lib.rs:296` | different | `SceneError; FontError; GradientError; AtlasError; wgpu::Error` — Subsystem-specific typed errors and PathGpu bool/Option failure APIs; no identical unified Vello error type. |
| `vello::FontEmbolden` | use; `src/lib.rs:148` | equivalent | `FontEmbolden; GlyphRunOptions::embolden` — Same Kurbo contour expansion, with finite bounds; no reshaping/advance update. |
| `vello::Glyph` | use; `src/lib.rs:148` | equivalent | `PositionedGlyph` — Glyph ID and baseline x/y positions; field types differ. |
| `vello::NormalizedCoord` | use; `src/lib.rs:148` | partial | `FontVariations; Font2D::with_variations` — GlyphRunOptions accepts exact F2Dot14 i16 coordinates, and Font2D exposes user-space normalization; own public storage type differs. |
| `vello::RenderParams` | struct; `src/lib.rs:359` | partial | `gpu::PathGpu` — Render-to-texture entry with explicit size/target, different compute vs stencil/triangle architecture. |
| `vello::RenderParams::antialiasing_method` | struct_field; `src/lib.rs:370` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::RenderParams::base_color` | struct_field; `src/lib.rs:362` | equivalent | `Canvas2D::set_fill_brush; fill_rect` — Jadix clears transparent; encode a first full-target fill for base color. |
| `vello::RenderParams::height` | struct_field; `src/lib.rs:366` | equivalent | `CanvasSize; PathGpu::render(size)` — Explicit render size. Target ownership/format is separate. |
| `vello::RenderParams::width` | struct_field; `src/lib.rs:365` | equivalent | `CanvasSize; PathGpu::render(size)` — Explicit render size. Target ownership/format is separate. |
| `vello::Renderer` | struct; `src/lib.rs:324` | partial | `gpu::PathGpu` — Render-to-texture entry with explicit size/target, different compute vs stencil/triangle architecture. |
| `vello::Renderer::mark_override_image_dirty` | function; `src/lib.rs:555` | equivalent | `gpu::PathGpu::mark_override_image_dirty` — Explicit revision invalidation; refreshes image/mask/pattern caches on next encode. Current implementation clears image/layer caches and may reallocate on dirty. |
| `vello::Renderer::new` | function; `src/lib.rs:434` | partial | `gpu::PathGpu::try_new_with_format` — Different pipeline architecture and configuration; supports RGBA8 sRGB and RGBA16F targets. |
| `vello::Renderer::override_image` | function; `src/lib.rs:538` | equivalent | `gpu::PathGpu::override_image` — Per-renderer GPU texture overrides with immutable CPU fallback, shared by nested group renderers. Requires same device and TEXTURE_BINDING or COPY_SRC, explicit color/alpha; converts to cached linear FP16 on GPU. |
| `vello::Renderer::register_texture` | function; `src/lib.rs:574` | equivalent | `gpu::PathGpu::register_texture; render_to_image` — GPU-only image handles with transparent CPU fallback. RGBA/BGRA8 or RGBA16F, single-sample D2; <=16M pixels and <=4096 registrations and <=64M total pixels. Optional GPU-only FP16 mip generation, independent cropped domains and sampled-scene registry snapshots. Different resource preconditions from Vello COPY_SRC. |
| `vello::Renderer::render_to_texture` | function; `src/lib.rs:476` | partial | `gpu::PathGpu::render; render_premultiplied` — Jadix caller target is render attachment; Vello uses RGBA8 storage output; alpha/encoding must be normalized before comparisons. |
| `vello::Renderer::unregister_texture` | function; `src/lib.rs:603` | equivalent | `gpu::PathGpu::unregister_texture; prune_registered_textures` — Restores original CPU fallback for subsequent GPU frames, invalidates nested caches, drops registration references. Submitted work keeps owned resources; submit pending encoders before registry mutations. |
| `vello::RendererOptions` | struct; `src/lib.rs:375` | partial | `gpu::RendererOptions; gpu::SceneRenderer2D; gpu::PathGpu::try_new_with_options` — Explicit CPU execution, initialization threads, same-device wgpu cache and quality options. CPU algorithm and AA behavior differ; see individual field rows. |
| `vello::RendererOptions::antialiasing_support` | struct_field; `src/lib.rs:389` | partial | `gpu::AaQuality; PathGpu::try_new` — Analytic per-primitive Area and explicit SampledScene2D 1/2/4/8/16/32/64 whole-scene ordered samples with one resolve; no native MSAA8/16 identity. Raw stencil-only expert fills reject in Area mode. |
| `vello::RendererOptions::num_init_threads` | struct_field; `src/lib.rs:401` | equivalent | `gpu::RendererOptions::num_init_threads; PathGpu::warm_up` — Explicit bounded native pipeline initialization workers over currently needed material jobs. macOS defaults to one, WebAssembly always serial. Not identical Vello stage scheduling. |
| `vello::RendererOptions::pipeline_cache` | struct_field; `src/lib.rs:406` | equivalent | `gpu::RendererOptions::pipeline_cache; PathGpu::try_new_with_options` — Caller-owned same-device wgpu PipelineCache propagated to owned leaf/layer/filter/import/mip/sample-resolve pipelines. Persistent cache data tested on Vulkan; wgpu does not supply the same persistence on DX12. Independent GpuMesh pipelines remain separate. |
| `vello::RendererOptions::use_cpu` | struct_field; `src/lib.rs:382` | partial | `gpu::RendererOptions::use_cpu; SceneRenderer2D` — Actual complete CPU drawing/clip/filter/composition followed by an FP16 upload/export pass; unlike Vello CPU coarse stages plus GPU fine rasterization. GPU-only source registrations require CPU fallback pixels. |
| `vello::Scene` | struct; `src/scene.rs:45` | equivalent | `Scene2D; GpuCanvas` — Immutable scene recording, shared subscene replay and prepared mesh snapshot. API shape differs. |
| `vello::Scene` | use; `src/lib.rs:147` | equivalent | `Scene2D; GpuCanvas` — Immutable scene recording, shared subscene replay and prepared mesh snapshot. API shape differs. |
| `vello::Scene::append` | function; `src/scene.rs:444` | equivalent | `Canvas2D::draw_scene; Scene2D::draw` — Nested reusable shared scenes with appended transform and optional isolation. |
| `vello::Scene::bump_estimate` | function; `src/scene.rs:68` | partial | `Scene2D::bump_estimate; PreparedSceneGpu::bump_estimate; gpu::SceneMemoryEstimate` — Checked CPU replay/tessellation with explicit target and transform estimates geometry, vertex/Area payloads, unique images and conservative layer/target surfaces. Not Vello compute bump-buffer ABI or process/driver allocation guarantee. |
| `vello::Scene::draw_blurred_rounded_rect` | function; `src/scene.rs:244` | partial | `Canvas2D::draw_blurred_rounded_rect; draw_rounded_rect_shadow` — Gaussian support differs: Jadix 4 sigma cutoff, independent corners/inset extension, not identical Vello 2.5 sigma clipping. |
| `vello::Scene::draw_blurred_rounded_rect_in` | function; `src/scene.rs:270` | equivalent | `Canvas2D::save; clip_path; draw_blurred_rounded_rect; restore` — Arbitrary shape clip composition through existing Canvas state. |
| `vello::Scene::draw_glyphs` | function; `src/scene.rs:435` | partial | `GlyphRun::with_options; ColorGlyphRun; PreparedText2D` — Low-level scalable hint/embolden/normalized/local transforms now available; color glyph graph remains a distinct preparation path. |
| `vello::Scene::draw_image` | function; `src/scene.rs:423` | equivalent | `Canvas2D::draw_image; Image2D; PathGpu::register_texture` — Nearest/Bilinear plus explicit 4x4 Mitchell Bicubic and 6x6 Lanczos3 on CPU/GPU; source crops clamp every tap, float/predefined-space and caller GPU textures. Explicit NPOT linear-premultiplied mip chains with Trilinear / bounded 16-tap Anisotropic / Gaussian elliptical Ewa sampling; GPU-only region imports can generate NPOT mip chains lazily without source readback; independently prefilter atlas crops with mipmapped_region; negative-lobe ringing clamp differs by source-domain contract. |
| `vello::Scene::encoding` | function; `src/scene.rs:74` | different | `PreparedSceneGpu::ops; GpuCanvas::into_ops` — Jadix triangle/brush/group ops are not Vello compute encoding. |
| `vello::Scene::encoding_mut` | function; `src/scene.rs:81` | different | `GpuCanvas::into_ops; PathGpuOp` — Editable owned prepared ops, not the same binary encoding. |
| `vello::Scene::fill` | function; `src/scene.rs:300` | equivalent | `Canvas2D::set_fill_brush; set_fill_rule; fill_path` — Solid/gradient/image brushes, explicit affine and fill rule. |
| `vello::Scene::new` | function; `src/scene.rs:54` | equivalent | `Scene2D::record; GpuCanvas::new_linear` — Recorded callback or direct Canvas recording. |
| `vello::Scene::pop_layer` | function; `src/scene.rs:239` | equivalent | `Canvas2D::pop_layer` — Closes an isolated group, applies ordered filters, then composition. |
| `vello::Scene::push_clip_layer` | function; `src/scene.rs:180` | equivalent | `Canvas2D::save; clip_path; restore` — Save/restore path mask clips, or final layer clips. Explicit PathGeometry2D Intersection resolves clip/subject geometry before AA and avoids correlated identical-edge coverage multiplication; existing mask clip semantics are unchanged. SampledScene2D additionally retains per-sample clips, painter order and group composition until one final resolve; finite ordered grid, not analytic exact scene integration. |
| `vello::Scene::push_layer` | function; `src/scene.rs:101` | equivalent | `Canvas2D::push_layer_with_options` — Blend/composition/opacity plus arbitrary filters and final clip/mask. Rasterization/region semantics differ. |
| `vello::Scene::push_luminance_mask_layer` | function; `src/scene.rs:146` | equivalent | `Canvas2D::push_luminance_layer` — Premultiplied luminance mask; invert extension available. |
| `vello::Scene::reset` | function; `src/scene.rs:59` | equivalent | `Scene2D::reset; reset_and_resize; rerecord` — Clear/reset a handle with copy-on-write snapshot isolation. Unique command storage retains capacity, clones/subscenes remain unchanged. Rerecord validates atomically in a fresh command vector; not Vello mutable Encoding storage. |
| `vello::Scene::stroke` | function; `src/scene.rs:327` | equivalent | `Canvas2D::stroke_path; PathStyle; StrokeOptions` — User-space stroke geometry, caps/joins/miter/dashes and arbitrary brushes. |
| `vello::kurbo` | use; `src/lib.rs:140` | shared | `jadix_2d::kurbo` — See the complete dependency appendix; reexporting a library does not imply equivalent renderer behavior. |
| `vello::low_level` | module; `src/lib.rs:122` | partial | `gpu::low_level` — Typed compute/render recording executor and staged Jadix scenes, enabled by compute. Native schema; no Vello shader-buffer encoding compatibility. |
| `vello::low_level::BindType` | enum; `src/recording.rs:91` | equivalent | `gpu::low_level::BindType` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BindType` | use; `src/lib.rs:129` | equivalent | `gpu::low_level::BindType` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BindType::BufReadOnly` | variant; `src/recording.rs:95` | equivalent | `gpu::low_level::BindType::BufReadOnly` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BindType::Buffer` | variant; `src/recording.rs:93` | equivalent | `gpu::low_level::BindType::Buffer` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BindType::Image` | variant; `src/recording.rs:99` | equivalent | `gpu::low_level::BindType::Image` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BindType::Image::0` | struct_field; `src/recording.rs:99` | equivalent | `gpu::low_level::BindType::Image::0` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BindType::ImageRead` | variant; `src/recording.rs:101` | equivalent | `gpu::low_level::BindType::ImageRead` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BindType::ImageRead::0` | struct_field; `src/recording.rs:101` | equivalent | `gpu::low_level::BindType::ImageRead::0` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BindType::Uniform` | variant; `src/recording.rs:97` | equivalent | `gpu::low_level::BindType::Uniform` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BufferProxy` | struct; `src/recording.rs:31` | equivalent | `gpu::low_level::BufferProxy` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BufferProxy` | use; `src/lib.rs:129` | equivalent | `gpu::low_level::BufferProxy` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BufferProxy::id` | struct_field; `src/recording.rs:33` | equivalent | `gpu::low_level::BufferProxy::id` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BufferProxy::name` | struct_field; `src/recording.rs:34` | equivalent | `gpu::low_level::BufferProxy::name` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BufferProxy::new` | function; `src/recording.rs:236` | equivalent | `gpu::low_level::BufferProxy::new` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::BufferProxy::size` | struct_field; `src/recording.rs:32` | equivalent | `gpu::low_level::BufferProxy::size` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::Command` | enum; `src/recording.rs:64` | equivalent | `gpu::low_level::Command` — Actual ordered  command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command` | use; `src/lib.rs:129` | equivalent | `gpu::low_level::Command` — Actual ordered  command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Clear` | variant; `src/recording.rs:75` | equivalent | `gpu::low_level::Command::Clear` — Actual ordered Clear command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Clear::0` | struct_field; `src/recording.rs:75` | equivalent | `gpu::low_level::Command::Clear::0` — Actual ordered Clear command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Clear::1` | struct_field; `src/recording.rs:75` | equivalent | `gpu::low_level::Command::Clear::1` — Actual ordered Clear command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Clear::2` | struct_field; `src/recording.rs:75` | equivalent | `gpu::low_level::Command::Clear::2` — Actual ordered Clear command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Dispatch` | variant; `src/recording.rs:83` | equivalent | `gpu::low_level::Command::Dispatch` — Actual ordered Dispatch command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Dispatch::0` | struct_field; `src/recording.rs:83` | equivalent | `gpu::low_level::Command::Dispatch::0` — Actual ordered Dispatch command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Dispatch::1` | struct_field; `src/recording.rs:83` | equivalent | `gpu::low_level::Command::Dispatch::1` — Actual ordered Dispatch command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Dispatch::2` | struct_field; `src/recording.rs:83` | equivalent | `gpu::low_level::Command::Dispatch::2` — Actual ordered Dispatch command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::DispatchIndirect` | variant; `src/recording.rs:84` | equivalent | `gpu::low_level::Command::DispatchIndirect` — Actual ordered DispatchIndirect command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::DispatchIndirect::0` | struct_field; `src/recording.rs:84` | equivalent | `gpu::low_level::Command::DispatchIndirect::0` — Actual ordered DispatchIndirect command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::DispatchIndirect::1` | struct_field; `src/recording.rs:84` | equivalent | `gpu::low_level::Command::DispatchIndirect::1` — Actual ordered DispatchIndirect command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::DispatchIndirect::2` | struct_field; `src/recording.rs:84` | equivalent | `gpu::low_level::Command::DispatchIndirect::2` — Actual ordered DispatchIndirect command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::DispatchIndirect::3` | struct_field; `src/recording.rs:84` | equivalent | `gpu::low_level::Command::DispatchIndirect::3` — Actual ordered DispatchIndirect command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Download` | variant; `src/recording.rs:72` | equivalent | `gpu::low_level::Command::Download` — Actual ordered Download command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Download::0` | struct_field; `src/recording.rs:72` | equivalent | `gpu::low_level::Command::Download::0` — Actual ordered Download command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Draw` | variant; `src/recording.rs:86` | equivalent | `gpu::low_level::Command::Draw` — Actual ordered Draw command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Draw::0` | struct_field; `src/recording.rs:86` | equivalent | `gpu::low_level::Command::Draw::0` — Actual ordered Draw command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::FreeBuffer` | variant; `src/recording.rs:77` | equivalent | `gpu::low_level::Command::FreeBuffer` — Actual ordered FreeBuffer command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::FreeBuffer::0` | struct_field; `src/recording.rs:77` | equivalent | `gpu::low_level::Command::FreeBuffer::0` — Actual ordered FreeBuffer command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::FreeImage` | variant; `src/recording.rs:79` | equivalent | `gpu::low_level::Command::FreeImage` — Actual ordered FreeImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::FreeImage::0` | struct_field; `src/recording.rs:79` | equivalent | `gpu::low_level::Command::FreeImage::0` — Actual ordered FreeImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Upload` | variant; `src/recording.rs:66` | equivalent | `gpu::low_level::Command::Upload` — Actual ordered Upload command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Upload::0` | struct_field; `src/recording.rs:66` | equivalent | `gpu::low_level::Command::Upload::0` — Actual ordered Upload command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::Upload::1` | struct_field; `src/recording.rs:66` | equivalent | `gpu::low_level::Command::Upload::1` — Actual ordered Upload command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::UploadImage` | variant; `src/recording.rs:70` | equivalent | `gpu::low_level::Command::UploadImage` — Actual ordered UploadImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::UploadImage::0` | struct_field; `src/recording.rs:70` | equivalent | `gpu::low_level::Command::UploadImage::0` — Actual ordered UploadImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::UploadImage::1` | struct_field; `src/recording.rs:70` | equivalent | `gpu::low_level::Command::UploadImage::1` — Actual ordered UploadImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::UploadUniform` | variant; `src/recording.rs:68` | equivalent | `gpu::low_level::Command::UploadUniform` — Actual ordered UploadUniform command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::UploadUniform::0` | struct_field; `src/recording.rs:68` | equivalent | `gpu::low_level::Command::UploadUniform::0` — Actual ordered UploadUniform command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::UploadUniform::1` | struct_field; `src/recording.rs:68` | equivalent | `gpu::low_level::Command::UploadUniform::1` — Actual ordered UploadUniform command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::WriteImage` | variant; `src/recording.rs:71` | equivalent | `gpu::low_level::Command::WriteImage` — Actual ordered WriteImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::WriteImage::0` | struct_field; `src/recording.rs:71` | equivalent | `gpu::low_level::Command::WriteImage::0` — Actual ordered WriteImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::WriteImage::1` | struct_field; `src/recording.rs:71` | equivalent | `gpu::low_level::Command::WriteImage::1` — Actual ordered WriteImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::Command::WriteImage::2` | struct_field; `src/recording.rs:71` | equivalent | `gpu::low_level::Command::WriteImage::2` — Actual ordered WriteImage command in Engine::execute; preflight/budgets, retained resources and per-position download snapshots. Command schema differs; GPU validation is synchronous on native and asynchronous on browser. |
| `vello::low_level::FullShaders` | struct; `src/shaders.rs:19` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific FullShaders stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders` | use; `src/lib.rs:133` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific FullShaders stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::backdrop` | struct_field; `src/shaders.rs:33` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific backdrop stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::bbox_clear` | struct_field; `src/shaders.rs:25` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific bbox_clear stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::binning` | struct_field; `src/shaders.rs:31` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific binning stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::clip_leaf` | struct_field; `src/shaders.rs:30` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific clip_leaf stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::clip_reduce` | struct_field; `src/shaders.rs:29` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific clip_reduce stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::coarse` | struct_field; `src/shaders.rs:36` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific coarse stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::draw_leaf` | struct_field; `src/shaders.rs:28` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific draw_leaf stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::draw_reduce` | struct_field; `src/shaders.rs:27` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific draw_reduce stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::fine_area` | struct_field; `src/shaders.rs:39` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Jadix Area integrates triangle coverage in a render pipeline, not the Vello fine compute stream. |
| `vello::low_level::FullShaders::fine_msaa16` | struct_field; `src/shaders.rs:41` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Jadix sampled scenes include16 ordered samples with different sample positions/whole-scene composition; no Vello fine shader ABI. |
| `vello::low_level::FullShaders::fine_msaa8` | struct_field; `src/shaders.rs:40` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Jadix sampled_grid supports2x4 and4x2 ordered8 samples including clip/layers/HDR/filter resolve; sample positions and render pipeline differ from native Vello fine_msaa8 compute kernel and ABI. |
| `vello::low_level::FullShaders::flatten` | struct_field; `src/shaders.rs:26` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Jadix PreparedSceneGpu replays and tessellates curves on CPU; no encoded Vello flatten kernel. |
| `vello::low_level::FullShaders::path_count` | struct_field; `src/shaders.rs:35` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific path_count stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::path_count_setup` | struct_field; `src/shaders.rs:34` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific path_count_setup stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::path_tiling` | struct_field; `src/shaders.rs:38` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific path_tiling stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::path_tiling_setup` | struct_field; `src/shaders.rs:37` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific path_tiling_setup stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::pathtag_is_cpu` | struct_field; `src/shaders.rs:44` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Jadix CPU preparation is explicit; no Vello pathtag stage toggle. |
| `vello::low_level::FullShaders::pathtag_reduce` | struct_field; `src/shaders.rs:20` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific pathtag_reduce stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::pathtag_reduce2` | struct_field; `src/shaders.rs:21` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific pathtag_reduce2 stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::pathtag_scan` | struct_field; `src/shaders.rs:23` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific pathtag_scan stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::pathtag_scan1` | struct_field; `src/shaders.rs:22` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific pathtag_scan1 stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::pathtag_scan_large` | struct_field; `src/shaders.rs:24` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific pathtag_scan_large stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::FullShaders::tile_alloc` | struct_field; `src/shaders.rs:32` | different | `gpu::low_level::Engine::add_compute_shader; gpu::low_level::Render; gpu::AaQuality` — Vello-specific tile_alloc stage indexes its encoded scene/intermediate buffers. Jadix retains native CPU preparation, Area/render paths and user WGSL dispatch; this algorithm-specific stage is not implemented as a compatible built-in kernel. |
| `vello::low_level::ImageFormat` | enum; `src/recording.rs:38` | equivalent | `gpu::low_level::ImageFormat` — RGBA8/BGRA8 conversions have direct counterparts; Jadix additionally admits linear signed/HDR Rgba16Float. Format support still follows device features. |
| `vello::low_level::ImageFormat` | use; `src/lib.rs:129` | equivalent | `gpu::low_level::ImageFormat` — RGBA8/BGRA8 conversions have direct counterparts; Jadix additionally admits linear signed/HDR Rgba16Float. Format support still follows device features. |
| `vello::low_level::ImageFormat::Bgra8` | variant; `src/recording.rs:40` | equivalent | `gpu::low_level::ImageFormat::Bgra8` — RGBA8/BGRA8 conversions have direct counterparts; Jadix additionally admits linear signed/HDR Rgba16Float. Format support still follows device features. |
| `vello::low_level::ImageFormat::Rgba8` | variant; `src/recording.rs:39` | equivalent | `gpu::low_level::ImageFormat::Rgba8` — RGBA8/BGRA8 conversions have direct counterparts; Jadix additionally admits linear signed/HDR Rgba16Float. Format support still follows device features. |
| `vello::low_level::ImageFormat::from_wgpu` | function; `src/recording.rs:253` | equivalent | `gpu::low_level::ImageFormat::from_wgpu` — RGBA8/BGRA8 conversions have direct counterparts; Jadix additionally admits linear signed/HDR Rgba16Float. Format support still follows device features. |
| `vello::low_level::ImageFormat::to_wgpu` | function; `src/recording.rs:245` | equivalent | `gpu::low_level::ImageFormat::to_wgpu` — RGBA8/BGRA8 conversions have direct counterparts; Jadix additionally admits linear signed/HDR Rgba16Float. Format support still follows device features. |
| `vello::low_level::ImageProxy` | struct; `src/recording.rs:45` | equivalent | `gpu::low_level::ImageProxy` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ImageProxy` | use; `src/lib.rs:129` | equivalent | `gpu::low_level::ImageProxy` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ImageProxy::format` | struct_field; `src/recording.rs:48` | equivalent | `gpu::low_level::ImageProxy::format` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ImageProxy::height` | struct_field; `src/recording.rs:47` | equivalent | `gpu::low_level::ImageProxy::height` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ImageProxy::id` | struct_field; `src/recording.rs:49` | equivalent | `gpu::low_level::ImageProxy::id` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ImageProxy::new` | function; `src/recording.rs:263` | equivalent | `gpu::low_level::ImageProxy::new` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ImageProxy::width` | struct_field; `src/recording.rs:46` | equivalent | `gpu::low_level::ImageProxy::width` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::Recording` | struct; `src/recording.rs:25` | equivalent | `gpu::low_level::Recording` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::Recording` | use; `src/lib.rs:129` | equivalent | `gpu::low_level::Recording` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::Recording::clear_all` | function; `src/recording.rs:202` | equivalent | `gpu::low_level::Recording::clear_all` — Whole aligned logical buffer clear at its recorded command position. |
| `vello::low_level::Recording::commands` | struct_field; `src/recording.rs:26` | equivalent | `gpu::low_level::Recording::commands` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::Recording::dispatch` | function; `src/recording.rs:159` | equivalent | `gpu::low_level::Recording::dispatch` — Consecutive group-0 bindings and explicit direct workgroup counts, cached lazy pipeline. |
| `vello::low_level::Recording::dispatch_indirect` | function; `src/recording.rs:173` | equivalent | `gpu::low_level::Recording::dispatch_indirect` — Checked 4-byte aligned 12-byte indirect range with actual GPU indirect execution. |
| `vello::low_level::Recording::download` | function; `src/recording.rs:197` | equivalent | `gpu::low_level::Recording::download` — Ordered COPY_SRC snapshot to returned MAP_READ staging buffer, preserving multiple snapshots of one resource. |
| `vello::low_level::Recording::draw` | function; `src/recording.rs:189` | equivalent | `gpu::low_level::Recording::draw` — Actual vertex-index render pipeline dispatch mixed with compute; explicit DrawParams and output format, no Vello DrawParams binary identity. |
| `vello::low_level::Recording::free_buffer` | function; `src/recording.rs:207` | equivalent | `gpu::low_level::Recording::free_buffer` — Retained buffer release after earlier encoded commands; old submitted work keeps handles alive. |
| `vello::low_level::Recording::free_image` | function; `src/recording.rs:212` | equivalent | `gpu::low_level::Recording::free_image` — Retained image release after earlier encoded commands; exported texture clones remain independently alive. |
| `vello::low_level::Recording::free_resource` | function; `src/recording.rs:217` | equivalent | `gpu::low_level::Recording::free_resource` — Explicit typed release; range release frees the underlying allocation; tombstones reject stale proxies until reset. |
| `vello::low_level::Recording::into_commands` | function; `src/recording.rs:230` | equivalent | `gpu::low_level::Recording::into_commands` — Consumes recorder into ordered commands. |
| `vello::low_level::Recording::push` | function; `src/recording.rs:118` | equivalent | `gpu::low_level::Recording::push` — Public owned typed Command recording. |
| `vello::low_level::Recording::upload` | function; `src/recording.rs:124` | equivalent | `gpu::low_level::Recording::upload` — Ordered staging COPY_SRC uploads into retained buffers; encoder copies preserve writes separated by dispatches. |
| `vello::low_level::Recording::upload_image` | function; `src/recording.rs:142` | equivalent | `gpu::low_level::Recording::upload_image` — Explicit ImageProxy with exact tightly packed input, padded encoder copies; supports RGBA8/BGRA8/FP16. |
| `vello::low_level::Recording::upload_uniform` | function; `src/recording.rs:133` | equivalent | `gpu::low_level::Recording::upload_uniform` — Uniform buffer creation/upload with device binding size and alignment validation. |
| `vello::low_level::Recording::write_image` | function; `src/recording.rs:155` | equivalent | `gpu::low_level::Recording::write_image` — Checked origin/size subregion copy; exact data length and no row overflow. |
| `vello::low_level::Render` | struct; `src/render.rs:23` | partial | `gpu::low_level::Render` — Staged Jadix CPU scene preparation plus caller-encoded GPU fine drawing with retained FP16 output. No Vello Encoding/Resolver or compute stage graph. |
| `vello::low_level::Render` | use; `src/lib.rs:132` | partial | `gpu::low_level::Render` — Staged Jadix CPU scene preparation plus caller-encoded GPU fine drawing with retained FP16 output. No Vello Encoding/Resolver or compute stage graph. |
| `vello::low_level::Render::bump_buf` | function; `src/render.rs:639` | different | `gpu::low_level::Render::bump_estimate` — Checked CPU payload planning only; no GPU bump allocator buffer is exposed. Different allocation model, not a simulated Vello bump buffer. |
| `vello::low_level::Render::new` | function; `src/render.rs:121` | partial | `gpu::low_level::Render` — Staged Jadix CPU scene preparation plus caller-encoded GPU fine drawing with retained FP16 output. No Vello Encoding/Resolver or compute stage graph. |
| `vello::low_level::Render::out_image` | function; `src/render.rs:635` | partial | `gpu::low_level::Render::out_image` — Retained FP16 wgpu texture usable by Canvas imports or following GPU operations, rather than a Vello image proxy. |
| `vello::low_level::Render::record_fine` | function; `src/render.rs:560` | partial | `gpu::low_level::Render::record_fine` — Actual drawing encoded into caller-owned wgpu encoder. Submit before encoding another frame on this retained Render. Geometry/AA algorithms differ from Vello fine compute. |
| `vello::low_level::Render::render_encoding_coarse` | function; `src/render.rs:135` | partial | `gpu::low_level::Render::new; PreparedSceneGpu::new` — CPU replay/flatten/tessellation of Scene2D with explicit target/transform, retained for later record_fine. Does not accept Vello Encoding/Resolver. |
| `vello::low_level::Render::take_captured_buffers` | function; `src/render.rs:650` | partial | `gpu::low_level::Render::take_captured_scene` — One-time owned prepared-geometry snapshot and checked resource estimates. GPU buffer downloads are separately available through Recording/Execution; no Vello intermediate-stage dump layout. |
| `vello::low_level::ResourceId` | struct; `src/recording.rs:13` | equivalent | `gpu::low_level::ResourceId` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ResourceId` | use; `src/lib.rs:129` | equivalent | `gpu::low_level::ResourceId` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ResourceId::0` | struct_field; `src/recording.rs:13` | equivalent | `gpu::low_level::ResourceId::0` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ResourceId::next` | function; `src/recording.rs:16` | equivalent | `gpu::low_level::ResourceId::next` — Native Jadix typed resource/recording interface; purpose correspondence, not a stable binary layout or Vello buffer encoding. |
| `vello::low_level::ResourceProxy` | enum; `src/recording.rs:53` | equivalent | `gpu::low_level::ResourceProxy` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy` | use; `src/lib.rs:130` | equivalent | `gpu::low_level::ResourceProxy` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::Buffer` | variant; `src/recording.rs:54` | equivalent | `gpu::low_level::ResourceProxy::Buffer` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::Buffer::0` | struct_field; `src/recording.rs:54` | equivalent | `gpu::low_level::ResourceProxy::Buffer::0` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::BufferRange` | variant; `src/recording.rs:55` | equivalent | `gpu::low_level::ResourceProxy::BufferRange` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::BufferRange::offset` | struct_field; `src/recording.rs:57` | equivalent | `gpu::low_level::ResourceProxy::BufferRange::offset` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::BufferRange::proxy` | struct_field; `src/recording.rs:56` | equivalent | `gpu::low_level::ResourceProxy::BufferRange::proxy` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::BufferRange::size` | struct_field; `src/recording.rs:58` | equivalent | `gpu::low_level::ResourceProxy::BufferRange::size` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::Image` | variant; `src/recording.rs:60` | equivalent | `gpu::low_level::ResourceProxy::Image` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::Image::0` | struct_field; `src/recording.rs:60` | equivalent | `gpu::low_level::ResourceProxy::Image::0` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::as_buf` | function; `src/recording.rs:283` | equivalent | `gpu::low_level::ResourceProxy::as_buf` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::as_image` | function; `src/recording.rs:290` | equivalent | `gpu::low_level::ResourceProxy::as_image` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::new_buf` | function; `src/recording.rs:275` | equivalent | `gpu::low_level::ResourceProxy::new_buf` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ResourceProxy::new_image` | function; `src/recording.rs:279` | equivalent | `gpu::low_level::ResourceProxy::new_image` — Buffer/image/ranged bindings with checked offset, nonzero size, device limits and descriptor identity. Native Rust schema, explicit errors instead of unchecked getters. |
| `vello::low_level::ShaderId` | struct; `src/recording.rs:10` | equivalent | `gpu::low_level::ShaderId` — Engine-owned registered WGSL pipeline identifier with globally unique values; cannot use Vello FullShaders IDs. |
| `vello::low_level::ShaderId` | use; `src/lib.rs:130` | equivalent | `gpu::low_level::ShaderId` — Engine-owned registered WGSL pipeline identifier with globally unique values; cannot use Vello FullShaders IDs. |
| `vello::low_level::ShaderId::0` | struct_field; `src/recording.rs:10` | equivalent | `gpu::low_level::ShaderId::0` — Engine-owned registered WGSL pipeline identifier with globally unique values; cannot use Vello FullShaders IDs. |
| `vello::low_level::vello_encoding` | use; `src/lib.rs:135` | different | `Scene2D; PreparedSceneGpu; gpu::low_level::Recording` — Jadix native scene and command schemas. Vello encoding dependency is not reexported or decoded. |
| `vello::peniko` | use; `src/lib.rs:138` | partial | `Brush2D; GradientStops; Color2D` — See the complete dependency appendix; reexporting a library does not imply equivalent renderer behavior. |
| `vello::util` | module; `src/util.rs:4` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::DeviceHandle` | struct; `src/util.rs:21` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::DeviceHandle::adapter` | function; `src/util.rs:217` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::DeviceHandle::device` | struct_field; `src/util.rs:23` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::DeviceHandle::queue` | struct_field; `src/util.rs:24` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext` | struct; `src/util.rs:16` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::configure_surface` | function; `src/util.rs:139` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::create_render_surface` | function; `src/util.rs:69` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::create_surface` | function; `src/util.rs:52` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::device` | function; `src/util.rs:145` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::devices` | struct_field; `src/util.rs:18` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::instance` | struct_field; `src/util.rs:17` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::new` | function; `src/util.rs:32` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::resize_surface` | function; `src/util.rs:119` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderContext::set_present_mode` | function; `src/util.rs:130` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderSurface` | struct; `src/util.rs:223` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderSurface::blitter` | struct_field; `src/util.rs:230` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderSurface::config` | struct_field; `src/util.rs:225` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderSurface::dev_id` | struct_field; `src/util.rs:226` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderSurface::format` | struct_field; `src/util.rs:227` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderSurface::surface` | struct_field; `src/util.rs:224` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderSurface::target_texture` | struct_field; `src/util.rs:228` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::util::RenderSurface::target_view` | struct_field; `src/util.rs:229` | partial | `jadix_renderer; jadix host; wgpu` — Window/surface/device lifecycle belongs to Jadix host/renderer and shared wgpu. No identical RenderContext/RenderSurface convenience type. Not a Canvas drawing feature. |
| `vello::wgpu` | use; `src/lib.rs:145` | shared | `jadix_2d::wgpu` — See the complete dependency appendix; reexporting a library does not imply equivalent renderer behavior. |

## 依赖重导出的完整逐项附录

Vello 通过 Peniko 重导出 Color、Kurbo、资源句柄；wgpu 自身再重导出 wgpu-types。这些接口的完整列表分别在下表和同目录 JSON。Jadix 直接重导出同版本 Color 0.3.3/Kurbo 0.13.1；共享类型支持不自动意味着所有 Brush/Scene 路径都支持该类型的每一种输入。wgpu 是 Jadix 工作区 fork，对等类型名并不保证与 upstream Rust 类型兼容。

### peniko 0.6.1

195 项。Jadix 自有类型；Peniko borrowed brush、GPU 图像 override 已有功能入口；资源句柄仍没有逐类型兼容。 完整签名/条件属性/trait 见 `benchmarks/vello-api/inventory/peniko-0.6.1.json`。

| 项 | 类别 | Jadix 对应 | 状态 |
|---|---|---|---|
| `peniko` | module | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::BlendMode` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::BlendMode` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::BlendMode::compose` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::BlendMode::is_destructive` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::BlendMode::mix` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::BlendMode::new` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Blob` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::Gradient` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::Gradient::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::Image` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::Image::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::Solid` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::Solid::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::multiply_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::to_owned` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Brush::with_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::BrushRef` | type_alias | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::BrushRef` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Color` | type_alias | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStop` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStop` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStop::color` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStop::multiply_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStop::offset` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStop::with_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStops` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStops` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStops::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStops::new` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStopsSource` | trait | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStopsSource` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ColorStopsSource::collect_stops` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::Clear` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::Copy` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::Dest` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::DestAtop` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::DestIn` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::DestOut` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::DestOver` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::Plus` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::PlusLighter` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::SrcAtop` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::SrcIn` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::SrcOut` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::SrcOver` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Compose::Xor` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Extend` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Extend` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Extend::Pad` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Extend::Reflect` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Extend::Repeat` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Fill` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Fill` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Fill::EvenOdd` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Fill::NonZero` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::FontData` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::extend` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::hue_direction` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::interpolation_alpha_space` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::interpolation_cs` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::kind` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::multiply_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::new_linear` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::new_radial` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::new_sweep` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::new_two_point_radial` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::stops` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::with_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::with_extend` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::with_hue_direction` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::with_interpolation_alpha_space` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::with_interpolation_cs` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Gradient::with_stops` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::GradientKind` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::GradientKind` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::GradientKind::Linear` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::GradientKind::Linear::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::GradientKind::Radial` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::GradientKind::Radial::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::GradientKind::Sweep` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::GradientKind::Sweep::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageAlphaType` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageAlphaType` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageAlphaType::Alpha` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageAlphaType::AlphaPremultiplied` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::as_ref` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::image` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::multiply_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::new` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::sampler` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::to_owned` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::with_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::with_extend` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::with_quality` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::with_x_extend` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrush::with_y_extend` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrushRef` | type_alias | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageBrushRef` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageData` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageData` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageData::alpha_type` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageData::data` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageData::format` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageData::height` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageData::width` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageFormat` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageFormat` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageFormat::Bgra8` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageFormat::Rgba8` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageFormat::size_in_bytes` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageQuality` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageQuality` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageQuality::High` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageQuality::Low` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageQuality::Medium` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::alpha` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::multiply_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::new` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::quality` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::with_alpha` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::with_extend` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::with_quality` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::with_x_extend` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::with_y_extend` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::x_extend` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::ImageSampler::y_extend` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::InterpolationAlphaSpace` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::InterpolationAlphaSpace` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::InterpolationAlphaSpace::Premultiplied` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::InterpolationAlphaSpace::Unpremultiplied` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::LinearGradientPosition` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::LinearGradientPosition` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::LinearGradientPosition::end` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::LinearGradientPosition::new` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::LinearGradientPosition::start` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Color` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::ColorBurn` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::ColorDodge` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Darken` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Difference` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Exclusion` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::HardLight` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Hue` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Lighten` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Luminosity` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Multiply` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Normal` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Overlay` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Saturation` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::Screen` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Mix::SoftLight` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::RadialGradientPosition` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::RadialGradientPosition` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::RadialGradientPosition::end_center` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::RadialGradientPosition::end_radius` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::RadialGradientPosition::new` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::RadialGradientPosition::new_two_point` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::RadialGradientPosition::start_center` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::RadialGradientPosition::start_radius` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Style` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Style` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Style::Fill` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Style::Fill::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Style::Stroke` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::Style::Stroke::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::StyleRef` | enum | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::StyleRef` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::StyleRef::Fill` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::StyleRef::Fill::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::StyleRef::Stroke` | variant | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::StyleRef::Stroke::0` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::StyleRef::to_owned` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::SweepGradientPosition` | struct | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::SweepGradientPosition` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::SweepGradientPosition::center` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::SweepGradientPosition::end_angle` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::SweepGradientPosition::new` | function | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::SweepGradientPosition::start_angle` | struct_field | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::WeakBlob` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::color` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::kurbo` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
| `peniko::linebender_resource_handle` | use | `Brush2D / LayerOptions / GradientStops / Image2D` | partial |
### color 0.3.3

400 项。同版本真实重导出；Color2D 提供绘制/输出桥，只有明确转换的入口进入渲染。 完整签名/条件属性/trait 见 `benchmarks/vello-api/inventory/color-0.3.3.json`。

| 项 | 类别 | Jadix 对应 | 状态 |
|---|---|---|---|
| `color` | module | `jadix_2d::color_types` | shared |
| `color::A98Rgb` | struct | `jadix_2d::color_types::A98Rgb` | shared |
| `color::A98Rgb` | use | `jadix_2d::color_types::A98Rgb` | shared |
| `color::Aces2065_1` | struct | `jadix_2d::color_types::Aces2065_1` | shared |
| `color::Aces2065_1` | use | `jadix_2d::color_types::Aces2065_1` | shared |
| `color::AcesCg` | struct | `jadix_2d::color_types::AcesCg` | shared |
| `color::AcesCg` | use | `jadix_2d::color_types::AcesCg` | shared |
| `color::AlphaColor` | struct | `jadix_2d::color_types::AlphaColor` | shared |
| `color::AlphaColor` | use | `jadix_2d::color_types::AlphaColor` | shared |
| `color::AlphaColor::BLACK` | assoc_const | `jadix_2d::color_types::AlphaColor::BLACK` | shared |
| `color::AlphaColor::TRANSPARENT` | assoc_const | `jadix_2d::color_types::AlphaColor::TRANSPARENT` | shared |
| `color::AlphaColor::WHITE` | assoc_const | `jadix_2d::color_types::AlphaColor::WHITE` | shared |
| `color::AlphaColor::components` | struct_field | `jadix_2d::color_types::AlphaColor::components` | shared |
| `color::AlphaColor::convert` | function | `jadix_2d::color_types::AlphaColor::convert` | shared |
| `color::AlphaColor::cs` | struct_field | `jadix_2d::color_types::AlphaColor::cs` | shared |
| `color::AlphaColor::discard_alpha` | function | `jadix_2d::color_types::AlphaColor::discard_alpha` | shared |
| `color::AlphaColor::from_rgb8` | function | `jadix_2d::color_types::AlphaColor::from_rgb8` | shared |
| `color::AlphaColor::from_rgba8` | function | `jadix_2d::color_types::AlphaColor::from_rgba8` | shared |
| `color::AlphaColor::lerp` | function | `jadix_2d::color_types::AlphaColor::lerp` | shared |
| `color::AlphaColor::lerp_rect` | function | `jadix_2d::color_types::AlphaColor::lerp_rect` | shared |
| `color::AlphaColor::map` | function | `jadix_2d::color_types::AlphaColor::map` | shared |
| `color::AlphaColor::map_hue` | function | `jadix_2d::color_types::AlphaColor::map_hue` | shared |
| `color::AlphaColor::map_in` | function | `jadix_2d::color_types::AlphaColor::map_in` | shared |
| `color::AlphaColor::map_lightness` | function | `jadix_2d::color_types::AlphaColor::map_lightness` | shared |
| `color::AlphaColor::multiply_alpha` | function | `jadix_2d::color_types::AlphaColor::multiply_alpha` | shared |
| `color::AlphaColor::new` | function | `jadix_2d::color_types::AlphaColor::new` | shared |
| `color::AlphaColor::premultiply` | function | `jadix_2d::color_types::AlphaColor::premultiply` | shared |
| `color::AlphaColor::scale_chroma` | function | `jadix_2d::color_types::AlphaColor::scale_chroma` | shared |
| `color::AlphaColor::split` | function | `jadix_2d::color_types::AlphaColor::split` | shared |
| `color::AlphaColor::to_rgba8` | function | `jadix_2d::color_types::AlphaColor::to_rgba8` | shared |
| `color::AlphaColor::with_alpha` | function | `jadix_2d::color_types::AlphaColor::with_alpha` | shared |
| `color::Chromaticity` | struct | `jadix_2d::color_types::Chromaticity` | shared |
| `color::Chromaticity` | use | `jadix_2d::color_types::Chromaticity` | shared |
| `color::Chromaticity::ACES` | assoc_const | `jadix_2d::color_types::Chromaticity::ACES` | shared |
| `color::Chromaticity::D50` | assoc_const | `jadix_2d::color_types::Chromaticity::D50` | shared |
| `color::Chromaticity::D65` | assoc_const | `jadix_2d::color_types::Chromaticity::D65` | shared |
| `color::Chromaticity::x` | struct_field | `jadix_2d::color_types::Chromaticity::x` | shared |
| `color::Chromaticity::y` | struct_field | `jadix_2d::color_types::Chromaticity::y` | shared |
| `color::ColorSpace` | trait | `jadix_2d::color_types::ColorSpace` | shared |
| `color::ColorSpace` | use | `jadix_2d::color_types::ColorSpace` | shared |
| `color::ColorSpace::IS_LINEAR` | assoc_const | `jadix_2d::color_types::ColorSpace::IS_LINEAR` | shared |
| `color::ColorSpace::LAYOUT` | assoc_const | `jadix_2d::color_types::ColorSpace::LAYOUT` | shared |
| `color::ColorSpace::TAG` | assoc_const | `jadix_2d::color_types::ColorSpace::TAG` | shared |
| `color::ColorSpace::WHITE_COMPONENTS` | assoc_const | `jadix_2d::color_types::ColorSpace::WHITE_COMPONENTS` | shared |
| `color::ColorSpace::WHITE_POINT` | assoc_const | `jadix_2d::color_types::ColorSpace::WHITE_POINT` | shared |
| `color::ColorSpace::chromatically_adapt` | function | `jadix_2d::color_types::ColorSpace::chromatically_adapt` | shared |
| `color::ColorSpace::clip` | function | `jadix_2d::color_types::ColorSpace::clip` | shared |
| `color::ColorSpace::convert` | function | `jadix_2d::color_types::ColorSpace::convert` | shared |
| `color::ColorSpace::convert_absolute` | function | `jadix_2d::color_types::ColorSpace::convert_absolute` | shared |
| `color::ColorSpace::from_linear_srgb` | function | `jadix_2d::color_types::ColorSpace::from_linear_srgb` | shared |
| `color::ColorSpace::from_linear_srgb_absolute` | function | `jadix_2d::color_types::ColorSpace::from_linear_srgb_absolute` | shared |
| `color::ColorSpace::scale_chroma` | function | `jadix_2d::color_types::ColorSpace::scale_chroma` | shared |
| `color::ColorSpace::to_linear_srgb` | function | `jadix_2d::color_types::ColorSpace::to_linear_srgb` | shared |
| `color::ColorSpace::to_linear_srgb_absolute` | function | `jadix_2d::color_types::ColorSpace::to_linear_srgb_absolute` | shared |
| `color::ColorSpaceLayout` | enum | `jadix_2d::color_types::ColorSpaceLayout` | shared |
| `color::ColorSpaceLayout` | use | `jadix_2d::color_types::ColorSpaceLayout` | shared |
| `color::ColorSpaceLayout::HueFirst` | variant | `jadix_2d::color_types::ColorSpaceLayout::HueFirst` | shared |
| `color::ColorSpaceLayout::HueThird` | variant | `jadix_2d::color_types::ColorSpaceLayout::HueThird` | shared |
| `color::ColorSpaceLayout::Rectangular` | variant | `jadix_2d::color_types::ColorSpaceLayout::Rectangular` | shared |
| `color::ColorSpaceTag` | enum | `jadix_2d::color_types::ColorSpaceTag` | shared |
| `color::ColorSpaceTag` | use | `jadix_2d::color_types::ColorSpaceTag` | shared |
| `color::ColorSpaceTag::A98Rgb` | variant | `jadix_2d::color_types::ColorSpaceTag::A98Rgb` | shared |
| `color::ColorSpaceTag::Aces2065_1` | variant | `jadix_2d::color_types::ColorSpaceTag::Aces2065_1` | shared |
| `color::ColorSpaceTag::AcesCg` | variant | `jadix_2d::color_types::ColorSpaceTag::AcesCg` | shared |
| `color::ColorSpaceTag::DisplayP3` | variant | `jadix_2d::color_types::ColorSpaceTag::DisplayP3` | shared |
| `color::ColorSpaceTag::Hsl` | variant | `jadix_2d::color_types::ColorSpaceTag::Hsl` | shared |
| `color::ColorSpaceTag::Hwb` | variant | `jadix_2d::color_types::ColorSpaceTag::Hwb` | shared |
| `color::ColorSpaceTag::Lab` | variant | `jadix_2d::color_types::ColorSpaceTag::Lab` | shared |
| `color::ColorSpaceTag::Lch` | variant | `jadix_2d::color_types::ColorSpaceTag::Lch` | shared |
| `color::ColorSpaceTag::LinearSrgb` | variant | `jadix_2d::color_types::ColorSpaceTag::LinearSrgb` | shared |
| `color::ColorSpaceTag::Oklab` | variant | `jadix_2d::color_types::ColorSpaceTag::Oklab` | shared |
| `color::ColorSpaceTag::Oklch` | variant | `jadix_2d::color_types::ColorSpaceTag::Oklch` | shared |
| `color::ColorSpaceTag::ProphotoRgb` | variant | `jadix_2d::color_types::ColorSpaceTag::ProphotoRgb` | shared |
| `color::ColorSpaceTag::Rec2020` | variant | `jadix_2d::color_types::ColorSpaceTag::Rec2020` | shared |
| `color::ColorSpaceTag::Srgb` | variant | `jadix_2d::color_types::ColorSpaceTag::Srgb` | shared |
| `color::ColorSpaceTag::XyzD50` | variant | `jadix_2d::color_types::ColorSpaceTag::XyzD50` | shared |
| `color::ColorSpaceTag::XyzD65` | variant | `jadix_2d::color_types::ColorSpaceTag::XyzD65` | shared |
| `color::ColorSpaceTag::chromatically_adapt` | function | `jadix_2d::color_types::ColorSpaceTag::chromatically_adapt` | shared |
| `color::ColorSpaceTag::clip` | function | `jadix_2d::color_types::ColorSpaceTag::clip` | shared |
| `color::ColorSpaceTag::convert` | function | `jadix_2d::color_types::ColorSpaceTag::convert` | shared |
| `color::ColorSpaceTag::convert_absolute` | function | `jadix_2d::color_types::ColorSpaceTag::convert_absolute` | shared |
| `color::ColorSpaceTag::from_linear_srgb` | function | `jadix_2d::color_types::ColorSpaceTag::from_linear_srgb` | shared |
| `color::ColorSpaceTag::from_linear_srgb_absolute` | function | `jadix_2d::color_types::ColorSpaceTag::from_linear_srgb_absolute` | shared |
| `color::ColorSpaceTag::scale_chroma` | function | `jadix_2d::color_types::ColorSpaceTag::scale_chroma` | shared |
| `color::ColorSpaceTag::to_linear_srgb` | function | `jadix_2d::color_types::ColorSpaceTag::to_linear_srgb` | shared |
| `color::ColorSpaceTag::to_linear_srgb_absolute` | function | `jadix_2d::color_types::ColorSpaceTag::to_linear_srgb_absolute` | shared |
| `color::DisplayP3` | struct | `jadix_2d::color_types::DisplayP3` | shared |
| `color::DisplayP3` | use | `jadix_2d::color_types::DisplayP3` | shared |
| `color::DynamicColor` | struct | `jadix_2d::color_types::DynamicColor` | shared |
| `color::DynamicColor` | use | `jadix_2d::color_types::DynamicColor` | shared |
| `color::DynamicColor::chromatically_adapt` | function | `jadix_2d::color_types::DynamicColor::chromatically_adapt` | shared |
| `color::DynamicColor::clip` | function | `jadix_2d::color_types::DynamicColor::clip` | shared |
| `color::DynamicColor::components` | struct_field | `jadix_2d::color_types::DynamicColor::components` | shared |
| `color::DynamicColor::convert` | function | `jadix_2d::color_types::DynamicColor::convert` | shared |
| `color::DynamicColor::convert_absolute` | function | `jadix_2d::color_types::DynamicColor::convert_absolute` | shared |
| `color::DynamicColor::cs` | struct_field | `jadix_2d::color_types::DynamicColor::cs` | shared |
| `color::DynamicColor::flags` | struct_field | `jadix_2d::color_types::DynamicColor::flags` | shared |
| `color::DynamicColor::from_alpha_color` | function | `jadix_2d::color_types::DynamicColor::from_alpha_color` | shared |
| `color::DynamicColor::interpolate` | function | `jadix_2d::color_types::DynamicColor::interpolate` | shared |
| `color::DynamicColor::interpolate_unpremultiplied` | function | `jadix_2d::color_types::DynamicColor::interpolate_unpremultiplied` | shared |
| `color::DynamicColor::map` | function | `jadix_2d::color_types::DynamicColor::map` | shared |
| `color::DynamicColor::map_hue` | function | `jadix_2d::color_types::DynamicColor::map_hue` | shared |
| `color::DynamicColor::map_in` | function | `jadix_2d::color_types::DynamicColor::map_in` | shared |
| `color::DynamicColor::map_lightness` | function | `jadix_2d::color_types::DynamicColor::map_lightness` | shared |
| `color::DynamicColor::multiply_alpha` | function | `jadix_2d::color_types::DynamicColor::multiply_alpha` | shared |
| `color::DynamicColor::relative_luminance` | function | `jadix_2d::color_types::DynamicColor::relative_luminance` | shared |
| `color::DynamicColor::scale_chroma` | function | `jadix_2d::color_types::DynamicColor::scale_chroma` | shared |
| `color::DynamicColor::to_alpha_color` | function | `jadix_2d::color_types::DynamicColor::to_alpha_color` | shared |
| `color::DynamicColor::with_alpha` | function | `jadix_2d::color_types::DynamicColor::with_alpha` | shared |
| `color::Flags` | struct | `jadix_2d::color_types::Flags` | shared |
| `color::Flags` | use | `jadix_2d::color_types::Flags` | shared |
| `color::Flags::color_name` | function | `jadix_2d::color_types::Flags::color_name` | shared |
| `color::Flags::discard_name` | function | `jadix_2d::color_types::Flags::discard_name` | shared |
| `color::Flags::from_missing` | function | `jadix_2d::color_types::Flags::from_missing` | shared |
| `color::Flags::missing` | function | `jadix_2d::color_types::Flags::missing` | shared |
| `color::Flags::named` | function | `jadix_2d::color_types::Flags::named` | shared |
| `color::Flags::set_missing` | function | `jadix_2d::color_types::Flags::set_missing` | shared |
| `color::GradientIter` | struct | `jadix_2d::color_types::GradientIter` | shared |
| `color::GradientIter` | use | `jadix_2d::color_types::GradientIter` | shared |
| `color::Hsl` | struct | `jadix_2d::color_types::Hsl` | shared |
| `color::Hsl` | use | `jadix_2d::color_types::Hsl` | shared |
| `color::HueDirection` | enum | `jadix_2d::color_types::HueDirection` | shared |
| `color::HueDirection` | use | `jadix_2d::color_types::HueDirection` | shared |
| `color::HueDirection::Decreasing` | variant | `jadix_2d::color_types::HueDirection::Decreasing` | shared |
| `color::HueDirection::Increasing` | variant | `jadix_2d::color_types::HueDirection::Increasing` | shared |
| `color::HueDirection::Longer` | variant | `jadix_2d::color_types::HueDirection::Longer` | shared |
| `color::HueDirection::Shorter` | variant | `jadix_2d::color_types::HueDirection::Shorter` | shared |
| `color::Hwb` | struct | `jadix_2d::color_types::Hwb` | shared |
| `color::Hwb` | use | `jadix_2d::color_types::Hwb` | shared |
| `color::Interpolator` | struct | `jadix_2d::color_types::Interpolator` | shared |
| `color::Interpolator` | use | `jadix_2d::color_types::Interpolator` | shared |
| `color::Interpolator::eval` | function | `jadix_2d::color_types::Interpolator::eval` | shared |
| `color::Lab` | struct | `jadix_2d::color_types::Lab` | shared |
| `color::Lab` | use | `jadix_2d::color_types::Lab` | shared |
| `color::Lch` | struct | `jadix_2d::color_types::Lch` | shared |
| `color::Lch` | use | `jadix_2d::color_types::Lch` | shared |
| `color::LinearSrgb` | struct | `jadix_2d::color_types::LinearSrgb` | shared |
| `color::LinearSrgb` | use | `jadix_2d::color_types::LinearSrgb` | shared |
| `color::Missing` | struct | `jadix_2d::color_types::Missing` | shared |
| `color::Missing` | use | `jadix_2d::color_types::Missing` | shared |
| `color::Missing::EMPTY` | assoc_const | `jadix_2d::color_types::Missing::EMPTY` | shared |
| `color::Missing::contains` | function | `jadix_2d::color_types::Missing::contains` | shared |
| `color::Missing::insert` | function | `jadix_2d::color_types::Missing::insert` | shared |
| `color::Missing::is_empty` | function | `jadix_2d::color_types::Missing::is_empty` | shared |
| `color::Missing::single` | function | `jadix_2d::color_types::Missing::single` | shared |
| `color::Oklab` | struct | `jadix_2d::color_types::Oklab` | shared |
| `color::Oklab` | use | `jadix_2d::color_types::Oklab` | shared |
| `color::Oklch` | struct | `jadix_2d::color_types::Oklch` | shared |
| `color::Oklch` | use | `jadix_2d::color_types::Oklch` | shared |
| `color::OpaqueColor` | struct | `jadix_2d::color_types::OpaqueColor` | shared |
| `color::OpaqueColor` | use | `jadix_2d::color_types::OpaqueColor` | shared |
| `color::OpaqueColor::BLACK` | assoc_const | `jadix_2d::color_types::OpaqueColor::BLACK` | shared |
| `color::OpaqueColor::WHITE` | assoc_const | `jadix_2d::color_types::OpaqueColor::WHITE` | shared |
| `color::OpaqueColor::components` | struct_field | `jadix_2d::color_types::OpaqueColor::components` | shared |
| `color::OpaqueColor::convert` | function | `jadix_2d::color_types::OpaqueColor::convert` | shared |
| `color::OpaqueColor::cs` | struct_field | `jadix_2d::color_types::OpaqueColor::cs` | shared |
| `color::OpaqueColor::difference` | function | `jadix_2d::color_types::OpaqueColor::difference` | shared |
| `color::OpaqueColor::fixup_hues` | function | `jadix_2d::color_types::OpaqueColor::fixup_hues` | shared |
| `color::OpaqueColor::from_rgb8` | function | `jadix_2d::color_types::OpaqueColor::from_rgb8` | shared |
| `color::OpaqueColor::lerp` | function | `jadix_2d::color_types::OpaqueColor::lerp` | shared |
| `color::OpaqueColor::lerp_rect` | function | `jadix_2d::color_types::OpaqueColor::lerp_rect` | shared |
| `color::OpaqueColor::map` | function | `jadix_2d::color_types::OpaqueColor::map` | shared |
| `color::OpaqueColor::map_hue` | function | `jadix_2d::color_types::OpaqueColor::map_hue` | shared |
| `color::OpaqueColor::map_in` | function | `jadix_2d::color_types::OpaqueColor::map_in` | shared |
| `color::OpaqueColor::map_lightness` | function | `jadix_2d::color_types::OpaqueColor::map_lightness` | shared |
| `color::OpaqueColor::new` | function | `jadix_2d::color_types::OpaqueColor::new` | shared |
| `color::OpaqueColor::relative_luminance` | function | `jadix_2d::color_types::OpaqueColor::relative_luminance` | shared |
| `color::OpaqueColor::scale_chroma` | function | `jadix_2d::color_types::OpaqueColor::scale_chroma` | shared |
| `color::OpaqueColor::to_rgba8` | function | `jadix_2d::color_types::OpaqueColor::to_rgba8` | shared |
| `color::OpaqueColor::with_alpha` | function | `jadix_2d::color_types::OpaqueColor::with_alpha` | shared |
| `color::ParseError` | enum | `jadix_2d::color_types::ParseError` | shared |
| `color::ParseError` | use | `jadix_2d::color_types::ParseError` | shared |
| `color::ParseError::ExpectedArguments` | variant | `jadix_2d::color_types::ParseError::ExpectedArguments` | shared |
| `color::ParseError::ExpectedClosingParenthesis` | variant | `jadix_2d::color_types::ParseError::ExpectedClosingParenthesis` | shared |
| `color::ParseError::ExpectedColorSpaceIdentifier` | variant | `jadix_2d::color_types::ParseError::ExpectedColorSpaceIdentifier` | shared |
| `color::ParseError::ExpectedComma` | variant | `jadix_2d::color_types::ParseError::ExpectedComma` | shared |
| `color::ParseError::ExpectedEndOfString` | variant | `jadix_2d::color_types::ParseError::ExpectedEndOfString` | shared |
| `color::ParseError::UnclosedComment` | variant | `jadix_2d::color_types::ParseError::UnclosedComment` | shared |
| `color::ParseError::UnknownAngle` | variant | `jadix_2d::color_types::ParseError::UnknownAngle` | shared |
| `color::ParseError::UnknownAngleDimension` | variant | `jadix_2d::color_types::ParseError::UnknownAngleDimension` | shared |
| `color::ParseError::UnknownColorComponent` | variant | `jadix_2d::color_types::ParseError::UnknownColorComponent` | shared |
| `color::ParseError::UnknownColorIdentifier` | variant | `jadix_2d::color_types::ParseError::UnknownColorIdentifier` | shared |
| `color::ParseError::UnknownColorSpace` | variant | `jadix_2d::color_types::ParseError::UnknownColorSpace` | shared |
| `color::ParseError::UnknownColorSyntax` | variant | `jadix_2d::color_types::ParseError::UnknownColorSyntax` | shared |
| `color::ParseError::WrongNumberOfHexDigits` | variant | `jadix_2d::color_types::ParseError::WrongNumberOfHexDigits` | shared |
| `color::PremulColor` | struct | `jadix_2d::color_types::PremulColor` | shared |
| `color::PremulColor` | use | `jadix_2d::color_types::PremulColor` | shared |
| `color::PremulColor::BLACK` | assoc_const | `jadix_2d::color_types::PremulColor::BLACK` | shared |
| `color::PremulColor::TRANSPARENT` | assoc_const | `jadix_2d::color_types::PremulColor::TRANSPARENT` | shared |
| `color::PremulColor::WHITE` | assoc_const | `jadix_2d::color_types::PremulColor::WHITE` | shared |
| `color::PremulColor::components` | struct_field | `jadix_2d::color_types::PremulColor::components` | shared |
| `color::PremulColor::convert` | function | `jadix_2d::color_types::PremulColor::convert` | shared |
| `color::PremulColor::cs` | struct_field | `jadix_2d::color_types::PremulColor::cs` | shared |
| `color::PremulColor::difference` | function | `jadix_2d::color_types::PremulColor::difference` | shared |
| `color::PremulColor::discard_alpha` | function | `jadix_2d::color_types::PremulColor::discard_alpha` | shared |
| `color::PremulColor::fixup_hues` | function | `jadix_2d::color_types::PremulColor::fixup_hues` | shared |
| `color::PremulColor::from_rgb8` | function | `jadix_2d::color_types::PremulColor::from_rgb8` | shared |
| `color::PremulColor::from_rgba8` | function | `jadix_2d::color_types::PremulColor::from_rgba8` | shared |
| `color::PremulColor::lerp` | function | `jadix_2d::color_types::PremulColor::lerp` | shared |
| `color::PremulColor::lerp_rect` | function | `jadix_2d::color_types::PremulColor::lerp_rect` | shared |
| `color::PremulColor::multiply_alpha` | function | `jadix_2d::color_types::PremulColor::multiply_alpha` | shared |
| `color::PremulColor::new` | function | `jadix_2d::color_types::PremulColor::new` | shared |
| `color::PremulColor::to_rgba8` | function | `jadix_2d::color_types::PremulColor::to_rgba8` | shared |
| `color::PremulColor::un_premultiply` | function | `jadix_2d::color_types::PremulColor::un_premultiply` | shared |
| `color::PremulRgba8` | struct | `jadix_2d::color_types::PremulRgba8` | shared |
| `color::PremulRgba8` | use | `jadix_2d::color_types::PremulRgba8` | shared |
| `color::PremulRgba8::a` | struct_field | `jadix_2d::color_types::PremulRgba8::a` | shared |
| `color::PremulRgba8::b` | struct_field | `jadix_2d::color_types::PremulRgba8::b` | shared |
| `color::PremulRgba8::from_u32` | function | `jadix_2d::color_types::PremulRgba8::from_u32` | shared |
| `color::PremulRgba8::from_u8_array` | function | `jadix_2d::color_types::PremulRgba8::from_u8_array` | shared |
| `color::PremulRgba8::g` | struct_field | `jadix_2d::color_types::PremulRgba8::g` | shared |
| `color::PremulRgba8::r` | struct_field | `jadix_2d::color_types::PremulRgba8::r` | shared |
| `color::PremulRgba8::to_u32` | function | `jadix_2d::color_types::PremulRgba8::to_u32` | shared |
| `color::PremulRgba8::to_u8_array` | function | `jadix_2d::color_types::PremulRgba8::to_u8_array` | shared |
| `color::ProphotoRgb` | struct | `jadix_2d::color_types::ProphotoRgb` | shared |
| `color::ProphotoRgb` | use | `jadix_2d::color_types::ProphotoRgb` | shared |
| `color::Rec2020` | struct | `jadix_2d::color_types::Rec2020` | shared |
| `color::Rec2020` | use | `jadix_2d::color_types::Rec2020` | shared |
| `color::Rgba8` | struct | `jadix_2d::color_types::Rgba8` | shared |
| `color::Rgba8` | use | `jadix_2d::color_types::Rgba8` | shared |
| `color::Rgba8::a` | struct_field | `jadix_2d::color_types::Rgba8::a` | shared |
| `color::Rgba8::b` | struct_field | `jadix_2d::color_types::Rgba8::b` | shared |
| `color::Rgba8::from_u32` | function | `jadix_2d::color_types::Rgba8::from_u32` | shared |
| `color::Rgba8::from_u8_array` | function | `jadix_2d::color_types::Rgba8::from_u8_array` | shared |
| `color::Rgba8::g` | struct_field | `jadix_2d::color_types::Rgba8::g` | shared |
| `color::Rgba8::r` | struct_field | `jadix_2d::color_types::Rgba8::r` | shared |
| `color::Rgba8::to_u32` | function | `jadix_2d::color_types::Rgba8::to_u32` | shared |
| `color::Rgba8::to_u8_array` | function | `jadix_2d::color_types::Rgba8::to_u8_array` | shared |
| `color::Srgb` | struct | `jadix_2d::color_types::Srgb` | shared |
| `color::Srgb` | use | `jadix_2d::color_types::Srgb` | shared |
| `color::UnpremultipliedGradientIter` | struct | `jadix_2d::color_types::UnpremultipliedGradientIter` | shared |
| `color::UnpremultipliedGradientIter` | use | `jadix_2d::color_types::UnpremultipliedGradientIter` | shared |
| `color::UnpremultipliedInterpolator` | struct | `jadix_2d::color_types::UnpremultipliedInterpolator` | shared |
| `color::UnpremultipliedInterpolator` | use | `jadix_2d::color_types::UnpremultipliedInterpolator` | shared |
| `color::UnpremultipliedInterpolator::eval` | function | `jadix_2d::color_types::UnpremultipliedInterpolator::eval` | shared |
| `color::XyzD50` | struct | `jadix_2d::color_types::XyzD50` | shared |
| `color::XyzD50` | use | `jadix_2d::color_types::XyzD50` | shared |
| `color::XyzD65` | struct | `jadix_2d::color_types::XyzD65` | shared |
| `color::XyzD65` | use | `jadix_2d::color_types::XyzD65` | shared |
| `color::cache_key` | module | `jadix_2d::color_types::cache_key` | shared |
| `color::cache_key::BitEq` | trait | `jadix_2d::color_types::cache_key::BitEq` | shared |
| `color::cache_key::BitEq::bit_eq` | function | `jadix_2d::color_types::cache_key::BitEq::bit_eq` | shared |
| `color::cache_key::BitHash` | trait | `jadix_2d::color_types::cache_key::BitHash` | shared |
| `color::cache_key::BitHash::bit_hash` | function | `jadix_2d::color_types::cache_key::BitHash::bit_hash` | shared |
| `color::cache_key::CacheKey` | struct | `jadix_2d::color_types::cache_key::CacheKey` | shared |
| `color::cache_key::CacheKey::0` | struct_field | `jadix_2d::color_types::cache_key::CacheKey::0` | shared |
| `color::cache_key::CacheKey::into_inner` | function | `jadix_2d::color_types::cache_key::CacheKey::into_inner` | shared |
| `color::cache_key::CacheKey::new` | function | `jadix_2d::color_types::cache_key::CacheKey::new` | shared |
| `color::gradient` | function | `jadix_2d::color_types::gradient` | shared |
| `color::gradient` | use | `jadix_2d::color_types::gradient` | shared |
| `color::gradient_unpremultiplied` | function | `jadix_2d::color_types::gradient_unpremultiplied` | shared |
| `color::gradient_unpremultiplied` | use | `jadix_2d::color_types::gradient_unpremultiplied` | shared |
| `color::palette` | module | `jadix_2d::color_types::palette` | shared |
| `color::palette::css` | module | `jadix_2d::color_types::palette::css` | shared |
| `color::palette::css::ALICE_BLUE` | constant | `jadix_2d::color_types::palette::css::ALICE_BLUE` | shared |
| `color::palette::css::ANTIQUE_WHITE` | constant | `jadix_2d::color_types::palette::css::ANTIQUE_WHITE` | shared |
| `color::palette::css::AQUA` | constant | `jadix_2d::color_types::palette::css::AQUA` | shared |
| `color::palette::css::AQUAMARINE` | constant | `jadix_2d::color_types::palette::css::AQUAMARINE` | shared |
| `color::palette::css::AZURE` | constant | `jadix_2d::color_types::palette::css::AZURE` | shared |
| `color::palette::css::BEIGE` | constant | `jadix_2d::color_types::palette::css::BEIGE` | shared |
| `color::palette::css::BISQUE` | constant | `jadix_2d::color_types::palette::css::BISQUE` | shared |
| `color::palette::css::BLACK` | constant | `jadix_2d::color_types::palette::css::BLACK` | shared |
| `color::palette::css::BLANCHED_ALMOND` | constant | `jadix_2d::color_types::palette::css::BLANCHED_ALMOND` | shared |
| `color::palette::css::BLUE` | constant | `jadix_2d::color_types::palette::css::BLUE` | shared |
| `color::palette::css::BLUE_VIOLET` | constant | `jadix_2d::color_types::palette::css::BLUE_VIOLET` | shared |
| `color::palette::css::BROWN` | constant | `jadix_2d::color_types::palette::css::BROWN` | shared |
| `color::palette::css::BURLYWOOD` | constant | `jadix_2d::color_types::palette::css::BURLYWOOD` | shared |
| `color::palette::css::CADET_BLUE` | constant | `jadix_2d::color_types::palette::css::CADET_BLUE` | shared |
| `color::palette::css::CHARTREUSE` | constant | `jadix_2d::color_types::palette::css::CHARTREUSE` | shared |
| `color::palette::css::CHOCOLATE` | constant | `jadix_2d::color_types::palette::css::CHOCOLATE` | shared |
| `color::palette::css::CORAL` | constant | `jadix_2d::color_types::palette::css::CORAL` | shared |
| `color::palette::css::CORNFLOWER_BLUE` | constant | `jadix_2d::color_types::palette::css::CORNFLOWER_BLUE` | shared |
| `color::palette::css::CORNSILK` | constant | `jadix_2d::color_types::palette::css::CORNSILK` | shared |
| `color::palette::css::CRIMSON` | constant | `jadix_2d::color_types::palette::css::CRIMSON` | shared |
| `color::palette::css::CYAN` | constant | `jadix_2d::color_types::palette::css::CYAN` | shared |
| `color::palette::css::DARK_BLUE` | constant | `jadix_2d::color_types::palette::css::DARK_BLUE` | shared |
| `color::palette::css::DARK_CYAN` | constant | `jadix_2d::color_types::palette::css::DARK_CYAN` | shared |
| `color::palette::css::DARK_GOLDENROD` | constant | `jadix_2d::color_types::palette::css::DARK_GOLDENROD` | shared |
| `color::palette::css::DARK_GRAY` | constant | `jadix_2d::color_types::palette::css::DARK_GRAY` | shared |
| `color::palette::css::DARK_GREEN` | constant | `jadix_2d::color_types::palette::css::DARK_GREEN` | shared |
| `color::palette::css::DARK_KHAKI` | constant | `jadix_2d::color_types::palette::css::DARK_KHAKI` | shared |
| `color::palette::css::DARK_MAGENTA` | constant | `jadix_2d::color_types::palette::css::DARK_MAGENTA` | shared |
| `color::palette::css::DARK_OLIVE_GREEN` | constant | `jadix_2d::color_types::palette::css::DARK_OLIVE_GREEN` | shared |
| `color::palette::css::DARK_ORANGE` | constant | `jadix_2d::color_types::palette::css::DARK_ORANGE` | shared |
| `color::palette::css::DARK_ORCHID` | constant | `jadix_2d::color_types::palette::css::DARK_ORCHID` | shared |
| `color::palette::css::DARK_RED` | constant | `jadix_2d::color_types::palette::css::DARK_RED` | shared |
| `color::palette::css::DARK_SALMON` | constant | `jadix_2d::color_types::palette::css::DARK_SALMON` | shared |
| `color::palette::css::DARK_SEA_GREEN` | constant | `jadix_2d::color_types::palette::css::DARK_SEA_GREEN` | shared |
| `color::palette::css::DARK_SLATE_BLUE` | constant | `jadix_2d::color_types::palette::css::DARK_SLATE_BLUE` | shared |
| `color::palette::css::DARK_SLATE_GRAY` | constant | `jadix_2d::color_types::palette::css::DARK_SLATE_GRAY` | shared |
| `color::palette::css::DARK_TURQUOISE` | constant | `jadix_2d::color_types::palette::css::DARK_TURQUOISE` | shared |
| `color::palette::css::DARK_VIOLET` | constant | `jadix_2d::color_types::palette::css::DARK_VIOLET` | shared |
| `color::palette::css::DEEP_PINK` | constant | `jadix_2d::color_types::palette::css::DEEP_PINK` | shared |
| `color::palette::css::DEEP_SKY_BLUE` | constant | `jadix_2d::color_types::palette::css::DEEP_SKY_BLUE` | shared |
| `color::palette::css::DIM_GRAY` | constant | `jadix_2d::color_types::palette::css::DIM_GRAY` | shared |
| `color::palette::css::DODGER_BLUE` | constant | `jadix_2d::color_types::palette::css::DODGER_BLUE` | shared |
| `color::palette::css::FIREBRICK` | constant | `jadix_2d::color_types::palette::css::FIREBRICK` | shared |
| `color::palette::css::FLORAL_WHITE` | constant | `jadix_2d::color_types::palette::css::FLORAL_WHITE` | shared |
| `color::palette::css::FOREST_GREEN` | constant | `jadix_2d::color_types::palette::css::FOREST_GREEN` | shared |
| `color::palette::css::FUCHSIA` | constant | `jadix_2d::color_types::palette::css::FUCHSIA` | shared |
| `color::palette::css::GAINSBORO` | constant | `jadix_2d::color_types::palette::css::GAINSBORO` | shared |
| `color::palette::css::GHOST_WHITE` | constant | `jadix_2d::color_types::palette::css::GHOST_WHITE` | shared |
| `color::palette::css::GOLD` | constant | `jadix_2d::color_types::palette::css::GOLD` | shared |
| `color::palette::css::GOLDENROD` | constant | `jadix_2d::color_types::palette::css::GOLDENROD` | shared |
| `color::palette::css::GRAY` | constant | `jadix_2d::color_types::palette::css::GRAY` | shared |
| `color::palette::css::GREEN` | constant | `jadix_2d::color_types::palette::css::GREEN` | shared |
| `color::palette::css::GREEN_YELLOW` | constant | `jadix_2d::color_types::palette::css::GREEN_YELLOW` | shared |
| `color::palette::css::HONEYDEW` | constant | `jadix_2d::color_types::palette::css::HONEYDEW` | shared |
| `color::palette::css::HOT_PINK` | constant | `jadix_2d::color_types::palette::css::HOT_PINK` | shared |
| `color::palette::css::INDIAN_RED` | constant | `jadix_2d::color_types::palette::css::INDIAN_RED` | shared |
| `color::palette::css::INDIGO` | constant | `jadix_2d::color_types::palette::css::INDIGO` | shared |
| `color::palette::css::IVORY` | constant | `jadix_2d::color_types::palette::css::IVORY` | shared |
| `color::palette::css::KHAKI` | constant | `jadix_2d::color_types::palette::css::KHAKI` | shared |
| `color::palette::css::LAVENDER` | constant | `jadix_2d::color_types::palette::css::LAVENDER` | shared |
| `color::palette::css::LAVENDER_BLUSH` | constant | `jadix_2d::color_types::palette::css::LAVENDER_BLUSH` | shared |
| `color::palette::css::LAWN_GREEN` | constant | `jadix_2d::color_types::palette::css::LAWN_GREEN` | shared |
| `color::palette::css::LEMON_CHIFFON` | constant | `jadix_2d::color_types::palette::css::LEMON_CHIFFON` | shared |
| `color::palette::css::LIGHT_BLUE` | constant | `jadix_2d::color_types::palette::css::LIGHT_BLUE` | shared |
| `color::palette::css::LIGHT_CORAL` | constant | `jadix_2d::color_types::palette::css::LIGHT_CORAL` | shared |
| `color::palette::css::LIGHT_CYAN` | constant | `jadix_2d::color_types::palette::css::LIGHT_CYAN` | shared |
| `color::palette::css::LIGHT_GOLDENROD_YELLOW` | constant | `jadix_2d::color_types::palette::css::LIGHT_GOLDENROD_YELLOW` | shared |
| `color::palette::css::LIGHT_GRAY` | constant | `jadix_2d::color_types::palette::css::LIGHT_GRAY` | shared |
| `color::palette::css::LIGHT_GREEN` | constant | `jadix_2d::color_types::palette::css::LIGHT_GREEN` | shared |
| `color::palette::css::LIGHT_PINK` | constant | `jadix_2d::color_types::palette::css::LIGHT_PINK` | shared |
| `color::palette::css::LIGHT_SALMON` | constant | `jadix_2d::color_types::palette::css::LIGHT_SALMON` | shared |
| `color::palette::css::LIGHT_SEA_GREEN` | constant | `jadix_2d::color_types::palette::css::LIGHT_SEA_GREEN` | shared |
| `color::palette::css::LIGHT_SKY_BLUE` | constant | `jadix_2d::color_types::palette::css::LIGHT_SKY_BLUE` | shared |
| `color::palette::css::LIGHT_SLATE_GRAY` | constant | `jadix_2d::color_types::palette::css::LIGHT_SLATE_GRAY` | shared |
| `color::palette::css::LIGHT_STEEL_BLUE` | constant | `jadix_2d::color_types::palette::css::LIGHT_STEEL_BLUE` | shared |
| `color::palette::css::LIGHT_YELLOW` | constant | `jadix_2d::color_types::palette::css::LIGHT_YELLOW` | shared |
| `color::palette::css::LIME` | constant | `jadix_2d::color_types::palette::css::LIME` | shared |
| `color::palette::css::LIME_GREEN` | constant | `jadix_2d::color_types::palette::css::LIME_GREEN` | shared |
| `color::palette::css::LINEN` | constant | `jadix_2d::color_types::palette::css::LINEN` | shared |
| `color::palette::css::MAGENTA` | constant | `jadix_2d::color_types::palette::css::MAGENTA` | shared |
| `color::palette::css::MAROON` | constant | `jadix_2d::color_types::palette::css::MAROON` | shared |
| `color::palette::css::MEDIUM_AQUAMARINE` | constant | `jadix_2d::color_types::palette::css::MEDIUM_AQUAMARINE` | shared |
| `color::palette::css::MEDIUM_BLUE` | constant | `jadix_2d::color_types::palette::css::MEDIUM_BLUE` | shared |
| `color::palette::css::MEDIUM_ORCHID` | constant | `jadix_2d::color_types::palette::css::MEDIUM_ORCHID` | shared |
| `color::palette::css::MEDIUM_PURPLE` | constant | `jadix_2d::color_types::palette::css::MEDIUM_PURPLE` | shared |
| `color::palette::css::MEDIUM_SEA_GREEN` | constant | `jadix_2d::color_types::palette::css::MEDIUM_SEA_GREEN` | shared |
| `color::palette::css::MEDIUM_SLATE_BLUE` | constant | `jadix_2d::color_types::palette::css::MEDIUM_SLATE_BLUE` | shared |
| `color::palette::css::MEDIUM_SPRING_GREEN` | constant | `jadix_2d::color_types::palette::css::MEDIUM_SPRING_GREEN` | shared |
| `color::palette::css::MEDIUM_TURQUOISE` | constant | `jadix_2d::color_types::palette::css::MEDIUM_TURQUOISE` | shared |
| `color::palette::css::MEDIUM_VIOLET_RED` | constant | `jadix_2d::color_types::palette::css::MEDIUM_VIOLET_RED` | shared |
| `color::palette::css::MIDNIGHT_BLUE` | constant | `jadix_2d::color_types::palette::css::MIDNIGHT_BLUE` | shared |
| `color::palette::css::MINT_CREAM` | constant | `jadix_2d::color_types::palette::css::MINT_CREAM` | shared |
| `color::palette::css::MISTY_ROSE` | constant | `jadix_2d::color_types::palette::css::MISTY_ROSE` | shared |
| `color::palette::css::MOCCASIN` | constant | `jadix_2d::color_types::palette::css::MOCCASIN` | shared |
| `color::palette::css::NAVAJO_WHITE` | constant | `jadix_2d::color_types::palette::css::NAVAJO_WHITE` | shared |
| `color::palette::css::NAVY` | constant | `jadix_2d::color_types::palette::css::NAVY` | shared |
| `color::palette::css::OLD_LACE` | constant | `jadix_2d::color_types::palette::css::OLD_LACE` | shared |
| `color::palette::css::OLIVE` | constant | `jadix_2d::color_types::palette::css::OLIVE` | shared |
| `color::palette::css::OLIVE_DRAB` | constant | `jadix_2d::color_types::palette::css::OLIVE_DRAB` | shared |
| `color::palette::css::ORANGE` | constant | `jadix_2d::color_types::palette::css::ORANGE` | shared |
| `color::palette::css::ORANGE_RED` | constant | `jadix_2d::color_types::palette::css::ORANGE_RED` | shared |
| `color::palette::css::ORCHID` | constant | `jadix_2d::color_types::palette::css::ORCHID` | shared |
| `color::palette::css::PALE_GOLDENROD` | constant | `jadix_2d::color_types::palette::css::PALE_GOLDENROD` | shared |
| `color::palette::css::PALE_GREEN` | constant | `jadix_2d::color_types::palette::css::PALE_GREEN` | shared |
| `color::palette::css::PALE_TURQUOISE` | constant | `jadix_2d::color_types::palette::css::PALE_TURQUOISE` | shared |
| `color::palette::css::PALE_VIOLET_RED` | constant | `jadix_2d::color_types::palette::css::PALE_VIOLET_RED` | shared |
| `color::palette::css::PAPAYA_WHIP` | constant | `jadix_2d::color_types::palette::css::PAPAYA_WHIP` | shared |
| `color::palette::css::PEACH_PUFF` | constant | `jadix_2d::color_types::palette::css::PEACH_PUFF` | shared |
| `color::palette::css::PERU` | constant | `jadix_2d::color_types::palette::css::PERU` | shared |
| `color::palette::css::PINK` | constant | `jadix_2d::color_types::palette::css::PINK` | shared |
| `color::palette::css::PLUM` | constant | `jadix_2d::color_types::palette::css::PLUM` | shared |
| `color::palette::css::POWDER_BLUE` | constant | `jadix_2d::color_types::palette::css::POWDER_BLUE` | shared |
| `color::palette::css::PURPLE` | constant | `jadix_2d::color_types::palette::css::PURPLE` | shared |
| `color::palette::css::REBECCA_PURPLE` | constant | `jadix_2d::color_types::palette::css::REBECCA_PURPLE` | shared |
| `color::palette::css::RED` | constant | `jadix_2d::color_types::palette::css::RED` | shared |
| `color::palette::css::ROSY_BROWN` | constant | `jadix_2d::color_types::palette::css::ROSY_BROWN` | shared |
| `color::palette::css::ROYAL_BLUE` | constant | `jadix_2d::color_types::palette::css::ROYAL_BLUE` | shared |
| `color::palette::css::SADDLE_BROWN` | constant | `jadix_2d::color_types::palette::css::SADDLE_BROWN` | shared |
| `color::palette::css::SALMON` | constant | `jadix_2d::color_types::palette::css::SALMON` | shared |
| `color::palette::css::SANDY_BROWN` | constant | `jadix_2d::color_types::palette::css::SANDY_BROWN` | shared |
| `color::palette::css::SEASHELL` | constant | `jadix_2d::color_types::palette::css::SEASHELL` | shared |
| `color::palette::css::SEA_GREEN` | constant | `jadix_2d::color_types::palette::css::SEA_GREEN` | shared |
| `color::palette::css::SIENNA` | constant | `jadix_2d::color_types::palette::css::SIENNA` | shared |
| `color::palette::css::SILVER` | constant | `jadix_2d::color_types::palette::css::SILVER` | shared |
| `color::palette::css::SKY_BLUE` | constant | `jadix_2d::color_types::palette::css::SKY_BLUE` | shared |
| `color::palette::css::SLATE_BLUE` | constant | `jadix_2d::color_types::palette::css::SLATE_BLUE` | shared |
| `color::palette::css::SLATE_GRAY` | constant | `jadix_2d::color_types::palette::css::SLATE_GRAY` | shared |
| `color::palette::css::SNOW` | constant | `jadix_2d::color_types::palette::css::SNOW` | shared |
| `color::palette::css::SPRING_GREEN` | constant | `jadix_2d::color_types::palette::css::SPRING_GREEN` | shared |
| `color::palette::css::STEEL_BLUE` | constant | `jadix_2d::color_types::palette::css::STEEL_BLUE` | shared |
| `color::palette::css::TAN` | constant | `jadix_2d::color_types::palette::css::TAN` | shared |
| `color::palette::css::TEAL` | constant | `jadix_2d::color_types::palette::css::TEAL` | shared |
| `color::palette::css::THISTLE` | constant | `jadix_2d::color_types::palette::css::THISTLE` | shared |
| `color::palette::css::TOMATO` | constant | `jadix_2d::color_types::palette::css::TOMATO` | shared |
| `color::palette::css::TRANSPARENT` | constant | `jadix_2d::color_types::palette::css::TRANSPARENT` | shared |
| `color::palette::css::TURQUOISE` | constant | `jadix_2d::color_types::palette::css::TURQUOISE` | shared |
| `color::palette::css::VIOLET` | constant | `jadix_2d::color_types::palette::css::VIOLET` | shared |
| `color::palette::css::WHEAT` | constant | `jadix_2d::color_types::palette::css::WHEAT` | shared |
| `color::palette::css::WHITE` | constant | `jadix_2d::color_types::palette::css::WHITE` | shared |
| `color::palette::css::WHITE_SMOKE` | constant | `jadix_2d::color_types::palette::css::WHITE_SMOKE` | shared |
| `color::palette::css::YELLOW` | constant | `jadix_2d::color_types::palette::css::YELLOW` | shared |
| `color::palette::css::YELLOW_GREEN` | constant | `jadix_2d::color_types::palette::css::YELLOW_GREEN` | shared |
| `color::parse_color` | function | `jadix_2d::color_types::parse_color` | shared |
| `color::parse_color` | use | `jadix_2d::color_types::parse_color` | shared |
| `color::parse_color_prefix` | function | `jadix_2d::color_types::parse_color_prefix` | shared |
| `color::parse_color_prefix` | use | `jadix_2d::color_types::parse_color_prefix` | shared |
### kurbo 0.13.1

632 项。同版本真实重导出；Canvas 仍接收自身 PathCommand，需要显式转换。 完整签名/条件属性/trait 见 `benchmarks/vello-api/inventory/kurbo-0.13.1.json`。

| 项 | 类别 | Jadix 对应 | 状态 |
|---|---|---|---|
| `kurbo` | module | `jadix_2d::kurbo` | shared |
| `kurbo::Affine` | struct | `jadix_2d::kurbo::Affine` | shared |
| `kurbo::Affine` | use | `jadix_2d::kurbo::Affine` | shared |
| `kurbo::Affine::FLIP_X` | assoc_const | `jadix_2d::kurbo::Affine::FLIP_X` | shared |
| `kurbo::Affine::FLIP_Y` | assoc_const | `jadix_2d::kurbo::Affine::FLIP_Y` | shared |
| `kurbo::Affine::IDENTITY` | assoc_const | `jadix_2d::kurbo::Affine::IDENTITY` | shared |
| `kurbo::Affine::as_coeffs` | function | `jadix_2d::kurbo::Affine::as_coeffs` | shared |
| `kurbo::Affine::determinant` | function | `jadix_2d::kurbo::Affine::determinant` | shared |
| `kurbo::Affine::frobenius_norm_squared` | function | `jadix_2d::kurbo::Affine::frobenius_norm_squared` | shared |
| `kurbo::Affine::inverse` | function | `jadix_2d::kurbo::Affine::inverse` | shared |
| `kurbo::Affine::is_finite` | function | `jadix_2d::kurbo::Affine::is_finite` | shared |
| `kurbo::Affine::is_nan` | function | `jadix_2d::kurbo::Affine::is_nan` | shared |
| `kurbo::Affine::map_unit_square` | function | `jadix_2d::kurbo::Affine::map_unit_square` | shared |
| `kurbo::Affine::new` | function | `jadix_2d::kurbo::Affine::new` | shared |
| `kurbo::Affine::nuclear_norm_squared` | function | `jadix_2d::kurbo::Affine::nuclear_norm_squared` | shared |
| `kurbo::Affine::pre_reflect` | function | `jadix_2d::kurbo::Affine::pre_reflect` | shared |
| `kurbo::Affine::pre_rotate` | function | `jadix_2d::kurbo::Affine::pre_rotate` | shared |
| `kurbo::Affine::pre_rotate_about` | function | `jadix_2d::kurbo::Affine::pre_rotate_about` | shared |
| `kurbo::Affine::pre_scale` | function | `jadix_2d::kurbo::Affine::pre_scale` | shared |
| `kurbo::Affine::pre_scale_non_uniform` | function | `jadix_2d::kurbo::Affine::pre_scale_non_uniform` | shared |
| `kurbo::Affine::pre_skew` | function | `jadix_2d::kurbo::Affine::pre_skew` | shared |
| `kurbo::Affine::pre_translate` | function | `jadix_2d::kurbo::Affine::pre_translate` | shared |
| `kurbo::Affine::reflect` | function | `jadix_2d::kurbo::Affine::reflect` | shared |
| `kurbo::Affine::rotate` | function | `jadix_2d::kurbo::Affine::rotate` | shared |
| `kurbo::Affine::rotate_about` | function | `jadix_2d::kurbo::Affine::rotate_about` | shared |
| `kurbo::Affine::scale` | function | `jadix_2d::kurbo::Affine::scale` | shared |
| `kurbo::Affine::scale_about` | function | `jadix_2d::kurbo::Affine::scale_about` | shared |
| `kurbo::Affine::scale_non_uniform` | function | `jadix_2d::kurbo::Affine::scale_non_uniform` | shared |
| `kurbo::Affine::skew` | function | `jadix_2d::kurbo::Affine::skew` | shared |
| `kurbo::Affine::spectral_norm` | function | `jadix_2d::kurbo::Affine::spectral_norm` | shared |
| `kurbo::Affine::then_reflect` | function | `jadix_2d::kurbo::Affine::then_reflect` | shared |
| `kurbo::Affine::then_rotate` | function | `jadix_2d::kurbo::Affine::then_rotate` | shared |
| `kurbo::Affine::then_rotate_about` | function | `jadix_2d::kurbo::Affine::then_rotate_about` | shared |
| `kurbo::Affine::then_scale` | function | `jadix_2d::kurbo::Affine::then_scale` | shared |
| `kurbo::Affine::then_scale_about` | function | `jadix_2d::kurbo::Affine::then_scale_about` | shared |
| `kurbo::Affine::then_scale_non_uniform` | function | `jadix_2d::kurbo::Affine::then_scale_non_uniform` | shared |
| `kurbo::Affine::then_skew` | function | `jadix_2d::kurbo::Affine::then_skew` | shared |
| `kurbo::Affine::then_translate` | function | `jadix_2d::kurbo::Affine::then_translate` | shared |
| `kurbo::Affine::transform_rect_bbox` | function | `jadix_2d::kurbo::Affine::transform_rect_bbox` | shared |
| `kurbo::Affine::translate` | function | `jadix_2d::kurbo::Affine::translate` | shared |
| `kurbo::Affine::translation` | function | `jadix_2d::kurbo::Affine::translation` | shared |
| `kurbo::Affine::with_translation` | function | `jadix_2d::kurbo::Affine::with_translation` | shared |
| `kurbo::Arc` | struct | `jadix_2d::kurbo::Arc` | shared |
| `kurbo::Arc` | use | `jadix_2d::kurbo::Arc` | shared |
| `kurbo::Arc::append_iter` | function | `jadix_2d::kurbo::Arc::append_iter` | shared |
| `kurbo::Arc::center` | struct_field | `jadix_2d::kurbo::Arc::center` | shared |
| `kurbo::Arc::from_svg_arc` | function | `jadix_2d::kurbo::Arc::from_svg_arc` | shared |
| `kurbo::Arc::new` | function | `jadix_2d::kurbo::Arc::new` | shared |
| `kurbo::Arc::radii` | struct_field | `jadix_2d::kurbo::Arc::radii` | shared |
| `kurbo::Arc::reversed` | function | `jadix_2d::kurbo::Arc::reversed` | shared |
| `kurbo::Arc::start_angle` | struct_field | `jadix_2d::kurbo::Arc::start_angle` | shared |
| `kurbo::Arc::sweep_angle` | struct_field | `jadix_2d::kurbo::Arc::sweep_angle` | shared |
| `kurbo::Arc::to_cubic_beziers` | function | `jadix_2d::kurbo::Arc::to_cubic_beziers` | shared |
| `kurbo::Arc::x_rotation` | struct_field | `jadix_2d::kurbo::Arc::x_rotation` | shared |
| `kurbo::Axis` | enum | `jadix_2d::kurbo::Axis` | shared |
| `kurbo::Axis` | use | `jadix_2d::kurbo::Axis` | shared |
| `kurbo::Axis::Horizontal` | variant | `jadix_2d::kurbo::Axis::Horizontal` | shared |
| `kurbo::Axis::Vertical` | variant | `jadix_2d::kurbo::Axis::Vertical` | shared |
| `kurbo::Axis::cross` | function | `jadix_2d::kurbo::Axis::cross` | shared |
| `kurbo::Axis::pack_point` | function | `jadix_2d::kurbo::Axis::pack_point` | shared |
| `kurbo::Axis::pack_size` | function | `jadix_2d::kurbo::Axis::pack_size` | shared |
| `kurbo::Axis::pack_vec2` | function | `jadix_2d::kurbo::Axis::pack_vec2` | shared |
| `kurbo::Axis::pack_xy` | function | `jadix_2d::kurbo::Axis::pack_xy` | shared |
| `kurbo::BezPath` | struct | `jadix_2d::kurbo::BezPath` | shared |
| `kurbo::BezPath` | use | `jadix_2d::kurbo::BezPath` | shared |
| `kurbo::BezPath::apply_affine` | function | `jadix_2d::kurbo::BezPath::apply_affine` | shared |
| `kurbo::BezPath::close_path` | function | `jadix_2d::kurbo::BezPath::close_path` | shared |
| `kurbo::BezPath::control_box` | function | `jadix_2d::kurbo::BezPath::control_box` | shared |
| `kurbo::BezPath::current_position` | function | `jadix_2d::kurbo::BezPath::current_position` | shared |
| `kurbo::BezPath::curve_to` | function | `jadix_2d::kurbo::BezPath::curve_to` | shared |
| `kurbo::BezPath::elements` | function | `jadix_2d::kurbo::BezPath::elements` | shared |
| `kurbo::BezPath::elements_mut` | function | `jadix_2d::kurbo::BezPath::elements_mut` | shared |
| `kurbo::BezPath::from_path_segments` | function | `jadix_2d::kurbo::BezPath::from_path_segments` | shared |
| `kurbo::BezPath::from_svg` | function | `jadix_2d::kurbo::BezPath::from_svg` | shared |
| `kurbo::BezPath::from_vec` | function | `jadix_2d::kurbo::BezPath::from_vec` | shared |
| `kurbo::BezPath::get_seg` | function | `jadix_2d::kurbo::BezPath::get_seg` | shared |
| `kurbo::BezPath::into_elements` | function | `jadix_2d::kurbo::BezPath::into_elements` | shared |
| `kurbo::BezPath::is_empty` | function | `jadix_2d::kurbo::BezPath::is_empty` | shared |
| `kurbo::BezPath::is_finite` | function | `jadix_2d::kurbo::BezPath::is_finite` | shared |
| `kurbo::BezPath::is_nan` | function | `jadix_2d::kurbo::BezPath::is_nan` | shared |
| `kurbo::BezPath::iter` | function | `jadix_2d::kurbo::BezPath::iter` | shared |
| `kurbo::BezPath::line_to` | function | `jadix_2d::kurbo::BezPath::line_to` | shared |
| `kurbo::BezPath::move_to` | function | `jadix_2d::kurbo::BezPath::move_to` | shared |
| `kurbo::BezPath::new` | function | `jadix_2d::kurbo::BezPath::new` | shared |
| `kurbo::BezPath::pop` | function | `jadix_2d::kurbo::BezPath::pop` | shared |
| `kurbo::BezPath::push` | function | `jadix_2d::kurbo::BezPath::push` | shared |
| `kurbo::BezPath::quad_to` | function | `jadix_2d::kurbo::BezPath::quad_to` | shared |
| `kurbo::BezPath::reverse_subpaths` | function | `jadix_2d::kurbo::BezPath::reverse_subpaths` | shared |
| `kurbo::BezPath::segments` | function | `jadix_2d::kurbo::BezPath::segments` | shared |
| `kurbo::BezPath::subpaths` | function | `jadix_2d::kurbo::BezPath::subpaths` | shared |
| `kurbo::BezPath::to_svg` | function | `jadix_2d::kurbo::BezPath::to_svg` | shared |
| `kurbo::BezPath::truncate` | function | `jadix_2d::kurbo::BezPath::truncate` | shared |
| `kurbo::BezPath::with_capacity` | function | `jadix_2d::kurbo::BezPath::with_capacity` | shared |
| `kurbo::BezPath::write_to` | function | `jadix_2d::kurbo::BezPath::write_to` | shared |
| `kurbo::Cap` | enum | `jadix_2d::kurbo::Cap` | shared |
| `kurbo::Cap` | use | `jadix_2d::kurbo::Cap` | shared |
| `kurbo::Cap::Butt` | variant | `jadix_2d::kurbo::Cap::Butt` | shared |
| `kurbo::Cap::Round` | variant | `jadix_2d::kurbo::Cap::Round` | shared |
| `kurbo::Cap::Square` | variant | `jadix_2d::kurbo::Cap::Square` | shared |
| `kurbo::Circle` | struct | `jadix_2d::kurbo::Circle` | shared |
| `kurbo::Circle` | use | `jadix_2d::kurbo::Circle` | shared |
| `kurbo::Circle::center` | struct_field | `jadix_2d::kurbo::Circle::center` | shared |
| `kurbo::Circle::is_finite` | function | `jadix_2d::kurbo::Circle::is_finite` | shared |
| `kurbo::Circle::is_nan` | function | `jadix_2d::kurbo::Circle::is_nan` | shared |
| `kurbo::Circle::new` | function | `jadix_2d::kurbo::Circle::new` | shared |
| `kurbo::Circle::radius` | struct_field | `jadix_2d::kurbo::Circle::radius` | shared |
| `kurbo::Circle::segment` | function | `jadix_2d::kurbo::Circle::segment` | shared |
| `kurbo::CircleSegment` | struct | `jadix_2d::kurbo::CircleSegment` | shared |
| `kurbo::CircleSegment` | use | `jadix_2d::kurbo::CircleSegment` | shared |
| `kurbo::CircleSegment::center` | struct_field | `jadix_2d::kurbo::CircleSegment::center` | shared |
| `kurbo::CircleSegment::inner_arc` | function | `jadix_2d::kurbo::CircleSegment::inner_arc` | shared |
| `kurbo::CircleSegment::inner_radius` | struct_field | `jadix_2d::kurbo::CircleSegment::inner_radius` | shared |
| `kurbo::CircleSegment::is_finite` | function | `jadix_2d::kurbo::CircleSegment::is_finite` | shared |
| `kurbo::CircleSegment::is_nan` | function | `jadix_2d::kurbo::CircleSegment::is_nan` | shared |
| `kurbo::CircleSegment::new` | function | `jadix_2d::kurbo::CircleSegment::new` | shared |
| `kurbo::CircleSegment::outer_arc` | function | `jadix_2d::kurbo::CircleSegment::outer_arc` | shared |
| `kurbo::CircleSegment::outer_radius` | struct_field | `jadix_2d::kurbo::CircleSegment::outer_radius` | shared |
| `kurbo::CircleSegment::start_angle` | struct_field | `jadix_2d::kurbo::CircleSegment::start_angle` | shared |
| `kurbo::CircleSegment::sweep_angle` | struct_field | `jadix_2d::kurbo::CircleSegment::sweep_angle` | shared |
| `kurbo::ConstPoint` | struct | `jadix_2d::kurbo::ConstPoint` | shared |
| `kurbo::ConstPoint` | use | `jadix_2d::kurbo::ConstPoint` | shared |
| `kurbo::ConstPoint::is_finite` | function | `jadix_2d::kurbo::ConstPoint::is_finite` | shared |
| `kurbo::ConstPoint::is_nan` | function | `jadix_2d::kurbo::ConstPoint::is_nan` | shared |
| `kurbo::CubicBez` | struct | `jadix_2d::kurbo::CubicBez` | shared |
| `kurbo::CubicBez` | use | `jadix_2d::kurbo::CubicBez` | shared |
| `kurbo::CubicBez::approx_spline` | function | `jadix_2d::kurbo::CubicBez::approx_spline` | shared |
| `kurbo::CubicBez::inflections` | function | `jadix_2d::kurbo::CubicBez::inflections` | shared |
| `kurbo::CubicBez::is_finite` | function | `jadix_2d::kurbo::CubicBez::is_finite` | shared |
| `kurbo::CubicBez::is_nan` | function | `jadix_2d::kurbo::CubicBez::is_nan` | shared |
| `kurbo::CubicBez::new` | function | `jadix_2d::kurbo::CubicBez::new` | shared |
| `kurbo::CubicBez::p0` | struct_field | `jadix_2d::kurbo::CubicBez::p0` | shared |
| `kurbo::CubicBez::p1` | struct_field | `jadix_2d::kurbo::CubicBez::p1` | shared |
| `kurbo::CubicBez::p2` | struct_field | `jadix_2d::kurbo::CubicBez::p2` | shared |
| `kurbo::CubicBez::p3` | struct_field | `jadix_2d::kurbo::CubicBez::p3` | shared |
| `kurbo::CubicBez::tangents_to_point` | function | `jadix_2d::kurbo::CubicBez::tangents_to_point` | shared |
| `kurbo::CubicBez::to_quads` | function | `jadix_2d::kurbo::CubicBez::to_quads` | shared |
| `kurbo::CubicBezIter` | struct | `jadix_2d::kurbo::CubicBezIter` | shared |
| `kurbo::CubicBezIter` | use | `jadix_2d::kurbo::CubicBezIter` | shared |
| `kurbo::CurveFitSample` | struct | `jadix_2d::kurbo::CurveFitSample` | shared |
| `kurbo::CurveFitSample` | use | `jadix_2d::kurbo::CurveFitSample` | shared |
| `kurbo::CurveFitSample::p` | struct_field | `jadix_2d::kurbo::CurveFitSample::p` | shared |
| `kurbo::CurveFitSample::tangent` | struct_field | `jadix_2d::kurbo::CurveFitSample::tangent` | shared |
| `kurbo::CuspType` | enum | `jadix_2d::kurbo::CuspType` | shared |
| `kurbo::CuspType` | use | `jadix_2d::kurbo::CuspType` | shared |
| `kurbo::CuspType::DoubleInflection` | variant | `jadix_2d::kurbo::CuspType::DoubleInflection` | shared |
| `kurbo::CuspType::Loop` | variant | `jadix_2d::kurbo::CuspType::Loop` | shared |
| `kurbo::DEFAULT_ACCURACY` | constant | `jadix_2d::kurbo::DEFAULT_ACCURACY` | shared |
| `kurbo::DEFAULT_ACCURACY` | use | `jadix_2d::kurbo::DEFAULT_ACCURACY` | shared |
| `kurbo::Dashes` | type_alias | `jadix_2d::kurbo::Dashes` | shared |
| `kurbo::Dashes` | use | `jadix_2d::kurbo::Dashes` | shared |
| `kurbo::Diagonal2` | struct | `jadix_2d::kurbo::Diagonal2` | shared |
| `kurbo::Diagonal2` | use | `jadix_2d::kurbo::Diagonal2` | shared |
| `kurbo::Diagonal2::abs` | function | `jadix_2d::kurbo::Diagonal2::abs` | shared |
| `kurbo::Diagonal2::inv` | function | `jadix_2d::kurbo::Diagonal2::inv` | shared |
| `kurbo::Diagonal2::new` | function | `jadix_2d::kurbo::Diagonal2::new` | shared |
| `kurbo::Diagonal2::scale_normal` | function | `jadix_2d::kurbo::Diagonal2::scale_normal` | shared |
| `kurbo::Diagonal2::xx` | struct_field | `jadix_2d::kurbo::Diagonal2::xx` | shared |
| `kurbo::Diagonal2::yy` | struct_field | `jadix_2d::kurbo::Diagonal2::yy` | shared |
| `kurbo::Ellipse` | struct | `jadix_2d::kurbo::Ellipse` | shared |
| `kurbo::Ellipse` | use | `jadix_2d::kurbo::Ellipse` | shared |
| `kurbo::Ellipse::center` | function | `jadix_2d::kurbo::Ellipse::center` | shared |
| `kurbo::Ellipse::from_affine` | function | `jadix_2d::kurbo::Ellipse::from_affine` | shared |
| `kurbo::Ellipse::from_rect` | function | `jadix_2d::kurbo::Ellipse::from_rect` | shared |
| `kurbo::Ellipse::is_finite` | function | `jadix_2d::kurbo::Ellipse::is_finite` | shared |
| `kurbo::Ellipse::is_nan` | function | `jadix_2d::kurbo::Ellipse::is_nan` | shared |
| `kurbo::Ellipse::major_radius` | function | `jadix_2d::kurbo::Ellipse::major_radius` | shared |
| `kurbo::Ellipse::minor_radius` | function | `jadix_2d::kurbo::Ellipse::minor_radius` | shared |
| `kurbo::Ellipse::new` | function | `jadix_2d::kurbo::Ellipse::new` | shared |
| `kurbo::Ellipse::radii` | function | `jadix_2d::kurbo::Ellipse::radii` | shared |
| `kurbo::Ellipse::radii_and_rotation` | function | `jadix_2d::kurbo::Ellipse::radii_and_rotation` | shared |
| `kurbo::Ellipse::rotation` | function | `jadix_2d::kurbo::Ellipse::rotation` | shared |
| `kurbo::Ellipse::with_center` | function | `jadix_2d::kurbo::Ellipse::with_center` | shared |
| `kurbo::Ellipse::with_radii` | function | `jadix_2d::kurbo::Ellipse::with_radii` | shared |
| `kurbo::Ellipse::with_rotation` | function | `jadix_2d::kurbo::Ellipse::with_rotation` | shared |
| `kurbo::Insets` | struct | `jadix_2d::kurbo::Insets` | shared |
| `kurbo::Insets` | use | `jadix_2d::kurbo::Insets` | shared |
| `kurbo::Insets::ZERO` | assoc_const | `jadix_2d::kurbo::Insets::ZERO` | shared |
| `kurbo::Insets::are_nonnegative` | function | `jadix_2d::kurbo::Insets::are_nonnegative` | shared |
| `kurbo::Insets::is_finite` | function | `jadix_2d::kurbo::Insets::is_finite` | shared |
| `kurbo::Insets::is_nan` | function | `jadix_2d::kurbo::Insets::is_nan` | shared |
| `kurbo::Insets::max` | function | `jadix_2d::kurbo::Insets::max` | shared |
| `kurbo::Insets::min` | function | `jadix_2d::kurbo::Insets::min` | shared |
| `kurbo::Insets::new` | function | `jadix_2d::kurbo::Insets::new` | shared |
| `kurbo::Insets::nonnegative` | function | `jadix_2d::kurbo::Insets::nonnegative` | shared |
| `kurbo::Insets::size` | function | `jadix_2d::kurbo::Insets::size` | shared |
| `kurbo::Insets::uniform` | function | `jadix_2d::kurbo::Insets::uniform` | shared |
| `kurbo::Insets::uniform_xy` | function | `jadix_2d::kurbo::Insets::uniform_xy` | shared |
| `kurbo::Insets::x0` | struct_field | `jadix_2d::kurbo::Insets::x0` | shared |
| `kurbo::Insets::x1` | struct_field | `jadix_2d::kurbo::Insets::x1` | shared |
| `kurbo::Insets::x_value` | function | `jadix_2d::kurbo::Insets::x_value` | shared |
| `kurbo::Insets::y0` | struct_field | `jadix_2d::kurbo::Insets::y0` | shared |
| `kurbo::Insets::y1` | struct_field | `jadix_2d::kurbo::Insets::y1` | shared |
| `kurbo::Insets::y_value` | function | `jadix_2d::kurbo::Insets::y_value` | shared |
| `kurbo::Join` | enum | `jadix_2d::kurbo::Join` | shared |
| `kurbo::Join` | use | `jadix_2d::kurbo::Join` | shared |
| `kurbo::Join::Bevel` | variant | `jadix_2d::kurbo::Join::Bevel` | shared |
| `kurbo::Join::Miter` | variant | `jadix_2d::kurbo::Join::Miter` | shared |
| `kurbo::Join::Round` | variant | `jadix_2d::kurbo::Join::Round` | shared |
| `kurbo::Line` | struct | `jadix_2d::kurbo::Line` | shared |
| `kurbo::Line` | use | `jadix_2d::kurbo::Line` | shared |
| `kurbo::Line::crossing_point` | function | `jadix_2d::kurbo::Line::crossing_point` | shared |
| `kurbo::Line::is_finite` | function | `jadix_2d::kurbo::Line::is_finite` | shared |
| `kurbo::Line::is_nan` | function | `jadix_2d::kurbo::Line::is_nan` | shared |
| `kurbo::Line::length` | function | `jadix_2d::kurbo::Line::length` | shared |
| `kurbo::Line::midpoint` | function | `jadix_2d::kurbo::Line::midpoint` | shared |
| `kurbo::Line::new` | function | `jadix_2d::kurbo::Line::new` | shared |
| `kurbo::Line::p0` | struct_field | `jadix_2d::kurbo::Line::p0` | shared |
| `kurbo::Line::p1` | struct_field | `jadix_2d::kurbo::Line::p1` | shared |
| `kurbo::Line::reversed` | function | `jadix_2d::kurbo::Line::reversed` | shared |
| `kurbo::LineIntersection` | struct | `jadix_2d::kurbo::LineIntersection` | shared |
| `kurbo::LineIntersection` | use | `jadix_2d::kurbo::LineIntersection` | shared |
| `kurbo::LineIntersection::is_finite` | function | `jadix_2d::kurbo::LineIntersection::is_finite` | shared |
| `kurbo::LineIntersection::is_nan` | function | `jadix_2d::kurbo::LineIntersection::is_nan` | shared |
| `kurbo::LineIntersection::line_t` | struct_field | `jadix_2d::kurbo::LineIntersection::line_t` | shared |
| `kurbo::LineIntersection::segment_t` | struct_field | `jadix_2d::kurbo::LineIntersection::segment_t` | shared |
| `kurbo::MAX_EXTREMA` | constant | `jadix_2d::kurbo::MAX_EXTREMA` | shared |
| `kurbo::MAX_EXTREMA` | use | `jadix_2d::kurbo::MAX_EXTREMA` | shared |
| `kurbo::MinDistance` | struct | `jadix_2d::kurbo::MinDistance` | shared |
| `kurbo::MinDistance` | use | `jadix_2d::kurbo::MinDistance` | shared |
| `kurbo::MinDistance::distance` | struct_field | `jadix_2d::kurbo::MinDistance::distance` | shared |
| `kurbo::MinDistance::t1` | struct_field | `jadix_2d::kurbo::MinDistance::t1` | shared |
| `kurbo::MinDistance::t2` | struct_field | `jadix_2d::kurbo::MinDistance::t2` | shared |
| `kurbo::Moments` | struct | `jadix_2d::kurbo::Moments` | shared |
| `kurbo::Moments` | use | `jadix_2d::kurbo::Moments` | shared |
| `kurbo::Moments::moment_x` | struct_field | `jadix_2d::kurbo::Moments::moment_x` | shared |
| `kurbo::Moments::moment_xx` | struct_field | `jadix_2d::kurbo::Moments::moment_xx` | shared |
| `kurbo::Moments::moment_xy` | struct_field | `jadix_2d::kurbo::Moments::moment_xy` | shared |
| `kurbo::Moments::moment_y` | struct_field | `jadix_2d::kurbo::Moments::moment_y` | shared |
| `kurbo::Moments::moment_yy` | struct_field | `jadix_2d::kurbo::Moments::moment_yy` | shared |
| `kurbo::Nearest` | struct | `jadix_2d::kurbo::Nearest` | shared |
| `kurbo::Nearest` | use | `jadix_2d::kurbo::Nearest` | shared |
| `kurbo::Nearest::distance_sq` | struct_field | `jadix_2d::kurbo::Nearest::distance_sq` | shared |
| `kurbo::Nearest::t` | struct_field | `jadix_2d::kurbo::Nearest::t` | shared |
| `kurbo::ParamCurve` | trait | `jadix_2d::kurbo::ParamCurve` | shared |
| `kurbo::ParamCurve` | use | `jadix_2d::kurbo::ParamCurve` | shared |
| `kurbo::ParamCurve::end` | function | `jadix_2d::kurbo::ParamCurve::end` | shared |
| `kurbo::ParamCurve::eval` | function | `jadix_2d::kurbo::ParamCurve::eval` | shared |
| `kurbo::ParamCurve::start` | function | `jadix_2d::kurbo::ParamCurve::start` | shared |
| `kurbo::ParamCurve::subdivide` | function | `jadix_2d::kurbo::ParamCurve::subdivide` | shared |
| `kurbo::ParamCurve::subsegment` | function | `jadix_2d::kurbo::ParamCurve::subsegment` | shared |
| `kurbo::ParamCurveArclen` | trait | `jadix_2d::kurbo::ParamCurveArclen` | shared |
| `kurbo::ParamCurveArclen` | use | `jadix_2d::kurbo::ParamCurveArclen` | shared |
| `kurbo::ParamCurveArclen::arclen` | function | `jadix_2d::kurbo::ParamCurveArclen::arclen` | shared |
| `kurbo::ParamCurveArclen::inv_arclen` | function | `jadix_2d::kurbo::ParamCurveArclen::inv_arclen` | shared |
| `kurbo::ParamCurveArea` | trait | `jadix_2d::kurbo::ParamCurveArea` | shared |
| `kurbo::ParamCurveArea` | use | `jadix_2d::kurbo::ParamCurveArea` | shared |
| `kurbo::ParamCurveArea::signed_area` | function | `jadix_2d::kurbo::ParamCurveArea::signed_area` | shared |
| `kurbo::ParamCurveCurvature` | trait | `jadix_2d::kurbo::ParamCurveCurvature` | shared |
| `kurbo::ParamCurveCurvature` | use | `jadix_2d::kurbo::ParamCurveCurvature` | shared |
| `kurbo::ParamCurveCurvature::curvature` | function | `jadix_2d::kurbo::ParamCurveCurvature::curvature` | shared |
| `kurbo::ParamCurveDeriv` | trait | `jadix_2d::kurbo::ParamCurveDeriv` | shared |
| `kurbo::ParamCurveDeriv` | use | `jadix_2d::kurbo::ParamCurveDeriv` | shared |
| `kurbo::ParamCurveDeriv::DerivResult` | assoc_type | `jadix_2d::kurbo::ParamCurveDeriv::DerivResult` | shared |
| `kurbo::ParamCurveDeriv::deriv` | function | `jadix_2d::kurbo::ParamCurveDeriv::deriv` | shared |
| `kurbo::ParamCurveDeriv::gauss_arclen` | function | `jadix_2d::kurbo::ParamCurveDeriv::gauss_arclen` | shared |
| `kurbo::ParamCurveExtrema` | trait | `jadix_2d::kurbo::ParamCurveExtrema` | shared |
| `kurbo::ParamCurveExtrema` | use | `jadix_2d::kurbo::ParamCurveExtrema` | shared |
| `kurbo::ParamCurveExtrema::bounding_box` | function | `jadix_2d::kurbo::ParamCurveExtrema::bounding_box` | shared |
| `kurbo::ParamCurveExtrema::extrema` | function | `jadix_2d::kurbo::ParamCurveExtrema::extrema` | shared |
| `kurbo::ParamCurveExtrema::extrema_ranges` | function | `jadix_2d::kurbo::ParamCurveExtrema::extrema_ranges` | shared |
| `kurbo::ParamCurveFit` | trait | `jadix_2d::kurbo::ParamCurveFit` | shared |
| `kurbo::ParamCurveFit` | use | `jadix_2d::kurbo::ParamCurveFit` | shared |
| `kurbo::ParamCurveFit::break_cusp` | function | `jadix_2d::kurbo::ParamCurveFit::break_cusp` | shared |
| `kurbo::ParamCurveFit::moment_integrals` | function | `jadix_2d::kurbo::ParamCurveFit::moment_integrals` | shared |
| `kurbo::ParamCurveFit::sample_pt_deriv` | function | `jadix_2d::kurbo::ParamCurveFit::sample_pt_deriv` | shared |
| `kurbo::ParamCurveFit::sample_pt_tangent` | function | `jadix_2d::kurbo::ParamCurveFit::sample_pt_tangent` | shared |
| `kurbo::ParamCurveMoments` | trait | `jadix_2d::kurbo::ParamCurveMoments` | shared |
| `kurbo::ParamCurveMoments` | use | `jadix_2d::kurbo::ParamCurveMoments` | shared |
| `kurbo::ParamCurveMoments::moments` | function | `jadix_2d::kurbo::ParamCurveMoments::moments` | shared |
| `kurbo::ParamCurveNearest` | trait | `jadix_2d::kurbo::ParamCurveNearest` | shared |
| `kurbo::ParamCurveNearest` | use | `jadix_2d::kurbo::ParamCurveNearest` | shared |
| `kurbo::ParamCurveNearest::nearest` | function | `jadix_2d::kurbo::ParamCurveNearest::nearest` | shared |
| `kurbo::PathEl` | enum | `jadix_2d::kurbo::PathEl` | shared |
| `kurbo::PathEl` | use | `jadix_2d::kurbo::PathEl` | shared |
| `kurbo::PathEl::ClosePath` | variant | `jadix_2d::kurbo::PathEl::ClosePath` | shared |
| `kurbo::PathEl::CurveTo` | variant | `jadix_2d::kurbo::PathEl::CurveTo` | shared |
| `kurbo::PathEl::CurveTo::0` | struct_field | `jadix_2d::kurbo::PathEl::CurveTo::0` | shared |
| `kurbo::PathEl::CurveTo::1` | struct_field | `jadix_2d::kurbo::PathEl::CurveTo::1` | shared |
| `kurbo::PathEl::CurveTo::2` | struct_field | `jadix_2d::kurbo::PathEl::CurveTo::2` | shared |
| `kurbo::PathEl::LineTo` | variant | `jadix_2d::kurbo::PathEl::LineTo` | shared |
| `kurbo::PathEl::LineTo::0` | struct_field | `jadix_2d::kurbo::PathEl::LineTo::0` | shared |
| `kurbo::PathEl::MoveTo` | variant | `jadix_2d::kurbo::PathEl::MoveTo` | shared |
| `kurbo::PathEl::MoveTo::0` | struct_field | `jadix_2d::kurbo::PathEl::MoveTo::0` | shared |
| `kurbo::PathEl::QuadTo` | variant | `jadix_2d::kurbo::PathEl::QuadTo` | shared |
| `kurbo::PathEl::QuadTo::0` | struct_field | `jadix_2d::kurbo::PathEl::QuadTo::0` | shared |
| `kurbo::PathEl::QuadTo::1` | struct_field | `jadix_2d::kurbo::PathEl::QuadTo::1` | shared |
| `kurbo::PathEl::end_point` | function | `jadix_2d::kurbo::PathEl::end_point` | shared |
| `kurbo::PathEl::is_finite` | function | `jadix_2d::kurbo::PathEl::is_finite` | shared |
| `kurbo::PathEl::is_nan` | function | `jadix_2d::kurbo::PathEl::is_nan` | shared |
| `kurbo::PathSeg` | enum | `jadix_2d::kurbo::PathSeg` | shared |
| `kurbo::PathSeg` | use | `jadix_2d::kurbo::PathSeg` | shared |
| `kurbo::PathSeg::Cubic` | variant | `jadix_2d::kurbo::PathSeg::Cubic` | shared |
| `kurbo::PathSeg::Cubic::0` | struct_field | `jadix_2d::kurbo::PathSeg::Cubic::0` | shared |
| `kurbo::PathSeg::Line` | variant | `jadix_2d::kurbo::PathSeg::Line` | shared |
| `kurbo::PathSeg::Line::0` | struct_field | `jadix_2d::kurbo::PathSeg::Line::0` | shared |
| `kurbo::PathSeg::Quad` | variant | `jadix_2d::kurbo::PathSeg::Quad` | shared |
| `kurbo::PathSeg::Quad::0` | struct_field | `jadix_2d::kurbo::PathSeg::Quad::0` | shared |
| `kurbo::PathSeg::as_path_el` | function | `jadix_2d::kurbo::PathSeg::as_path_el` | shared |
| `kurbo::PathSeg::intersect_line` | function | `jadix_2d::kurbo::PathSeg::intersect_line` | shared |
| `kurbo::PathSeg::is_finite` | function | `jadix_2d::kurbo::PathSeg::is_finite` | shared |
| `kurbo::PathSeg::is_nan` | function | `jadix_2d::kurbo::PathSeg::is_nan` | shared |
| `kurbo::PathSeg::min_dist` | function | `jadix_2d::kurbo::PathSeg::min_dist` | shared |
| `kurbo::PathSeg::reverse` | function | `jadix_2d::kurbo::PathSeg::reverse` | shared |
| `kurbo::PathSeg::to_cubic` | function | `jadix_2d::kurbo::PathSeg::to_cubic` | shared |
| `kurbo::PathSegIter` | struct | `jadix_2d::kurbo::PathSegIter` | shared |
| `kurbo::PathSegIter` | use | `jadix_2d::kurbo::PathSegIter` | shared |
| `kurbo::Point` | struct | `jadix_2d::kurbo::Point` | shared |
| `kurbo::Point` | use | `jadix_2d::kurbo::Point` | shared |
| `kurbo::Point::ORIGIN` | assoc_const | `jadix_2d::kurbo::Point::ORIGIN` | shared |
| `kurbo::Point::ZERO` | assoc_const | `jadix_2d::kurbo::Point::ZERO` | shared |
| `kurbo::Point::ceil` | function | `jadix_2d::kurbo::Point::ceil` | shared |
| `kurbo::Point::distance` | function | `jadix_2d::kurbo::Point::distance` | shared |
| `kurbo::Point::distance_squared` | function | `jadix_2d::kurbo::Point::distance_squared` | shared |
| `kurbo::Point::expand` | function | `jadix_2d::kurbo::Point::expand` | shared |
| `kurbo::Point::floor` | function | `jadix_2d::kurbo::Point::floor` | shared |
| `kurbo::Point::get_coord` | function | `jadix_2d::kurbo::Point::get_coord` | shared |
| `kurbo::Point::get_coord_mut` | function | `jadix_2d::kurbo::Point::get_coord_mut` | shared |
| `kurbo::Point::is_finite` | function | `jadix_2d::kurbo::Point::is_finite` | shared |
| `kurbo::Point::is_nan` | function | `jadix_2d::kurbo::Point::is_nan` | shared |
| `kurbo::Point::lerp` | function | `jadix_2d::kurbo::Point::lerp` | shared |
| `kurbo::Point::midpoint` | function | `jadix_2d::kurbo::Point::midpoint` | shared |
| `kurbo::Point::new` | function | `jadix_2d::kurbo::Point::new` | shared |
| `kurbo::Point::round` | function | `jadix_2d::kurbo::Point::round` | shared |
| `kurbo::Point::set_coord` | function | `jadix_2d::kurbo::Point::set_coord` | shared |
| `kurbo::Point::to_vec2` | function | `jadix_2d::kurbo::Point::to_vec2` | shared |
| `kurbo::Point::trunc` | function | `jadix_2d::kurbo::Point::trunc` | shared |
| `kurbo::Point::x` | struct_field | `jadix_2d::kurbo::Point::x` | shared |
| `kurbo::Point::y` | struct_field | `jadix_2d::kurbo::Point::y` | shared |
| `kurbo::QuadBez` | struct | `jadix_2d::kurbo::QuadBez` | shared |
| `kurbo::QuadBez` | use | `jadix_2d::kurbo::QuadBez` | shared |
| `kurbo::QuadBez::is_finite` | function | `jadix_2d::kurbo::QuadBez::is_finite` | shared |
| `kurbo::QuadBez::is_nan` | function | `jadix_2d::kurbo::QuadBez::is_nan` | shared |
| `kurbo::QuadBez::new` | function | `jadix_2d::kurbo::QuadBez::new` | shared |
| `kurbo::QuadBez::p0` | struct_field | `jadix_2d::kurbo::QuadBez::p0` | shared |
| `kurbo::QuadBez::p1` | struct_field | `jadix_2d::kurbo::QuadBez::p1` | shared |
| `kurbo::QuadBez::p2` | struct_field | `jadix_2d::kurbo::QuadBez::p2` | shared |
| `kurbo::QuadBez::raise` | function | `jadix_2d::kurbo::QuadBez::raise` | shared |
| `kurbo::QuadBezIter` | struct | `jadix_2d::kurbo::QuadBezIter` | shared |
| `kurbo::QuadBezIter` | use | `jadix_2d::kurbo::QuadBezIter` | shared |
| `kurbo::QuadSpline` | struct | `jadix_2d::kurbo::QuadSpline` | shared |
| `kurbo::QuadSpline` | use | `jadix_2d::kurbo::QuadSpline` | shared |
| `kurbo::QuadSpline::new` | function | `jadix_2d::kurbo::QuadSpline::new` | shared |
| `kurbo::QuadSpline::points` | function | `jadix_2d::kurbo::QuadSpline::points` | shared |
| `kurbo::QuadSpline::to_quads` | function | `jadix_2d::kurbo::QuadSpline::to_quads` | shared |
| `kurbo::Rect` | struct | `jadix_2d::kurbo::Rect` | shared |
| `kurbo::Rect` | use | `jadix_2d::kurbo::Rect` | shared |
| `kurbo::Rect::ZERO` | assoc_const | `jadix_2d::kurbo::Rect::ZERO` | shared |
| `kurbo::Rect::abs` | function | `jadix_2d::kurbo::Rect::abs` | shared |
| `kurbo::Rect::area` | function | `jadix_2d::kurbo::Rect::area` | shared |
| `kurbo::Rect::aspect_ratio` | function | `jadix_2d::kurbo::Rect::aspect_ratio` | shared |
| `kurbo::Rect::aspect_ratio_width` | function | `jadix_2d::kurbo::Rect::aspect_ratio_width` | shared |
| `kurbo::Rect::ceil` | function | `jadix_2d::kurbo::Rect::ceil` | shared |
| `kurbo::Rect::center` | function | `jadix_2d::kurbo::Rect::center` | shared |
| `kurbo::Rect::contained_rect_with_aspect_ratio` | function | `jadix_2d::kurbo::Rect::contained_rect_with_aspect_ratio` | shared |
| `kurbo::Rect::contains` | function | `jadix_2d::kurbo::Rect::contains` | shared |
| `kurbo::Rect::contains_rect` | function | `jadix_2d::kurbo::Rect::contains_rect` | shared |
| `kurbo::Rect::expand` | function | `jadix_2d::kurbo::Rect::expand` | shared |
| `kurbo::Rect::floor` | function | `jadix_2d::kurbo::Rect::floor` | shared |
| `kurbo::Rect::from_center_size` | function | `jadix_2d::kurbo::Rect::from_center_size` | shared |
| `kurbo::Rect::from_origin_size` | function | `jadix_2d::kurbo::Rect::from_origin_size` | shared |
| `kurbo::Rect::from_points` | function | `jadix_2d::kurbo::Rect::from_points` | shared |
| `kurbo::Rect::get_coords` | function | `jadix_2d::kurbo::Rect::get_coords` | shared |
| `kurbo::Rect::get_coords_mut` | function | `jadix_2d::kurbo::Rect::get_coords_mut` | shared |
| `kurbo::Rect::height` | function | `jadix_2d::kurbo::Rect::height` | shared |
| `kurbo::Rect::inflate` | function | `jadix_2d::kurbo::Rect::inflate` | shared |
| `kurbo::Rect::inscribed_rect_with_aspect_ratio` | function | `jadix_2d::kurbo::Rect::inscribed_rect_with_aspect_ratio` | shared |
| `kurbo::Rect::inset` | function | `jadix_2d::kurbo::Rect::inset` | shared |
| `kurbo::Rect::intersect` | function | `jadix_2d::kurbo::Rect::intersect` | shared |
| `kurbo::Rect::is_finite` | function | `jadix_2d::kurbo::Rect::is_finite` | shared |
| `kurbo::Rect::is_nan` | function | `jadix_2d::kurbo::Rect::is_nan` | shared |
| `kurbo::Rect::is_zero_area` | function | `jadix_2d::kurbo::Rect::is_zero_area` | shared |
| `kurbo::Rect::max_x` | function | `jadix_2d::kurbo::Rect::max_x` | shared |
| `kurbo::Rect::max_y` | function | `jadix_2d::kurbo::Rect::max_y` | shared |
| `kurbo::Rect::min_x` | function | `jadix_2d::kurbo::Rect::min_x` | shared |
| `kurbo::Rect::min_y` | function | `jadix_2d::kurbo::Rect::min_y` | shared |
| `kurbo::Rect::new` | function | `jadix_2d::kurbo::Rect::new` | shared |
| `kurbo::Rect::origin` | function | `jadix_2d::kurbo::Rect::origin` | shared |
| `kurbo::Rect::overlaps` | function | `jadix_2d::kurbo::Rect::overlaps` | shared |
| `kurbo::Rect::round` | function | `jadix_2d::kurbo::Rect::round` | shared |
| `kurbo::Rect::scale_from_origin` | function | `jadix_2d::kurbo::Rect::scale_from_origin` | shared |
| `kurbo::Rect::set_coords` | function | `jadix_2d::kurbo::Rect::set_coords` | shared |
| `kurbo::Rect::size` | function | `jadix_2d::kurbo::Rect::size` | shared |
| `kurbo::Rect::to_ellipse` | function | `jadix_2d::kurbo::Rect::to_ellipse` | shared |
| `kurbo::Rect::to_rounded_rect` | function | `jadix_2d::kurbo::Rect::to_rounded_rect` | shared |
| `kurbo::Rect::trunc` | function | `jadix_2d::kurbo::Rect::trunc` | shared |
| `kurbo::Rect::union` | function | `jadix_2d::kurbo::Rect::union` | shared |
| `kurbo::Rect::union_pt` | function | `jadix_2d::kurbo::Rect::union_pt` | shared |
| `kurbo::Rect::width` | function | `jadix_2d::kurbo::Rect::width` | shared |
| `kurbo::Rect::with_origin` | function | `jadix_2d::kurbo::Rect::with_origin` | shared |
| `kurbo::Rect::with_size` | function | `jadix_2d::kurbo::Rect::with_size` | shared |
| `kurbo::Rect::x0` | struct_field | `jadix_2d::kurbo::Rect::x0` | shared |
| `kurbo::Rect::x1` | struct_field | `jadix_2d::kurbo::Rect::x1` | shared |
| `kurbo::Rect::y0` | struct_field | `jadix_2d::kurbo::Rect::y0` | shared |
| `kurbo::Rect::y1` | struct_field | `jadix_2d::kurbo::Rect::y1` | shared |
| `kurbo::RoundedRect` | struct | `jadix_2d::kurbo::RoundedRect` | shared |
| `kurbo::RoundedRect` | use | `jadix_2d::kurbo::RoundedRect` | shared |
| `kurbo::RoundedRect::center` | function | `jadix_2d::kurbo::RoundedRect::center` | shared |
| `kurbo::RoundedRect::from_origin_size` | function | `jadix_2d::kurbo::RoundedRect::from_origin_size` | shared |
| `kurbo::RoundedRect::from_points` | function | `jadix_2d::kurbo::RoundedRect::from_points` | shared |
| `kurbo::RoundedRect::from_rect` | function | `jadix_2d::kurbo::RoundedRect::from_rect` | shared |
| `kurbo::RoundedRect::height` | function | `jadix_2d::kurbo::RoundedRect::height` | shared |
| `kurbo::RoundedRect::is_finite` | function | `jadix_2d::kurbo::RoundedRect::is_finite` | shared |
| `kurbo::RoundedRect::is_nan` | function | `jadix_2d::kurbo::RoundedRect::is_nan` | shared |
| `kurbo::RoundedRect::new` | function | `jadix_2d::kurbo::RoundedRect::new` | shared |
| `kurbo::RoundedRect::origin` | function | `jadix_2d::kurbo::RoundedRect::origin` | shared |
| `kurbo::RoundedRect::radii` | function | `jadix_2d::kurbo::RoundedRect::radii` | shared |
| `kurbo::RoundedRect::rect` | function | `jadix_2d::kurbo::RoundedRect::rect` | shared |
| `kurbo::RoundedRect::width` | function | `jadix_2d::kurbo::RoundedRect::width` | shared |
| `kurbo::RoundedRectRadii` | struct | `jadix_2d::kurbo::RoundedRectRadii` | shared |
| `kurbo::RoundedRectRadii` | use | `jadix_2d::kurbo::RoundedRectRadii` | shared |
| `kurbo::RoundedRectRadii::abs` | function | `jadix_2d::kurbo::RoundedRectRadii::abs` | shared |
| `kurbo::RoundedRectRadii::as_single_radius` | function | `jadix_2d::kurbo::RoundedRectRadii::as_single_radius` | shared |
| `kurbo::RoundedRectRadii::bottom_left` | struct_field | `jadix_2d::kurbo::RoundedRectRadii::bottom_left` | shared |
| `kurbo::RoundedRectRadii::bottom_right` | struct_field | `jadix_2d::kurbo::RoundedRectRadii::bottom_right` | shared |
| `kurbo::RoundedRectRadii::clamp` | function | `jadix_2d::kurbo::RoundedRectRadii::clamp` | shared |
| `kurbo::RoundedRectRadii::from_single_radius` | function | `jadix_2d::kurbo::RoundedRectRadii::from_single_radius` | shared |
| `kurbo::RoundedRectRadii::is_finite` | function | `jadix_2d::kurbo::RoundedRectRadii::is_finite` | shared |
| `kurbo::RoundedRectRadii::is_nan` | function | `jadix_2d::kurbo::RoundedRectRadii::is_nan` | shared |
| `kurbo::RoundedRectRadii::new` | function | `jadix_2d::kurbo::RoundedRectRadii::new` | shared |
| `kurbo::RoundedRectRadii::top_left` | struct_field | `jadix_2d::kurbo::RoundedRectRadii::top_left` | shared |
| `kurbo::RoundedRectRadii::top_right` | struct_field | `jadix_2d::kurbo::RoundedRectRadii::top_right` | shared |
| `kurbo::Segments` | struct | `jadix_2d::kurbo::Segments` | shared |
| `kurbo::Segments` | use | `jadix_2d::kurbo::Segments` | shared |
| `kurbo::Shape` | trait | `jadix_2d::kurbo::Shape` | shared |
| `kurbo::Shape` | use | `jadix_2d::kurbo::Shape` | shared |
| `kurbo::Shape::PathElementsIter` | assoc_type | `jadix_2d::kurbo::Shape::PathElementsIter` | shared |
| `kurbo::Shape::area` | function | `jadix_2d::kurbo::Shape::area` | shared |
| `kurbo::Shape::as_circle` | function | `jadix_2d::kurbo::Shape::as_circle` | shared |
| `kurbo::Shape::as_line` | function | `jadix_2d::kurbo::Shape::as_line` | shared |
| `kurbo::Shape::as_path_slice` | function | `jadix_2d::kurbo::Shape::as_path_slice` | shared |
| `kurbo::Shape::as_rect` | function | `jadix_2d::kurbo::Shape::as_rect` | shared |
| `kurbo::Shape::as_rounded_rect` | function | `jadix_2d::kurbo::Shape::as_rounded_rect` | shared |
| `kurbo::Shape::bounding_box` | function | `jadix_2d::kurbo::Shape::bounding_box` | shared |
| `kurbo::Shape::contains` | function | `jadix_2d::kurbo::Shape::contains` | shared |
| `kurbo::Shape::into_path` | function | `jadix_2d::kurbo::Shape::into_path` | shared |
| `kurbo::Shape::path_elements` | function | `jadix_2d::kurbo::Shape::path_elements` | shared |
| `kurbo::Shape::path_segments` | function | `jadix_2d::kurbo::Shape::path_segments` | shared |
| `kurbo::Shape::perimeter` | function | `jadix_2d::kurbo::Shape::perimeter` | shared |
| `kurbo::Shape::to_path` | function | `jadix_2d::kurbo::Shape::to_path` | shared |
| `kurbo::Shape::winding` | function | `jadix_2d::kurbo::Shape::winding` | shared |
| `kurbo::Size` | struct | `jadix_2d::kurbo::Size` | shared |
| `kurbo::Size` | use | `jadix_2d::kurbo::Size` | shared |
| `kurbo::Size::INFINITY` | assoc_const | `jadix_2d::kurbo::Size::INFINITY` | shared |
| `kurbo::Size::ZERO` | assoc_const | `jadix_2d::kurbo::Size::ZERO` | shared |
| `kurbo::Size::area` | function | `jadix_2d::kurbo::Size::area` | shared |
| `kurbo::Size::aspect_ratio` | function | `jadix_2d::kurbo::Size::aspect_ratio` | shared |
| `kurbo::Size::aspect_ratio_width` | function | `jadix_2d::kurbo::Size::aspect_ratio_width` | shared |
| `kurbo::Size::ceil` | function | `jadix_2d::kurbo::Size::ceil` | shared |
| `kurbo::Size::clamp` | function | `jadix_2d::kurbo::Size::clamp` | shared |
| `kurbo::Size::expand` | function | `jadix_2d::kurbo::Size::expand` | shared |
| `kurbo::Size::floor` | function | `jadix_2d::kurbo::Size::floor` | shared |
| `kurbo::Size::get_coord` | function | `jadix_2d::kurbo::Size::get_coord` | shared |
| `kurbo::Size::get_coord_mut` | function | `jadix_2d::kurbo::Size::get_coord_mut` | shared |
| `kurbo::Size::height` | struct_field | `jadix_2d::kurbo::Size::height` | shared |
| `kurbo::Size::is_finite` | function | `jadix_2d::kurbo::Size::is_finite` | shared |
| `kurbo::Size::is_nan` | function | `jadix_2d::kurbo::Size::is_nan` | shared |
| `kurbo::Size::is_zero_area` | function | `jadix_2d::kurbo::Size::is_zero_area` | shared |
| `kurbo::Size::max` | function | `jadix_2d::kurbo::Size::max` | shared |
| `kurbo::Size::max_side` | function | `jadix_2d::kurbo::Size::max_side` | shared |
| `kurbo::Size::min` | function | `jadix_2d::kurbo::Size::min` | shared |
| `kurbo::Size::min_side` | function | `jadix_2d::kurbo::Size::min_side` | shared |
| `kurbo::Size::new` | function | `jadix_2d::kurbo::Size::new` | shared |
| `kurbo::Size::round` | function | `jadix_2d::kurbo::Size::round` | shared |
| `kurbo::Size::set_coord` | function | `jadix_2d::kurbo::Size::set_coord` | shared |
| `kurbo::Size::to_rect` | function | `jadix_2d::kurbo::Size::to_rect` | shared |
| `kurbo::Size::to_rounded_rect` | function | `jadix_2d::kurbo::Size::to_rounded_rect` | shared |
| `kurbo::Size::to_vec2` | function | `jadix_2d::kurbo::Size::to_vec2` | shared |
| `kurbo::Size::trunc` | function | `jadix_2d::kurbo::Size::trunc` | shared |
| `kurbo::Size::width` | struct_field | `jadix_2d::kurbo::Size::width` | shared |
| `kurbo::Stroke` | struct | `jadix_2d::kurbo::Stroke` | shared |
| `kurbo::Stroke` | use | `jadix_2d::kurbo::Stroke` | shared |
| `kurbo::Stroke::dash_offset` | struct_field | `jadix_2d::kurbo::Stroke::dash_offset` | shared |
| `kurbo::Stroke::dash_pattern` | struct_field | `jadix_2d::kurbo::Stroke::dash_pattern` | shared |
| `kurbo::Stroke::end_cap` | struct_field | `jadix_2d::kurbo::Stroke::end_cap` | shared |
| `kurbo::Stroke::is_finite` | function | `jadix_2d::kurbo::Stroke::is_finite` | shared |
| `kurbo::Stroke::is_nan` | function | `jadix_2d::kurbo::Stroke::is_nan` | shared |
| `kurbo::Stroke::join` | struct_field | `jadix_2d::kurbo::Stroke::join` | shared |
| `kurbo::Stroke::miter_limit` | struct_field | `jadix_2d::kurbo::Stroke::miter_limit` | shared |
| `kurbo::Stroke::new` | function | `jadix_2d::kurbo::Stroke::new` | shared |
| `kurbo::Stroke::start_cap` | struct_field | `jadix_2d::kurbo::Stroke::start_cap` | shared |
| `kurbo::Stroke::width` | struct_field | `jadix_2d::kurbo::Stroke::width` | shared |
| `kurbo::Stroke::with_caps` | function | `jadix_2d::kurbo::Stroke::with_caps` | shared |
| `kurbo::Stroke::with_dashes` | function | `jadix_2d::kurbo::Stroke::with_dashes` | shared |
| `kurbo::Stroke::with_end_cap` | function | `jadix_2d::kurbo::Stroke::with_end_cap` | shared |
| `kurbo::Stroke::with_join` | function | `jadix_2d::kurbo::Stroke::with_join` | shared |
| `kurbo::Stroke::with_miter_limit` | function | `jadix_2d::kurbo::Stroke::with_miter_limit` | shared |
| `kurbo::Stroke::with_start_cap` | function | `jadix_2d::kurbo::Stroke::with_start_cap` | shared |
| `kurbo::StrokeCtx` | struct | `jadix_2d::kurbo::StrokeCtx` | shared |
| `kurbo::StrokeCtx` | use | `jadix_2d::kurbo::StrokeCtx` | shared |
| `kurbo::StrokeCtx::output` | function | `jadix_2d::kurbo::StrokeCtx::output` | shared |
| `kurbo::StrokeOptLevel` | enum | `jadix_2d::kurbo::StrokeOptLevel` | shared |
| `kurbo::StrokeOptLevel` | use | `jadix_2d::kurbo::StrokeOptLevel` | shared |
| `kurbo::StrokeOptLevel::Optimized` | variant | `jadix_2d::kurbo::StrokeOptLevel::Optimized` | shared |
| `kurbo::StrokeOptLevel::Subdivide` | variant | `jadix_2d::kurbo::StrokeOptLevel::Subdivide` | shared |
| `kurbo::StrokeOpts` | struct | `jadix_2d::kurbo::StrokeOpts` | shared |
| `kurbo::StrokeOpts` | use | `jadix_2d::kurbo::StrokeOpts` | shared |
| `kurbo::StrokeOpts::opt_level` | function | `jadix_2d::kurbo::StrokeOpts::opt_level` | shared |
| `kurbo::StrokeOpts::stable_dash_order` | function | `jadix_2d::kurbo::StrokeOpts::stable_dash_order` | shared |
| `kurbo::SvgArc` | struct | `jadix_2d::kurbo::SvgArc` | shared |
| `kurbo::SvgArc` | use | `jadix_2d::kurbo::SvgArc` | shared |
| `kurbo::SvgArc::from` | struct_field | `jadix_2d::kurbo::SvgArc::from` | shared |
| `kurbo::SvgArc::is_straight_line` | function | `jadix_2d::kurbo::SvgArc::is_straight_line` | shared |
| `kurbo::SvgArc::large_arc` | struct_field | `jadix_2d::kurbo::SvgArc::large_arc` | shared |
| `kurbo::SvgArc::radii` | struct_field | `jadix_2d::kurbo::SvgArc::radii` | shared |
| `kurbo::SvgArc::sweep` | struct_field | `jadix_2d::kurbo::SvgArc::sweep` | shared |
| `kurbo::SvgArc::to` | struct_field | `jadix_2d::kurbo::SvgArc::to` | shared |
| `kurbo::SvgArc::x_rotation` | struct_field | `jadix_2d::kurbo::SvgArc::x_rotation` | shared |
| `kurbo::SvgParseError` | enum | `jadix_2d::kurbo::SvgParseError` | shared |
| `kurbo::SvgParseError` | use | `jadix_2d::kurbo::SvgParseError` | shared |
| `kurbo::SvgParseError::UnexpectedEof` | variant | `jadix_2d::kurbo::SvgParseError::UnexpectedEof` | shared |
| `kurbo::SvgParseError::UninitializedPath` | variant | `jadix_2d::kurbo::SvgParseError::UninitializedPath` | shared |
| `kurbo::SvgParseError::UnknownCommand` | variant | `jadix_2d::kurbo::SvgParseError::UnknownCommand` | shared |
| `kurbo::SvgParseError::UnknownCommand::0` | struct_field | `jadix_2d::kurbo::SvgParseError::UnknownCommand::0` | shared |
| `kurbo::SvgParseError::Wrong` | variant | `jadix_2d::kurbo::SvgParseError::Wrong` | shared |
| `kurbo::TranslateScale` | struct | `jadix_2d::kurbo::TranslateScale` | shared |
| `kurbo::TranslateScale` | use | `jadix_2d::kurbo::TranslateScale` | shared |
| `kurbo::TranslateScale::from_scale_about` | function | `jadix_2d::kurbo::TranslateScale::from_scale_about` | shared |
| `kurbo::TranslateScale::inverse` | function | `jadix_2d::kurbo::TranslateScale::inverse` | shared |
| `kurbo::TranslateScale::is_finite` | function | `jadix_2d::kurbo::TranslateScale::is_finite` | shared |
| `kurbo::TranslateScale::is_nan` | function | `jadix_2d::kurbo::TranslateScale::is_nan` | shared |
| `kurbo::TranslateScale::new` | function | `jadix_2d::kurbo::TranslateScale::new` | shared |
| `kurbo::TranslateScale::scale` | function | `jadix_2d::kurbo::TranslateScale::scale` | shared |
| `kurbo::TranslateScale::scale` | struct_field | `jadix_2d::kurbo::TranslateScale::scale` | shared |
| `kurbo::TranslateScale::translate` | function | `jadix_2d::kurbo::TranslateScale::translate` | shared |
| `kurbo::TranslateScale::translation` | struct_field | `jadix_2d::kurbo::TranslateScale::translation` | shared |
| `kurbo::Triangle` | struct | `jadix_2d::kurbo::Triangle` | shared |
| `kurbo::Triangle` | use | `jadix_2d::kurbo::Triangle` | shared |
| `kurbo::Triangle::EQUILATERAL` | assoc_const | `jadix_2d::kurbo::Triangle::EQUILATERAL` | shared |
| `kurbo::Triangle::ZERO` | assoc_const | `jadix_2d::kurbo::Triangle::ZERO` | shared |
| `kurbo::Triangle::a` | struct_field | `jadix_2d::kurbo::Triangle::a` | shared |
| `kurbo::Triangle::area` | function | `jadix_2d::kurbo::Triangle::area` | shared |
| `kurbo::Triangle::b` | struct_field | `jadix_2d::kurbo::Triangle::b` | shared |
| `kurbo::Triangle::c` | struct_field | `jadix_2d::kurbo::Triangle::c` | shared |
| `kurbo::Triangle::centroid` | function | `jadix_2d::kurbo::Triangle::centroid` | shared |
| `kurbo::Triangle::circumscribed_circle` | function | `jadix_2d::kurbo::Triangle::circumscribed_circle` | shared |
| `kurbo::Triangle::from_coords` | function | `jadix_2d::kurbo::Triangle::from_coords` | shared |
| `kurbo::Triangle::inflate` | function | `jadix_2d::kurbo::Triangle::inflate` | shared |
| `kurbo::Triangle::inscribed_circle` | function | `jadix_2d::kurbo::Triangle::inscribed_circle` | shared |
| `kurbo::Triangle::is_finite` | function | `jadix_2d::kurbo::Triangle::is_finite` | shared |
| `kurbo::Triangle::is_nan` | function | `jadix_2d::kurbo::Triangle::is_nan` | shared |
| `kurbo::Triangle::is_zero_area` | function | `jadix_2d::kurbo::Triangle::is_zero_area` | shared |
| `kurbo::Triangle::new` | function | `jadix_2d::kurbo::Triangle::new` | shared |
| `kurbo::Triangle::offsets` | function | `jadix_2d::kurbo::Triangle::offsets` | shared |
| `kurbo::Vec2` | struct | `jadix_2d::kurbo::Vec2` | shared |
| `kurbo::Vec2` | use | `jadix_2d::kurbo::Vec2` | shared |
| `kurbo::Vec2::ZERO` | assoc_const | `jadix_2d::kurbo::Vec2::ZERO` | shared |
| `kurbo::Vec2::angle` | function | `jadix_2d::kurbo::Vec2::angle` | shared |
| `kurbo::Vec2::atan2` | function | `jadix_2d::kurbo::Vec2::atan2` | shared |
| `kurbo::Vec2::ceil` | function | `jadix_2d::kurbo::Vec2::ceil` | shared |
| `kurbo::Vec2::cross` | function | `jadix_2d::kurbo::Vec2::cross` | shared |
| `kurbo::Vec2::dot` | function | `jadix_2d::kurbo::Vec2::dot` | shared |
| `kurbo::Vec2::expand` | function | `jadix_2d::kurbo::Vec2::expand` | shared |
| `kurbo::Vec2::floor` | function | `jadix_2d::kurbo::Vec2::floor` | shared |
| `kurbo::Vec2::from_angle` | function | `jadix_2d::kurbo::Vec2::from_angle` | shared |
| `kurbo::Vec2::get_coord` | function | `jadix_2d::kurbo::Vec2::get_coord` | shared |
| `kurbo::Vec2::get_coord_mut` | function | `jadix_2d::kurbo::Vec2::get_coord_mut` | shared |
| `kurbo::Vec2::hypot` | function | `jadix_2d::kurbo::Vec2::hypot` | shared |
| `kurbo::Vec2::hypot2` | function | `jadix_2d::kurbo::Vec2::hypot2` | shared |
| `kurbo::Vec2::is_finite` | function | `jadix_2d::kurbo::Vec2::is_finite` | shared |
| `kurbo::Vec2::is_nan` | function | `jadix_2d::kurbo::Vec2::is_nan` | shared |
| `kurbo::Vec2::length` | function | `jadix_2d::kurbo::Vec2::length` | shared |
| `kurbo::Vec2::length_squared` | function | `jadix_2d::kurbo::Vec2::length_squared` | shared |
| `kurbo::Vec2::lerp` | function | `jadix_2d::kurbo::Vec2::lerp` | shared |
| `kurbo::Vec2::new` | function | `jadix_2d::kurbo::Vec2::new` | shared |
| `kurbo::Vec2::normalize` | function | `jadix_2d::kurbo::Vec2::normalize` | shared |
| `kurbo::Vec2::rotate_scale` | function | `jadix_2d::kurbo::Vec2::rotate_scale` | shared |
| `kurbo::Vec2::round` | function | `jadix_2d::kurbo::Vec2::round` | shared |
| `kurbo::Vec2::set_coord` | function | `jadix_2d::kurbo::Vec2::set_coord` | shared |
| `kurbo::Vec2::splat` | function | `jadix_2d::kurbo::Vec2::splat` | shared |
| `kurbo::Vec2::to_point` | function | `jadix_2d::kurbo::Vec2::to_point` | shared |
| `kurbo::Vec2::to_size` | function | `jadix_2d::kurbo::Vec2::to_size` | shared |
| `kurbo::Vec2::trunc` | function | `jadix_2d::kurbo::Vec2::trunc` | shared |
| `kurbo::Vec2::turn_90` | function | `jadix_2d::kurbo::Vec2::turn_90` | shared |
| `kurbo::Vec2::x` | struct_field | `jadix_2d::kurbo::Vec2::x` | shared |
| `kurbo::Vec2::y` | struct_field | `jadix_2d::kurbo::Vec2::y` | shared |
| `kurbo::common` | module | `jadix_2d::kurbo::common` | shared |
| `kurbo::common::FloatExt` | trait | `jadix_2d::kurbo::common::FloatExt` | shared |
| `kurbo::common::FloatExt::expand` | function | `jadix_2d::kurbo::common::FloatExt::expand` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_11` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_11` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_16` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_16` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_16_HALF` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_16_HALF` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_24` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_24` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_24_HALF` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_24_HALF` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_3` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_3` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_32` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_32` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_32_HALF` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_32_HALF` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_4` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_4` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_5` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_5` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_6` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_6` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_7` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_7` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_8` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_8` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_8_HALF` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_8_HALF` | shared |
| `kurbo::common::GAUSS_LEGENDRE_COEFFS_9` | constant | `jadix_2d::kurbo::common::GAUSS_LEGENDRE_COEFFS_9` | shared |
| `kurbo::common::factor_quartic_inner` | function | `jadix_2d::kurbo::common::factor_quartic_inner` | shared |
| `kurbo::common::solve_cubic` | function | `jadix_2d::kurbo::common::solve_cubic` | shared |
| `kurbo::common::solve_itp` | function | `jadix_2d::kurbo::common::solve_itp` | shared |
| `kurbo::common::solve_quadratic` | function | `jadix_2d::kurbo::common::solve_quadratic` | shared |
| `kurbo::common::solve_quartic` | function | `jadix_2d::kurbo::common::solve_quartic` | shared |
| `kurbo::cubics_to_quadratic_splines` | function | `jadix_2d::kurbo::cubics_to_quadratic_splines` | shared |
| `kurbo::cubics_to_quadratic_splines` | use | `jadix_2d::kurbo::cubics_to_quadratic_splines` | shared |
| `kurbo::dash` | function | `jadix_2d::kurbo::dash` | shared |
| `kurbo::dash` | use | `jadix_2d::kurbo::dash` | shared |
| `kurbo::expand_path` | function | `jadix_2d::kurbo::expand_path` | shared |
| `kurbo::expand_path` | use | `jadix_2d::kurbo::expand_path` | shared |
| `kurbo::expand_path_signed` | function | `jadix_2d::kurbo::expand_path_signed` | shared |
| `kurbo::expand_path_signed` | use | `jadix_2d::kurbo::expand_path_signed` | shared |
| `kurbo::fit_to_bezpath` | function | `jadix_2d::kurbo::fit_to_bezpath` | shared |
| `kurbo::fit_to_bezpath` | use | `jadix_2d::kurbo::fit_to_bezpath` | shared |
| `kurbo::fit_to_bezpath_opt` | function | `jadix_2d::kurbo::fit_to_bezpath_opt` | shared |
| `kurbo::fit_to_bezpath_opt` | use | `jadix_2d::kurbo::fit_to_bezpath_opt` | shared |
| `kurbo::fit_to_cubic` | function | `jadix_2d::kurbo::fit_to_cubic` | shared |
| `kurbo::fit_to_cubic` | use | `jadix_2d::kurbo::fit_to_cubic` | shared |
| `kurbo::flatten` | function | `jadix_2d::kurbo::flatten` | shared |
| `kurbo::flatten` | use | `jadix_2d::kurbo::flatten` | shared |
| `kurbo::offset` | module | `jadix_2d::kurbo::offset` | shared |
| `kurbo::offset::offset_cubic` | function | `jadix_2d::kurbo::offset::offset_cubic` | shared |
| `kurbo::segments` | function | `jadix_2d::kurbo::segments` | shared |
| `kurbo::segments` | use | `jadix_2d::kurbo::segments` | shared |
| `kurbo::simplify` | module | `jadix_2d::kurbo::simplify` | shared |
| `kurbo::simplify::SimplifyBezPath` | struct | `jadix_2d::kurbo::simplify::SimplifyBezPath` | shared |
| `kurbo::simplify::SimplifyBezPath::new` | function | `jadix_2d::kurbo::simplify::SimplifyBezPath::new` | shared |
| `kurbo::simplify::SimplifyOptLevel` | enum | `jadix_2d::kurbo::simplify::SimplifyOptLevel` | shared |
| `kurbo::simplify::SimplifyOptLevel::Optimize` | variant | `jadix_2d::kurbo::simplify::SimplifyOptLevel::Optimize` | shared |
| `kurbo::simplify::SimplifyOptLevel::Subdivide` | variant | `jadix_2d::kurbo::simplify::SimplifyOptLevel::Subdivide` | shared |
| `kurbo::simplify::SimplifyOptions` | struct | `jadix_2d::kurbo::simplify::SimplifyOptions` | shared |
| `kurbo::simplify::SimplifyOptions::angle_thresh` | function | `jadix_2d::kurbo::simplify::SimplifyOptions::angle_thresh` | shared |
| `kurbo::simplify::SimplifyOptions::opt_level` | function | `jadix_2d::kurbo::simplify::SimplifyOptions::opt_level` | shared |
| `kurbo::simplify::simplify_bezpath` | function | `jadix_2d::kurbo::simplify::simplify_bezpath` | shared |
| `kurbo::stroke` | function | `jadix_2d::kurbo::stroke` | shared |
| `kurbo::stroke` | use | `jadix_2d::kurbo::stroke` | shared |
| `kurbo::stroke_with` | function | `jadix_2d::kurbo::stroke_with` | shared |
| `kurbo::stroke_with` | use | `jadix_2d::kurbo::stroke_with` | shared |
### vello_encoding 0.11.0

459 项。Vello compute encoding/resolve ABI 不兼容，不以相似名称宣称等价。 完整签名/条件属性/trait 见 `benchmarks/vello-api/inventory/vello_encoding-0.11.0.json`。

| 项 | 类别 | Jadix 对应 | 状态 |
|---|---|---|---|
| `vello_encoding` | module | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BinHeader` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BinHeader` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BinHeader::chunk_offset` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BinHeader::element_count` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSize` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSize` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSize::aligned_in_bytes` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSize::from_size_in_bytes` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSize::len` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSize::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSize::size_in_bytes` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::bin_data` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::bin_headers` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::blend_spill` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::bump_alloc` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::clip_bboxes` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::clip_bics` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::clip_els` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::clip_inps` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::draw_bboxes` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::draw_monoids` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::draw_reduced` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::indirect_count` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::info` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::lines` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::path_bboxes` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::path_monoids` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::path_reduced` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::path_reduced2` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::path_reduced_scan` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::paths` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::ptcl` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::seg_counts` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::segments` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BufferSizes::tiles` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory::binning` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory::lines` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory::ptcl` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory::seg_counts` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory::segments` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory::tile` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocatorMemory::total` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::binning` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::blend` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::failed` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::lines` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::memory` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::ptcl` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::seg_counts` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::segments` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpAllocators::tile` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpEstimator` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpEstimator` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpEstimator::append` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpEstimator::count_path` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpEstimator::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpEstimator::reset` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::BumpEstimator::tally` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Clip` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Clip` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Clip::ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Clip::path_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBbox` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBbox` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBbox::bbox` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBic` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBic` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBic::a` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBic::b` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBic::combine` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipBic::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipElement` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipElement` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipElement::bbox` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ClipElement::parent_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::base_color` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::binning_size` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::blend_size` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::height_in_tiles` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::layout` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::lines_size` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::ptcl_size` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::seg_counts_size` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::segments_size` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::target_height` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::target_width` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::tiles_size` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::ConfigUniform::width_in_tiles` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic::flags` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic::p0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic::p1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic::p2` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic::p3` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic::path_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Cubic::stroke` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DRAW_INFO_FLAGS_FILL_RULE_BIT` | constant | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DRAW_INFO_FLAGS_FILL_RULE_BIT` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBbox` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBbox` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBbox::bbox` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip::CLIP_BLEND_MODE` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip::LUMINANCE_MASK_BLEND_MODE` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip::alpha` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip::blend_mode` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip::clip` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip::luminance_mask` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBeginClip::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBlurRoundedRect` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBlurRoundedRect` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBlurRoundedRect::color` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBlurRoundedRect::height` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBlurRoundedRect::radius` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBlurRoundedRect::std_dev` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawBlurRoundedRect::width` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawColor` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawColor` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawColor::rgba` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawImage` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawImage` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawImage::sample_alpha` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawImage::width_height` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawImage::xy` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawLinearGradient` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawLinearGradient` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawLinearGradient::index` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawLinearGradient::p0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawLinearGradient::p1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawMonoid` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawMonoid` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawMonoid::clip_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawMonoid::info_offset` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawMonoid::path_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawMonoid::scene_offset` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawRadialGradient` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawRadialGradient` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawRadialGradient::index` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawRadialGradient::p0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawRadialGradient::p1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawRadialGradient::r0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawRadialGradient::r1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawSweepGradient` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawSweepGradient` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawSweepGradient::index` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawSweepGradient::p0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawSweepGradient::t0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawSweepGradient::t1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::BEGIN_CLIP` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::BLUR_RECT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::COLOR` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::END_CLIP` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::IMAGE` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::LINEAR_GRADIENT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::NOP` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::RADIAL_GRADIENT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::SWEEP_GRADIENT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::DrawTag::info_size` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::FORCE_NEXT_STYLE` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::FORCE_NEXT_TRANSFORM` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::append` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::draw_data` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::draw_tags` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_begin_clip` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_blurred_rounded_rect` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_brush` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_color` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_empty_shape` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_end_clip` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_fill_style` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_image` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_linear_gradient` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_path` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_path_elements` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_radial_gradient` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_shape` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_stroke_style` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_sweep_gradient` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::encode_transform` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::flags` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::force_next_transform_and_style` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::is_empty` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::n_clips` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::n_open_clips` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::n_path_segments` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::n_paths` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::path_data` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::path_tags` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::reset` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::resources` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::stream_offsets` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::styles` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::swap_last_path_tags` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Encoding::transforms` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden::amount` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden::join` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden::miter_limit` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden::tolerance` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden::with_join` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden::with_miter_limit` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::FontEmbolden::with_tolerance` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Glyph` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Glyph` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Glyph::id` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Glyph::x` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Glyph::y` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::brush_transform` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::font` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::font_embolden` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::font_size` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::glyph_transform` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::glyphs` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::hint` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::normalized_coords` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::stream_offsets` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::style` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::GlyphRun::transform` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::IndirectCount` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::IndirectCount` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::IndirectCount::count_x` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::IndirectCount::count_y` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::IndirectCount::count_z` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::IndirectCount::pad0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::bin_data_start` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::draw_data` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::draw_data_base` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::draw_tag_base` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::draw_tags` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::n_clips` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::n_draw_objects` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::n_paths` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::path_data` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::path_data_base` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::path_tag_base` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::path_tags` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::path_tags_chunked` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::path_tags_size` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::style_base` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::styles` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::transform_base` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Layout::transforms` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::LineSoup` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::LineSoup` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::LineSoup::_padding` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::LineSoup::p0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::LineSoup::p1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::LineSoup::path_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Monoid` | trait | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Monoid` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Monoid::SourceValue` | assoc_type | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Monoid::combine` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Monoid::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::NormalizedCoord` | type_alias | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch` | enum | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::GlyphRun` | variant | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::GlyphRun::index` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::Image` | variant | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::Image::draw_data_offset` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::Image::image` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::Ramp` | variant | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::Ramp::draw_data_offset` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::Ramp::extend` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::Ramp::interpolation_alpha_space` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Patch::Ramp::stops` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Path` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Path` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Path::bbox` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Path::tiles` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathBbox` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathBbox` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathBbox::draw_flags` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathBbox::trans_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathBbox::x0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathBbox::x1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathBbox::y0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathBbox::y1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::close` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::cubic_to` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::finish` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::line_to` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::move_to` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::path_elements` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::quad_to` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathEncoder::shape` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathMonoid` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathMonoid` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathMonoid::path_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathMonoid::pathseg_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathMonoid::pathseg_offset` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathMonoid::style_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathMonoid::trans_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegment` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegment` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegment::_padding` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegment::point0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegment::point1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegment::y_edge` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegmentType` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegmentType` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegmentType::0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegmentType::CUBIC_TO` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegmentType::LINE_TO` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathSegmentType::QUAD_TO` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::0` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::CUBIC_TO_F32` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::CUBIC_TO_I16` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::LINE_TO_F32` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::LINE_TO_I16` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::PATH` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::QUAD_TO_F32` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::QUAD_TO_I16` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::STYLE` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::SUBPATH_END_BIT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::TRANSFORM` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::is_f32` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::is_path_segment` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::is_subpath_end` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::path_segment_type` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::PathTag::set_subpath_end` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Ramps` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Ramps` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Ramps::data` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Ramps::height` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Ramps::width` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::RenderConfig` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::RenderConfig` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::RenderConfig::buffer_sizes` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::RenderConfig::gpu` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::RenderConfig::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::RenderConfig::workgroup_counts` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resolver` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resolver` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resolver::mark_image_dirty` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resolver::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resolver::resolve` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resources` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resources` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resources::color_stops` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resources::glyph_runs` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resources::glyphs` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resources::normalized_coords` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Resources::patches` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::SegmentCount` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::SegmentCount` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::SegmentCount::counts` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::SegmentCount::line_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::StreamOffsets` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::StreamOffsets` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::StreamOffsets::draw_data` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::StreamOffsets::draw_tags` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::StreamOffsets::path_data` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::StreamOffsets::path_tags` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::StreamOffsets::styles` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::StreamOffsets::transforms` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_CAP_BITS_BUTT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_CAP_BITS_ROUND` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_CAP_BITS_SQUARE` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_END_CAP_BITS_BUTT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_END_CAP_BITS_ROUND` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_END_CAP_BITS_SQUARE` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_END_CAP_MASK` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_FILL_BIT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_JOIN_BITS_BEVEL` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_JOIN_BITS_MITER` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_JOIN_BITS_ROUND` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_JOIN_MASK` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_START_CAP_BITS_BUTT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_START_CAP_BITS_ROUND` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_START_CAP_BITS_SQUARE` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_START_CAP_MASK` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::FLAGS_STYLE_BIT` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::MITER_LIMIT_MASK` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::flags_and_miter_limit` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::from_fill` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::from_stroke` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Style::line_width` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Tile` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Tile` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Tile::backdrop` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Tile::segment_count_or_ix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Transform` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Transform` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Transform::IDENTITY` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Transform::from_kurbo` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Transform::matrix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Transform::to_kurbo` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::Transform::translation` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::backdrop` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::bbox_clear` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::binning` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::clip_leaf` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::clip_reduce` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::coarse` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::draw_leaf` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::draw_reduce` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::fine` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::flatten` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::new` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::path_count_setup` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::path_reduce` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::path_reduce2` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::path_scan` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::path_scan1` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::path_tiling_setup` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::tile_alloc` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupCounts::use_large_path_scan` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupSize` | type_alias | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::WorkgroupSize` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::make_mask_lut` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::make_mask_lut` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::make_mask_lut_16` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::make_mask_lut_16` | use | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math` | module | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math::Transform` | struct | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math::Transform::IDENTITY` | assoc_const | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math::Transform::from_kurbo` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math::Transform::matrix` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math::Transform::to_kurbo` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math::Transform::translation` | struct_field | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math::f16_to_f32` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::math::point_to_f32` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::resolve_solid_paths_only` | function | `PathGpuOp / PreparedSceneGpu` | different |
| `vello_encoding::resolve_solid_paths_only` | use | `PathGpuOp / PreparedSceneGpu` | different |
### linebender_resource_handle 0.1.1

21 项。Arc 资源所有权有对应设计；不是可替换的 Blob/FontData 类型。 完整签名/条件属性/trait 见 `benchmarks/vello-api/inventory/linebender_resource_handle-0.1.1.json`。

| 项 | 类别 | Jadix 对应 | 状态 |
|---|---|---|---|
| `linebender_resource_handle` | module | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob` | struct | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob` | use | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::data` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::downgrade` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::from_raw_parts` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::id` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::into_raw_parts` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::is_empty` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::len` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::new` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::Blob::strong_count` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::FontData` | struct | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::FontData` | use | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::FontData::data` | struct_field | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::FontData::index` | struct_field | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::FontData::new` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::WeakBlob` | struct | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::WeakBlob` | use | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::WeakBlob::id` | function | `Image2D / Font2D shared snapshots` | different |
| `linebender_resource_handle::WeakBlob::upgrade` | function | `Image2D / Font2D shared snapshots` | different |
### wgpu 30.0.0

1322 项。upstream vs 工作区 fork；命令、适配器、buffer/texture/surface 概念共同，host 负责资源状态与平台接入。 完整签名/条件属性/trait 见 `benchmarks/vello-api/inventory/wgpu-30.0.0.json`。

| 项 | 类别 | Jadix 对应 | 状态 |
|---|---|---|---|
| `wgpu` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::AdapterInfo` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::AddressMode` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::AllocatorReport` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::AstcBlock` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::AstcChannel` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Backend` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BackendOptions` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Backends` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BindGroupLayoutEntry` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BindingType` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BlendComponent` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BlendFactor` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BlendOperation` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BlendState` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BufferAddress` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BufferBindingType` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BufferSize` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BufferTextureCopyInfo` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BufferTransition` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BufferUsages` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::BufferUses` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::COPY_BUFFER_ALIGNMENT` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Color` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ColorTargetState` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ColorWrites` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::CommandBufferDescriptor` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::CompareFunction` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::CompositeAlphaMode` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::CooperativeMatrixProperties` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::CooperativeScalarType` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::CopyExternalImageDestInfo` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::CoreCounters` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DepthBiasState` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DepthStencilState` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DeviceLostReason` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DeviceType` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DisplayChromaticity` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DisplayCoarseRange` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DisplayGamut` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DisplayHdrInfo` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DisplayHeadroom` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DisplayLuminance` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DownlevelCapabilities` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DownlevelFlags` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DownlevelLimits` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Dx12BackendOptions` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Dx12Compiler` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Dx12SwapchainKind` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Dx12UseFrameLatencyWaitableObject` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DxcShaderModel` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::DynamicOffset` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ExperimentalFeatures` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Extent3d` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ExternalTextureFormat` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ExternalTextureTransferFunction` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Face` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Features` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::FeaturesWGPU` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::FeaturesWebGPU` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::FilterMode` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ForceShaderModelToken` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::FrontFace` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::GlBackendOptions` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::GlDebugFns` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::GlFenceBehavior` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Gles3MinorVersion` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::HalCounters` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::IMMEDIATE_DATA_ALIGNMENT` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ImageSubresourceRange` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::IndexFormat` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::InstanceDescriptor` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::InstanceFlags` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::InternalCounters` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Label` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Limits` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::LoadOpDontCare` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::MAP_ALIGNMENT` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::MAXIMUM_SUBGROUP_MAX_SIZE` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::MINIMUM_SUBGROUP_MIN_SIZE` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::MemoryBudgetThresholds` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::MemoryHints` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::MipmapFilterMode` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::MultisampleState` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::NoopBackendOptions` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Origin2d` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Origin3d` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PassthroughShaderEntryPoint` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PipelineStatisticsTypes` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PollError` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PollStatus` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PolygonMode` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PowerPreference` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PredefinedColorSpace` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PresentMode` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PresentationTimestamp` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PrimitiveState` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::PrimitiveTopology` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::QUERY_SET_MAX_QUERIES` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::QUERY_SIZE` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::QueryType` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::RenderBundleDepthStencil` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::RequestAdapterError` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::SamplerBindingType` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::SamplerBorderColor` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ShaderLocation` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ShaderModel` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ShaderRuntimeChecks` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::ShaderStages` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::StencilFaceState` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::StencilOperation` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::StencilState` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::StorageTextureAccess` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::SurfaceCapabilities` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::SurfaceColorSpace` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::SurfaceColorSpaces` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::SurfaceFormatCapabilities` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::SurfaceStatus` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TexelCopyBufferLayout` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureAspect` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureChannel` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureDimension` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureFormat` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureFormatFeatureFlags` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureFormatFeatures` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureSampleType` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureTransition` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureUsages` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureUses` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::TextureViewDimension` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::Trace` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::VERTEX_ALIGNMENT` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::VERTEX_STRIDE_ALIGNMENT` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::VertexAttribute` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::VertexFormat` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::VertexStepMode` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::WasmNotSend` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::WasmNotSendSync` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::WasmNotSync` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::WriteOnly` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::WriteOnlyIter` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::cooperative_matrix_properties` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::create_device_from_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::from_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::get_downlevel_capabilities` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::get_info` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::get_presentation_timestamp` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::get_texture_format_features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::is_surface_supported` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::limits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::Adapter::request_device` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::RequestAdapterOptions` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::adapter::RequestAdapterOptionsBase` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroup` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroup::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroupDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroupDescriptor::entries` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroupDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroupDescriptor::layout` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroupEntry` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroupEntry::binding` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindGroupEntry::resource` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::AccelerationStructure` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::AccelerationStructure::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::AccelerationStructureArray` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::AccelerationStructureArray::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::Buffer` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::Buffer::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::BufferArray` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::BufferArray::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::ExternalTexture` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::ExternalTexture::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::Sampler` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::Sampler::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::SamplerArray` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::SamplerArray::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::TextureView` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::TextureView::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::TextureViewArray` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BindingResource::TextureViewArray::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BufferBinding` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BufferBinding::buffer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BufferBinding::offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group::BufferBinding::size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group_layout` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group_layout` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group_layout::BindGroupLayout` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group_layout::BindGroupLayout::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group_layout::BindGroupLayoutDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group_layout::BindGroupLayoutDescriptor::entries` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::bind_group_layout::BindGroupLayoutDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::AABB_GEOMETRY_MIN_STRIDE` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::AccelerationStructureFlags` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::AccelerationStructureGeometryFlags` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::AccelerationStructureUpdateMode` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::Blas` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::Blas::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::Blas::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::Blas::handle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::Blas::prepare_compaction_async` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::Blas::ready_for_compaction` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasAABBGeometrySizeDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasAabbGeometry` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasAabbGeometry::aabb_buffer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasAabbGeometry::primitive_offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasAabbGeometry::size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasAabbGeometry::stride` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasAsyncError` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasBuildEntry` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasBuildEntry::blas` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasBuildEntry::geometry` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasGeometries` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasGeometries::AabbGeometries` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasGeometries::AabbGeometries::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasGeometries::TriangleGeometries` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasGeometries::TriangleGeometries::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasGeometrySizeDescriptors` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry::first_index` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry::first_vertex` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry::index_buffer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry::size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry::transform_buffer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry::transform_buffer_offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry::vertex_buffer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometry::vertex_stride` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::BlasTriangleGeometrySizeDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::ContextBlasAabbGeometry` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::ContextBlasBuildEntry` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::ContextBlasGeometries` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::ContextBlasGeometries::AabbGeometries` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::ContextBlasGeometries::AabbGeometries::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::ContextBlasGeometries::TriangleGeometries` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::ContextBlasGeometries::TriangleGeometries::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::ContextBlasTriangleGeometry` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::CreateBlasDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::TlasInstance` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::TlasInstance::blas_as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::TlasInstance::custom_data` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::TlasInstance::mask` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::TlasInstance::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::TlasInstance::set_blas` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::blas::TlasInstance::transform` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::as_entire_binding` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::as_entire_buffer_binding` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::get_mapped_range` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::get_mapped_range_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::map_async` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::size` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::unmap` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::Buffer::usage` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferAsyncError` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferSlice` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferSlice::buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferSlice::get_mapped_range` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferSlice::get_mapped_range_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferSlice::map_async` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferSlice::offset` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferSlice::size` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferSlice::slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferView` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferViewMut` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferViewMut::copy_from_slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferViewMut::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferViewMut::len` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::BufferViewMut::slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::MapMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::MapMode::Read` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::MapMode::Write` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::buffer::MapRangeError` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_buffer` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_buffer` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_buffer::CommandBuffer` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_buffer::CommandBuffer::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_buffer::CommandBuffer::map_buffer_on_submit` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_buffer::CommandBuffer::on_submitted_work_done` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::as_hal_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::begin_compute_pass` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::begin_render_pass` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::build_acceleration_structures` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::clear_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::clear_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::copy_buffer_to_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::copy_buffer_to_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::copy_texture_to_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::copy_texture_to_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::finish` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::insert_debug_marker` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::map_buffer_on_submit` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::mark_acceleration_structures_built` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::on_submitted_work_done` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::pop_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::push_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::resolve_query_set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::transition_resources` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoder::write_timestamp` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::CommandEncoderDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::TexelCopyBufferInfo` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::TexelCopyBufferInfoBase` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::TexelCopyTextureInfo` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::command_encoder::TexelCopyTextureInfoBase` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline::PipelineCacheDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline::PipelineCacheDescriptor::data` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline::PipelineCacheDescriptor::fallback` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline::PipelineCacheDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline::PipelineCompilationOptions` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline::PipelineCompilationOptions::constants` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::common_pipeline::PipelineCompilationOptions::zero_initialize_workgroup_memory` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::begin_pipeline_statistics_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::dispatch_workgroups` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::dispatch_workgroups_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::end_pipeline_statistics_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::forget_lifetime` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::insert_debug_marker` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::map_buffer_on_submit` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::on_submitted_work_done` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::pop_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::push_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::set_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::set_immediates` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::set_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::transition_resources` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePass::write_timestamp` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePassDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePassDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePassDescriptor::timestamp_writes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePassTimestampWrites` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePassTimestampWrites::beginning_of_pass_write_index` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePassTimestampWrites::end_of_pass_write_index` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pass::ComputePassTimestampWrites::query_set` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipeline` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipeline::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipeline::get_bind_group_layout` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipelineDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipelineDescriptor::cache` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipelineDescriptor::compilation_options` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipelineDescriptor::entry_point` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipelineDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipelineDescriptor::layout` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::compute_pipeline::ComputePipelineDescriptor::module` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::cooperative_matrix_properties` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::downlevel_capabilities` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::get_info` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::get_presentation_timestamp` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::get_texture_format_features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::is_surface_supported` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::limits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AdapterInterface::request_device` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AsAny` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::AsAny::as_any` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BindGroupInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BindGroupLayoutInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BlasCompactCallback` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BlasInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BlasInterface::prepare_compact_async` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BlasInterface::ready_for_compaction` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BoxDeviceLostCallback` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BoxSubmittedWorkDoneCallback` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferInterface::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferInterface::get_mapped_range` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferInterface::map_async` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferInterface::unmap` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferMapCallback` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferMappedRangeInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferMappedRangeInterface::len` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferMappedRangeInterface::read_slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::BufferMappedRangeInterface::write_slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandBufferInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::begin_compute_pass` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::begin_render_pass` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::build_acceleration_structures` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::clear_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::clear_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::copy_buffer_to_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::copy_buffer_to_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::copy_texture_to_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::copy_texture_to_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::finish` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::insert_debug_marker` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::mark_acceleration_structures_built` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::pop_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::push_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::resolve_query_set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::transition_resources` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommandEncoderInterface::write_timestamp` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::CommonTraits` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::begin_pipeline_statistics_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::dispatch_workgroups` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::dispatch_workgroups_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::end_pipeline_statistics_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::insert_debug_marker` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::pop_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::push_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::set_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::set_immediates` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::set_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::transition_resources` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePassInterface::write_timestamp` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePipelineInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ComputePipelineInterface::get_bind_group_layout` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::adapter_info` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_bind_group_layout` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_blas` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_command_encoder` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_compute_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_external_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_mesh_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_pipeline_cache` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_pipeline_layout` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_query_set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_render_bundle_encoder` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_render_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_sampler` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_shader_module` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_shader_module_passthrough` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::create_tlas` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::generate_allocator_report` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::get_internal_counters` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::limits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::on_uncaptured_error` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::poll` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::pop_error_scope` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::push_error_scope` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::set_device_lost_callback` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::start_graphics_debugger_capture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DeviceInterface::stop_graphics_debugger_capture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchAdapter::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroup::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBindGroupLayout::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBlas::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBuffer::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::as_core_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::as_core_mut_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchBufferMappedRange::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::as_core_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::as_core_mut_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandBuffer::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::as_core_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::as_core_mut_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchCommandEncoder::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::as_core_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::as_core_mut_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePass::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchComputePipeline::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchDevice::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchExternalTexture::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchInstance::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineCache::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchPipelineLayout::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQuerySet::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueue::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::as_core_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::as_core_mut_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchQueueWriteBuffer::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundle::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::as_core_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::as_core_mut_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderBundleEncoder::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::as_core_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::as_core_mut_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPass::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchRenderPipeline::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSampler::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchShaderModule::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurface::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchSurfaceOutputDetail::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTexture::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTextureView::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas::Core::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas::Custom` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas::Custom::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas::as_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas::as_core_opt` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::DispatchTlas::custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::EnumerateAdapterFuture` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ExternalTextureInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ExternalTextureInterface::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::InstanceInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::InstanceInterface::create_surface` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::InstanceInterface::enumerate_adapters` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::InstanceInterface::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::InstanceInterface::poll_all_devices` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::InstanceInterface::request_adapter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::InstanceInterface::wgsl_language_features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::PipelineCacheInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::PipelineCacheInterface::get_data` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::PipelineLayoutInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::PopErrorScopeFuture` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QuerySetInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QuerySetInterface::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::compact_blas` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::create_staging_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::get_timestamp_period` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::on_submitted_work_done` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::present` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::submit` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::validate_write_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::write_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::write_staging_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueInterface::write_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueWriteBufferInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueWriteBufferInterface::len` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::QueueWriteBufferInterface::write_slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::draw` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::draw_indexed` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::draw_indexed_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::draw_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::finish` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::finish_boxed` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::set_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::set_immediates` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::set_index_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::set_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleEncoderInterface::set_vertex_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderBundleInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::begin_occlusion_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::begin_pipeline_statistics_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::draw` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::draw_indexed` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::draw_indexed_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::draw_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::draw_mesh_tasks` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::draw_mesh_tasks_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::end_occlusion_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::end_pipeline_statistics_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::execute_bundles` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::insert_debug_marker` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::multi_draw_indexed_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::multi_draw_indexed_indirect_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::multi_draw_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::multi_draw_indirect_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::multi_draw_mesh_tasks_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::multi_draw_mesh_tasks_indirect_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::pop_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::push_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_blend_constant` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_immediates` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_index_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_scissor_rect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_stencil_reference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_vertex_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::set_viewport` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPassInterface::write_timestamp` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPipelineInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RenderPipelineInterface::get_bind_group_layout` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RequestAdapterFuture` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::RequestDeviceFuture` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SamplerInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ShaderCompilationInfoFuture` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ShaderModuleInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::ShaderModuleInterface::get_compilation_info` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SurfaceInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SurfaceInterface::configure` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SurfaceInterface::display_hdr_info` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SurfaceInterface::get_capabilities` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SurfaceInterface::get_current_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SurfaceOutputDetailInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SurfaceOutputDetailInterface::texture_discard` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::SurfaceOutputDetailInterface::texture_release` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::TextureInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::TextureInterface::create_view` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::TextureInterface::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::TextureViewInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::custom::TlasInterface` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::adapter_info` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_bind_group_layout` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_blas` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_buffer_from_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_command_encoder` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_compute_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_external_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_mesh_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_pipeline_cache` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_pipeline_layout` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_query_set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_render_bundle_encoder` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_render_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_sampler` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_shader_module` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_shader_module_passthrough` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_shader_module_trusted` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_shader_module_unchecked` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_texture_from_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::create_tlas` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::from_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::generate_allocator_report` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::get_internal_counters` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::limits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::noop` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::on_uncaptured_error` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::poll` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::push_error_scope` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::set_device_lost_callback` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::start_graphics_debugger_capture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Device::stop_graphics_debugger_capture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::DeviceDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error::Internal` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error::Internal::description` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error::Internal::source` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error::OutOfMemory` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error::OutOfMemory::source` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error::Validation` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error::Validation::description` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::Error::Validation::source` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::ErrorFilter` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::ErrorFilter::Internal` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::ErrorFilter::OutOfMemory` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::ErrorFilter::Validation` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::ErrorScopeGuard` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::ErrorScopeGuard::pop` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::ErrorSource` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::RequestDeviceError` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::RequestDeviceError::from_message` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::device::UncapturedErrorHandler` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::external_texture` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::external_texture` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::external_texture::ExternalTexture` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::external_texture::ExternalTexture::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::external_texture::ExternalTexture::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::external_texture::ExternalTextureDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::hal` | extern_crate | `jadix_2d::wgpu` | shared-fork |
| `wgpu::include_spirv` | macro | `jadix_2d::wgpu` | shared-fork |
| `wgpu::include_spirv_raw` | macro | `jadix_2d::wgpu` | shared-fork |
| `wgpu::include_wgsl` | macro | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::create_adapter_from_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::create_surface` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::create_surface_unsafe` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::enabled_backend_features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::enumerate_adapters` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::from_core` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::from_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::from_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::generate_report` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::poll_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::request_adapter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::Instance::wgsl_language_features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::Packed4x8IntegerDotProduct` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::PointerCompositeAccess` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::ReadOnlyAndReadWriteStorageTextures` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::UnrestrictedPointerParameters` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::instance::WgslLanguageFeatures::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::naga` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_cache` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_cache` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_cache::PipelineCache` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_cache::PipelineCache::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_cache::PipelineCache::get_data` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_layout` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_layout` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_layout::PipelineLayout` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_layout::PipelineLayout::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_layout::PipelineLayoutDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_layout::PipelineLayoutDescriptor::bind_group_layouts` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_layout::PipelineLayoutDescriptor::immediate_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::pipeline_layout::PipelineLayoutDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::query_set` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::query_set` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::query_set::QuerySet` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::query_set::QuerySet::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::query_set::QuerySet::count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::query_set::QuerySet::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::query_set::QuerySet::ty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::query_set::QuerySetDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::PollType` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::compact_blas` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::from_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::get_timestamp_period` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::on_submitted_work_done` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::present` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::submit` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::write_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::write_buffer_with` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::Queue::write_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::QueueWriteBufferView` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::QueueWriteBufferView::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::QueueWriteBufferView::copy_from_slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::QueueWriteBufferView::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::QueueWriteBufferView::len` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::QueueWriteBufferView::slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::queue::SubmissionIndex` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle::RenderBundle` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle::RenderBundle::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle::RenderBundleDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::draw` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::draw_indexed` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::draw_indexed_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::draw_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::finish` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::set_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::set_immediates` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::set_index_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::set_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoder::set_vertex_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoderDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoderDescriptor::color_formats` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoderDescriptor::depth_stencil` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoderDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoderDescriptor::multiview` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_bundle_encoder::RenderBundleEncoderDescriptor::sample_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::LoadOp` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::Operations` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::begin_occlusion_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::begin_pipeline_statistics_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::draw` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::draw_indexed` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::draw_indexed_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::draw_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::draw_mesh_tasks` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::draw_mesh_tasks_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::end_occlusion_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::end_pipeline_statistics_query` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::execute_bundles` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::forget_lifetime` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::insert_debug_marker` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::map_buffer_on_submit` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::multi_draw_indexed_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::multi_draw_indexed_indirect_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::multi_draw_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::multi_draw_indirect_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::multi_draw_mesh_tasks_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::multi_draw_mesh_tasks_indirect_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::on_submitted_work_done` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::pop_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::push_debug_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_blend_constant` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_immediates` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_index_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_scissor_rect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_stencil_reference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_vertex_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::set_viewport` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPass::write_timestamp` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassColorAttachment` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassColorAttachment::depth_slice` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassColorAttachment::ops` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassColorAttachment::resolve_target` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassColorAttachment::view` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDepthStencilAttachment` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDepthStencilAttachment::depth_ops` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDepthStencilAttachment::stencil_ops` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDepthStencilAttachment::view` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDescriptor::color_attachments` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDescriptor::depth_stencil_attachment` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDescriptor::multiview_mask` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDescriptor::occlusion_query_set` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassDescriptor::timestamp_writes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassTimestampWrites` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassTimestampWrites::beginning_of_pass_write_index` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassTimestampWrites::end_of_pass_write_index` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::RenderPassTimestampWrites::query_set` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pass::StoreOp` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::FragmentState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::FragmentState::compilation_options` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::FragmentState::entry_point` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::FragmentState::module` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::FragmentState::targets` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::cache` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::depth_stencil` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::fragment` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::layout` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::mesh` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::multisample` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::multiview` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::primitive` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshPipelineDescriptor::task` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshState::compilation_options` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshState::entry_point` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::MeshState::module` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipeline` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipeline::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipeline::get_bind_group_layout` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::cache` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::depth_stencil` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::fragment` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::layout` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::multisample` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::multiview_mask` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::primitive` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::RenderPipelineDescriptor::vertex` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::TaskState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::TaskState::compilation_options` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::TaskState::entry_point` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::TaskState::module` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexBufferLayout` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexBufferLayout::array_stride` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexBufferLayout::attributes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexBufferLayout::step_mode` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexState::buffers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexState::compilation_options` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexState::entry_point` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::render_pipeline::VertexState::module` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::rwh` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::sampler` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::sampler` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::sampler::Sampler` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::sampler::Sampler::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::sampler::SamplerDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationInfo` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationInfo::messages` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationMessage` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationMessage::location` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationMessage::message` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationMessage::message_type` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationMessageType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationMessageType::Error` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationMessageType::Info` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::CompilationMessageType::Warning` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderModule` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderModule::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderModule::get_compilation_info` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderModuleDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderModuleDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderModuleDescriptor::source` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderModuleDescriptorPassthrough` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::Glsl` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::Glsl::defines` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::Glsl::shader` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::Glsl::stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::Naga` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::Naga::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::SpirV` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::SpirV::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::Wgsl` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::ShaderSource::Wgsl::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::SourceLocation` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::SourceLocation::length` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::SourceLocation::line_number` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::SourceLocation::line_position` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::shader_module::SourceLocation::offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::CreateSurfaceError` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::DisplayAndWindowHandle` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface::configure` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface::display_hdr_info` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface::get_capabilities` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface::get_configuration` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface::get_current_texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::Surface::get_default_config` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceConfiguration` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTarget` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTarget::DisplayAndWindow` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTarget::DisplayAndWindow::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTarget::Window` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTarget::Window::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTarget::from_window_without_display` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::Drm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::Drm::connector_id` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::Drm::fd` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::Drm::height` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::Drm::plane` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::Drm::refresh_rate` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::Drm::width` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::RawHandle` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::RawHandle::raw_display_handle` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::RawHandle::raw_window_handle` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::from_display_and_window` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::SurfaceTargetUnsafe::from_window` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface::WindowHandle` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Lost` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Occluded` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Outdated` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Suboptimal` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Suboptimal::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Success` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Success::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Timeout` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::CurrentSurfaceTexture::Validation` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::SurfaceTexture` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::SurfaceTexture::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::surface_texture::SurfaceTexture::texture` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::as_image_copy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::create_view` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::depth_or_array_layers` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::destroy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::dimension` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::format` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::from_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::height` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::mip_level_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::sample_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::size` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::usage` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::Texture::width` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture::TextureDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture_view` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture_view` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture_view::TextureView` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture_view::TextureView::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture_view::TextureView::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture_view::TextureView::texture` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::texture_view::TextureViewDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::CreateTlasDescriptor` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::Tlas` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::Tlas::as_binding` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::Tlas::as_custom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::Tlas::as_hal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::Tlas::get` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::Tlas::get_mut_single` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::Tlas::get_mut_slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::tlas::Tlas::lowest_unmodified` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::BufferInitDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::BufferInitDescriptor` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::BufferInitDescriptor::contents` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::BufferInitDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::BufferInitDescriptor::usage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DeviceExt` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DeviceExt` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DeviceExt::create_buffer_init` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DeviceExt::create_texture_with_data` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DispatchIndirectArgs` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DownloadBuffer` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DownloadBuffer::read_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DrawIndexedIndirectArgs` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::DrawIndirectArgs` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::draw` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::draw_indexed` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::draw_indexed_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::draw_indirect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::set_bind_group` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::set_immediates` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::set_index_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::set_pipeline` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::RenderEncoder::set_vertex_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt::allocate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt::finish` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt::finish_and_recall_on_submit` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt::new_with_buffer_usages` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt::recall` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::StagingBelt::write_buffer` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitter` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitter` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitter::copy` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitter::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitterBuilder` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitterBuilder` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitterBuilder::blend_state` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitterBuilder::build` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitterBuilder::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureBlitterBuilder::sample_type` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureDataOrder` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureFormatExt` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureFormatExt::from_storage_format` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::TextureFormatExt::to_storage_format` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::initialize_adapter_from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::initialize_adapter_from_env_or_default` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::is_browser_webgpu_supported` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::make_spirv` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::make_spirv_raw` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::math` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::new_instance_with_webgpu_detection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::util::pipeline_cache_key` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu::vertex_attr_array` | macro | `jadix_2d::wgpu` | shared-fork |
| `wgpu::wgc` | extern_crate | `jadix_2d::wgpu` | shared-fork |
| `wgpu::wgt` | extern_crate | `jadix_2d::wgpu` | shared-fork |
### wgpu_types 30.0.0

1930 项。wgpu-types 的类型/标志/描述符接口；不纳入绘制能力统计。 完整签名/条件属性/trait 见 `benchmarks/vello-api/inventory/wgpu_types-30.0.0.json`。

| 项 | 类别 | Jadix 对应 | 状态 |
|---|---|---|---|
| `wgpu_types` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AABB_GEOMETRY_MIN_STRIDE` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureCopy` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureCopy::Clone` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureCopy::Compact` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::ALLOW_COMPACTION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::ALLOW_RAY_HIT_VERTEX_RETURN` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::ALLOW_UPDATE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::LOW_MEMORY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::PREFER_FAST_BUILD` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::PREFER_FAST_TRACE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::USE_TRANSFORM` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureFlags::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::NO_DUPLICATE_ANY_HIT_INVOCATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::OPAQUE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureGeometryFlags::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureType::AABBs` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureType::Instances` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureType::Triangles` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureUpdateMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureUpdateMode::Build` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AccelerationStructureUpdateMode::PreferUpdate` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::backend` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::device` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::device_pci_bus_id` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::device_type` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::driver` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::driver_info` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::limit_bucket` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::name` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::subgroup_max_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::subgroup_min_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::transient_saves_memory` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterInfo::vendor` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterLimitBucketInfo` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterLimitBucketInfo::name` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterLimitBucketInfo::raw_features` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterLimitBucketInfo::raw_limits` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterLimitBucketInfo::raw_subgroup_max_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AdapterLimitBucketInfo::raw_subgroup_min_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AddressMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AddressMode::ClampToBorder` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AddressMode::ClampToEdge` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AddressMode::MirrorRepeat` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AddressMode::Repeat` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocationReport` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocationReport::name` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocationReport::offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocationReport::size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocatorReport` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocatorReport::allocations` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocatorReport::blocks` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocatorReport::total_allocated_bytes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::AllocatorReport::total_reserved_bytes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend::ALL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend::BrowserWebGpu` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend::Dx12` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend::Gl` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend::Metal` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend::Noop` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend::Vulkan` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backend::to_str` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BackendOptions` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BackendOptions::dx12` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BackendOptions::from_env_or_default` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BackendOptions::gl` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BackendOptions::noop` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BackendOptions::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::BROWSER_WEBGPU` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::DX12` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::GL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::METAL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::NOOP` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::PRIMARY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::SECONDARY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::VULKAN` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::from_comma_list` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Backends::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindGroupLayoutEntry` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindGroupLayoutEntry::binding` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindGroupLayoutEntry::count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindGroupLayoutEntry::ty` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindGroupLayoutEntry::visibility` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::AccelerationStructure` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::AccelerationStructure::vertex_return` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Buffer` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Buffer::has_dynamic_offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Buffer::min_binding_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Buffer::ty` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::ExternalTexture` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Sampler` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Sampler::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::StorageTexture` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::StorageTexture::access` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::StorageTexture::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::StorageTexture::view_dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Texture` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Texture::multisampled` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Texture::sample_type` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::Texture::view_dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BindingType::has_dynamic_offset` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasAABBGeometrySizeDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasAABBGeometrySizeDescriptor::flags` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasAABBGeometrySizeDescriptor::primitive_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasGeometrySizeDescriptors` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasGeometrySizeDescriptors::AABBs` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasGeometrySizeDescriptors::AABBs::descriptors` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasGeometrySizeDescriptors::Triangles` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasGeometrySizeDescriptors::Triangles::descriptors` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasTriangleGeometrySizeDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasTriangleGeometrySizeDescriptor::flags` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasTriangleGeometrySizeDescriptor::index_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasTriangleGeometrySizeDescriptor::index_format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasTriangleGeometrySizeDescriptor::vertex_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlasTriangleGeometrySizeDescriptor::vertex_format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendComponent` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendComponent::OVER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendComponent::REPLACE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendComponent::dst_factor` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendComponent::operation` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendComponent::src_factor` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendComponent::uses_constant` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::Constant` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::Dst` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::DstAlpha` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::One` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::OneMinusConstant` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::OneMinusDst` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::OneMinusDstAlpha` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::OneMinusSrc` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::OneMinusSrc1` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::OneMinusSrc1Alpha` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::OneMinusSrcAlpha` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::Src` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::Src1` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::Src1Alpha` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::SrcAlpha` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::SrcAlphaSaturated` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::Zero` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendFactor::ref_second_blend_source` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendOperation` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendOperation::Add` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendOperation::Max` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendOperation::Min` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendOperation::ReverseSubtract` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendOperation::Subtract` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendState::ADDITIVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendState::ALPHA_BLENDING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendState::PREMULTIPLIED_ALPHA_BLENDING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendState::REPLACE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendState::alpha` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BlendState::color` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferAddress` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferBindingType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferBindingType::Storage` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferBindingType::Storage::read_only` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferBindingType::Uniform` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferDescriptor::mapped_at_creation` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferDescriptor::size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferDescriptor::usage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferSize` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::block_height_texels` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::block_size_bytes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::block_width_texels` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::bytes_in_copy` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::copy_height` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::copy_width` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::depth_or_array_layers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::height_blocks` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::image_bytes_dense` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::image_rows_dense` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::image_stride_bytes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::image_stride_rows` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::row_bytes_dense` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::row_stride_bytes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfo::width_blocks` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfoError` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfoError::ArraySizeOverflow` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfoError::ArraySizeOverflow::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfoError::ImageBytesOverflow` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfoError::ImageBytesOverflow::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfoError::ImageStrideOverflow` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfoError::InvalidBytesPerRow` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTextureCopyInfoError::InvalidRowsPerImage` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTransition` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTransition::buffer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferTransition::state` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::BLAS_INPUT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::COPY_DST` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::COPY_SRC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::INDEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::INDIRECT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::MAP_READ` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::MAP_WRITE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::QUERY_RESOLVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::STORAGE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::TLAS_INPUT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::UNIFORM` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::VERTEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUsages::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::ACCELERATION_STRUCTURE_QUERY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::ACCELERATION_STRUCTURE_SCRATCH` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::BOTTOM_LEVEL_ACCELERATION_STRUCTURE_INPUT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::COPY_DST` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::COPY_SRC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::EXCLUSIVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::INCLUSIVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::INDEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::INDIRECT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::MAP_READ` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::MAP_WRITE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::QUERY_RESOLVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::RAY_TRACING_PIPELINE_SHADER_DATA` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::STORAGE_READ_ONLY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::STORAGE_READ_WRITE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::TOP_LEVEL_ACCELERATION_STRUCTURE_INPUT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::UNIFORM` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::VERTEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::BufferUses::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::COPY_BUFFER_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::COPY_BYTES_PER_ROW_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::BLACK` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::BLUE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::GREEN` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::RED` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::TRANSPARENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::WHITE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::a` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::b` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::g` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Color::r` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorTargetState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorTargetState::blend` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorTargetState::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorTargetState::write_mask` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::ALL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::ALPHA` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::BLUE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::COLOR` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::GREEN` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::RED` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ColorWrites::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CommandBufferDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CommandBufferDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CommandBufferDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CommandEncoderDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CommandEncoderDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CommandEncoderDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::Always` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::Equal` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::Greater` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::GreaterEqual` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::Less` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::LessEqual` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::Never` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::NotEqual` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompareFunction::needs_ref_value` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompositeAlphaMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompositeAlphaMode::Auto` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompositeAlphaMode::Inherit` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompositeAlphaMode::Opaque` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompositeAlphaMode::PostMultiplied` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CompositeAlphaMode::PreMultiplied` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeMatrixProperties` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeMatrixProperties::ab_type` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeMatrixProperties::cr_type` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeMatrixProperties::k_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeMatrixProperties::m_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeMatrixProperties::n_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeMatrixProperties::saturating_accumulation` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeScalarType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeScalarType::F16` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeScalarType::F32` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeScalarType::I32` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CooperativeScalarType::U32` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CoreCounters` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateBlasDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateBlasDescriptor::flags` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateBlasDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateBlasDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateBlasDescriptor::update_mode` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::dxil` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::entry_points` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::glsl` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::hlsl` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::metallib` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::msl` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::spirv` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::trace_binary_ext` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::trace_data` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateShaderModuleDescriptorPassthrough::wgsl` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateTlasDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateTlasDescriptor::flags` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateTlasDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateTlasDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateTlasDescriptor::max_instances` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::CreateTlasDescriptor::update_mode` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthBiasState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthBiasState::clamp` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthBiasState::constant` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthBiasState::is_enabled` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthBiasState::slope_scale` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::bias` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::depth_compare` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::depth_write_enabled` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::is_depth_enabled` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::is_depth_read_only` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::is_read_only` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::is_stencil_read_only` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::stencil` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DepthStencilState::stencil` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceDescriptor::experimental_features` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceDescriptor::memory_hints` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceDescriptor::required_features` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceDescriptor::required_limits` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceDescriptor::trace` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceLostReason` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceLostReason::Destroyed` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceLostReason::Unknown` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceType::Cpu` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceType::DiscreteGpu` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceType::IntegratedGpu` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceType::Other` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DeviceType::VirtualGpu` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DispatchIndirectArgs` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DispatchIndirectArgs::as_bytes` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DispatchIndirectArgs::x` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DispatchIndirectArgs::y` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DispatchIndirectArgs::z` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayChromaticity` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayChromaticity::blue` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayChromaticity::green` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayChromaticity::red` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayChromaticity::white` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayCoarseRange` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayCoarseRange::gamut` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayCoarseRange::high_dynamic_range` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayGamut` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayGamut::DisplayP3` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayGamut::Rec2020` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayGamut::Srgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHdrInfo` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHdrInfo::bits_per_color` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHdrInfo::chromaticity` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHdrInfo::coarse` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHdrInfo::headroom` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHdrInfo::luminance` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHdrInfo::tone_map_headroom` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHeadroom` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHeadroom::current` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHeadroom::potential` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayHeadroom::reference` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayLuminance` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayLuminance::max_full_frame_nits` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayLuminance::max_nits` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayLuminance::min_nits` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DisplayLuminance::sdr_white_nits` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelCapabilities` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelCapabilities::flags` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelCapabilities::is_webgpu_compliant` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelCapabilities::limits` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelCapabilities::shader_model` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::ANISOTROPIC_FILTERING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::BASE_VERTEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::COMPARISON_SAMPLERS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::COMPUTE_SHADERS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::CUBE_ARRAY_TEXTURES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::DEPTH_BIAS_CLAMP` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::DEPTH_TEXTURE_AND_BUFFER_COPIES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::FRAGMENT_STORAGE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::FRAGMENT_WRITABLE_STORAGE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::FULL_DRAW_INDEX_UINT32` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::INDEPENDENT_BLEND` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::INDIRECT_EXECUTION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::MSL2_1` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::MULTISAMPLED_SHADING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::NONBLOCKING_QUERY_RESOLVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::READ_ONLY_DEPTH_STENCIL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::SHADER_F16_IN_F32` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::SURFACE_VIEW_FORMATS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::TEXTURE_COMPRESSION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::UNRESTRICTED_EXTERNAL_TEXTURE_COPIES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::UNRESTRICTED_INDEX_BUFFER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::VERTEX_STORAGE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::VIEW_FORMATS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::WEBGPU_TEXTURE_FORMAT_SUPPORT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::compliant` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelFlags::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DownlevelLimits` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndexedIndirectArgs` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndexedIndirectArgs::as_bytes` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndexedIndirectArgs::base_vertex` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndexedIndirectArgs::first_index` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndexedIndirectArgs::first_instance` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndexedIndirectArgs::index_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndexedIndirectArgs::instance_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndirectArgs` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndirectArgs::as_bytes` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndirectArgs::first_instance` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndirectArgs::first_vertex` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndirectArgs::instance_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DrawIndirectArgs::vertex_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDK` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDK::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDK::on_load_failure` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDK::sdk_path` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDK::sdk_version` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDK::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDKLoadFailure` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDKLoadFailure::Error` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDKLoadFailure::Fallback` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDKLoadFailure::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12AgilitySDKLoadFailure::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12BackendOptions` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12BackendOptions::agility_sdk` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12BackendOptions::force_shader_model` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12BackendOptions::from_env_or_default` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12BackendOptions::latency_waitable_object` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12BackendOptions::presentation_system` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12BackendOptions::shader_compiler` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12BackendOptions::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler::Auto` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler::DynamicDxc` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler::DynamicDxc::dxc_path` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler::Fxc` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler::StaticDxc` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler::default_dynamic_dxc` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12Compiler::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12SwapchainKind` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12SwapchainKind::DxgiFromHwnd` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12SwapchainKind::DxgiFromVisual` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12SwapchainKind::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12SwapchainKind::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12UseFrameLatencyWaitableObject` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12UseFrameLatencyWaitableObject::DontWait` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12UseFrameLatencyWaitableObject::None` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12UseFrameLatencyWaitableObject::Wait` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12UseFrameLatencyWaitableObject::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Dx12UseFrameLatencyWaitableObject::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_0` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_1` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_2` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_3` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_4` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_5` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_6` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_7` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_8` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::V6_9` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::from_dxc_version` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DxcShaderModel::from_parts` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::DynamicOffset` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ExperimentalFeatures` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ExperimentalFeatures::disabled` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ExperimentalFeatures::enabled` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ExperimentalFeatures::is_enabled` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Extent3d` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Extent3d::depth_or_array_layers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Extent3d::height` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Extent3d::max_mips` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Extent3d::physical_size` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Extent3d::width` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Face` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Face::Back` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Face::Front` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeatureBits` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeatureBits::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeatureLevel` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeatureLevel::Compatibility` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeatureLevel::Core` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::ACCELERATION_STRUCTURE_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::ADDRESS_MODE_CLAMP_TO_BORDER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::ADDRESS_MODE_CLAMP_TO_ZERO` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::BGRA8UNORM_STORAGE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::BUFFER_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::CLEAR_TEXTURE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::CLIP_DISTANCES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::CONSERVATIVE_RASTERIZATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::DEPTH32FLOAT_STENCIL8` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::DEPTH_CLIP_CONTROL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::DUAL_SOURCE_BLENDING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXPERIMENTAL_COOPERATIVE_MATRIX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXPERIMENTAL_MESH_SHADER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXPERIMENTAL_MESH_SHADER_MULTIVIEW` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXPERIMENTAL_MESH_SHADER_POINTS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXPERIMENTAL_RAY_QUERY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXPERIMENTAL_RAY_TRACING_PIPELINES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXTENDED_ACCELERATION_STRUCTURE_VERTEX_FORMATS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::EXTERNAL_TEXTURE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::FLOAT32_BLENDABLE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::FLOAT32_FILTERABLE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::IMMEDIATES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::INDIRECT_FIRST_INSTANCE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::MAPPABLE_PRIMARY_BUFFERS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::MEMORY_DECORATION_COHERENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::MEMORY_DECORATION_VOLATILE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::MULTISAMPLE_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::MULTIVIEW` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::MULTI_DRAW_INDIRECT_COUNT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::PARTIALLY_BOUND_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::PASSTHROUGH_SHADERS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::PIPELINE_CACHE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::PIPELINE_STATISTICS_QUERY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::POLYGON_MODE_LINE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::POLYGON_MODE_POINT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::PRIMITIVE_INDEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::RG11B10UFLOAT_RENDERABLE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SELECTIVE_MULTIVIEW` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_BARYCENTRICS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_DRAW_INDEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_EARLY_DEPTH_TEST` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_F16` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_F64` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_FLOAT32_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_I16` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_INT64` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_INT64_ATOMIC_ALL_OPS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_INT64_ATOMIC_MIN_MAX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SHADER_PER_VERTEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::STORAGE_RESOURCE_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SUBGROUP` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SUBGROUP_BARRIER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::SUBGROUP_VERTEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_COMPRESSION_ASTC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_COMPRESSION_ASTC_HDR` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_COMPRESSION_ASTC_SLICED_3D` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_COMPRESSION_BC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_COMPRESSION_BC_SLICED_3D` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_COMPRESSION_ETC2` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_FORMAT_16BIT_NORM` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_FORMAT_NV12` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_FORMAT_P010` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TEXTURE_INT64_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TIMESTAMP_QUERY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::TIMESTAMP_QUERY_INSIDE_PASSES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::UNIFORM_BUFFER_BINDING_ARRAYS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::VERTEX_ATTRIBUTE_64BIT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::VERTEX_WRITABLE_STORAGE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::VULKAN_EXTERNAL_MEMORY_FD` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::VULKAN_EXTERNAL_MEMORY_WIN32` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::VULKAN_GOOGLE_DISPLAY_TIMING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::all_experimental_mask` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::all_native_mask` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::all_webgpu_mask` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::allowed_vertex_formats_for_blas` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::as_str` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::features_webgpu` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::features_wgpu` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::from_internal_flags` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Features::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::ACCELERATION_STRUCTURE_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::ADDRESS_MODE_CLAMP_TO_BORDER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::ADDRESS_MODE_CLAMP_TO_ZERO` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::BUFFER_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::CLEAR_TEXTURE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::CONSERVATIVE_RASTERIZATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXPERIMENTAL_COOPERATIVE_MATRIX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXPERIMENTAL_MESH_SHADER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXPERIMENTAL_MESH_SHADER_MULTIVIEW` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXPERIMENTAL_MESH_SHADER_POINTS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXPERIMENTAL_RAY_QUERY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXPERIMENTAL_RAY_TRACING_PIPELINES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXTENDED_ACCELERATION_STRUCTURE_VERTEX_FORMATS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::EXTERNAL_TEXTURE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::MAPPABLE_PRIMARY_BUFFERS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::MEMORY_DECORATION_COHERENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::MEMORY_DECORATION_VOLATILE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::MULTISAMPLE_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::MULTIVIEW` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::MULTI_DRAW_INDIRECT_COUNT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::PARTIALLY_BOUND_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::PASSTHROUGH_SHADERS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::PIPELINE_CACHE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::PIPELINE_STATISTICS_QUERY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::POLYGON_MODE_LINE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::POLYGON_MODE_POINT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SELECTIVE_MULTIVIEW` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_BARYCENTRICS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_DRAW_INDEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_EARLY_DEPTH_TEST` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_F64` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_FLOAT32_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_I16` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_INT64` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_INT64_ATOMIC_ALL_OPS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_INT64_ATOMIC_MIN_MAX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SHADER_PER_VERTEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::STORAGE_RESOURCE_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SUBGROUP` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SUBGROUP_BARRIER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::SUBGROUP_VERTEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TEXTURE_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TEXTURE_BINDING_ARRAY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TEXTURE_COMPRESSION_ASTC_HDR` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TEXTURE_FORMAT_16BIT_NORM` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TEXTURE_FORMAT_NV12` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TEXTURE_FORMAT_P010` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TEXTURE_INT64_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TIMESTAMP_QUERY_INSIDE_ENCODERS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::TIMESTAMP_QUERY_INSIDE_PASSES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::UNIFORM_BUFFER_BINDING_ARRAYS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::VERTEX_ATTRIBUTE_64BIT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::VERTEX_WRITABLE_STORAGE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::VULKAN_EXTERNAL_MEMORY_DMA_BUF` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::VULKAN_EXTERNAL_MEMORY_FD` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::VULKAN_EXTERNAL_MEMORY_WIN32` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::VULKAN_GOOGLE_DISPLAY_TIMING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWGPU::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::BGRA8UNORM_STORAGE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::CLIP_DISTANCES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::DEPTH32FLOAT_STENCIL8` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::DEPTH_CLIP_CONTROL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::DUAL_SOURCE_BLENDING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::FLOAT32_BLENDABLE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::FLOAT32_FILTERABLE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::IMMEDIATES` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::INDIRECT_FIRST_INSTANCE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::PRIMITIVE_INDEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::RG11B10UFLOAT_RENDERABLE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::SHADER_F16` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::TEXTURE_COMPRESSION_ASTC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::TEXTURE_COMPRESSION_ASTC_SLICED_3D` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::TEXTURE_COMPRESSION_BC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::TEXTURE_COMPRESSION_BC_SLICED_3D` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::TEXTURE_COMPRESSION_ETC2` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::TIMESTAMP_QUERY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FeaturesWebGPU::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FilterMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FilterMode::Linear` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FilterMode::Nearest` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ForceShaderModelToken` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ForceShaderModelToken::get` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ForceShaderModelToken::with_shader_model` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FrontFace` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FrontFace::Ccw` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::FrontFace::Cw` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlBackendOptions` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlBackendOptions::debug_fns` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlBackendOptions::fence_behavior` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlBackendOptions::from_env_or_default` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlBackendOptions::gles_minor_version` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlBackendOptions::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlDebugFns` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlDebugFns::Auto` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlDebugFns::Disabled` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlDebugFns::ForceEnabled` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlDebugFns::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlDebugFns::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlFenceBehavior` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlFenceBehavior::AutoFinish` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlFenceBehavior::Normal` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlFenceBehavior::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlFenceBehavior::is_auto_finish` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlFenceBehavior::is_normal` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::GlFenceBehavior::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Gles3MinorVersion` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Gles3MinorVersion::Automatic` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Gles3MinorVersion::Version0` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Gles3MinorVersion::Version1` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Gles3MinorVersion::Version2` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Gles3MinorVersion::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Gles3MinorVersion::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::acceleration_structure_memory` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::bind_group_layouts` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::bind_groups` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::buffer_memory` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::buffers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::command_encoders` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::compute_pipelines` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::fences` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::memory_allocations` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::pipeline_layouts` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::query_sets` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::ray_tracing_pipelines` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::render_pipelines` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::samplers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::shader_modules` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::texture_memory` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::texture_views` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::HalCounters::textures` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::IMMEDIATE_DATA_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::INSTANCE_BUFFER_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange::array_layer_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange::aspect` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange::base_array_layer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange::base_mip_level` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange::is_full_resource` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange::layer_range` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange::mip_level_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ImageSubresourceRange::mip_range` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::IndexFormat` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::IndexFormat::Uint16` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::IndexFormat::Uint32` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::IndexFormat::byte_size` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::backend_options` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::backends` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::display` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::flags` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::memory_budget_thresholds` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::new_with_display_handle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::new_with_display_handle_from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::new_without_display_handle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::new_without_display_handle_from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::with_display_handle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceDescriptor::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::AUTOMATIC_TIMESTAMP_NORMALIZATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::DEBUG` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::DISCARD_HAL_LABELS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::GPU_BASED_VALIDATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::STRICT_WEBGPU_COMPLIANCE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::VALIDATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::VALIDATION_INDIRECT_CALL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::advanced_debugging` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::debugging` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::from_build_config` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::from_env_or_default` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InstanceFlags::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounter` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounter::add` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounter::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounter::read` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounter::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounter::sub` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounter::take` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounters` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounters::core` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::InternalCounters::hal` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::check_limits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::check_limits_with_fail_fn` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::defaults` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::downlevel_defaults` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::downlevel_webgl2_defaults` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_acceleration_structures_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_bind_groups` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_bind_groups_plus_vertex_buffers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_binding_array_acceleration_structure_elements_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_binding_array_elements_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_binding_array_sampler_elements_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_bindings_per_bind_group` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_blas_geometry_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_blas_primitive_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_buffer_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_buffers_and_acceleration_structures_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_color_attachment_bytes_per_sample` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_color_attachments` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_compute_invocations_per_workgroup` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_compute_workgroup_size_x` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_compute_workgroup_size_y` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_compute_workgroup_size_z` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_compute_workgroup_storage_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_compute_workgroups_per_dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_dynamic_storage_buffers_per_pipeline_layout` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_dynamic_uniform_buffers_per_pipeline_layout` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_immediate_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_inter_stage_shader_variables` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_mesh_invocations_per_dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_mesh_invocations_per_workgroup` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_mesh_multiview_view_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_mesh_output_layers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_mesh_output_primitives` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_mesh_output_vertices` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_mesh_workgroup_total_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_mesh_workgroups_per_dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_multiview_view_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_non_sampler_bindings` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_ray_dispatch_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_ray_recursion_depth` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_sampled_textures_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_samplers_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_storage_buffer_binding_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_storage_buffers_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_storage_textures_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_task_invocations_per_dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_task_invocations_per_workgroup` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_task_payload_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_task_workgroup_total_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_task_workgroups_per_dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_texture_array_layers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_texture_dimension_1d` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_texture_dimension_2d` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_texture_dimension_3d` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_tlas_instance_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_uniform_buffer_binding_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_uniform_buffers_per_shader_stage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_vertex_attributes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_vertex_buffer_array_stride` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::max_vertex_buffers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::min_storage_buffer_offset_alignment` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::min_uniform_buffer_offset_alignment` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::or_better_values_from` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::or_worse_values_from` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::unlimited` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::using_acceleration_structure_values` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::using_alignment` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::using_minimum_supported_acceleration_structure_values` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::using_minimum_supported_ray_tracing_pipeline_values` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::using_recommended_minimum_mesh_shader_values` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::using_resolution` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Limits::zero_native_only` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOp` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOp::Clear` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOp::Clear::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOp::DontCare` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOp::DontCare::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOp::Load` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOp::eq_variant` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOpDontCare` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::LoadOpDontCare::enabled` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MAP_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MAXIMUM_SUBGROUP_MAX_SIZE` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MINIMUM_SUBGROUP_MIN_SIZE` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryBlockReport` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryBlockReport::allocations` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryBlockReport::size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryBudgetThresholds` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryBudgetThresholds::for_device_loss` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryBudgetThresholds::for_resource_creation` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryHints` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryHints::Manual` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryHints::Manual::suballocated_device_memory_block_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryHints::MemoryUsage` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MemoryHints::Performance` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MipmapFilterMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MipmapFilterMode::Linear` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MipmapFilterMode::Nearest` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MultisampleState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MultisampleState::alpha_to_coverage_enabled` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MultisampleState::count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::MultisampleState::mask` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::device_type` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::enable` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::enabled` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::features` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::from_env_or_default` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::limits` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::subgroup_max_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::subgroup_min_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::NoopBackendOptions::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Operations` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Operations::load` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Operations::store` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin2d` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin2d::ZERO` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin2d::to_3d` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin2d::x` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin2d::y` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin3d` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin3d::ZERO` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin3d::to_2d` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin3d::x` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin3d::y` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Origin3d::z` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PassthroughShaderEntryPoint` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PassthroughShaderEntryPoint::name` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PassthroughShaderEntryPoint::workgroup_size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::CLIPPER_INVOCATIONS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::CLIPPER_PRIMITIVES_OUT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::COMPUTE_SHADER_INVOCATIONS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::FRAGMENT_SHADER_INVOCATIONS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::VERTEX_SHADER_INVOCATIONS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PipelineStatisticsTypes::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollError` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollError::Timeout` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollError::WrongSubmissionIndex` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollError::WrongSubmissionIndex::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollError::WrongSubmissionIndex::1` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollStatus` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollStatus::Poll` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollStatus::QueueEmpty` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollStatus::WaitSucceeded` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollStatus::is_queue_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollStatus::wait_finished` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollType::Poll` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollType::Wait` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollType::Wait::submission_index` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollType::Wait::timeout` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollType::is_wait` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollType::map_index` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PollType::wait_indefinitely` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PolygonMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PolygonMode::Fill` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PolygonMode::Line` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PolygonMode::Point` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PowerPreference` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PowerPreference::HighPerformance` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PowerPreference::LowPower` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PowerPreference::None` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PowerPreference::from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentMode::AutoNoVsync` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentMode::AutoVsync` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentMode::Fifo` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentMode::FifoRelaxed` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentMode::Immediate` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentMode::Mailbox` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentationTimestamp` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentationTimestamp::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentationTimestamp::INVALID_TIMESTAMP` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PresentationTimestamp::is_invalid` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveState::conservative` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveState::cull_mode` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveState::front_face` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveState::polygon_mode` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveState::strip_index_format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveState::topology` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveState::unclipped_depth` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveTopology` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveTopology::LineList` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveTopology::LineStrip` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveTopology::PointList` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveTopology::TriangleList` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveTopology::TriangleStrip` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveTopology::is_strip` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::PrimitiveTopology::is_triangles` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QUERY_RESOLVE_BUFFER_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QUERY_SET_MAX_QUERIES` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QUERY_SIZE` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QuerySetDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QuerySetDescriptor::count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QuerySetDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QuerySetDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QuerySetDescriptor::ty` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QueryType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QueryType::Occlusion` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QueryType::PipelineStatistics` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QueryType::PipelineStatistics::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::QueryType::Timestamp` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RenderBundleDepthStencil` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RenderBundleDepthStencil::depth_read_only` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RenderBundleDepthStencil::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RenderBundleDepthStencil::stencil_read_only` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RenderBundleDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RenderBundleDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RenderBundleDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RequestAdapterError` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RequestAdapterError::EnvNotSet` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RequestAdapterError::NotFound` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RequestAdapterOptions` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RequestAdapterOptions::apply_limit_buckets` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RequestAdapterOptions::compatible_surface` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RequestAdapterOptions::force_fallback_adapter` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::RequestAdapterOptions::power_preference` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBindingType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBindingType::Comparison` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBindingType::Filtering` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBindingType::NonFiltering` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBorderColor` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBorderColor::OpaqueBlack` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBorderColor::OpaqueWhite` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBorderColor::TransparentBlack` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerBorderColor::Zero` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::address_mode_u` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::address_mode_v` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::address_mode_w` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::anisotropy_clamp` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::border_color` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::compare` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::lod_max_clamp` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::lod_min_clamp` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::mag_filter` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::min_filter` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SamplerDescriptor::mipmap_filter` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderLocation` | type_alias | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderModel` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderModel::Sm2` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderModel::Sm4` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderModel::Sm5` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::bounds_checks` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::checked` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::force_loop_bounding` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::int_div_checks` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::mesh_shader_primitive_indices_clamp` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::ray_query_initialization_tracking` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::task_shader_dispatch_tracking` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderRuntimeChecks::unchecked` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::ANY_HIT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::CLOSEST_HIT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::COMPUTE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::FRAGMENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::MESH` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::MISS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::NONE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::RAY_GENERATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::TASK` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::VERTEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::VERTEX_FRAGMENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::ShaderStages::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilFaceState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilFaceState::IGNORE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilFaceState::compare` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilFaceState::depth_fail_op` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilFaceState::fail_op` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilFaceState::is_read_only` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilFaceState::needs_ref_value` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilFaceState::pass_op` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation::DecrementClamp` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation::DecrementWrap` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation::IncrementClamp` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation::IncrementWrap` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation::Invert` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation::Keep` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation::Replace` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilOperation::Zero` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilState` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilState::back` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilState::front` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilState::is_enabled` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilState::is_read_only` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilState::needs_ref_value` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilState::read_mask` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StencilState::write_mask` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StorageTextureAccess` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StorageTextureAccess::Atomic` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StorageTextureAccess::ReadOnly` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StorageTextureAccess::ReadWrite` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StorageTextureAccess::WriteOnly` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StoreOp` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StoreOp::Discard` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::StoreOp::Store` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceCapabilities` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceCapabilities::alpha_modes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceCapabilities::color_spaces` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceCapabilities::format_capabilities` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceCapabilities::formats` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceCapabilities::present_modes` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceCapabilities::usages` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::Auto` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::Bt2100Hlg` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::Bt2100Pq` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::DisplayP3` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::ExtendedDisplayP3` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::ExtendedSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::ExtendedSrgbLinear` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::Srgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::is_hdr` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpace::to_color_spaces` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::BT2100_HLG` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::BT2100_PQ` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::DISPLAY_P3` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::EXTENDED_DISPLAY_P3` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::EXTENDED_SRGB` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::SRGB` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceColorSpaces::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::alpha_mode` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::color_space` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::desired_maximum_frame_latency` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::height` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::map_view_formats` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::present_mode` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::usage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::view_formats` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceConfiguration::width` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceFormatCapabilities` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceFormatCapabilities::color_spaces` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceFormatCapabilities::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceStatus` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceStatus::Good` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceStatus::Lost` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceStatus::Occluded` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceStatus::Outdated` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceStatus::Suboptimal` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceStatus::Timeout` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::SurfaceStatus::Validation` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TRANSFORM_BUFFER_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyBufferInfo` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyBufferInfo::buffer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyBufferInfo::layout` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyBufferLayout` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyBufferLayout::bytes_per_row` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyBufferLayout::offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyBufferLayout::rows_per_image` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyTextureInfo` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyTextureInfo::aspect` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyTextureInfo::mip_level` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyTextureInfo::origin` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyTextureInfo::texture` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TexelCopyTextureInfo::to_tagged` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect::All` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect::DepthOnly` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect::Plane0` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect::Plane1` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect::Plane2` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect::StencilOnly` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect::from_plane` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureAspect::to_plane` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDataOrder` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDataOrder::LayerMajor` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDataOrder::MipMajor` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::array_layer_count` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::map_label_and_view_formats` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::mip_level_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::mip_level_size` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::sample_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::size` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::usage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDescriptor::view_formats` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDimension` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDimension::D1` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDimension::D2` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureDimension::D3` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSampleType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSampleType::Depth` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSampleType::Float` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSampleType::Float::filterable` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSampleType::Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSampleType::Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSelector` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSelector::layers` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureSelector::mips` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureTransition` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureTransition::selector` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureTransition::state` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureTransition::texture` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::COPY_DST` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::COPY_SRC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::RENDER_ATTACHMENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::STORAGE_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::STORAGE_BINDING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::TEXTURE_BINDING` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::TRANSIENT_ATTACHMENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUsages::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::COLOR_TARGET` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::COMPLEX` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::COPY_DST` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::COPY_SRC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::DEPTH_STENCIL_READ` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::DEPTH_STENCIL_WRITE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::EXCLUSIVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::INCLUSIVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::PRESENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::RESOURCE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::STORAGE_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::STORAGE_READ_ONLY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::STORAGE_READ_WRITE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::STORAGE_WRITE_ONLY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::TRANSIENT` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::UNINITIALIZED` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::UNKNOWN` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureUses::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::array_layer_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::aspect` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::base_array_layer` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::base_mip_level` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::dimension` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::mip_level_count` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDescriptor::usage` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDimension` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDimension::Cube` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDimension::CubeArray` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDimension::D1` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDimension::D2` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDimension::D2Array` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDimension::D3` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::TextureViewDimension::compatible_texture_dimension` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Trace` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Trace::Directory` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Trace::Directory::0` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Trace::Memory` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::Trace::Off` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VERTEX_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VERTEX_STRIDE_ALIGNMENT` | constant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VertexAttribute` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VertexAttribute::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VertexAttribute::offset` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VertexAttribute::shader_location` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VertexFormat` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VertexStepMode` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VertexStepMode::Instance` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::VertexStepMode::Vertex` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WgpuHasDisplayHandle` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::as_raw_element_ptr` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::as_raw_ptr` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::cast_elements` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::copy_from_slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::fill` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::from_mut` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::into_chunks` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::into_slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::len` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::new` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::slice` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::split_at` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::split_at_checked` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::split_off` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::split_off_first` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::split_off_last` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::write` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnly::write_iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::WriteOnlyIter` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::assertions` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::bytemuck_wrapper` | macro | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::error` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::error::ErrorType` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::error::ErrorType::DeviceLost` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::error::ErrorType::Internal` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::error::ErrorType::OutOfMemory` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::error::ErrorType::Validation` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::error::WebGpuError` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::error::WebGpuError::webgpu_error_type` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::CopyExternalImageDestInfo` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::CopyExternalImageDestInfo::aspect` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::CopyExternalImageDestInfo::color_space` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::CopyExternalImageDestInfo::mip_level` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::CopyExternalImageDestInfo::origin` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::CopyExternalImageDestInfo::premultiplied_alpha` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::CopyExternalImageDestInfo::texture` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::CopyExternalImageDestInfo::to_untagged` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::PredefinedColorSpace` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::PredefinedColorSpace::DisplayP3` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_image::PredefinedColorSpace::Srgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::dst_transfer_function` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::format` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::gamut_conversion_matrix` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::height` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::label` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::load_transform` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::map_label` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::num_planes` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::sample_transform` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::src_transfer_function` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::width` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureDescriptor::yuv_conversion_matrix` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureFormat` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureFormat::Nv12` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureFormat::Rgba` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureFormat::Yu12` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureTransferFunction` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureTransferFunction::a` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureTransferFunction::b` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureTransferFunction::g` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::external_texture::ExternalTextureTransferFunction::k` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format` | use | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B10x10` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B10x5` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B10x6` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B10x8` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B12x10` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B12x12` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B4x4` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B5x4` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B5x5` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B6x5` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B6x6` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B8x5` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B8x6` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcBlock::B8x8` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcChannel` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcChannel::Hdr` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcChannel::Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::AstcChannel::UnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::ALPHA` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::BLUE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::CHROMINANCE_BLUE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::CHROMINANCE_RED` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::DEPTH` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::DEPTH_STENCIL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::GREEN` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::LUMINANCE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::LUMINANCE_CHROMINANCE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::RED` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::RG` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::RGB` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::RGBA` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::STENCIL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureChannel::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat` | enum | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Astc` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Astc::block` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Astc::channel` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc1RgbaUnorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc1RgbaUnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc2RgbaUnorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc2RgbaUnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc3RgbaUnorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc3RgbaUnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc4RSnorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc4RUnorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc5RgSnorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc5RgUnorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc6hRgbFloat` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc6hRgbUfloat` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc7RgbaUnorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bc7RgbaUnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bgra8Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Bgra8UnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Depth16Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Depth24Plus` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Depth24PlusStencil8` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Depth32Float` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Depth32FloatStencil8` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::EacR11Snorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::EacR11Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::EacRg11Snorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::EacRg11Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Etc2Rgb8A1Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Etc2Rgb8A1UnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Etc2Rgb8Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Etc2Rgb8UnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Etc2Rgba8Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Etc2Rgba8UnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::MAX_TARGET_PIXEL_BYTE_COST` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::NV12` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::P010` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R16Float` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R16Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R16Snorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R16Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R16Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R32Float` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R32Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R32Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R64Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R8Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R8Snorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R8Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::R8Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg11b10Ufloat` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg16Float` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg16Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg16Snorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg16Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg16Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg32Float` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg32Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg32Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg8Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg8Snorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg8Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rg8Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgb10a2Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgb10a2Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgb9e5Ufloat` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba16Float` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba16Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba16Snorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba16Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba16Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba32Float` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba32Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba32Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba8Sint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba8Snorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba8Uint` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba8Unorm` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Rgba8UnormSrgb` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::Stencil8` | variant | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::add_srgb_suffix` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::aspect_specific_format` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::block_copy_size` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::block_dimensions` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::block_size` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::channels` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::components` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::components_with_aspect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::guaranteed_format_features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::has_color_aspect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::has_depth_aspect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::has_stencil_aspect` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::is_astc` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::is_bcn` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::is_combined_depth_stencil_format` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::is_compressed` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::is_depth_stencil_component` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::is_depth_stencil_format` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::is_multi_planar_format` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::is_srgb` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::planes` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::remove_srgb_suffix` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::required_features` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::sample_type` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::size_multiple_requirement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::subsampling_factors` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::target_component_alignment` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::target_pixel_byte_cost` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormat::theoretical_memory_footprint` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::BLENDABLE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::FILTERABLE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::MULTISAMPLE_X16` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::MULTISAMPLE_X2` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::MULTISAMPLE_X4` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::MULTISAMPLE_X8` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::STORAGE_ATOMIC` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::STORAGE_READ_ONLY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::STORAGE_READ_WRITE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::STORAGE_WRITE_ONLY` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::sample_count_supported` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::supported_sample_counts` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatureFlags::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatures` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatures::allowed_usages` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::format::TextureFormatFeatures::flags` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::backend_options` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::backends` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::display` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::flags` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::memory_budget_thresholds` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::new_with_display_handle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::new_with_display_handle_from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::new_without_display_handle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::new_without_display_handle_from_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::with_display_handle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceDescriptor::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::AUTOMATIC_TIMESTAMP_NORMALIZATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::DEBUG` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::DISCARD_HAL_LABELS` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::GPU_BASED_VALIDATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::STRICT_WEBGPU_COMPLIANCE` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::VALIDATION` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::VALIDATION_INDIRECT_CALL` | assoc_const | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::advanced_debugging` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::complement` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::contains` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::debugging` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::from_bits` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::from_bits_retain` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::from_bits_truncate` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::from_build_config` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::from_env_or_default` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::from_name` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::insert` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::intersection` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::intersects` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::is_all` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::is_empty` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::iter` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::iter_names` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::remove` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::set` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::symmetric_difference` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::toggle` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::union` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::InstanceFlags::with_env` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::MemoryBudgetThresholds` | struct | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::MemoryBudgetThresholds::for_device_loss` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::MemoryBudgetThresholds::for_resource_creation` | struct_field | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::instance::WgpuHasDisplayHandle` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::math` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::math::AlignTo` | trait | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::math::AlignTo::align_to` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::math::align_to` | function | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::strict_assert` | macro | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::strict_assert_eq` | macro | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::strict_assert_ne` | macro | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::webgpu_impl` | module | `jadix_2d::wgpu` | shared-fork |
| `wgpu_types::webgpu_impl` | use | `jadix_2d::wgpu` | shared-fork |

## 累计能力边界与标明轮次的证据

- **HDR/广色域**：Color2D、float Image2D、GradientStops::new_linear、RasterCanvas::new_hdr、RGBA16F PathGpu/图层/coverage/filter scratch/输出贯通。线性 sRGB 是扩展工作空间，允许负分量和大于 1；可显式转换 P3/Rec2020 等编码。尚未提供 OS HDR swapchain、HDR metadata/PQ/HLG 端到端显示管理；非 Normal blend 与 bounded imaging 滤镜仍有 LDR clamp。
- **CSS/色域**：与 Vello 共享 Color 0.3.3，支持 Color 4 字面量和 none、标准空间转换；Color2D::map_to 提供 Clip 和 Oklch local-MINDE。Color2D::parse 仍为字面量 parser；新增 ColorContext2D 支持调用者提供变量/currentColor/light-dark、两色 color-mix、现代相对颜色和标量 calc/min/max/clamp，真实 Edge 33 项差异 <=1 byte。仍非完整 stylesheet cascade/tokenizer/general typed CSS math；escapes/comments/device-cmyk/system colors 等未实现，不宣称全 CSS 一致性。
- **ICC**：可选 icc feature，通过已有 moxcms 0.8.1 extended-range 解析/转换 RGB/RGBA，限制 profile/CLUT/TRC 输入，真实合成 P3/sRGB profiles 验证。ICC LUT/意图支持仍遵循依赖本身；不声称所有 ICC profile、CMYK、多通道或 print workflow 完整覆盖。
- **增量 atlas**：AtlasCache<K> 显式 page/entry 预算、LRU、无移动 free rect、清空释放；Image2D immutable 更新记录 dirty rect；仅在 predecessor 不再被 CPU snapshot 持有且 GPU 纹理可复用时局部上传。旧 PreparedText 快照不会因淘汰破坏。CPU page copy 和新快照可能仍占整页，长期持有旧快照不受当前 cache 预算约束；FontSystem/Swash 缓存没有自动 LRU。
- **紧致/离屏滤镜**：SceneRegion 显式 origin/size/halo；负坐标源在 padded domain 上绘制滤镜后裁切。CPU 输出 RegionImage、GPU prepare_gpu 可在同样小目标绘制；本轮 SceneRegion::render_gpu 与 PathGpu::render_to_image 可直接把结果注册为后续 Canvas 图像。新增 PathGpu::set_tight_layer_bounds，自动为普通隔离组推导保守 source/filter 域并传递 texture origin；默认关闭。mask、生成透明背景内容、convolution 保留全域；不是数学最小域，也不减少全幅 compositor 目标。显式 SceneRegion 的 halo 仍由调用者预算。
- **硬件实例与网格**：GpuMesh 保留几何 GPU buffer，Instance step mode，一次 draw 多实例；warm 相同实例不上传；PreparedSceneGpu 避免 replay/tessellation。此显式 API 由 host render pass 负责 clip/MSAA/顺序；不是所有 Scene2D 自动硬件实例化，也不是透明的全局 mesh LRU。
- **几何集合**：PathGeometry2D 解析 winding 后保留共享三角形；有四种集合运算、面积/bounds/contains/变换和一次 fill。Curve flatten/Lyon 为 f32，布尔裁切 f64，输出路径再转 f32；不是 exact curve algebra，不能将三角形 contour 用于 stroke。max_work/max_triangles 显式失败；Area GPU 另有独立预算。
- **图像重建/缩小**：七种 ImageSampling，Mitchell Bicubic、Lanczos3、Trilinear、16-tap Anisotropic、17×17/层 Gaussian Ewa；linear-premultiplied 与明确 Extend/crop 合同。显式 CPU mip 链，以及 GPU-only region 转换/NPOT mip 生成均已实现。raw crop 仍 base bilinear；EWA 在缺失/截断 mip 时缩小支持范围至8 texel，不能当作任意 footprint 精确积分。
- **平台**：Windows DX12/Vulkan/native GLES 实测；Edge WebGPU/WebGL 见 [browser-runtime.json](../crates/jadix_2d/evidence/round18/browser-runtime.json)，失败同样保留；WASM check 仅证明编译。本轮未执行 Metal/mobile 真机验收，不将 Windows/Edge 实测称为这些平台的结果。
- **压力**：每后端 100,000 硬件实例 ×60 帧，记录 Windows peak working set/private commit/CPU 与 GPU dedicated/shared/committed 采样。操作系统计数存在采样间隔，不等于完整 driver VRAM 峰值。

详见 [round18原始实现与验收](2d-round18.md)、[round18结构化汇总](../crates/jadix_2d/evidence/round18/summary.json)。

## Vello 与 Jadix 像素和性能实测

round17 固定 Vello 0.11.0 与 Jadix，在 256²/512²、三后端、Area/Balanced/Reference 上比较八场景，共144项：三组字形、compound、gradient grid、dynamic curves、blurred rectangle、duplicate nonzero thin rectangles。hinted Jadix 在 device ppem 准备。五帧预热+20帧测量并等待 GPU。Vello 内部 glyph cache 与 Jadix 本次每帧显式 run preparation 的保留工作不同，不能把结果当相同缓存策略下的算法速度。历史四场景48项保存在 round15，字形36项保存在 round16。30k 矩形round17已按双方0.8px重跑，见独立压力表。


### 场景适配、失败与结论边界

固定版本 Vello fine.wgsl 在最终 textureStore 前显式 unpremultiply，因此比较使用其 straight sRGB 与 Jadix linear-premultiplied 输出的实际合同；round17脚本规范化后比较。

固定 Vello ramp encoder 未使用 Peniko interpolation_cs，gradient-grid 仍用513个预展开色标匹配线性 sRGB 插值。该适配成本、AA、缓存工作均不同，不能以计时宣称算法全面更快。原始探针保留于 round15/comparison-api-probe.json。

历史 30,000 个微矩形失败：[失败记录](../crates/jadix_2d/evidence/round15/large-scene-runs.json)。Vello DX12/FXC device lost；Jadix 超出原像素阈值。round17按双方0.8px重跑；旧场景原为双方实际1px，不与新几何混算。旧失败仍保留，不将设备失败计为速度胜出。

有限数据不能证明全 API/全平台超过 Vello。低层新字形路径使用相同 Skrifa/Kurbo 算法，但 raster AA、变换/网格拟合时机和缓存结构仍需按场景判断。


### 独立进程资源压力

下表是 round15 保存的独立进程 100,000 硬件实例×60帧资源压力证据，未伪装成round17重测。实例payload 2,800,000 bytes、几何144 bytes；Windows working set 高水位与六次GPU内存计数采样均不等于完整驱动VRAM峰值。新外部纹理路径另有多格式/alpha/嵌套缓存回归。

| 后端 | CPU 总秒 | peak working set MiB | peak private commit MiB | max sampled dedicated/shared/committed MiB |
|---|---|---|---|---|
| dx12 | 0.375 | 380.14 | 375.99 | 0.00/260.73/260.74 |
| vulkan | 0.219 | 320.01 | 341.85 | 0.02/198.61/198.70 |
| gles | 0.250 | 81.88 | 51.62 | 0.00/49.79/33.88 |

上述压力测试通过，但不是所有字体/纹理/离屏图层/无限快照的混合资源压力；30k 普通 Canvas 微几何的失败另见上文。

| 后端/场景/尺寸/Jadix AA | Vello P50/P95 ms | Jadix P50/P95 ms | Vello/Jadix P50 | linear-premul MAE (0..255) |
|---|---|---|---|---|
| dx12/glyph-unhinted/256/area | 2.260/2.854 | 0.893/1.022 | 2.530 | 0.0835 |
| dx12/glyph-hinted/256/area | 1.558/2.045 | 0.903/1.007 | 1.726 | 0.0806 |
| dx12/glyph-emboldened/256/area | 1.233/1.537 | 1.293/1.529 | 0.953 | 0.1245 |
| dx12/compound/256/area | 1.440/2.214 | 2.795/4.060 | 0.515 | 0.2614 |
| dx12/gradient-grid/256/area | 3.611/5.309 | 0.936/1.032 | 3.857 | 1.5543 |
| dx12/dynamic-curves/256/area | 1.375/1.656 | 5.532/14.044 | 0.249 | 3.3146 |
| dx12/blurred-rrect/256/area | 1.252/2.039 | 0.953/1.061 | 1.314 | 2.9588 |
| dx12/winding-overlap/256/area | 1.867/2.524 | 2.372/2.689 | 0.787 | 0.8677 |
| dx12/glyph-unhinted/512/area | 1.513/2.159 | 1.232/1.430 | 1.228 | 0.0382 |
| dx12/glyph-hinted/512/area | 1.385/1.961 | 1.080/2.104 | 1.282 | 0.0406 |
| dx12/glyph-emboldened/512/area | 1.518/2.106 | 2.363/3.021 | 0.642 | 0.0601 |
| dx12/compound/512/area | 1.486/2.104 | 12.360/12.957 | 0.120 | 0.1238 |
| dx12/gradient-grid/512/area | 3.431/5.017 | 1.408/1.551 | 2.437 | 0.8138 |
| dx12/dynamic-curves/512/area | 2.700/4.900 | 14.078/20.094 | 0.192 | 1.2439 |
| dx12/blurred-rrect/512/area | 2.147/3.191 | 2.026/2.512 | 1.060 | 2.6623 |
| dx12/winding-overlap/512/area | 1.388/2.000 | 7.803/8.319 | 0.178 | 0.6714 |
| dx12/glyph-unhinted/256/balanced | 2.260/2.854 | 1.038/1.558 | 2.178 | 0.7242 |
| dx12/glyph-hinted/256/balanced | 1.558/2.045 | 0.798/1.153 | 1.952 | 0.6516 |
| dx12/glyph-emboldened/256/balanced | 1.233/1.537 | 1.091/1.302 | 1.129 | 0.7552 |
| dx12/compound/256/balanced | 1.440/2.214 | 0.513/0.751 | 2.805 | 0.3155 |
| dx12/gradient-grid/256/balanced | 3.611/5.309 | 0.906/1.333 | 3.988 | 1.5543 |
| dx12/dynamic-curves/256/balanced | 1.375/1.656 | 1.101/1.399 | 1.249 | 4.1141 |
| dx12/blurred-rrect/256/balanced | 1.252/2.039 | 1.114/1.388 | 1.124 | 2.9588 |
| dx12/winding-overlap/256/balanced | 1.867/2.524 | 0.466/0.638 | 4.010 | 1.1470 |
| dx12/glyph-unhinted/512/balanced | 1.513/2.159 | 0.927/1.062 | 1.632 | 0.2800 |
| dx12/glyph-hinted/512/balanced | 1.385/1.961 | 0.966/1.237 | 1.433 | 0.2887 |
| dx12/glyph-emboldened/512/balanced | 1.518/2.106 | 1.203/1.576 | 1.262 | 0.3816 |
| dx12/compound/512/balanced | 1.486/2.104 | 0.702/0.826 | 2.117 | 0.1600 |
| dx12/gradient-grid/512/balanced | 3.431/5.017 | 1.520/1.738 | 2.257 | 0.8138 |
| dx12/dynamic-curves/512/balanced | 2.700/4.900 | 1.623/1.910 | 1.664 | 1.6568 |
| dx12/blurred-rrect/512/balanced | 2.147/3.191 | 3.309/3.530 | 0.649 | 2.6623 |
| dx12/winding-overlap/512/balanced | 1.388/2.000 | 0.714/0.800 | 1.945 | 0.9442 |
| dx12/glyph-unhinted/256/reference | 2.260/2.854 | 1.603/4.432 | 1.410 | 0.1891 |
| dx12/glyph-hinted/256/reference | 1.558/2.045 | 1.524/2.105 | 1.023 | 0.1523 |
| dx12/glyph-emboldened/256/reference | 1.233/1.537 | 1.763/2.084 | 0.699 | 0.2347 |
| dx12/compound/256/reference | 1.440/2.214 | 1.660/2.967 | 0.867 | 0.2624 |
| dx12/gradient-grid/256/reference | 3.611/5.309 | 3.970/4.831 | 0.910 | 1.4889 |
| dx12/dynamic-curves/256/reference | 1.375/1.656 | 2.622/3.002 | 0.524 | 3.3118 |
| dx12/blurred-rrect/256/reference | 1.252/2.039 | 9.929/10.666 | 0.126 | 2.9802 |
| dx12/winding-overlap/256/reference | 1.867/2.524 | 1.234/1.605 | 1.514 | 0.8002 |
| dx12/glyph-unhinted/512/reference | 1.513/2.159 | 6.328/6.921 | 0.239 | 0.0893 |
| dx12/glyph-hinted/512/reference | 1.385/1.961 | 3.599/3.937 | 0.385 | 0.0921 |
| dx12/glyph-emboldened/512/reference | 1.518/2.106 | 3.945/4.805 | 0.385 | 0.1051 |
| dx12/compound/512/reference | 1.486/2.104 | 3.857/4.473 | 0.385 | 0.1244 |
| dx12/gradient-grid/512/reference | 3.431/5.017 | 9.337/9.834 | 0.367 | 0.8213 |
| dx12/dynamic-curves/512/reference | 2.700/4.900 | 5.798/8.130 | 0.466 | 1.2409 |
| dx12/blurred-rrect/512/reference | 2.147/3.191 | 20.283/22.217 | 0.106 | 2.6569 |
| dx12/winding-overlap/512/reference | 1.388/2.000 | 3.434/3.783 | 0.404 | 0.6043 |
| vulkan/glyph-unhinted/256/area | 1.054/1.325 | 0.814/0.997 | 1.295 | 0.0835 |
| vulkan/glyph-hinted/256/area | 0.628/0.782 | 0.678/1.061 | 0.927 | 0.0806 |
| vulkan/glyph-emboldened/256/area | 0.851/0.907 | 1.126/1.650 | 0.756 | 0.1245 |
| vulkan/compound/256/area | 0.598/0.954 | 2.526/3.210 | 0.237 | 0.2625 |
| vulkan/gradient-grid/256/area | 2.562/2.942 | 0.514/0.603 | 4.982 | 1.5217 |
| vulkan/dynamic-curves/256/area | 0.690/0.927 | 2.754/3.690 | 0.251 | 3.3125 |
| vulkan/blurred-rrect/256/area | 0.597/0.730 | 0.703/2.240 | 0.850 | 2.9588 |
| vulkan/winding-overlap/256/area | 0.634/0.830 | 1.641/6.638 | 0.387 | 0.8677 |
| vulkan/glyph-unhinted/512/area | 0.942/1.201 | 0.688/0.907 | 1.369 | 0.0382 |
| vulkan/glyph-hinted/512/area | 0.937/1.063 | 0.581/0.832 | 1.612 | 0.0406 |
| vulkan/glyph-emboldened/512/area | 1.126/3.411 | 1.144/1.358 | 0.984 | 0.0601 |
| vulkan/compound/512/area | 1.262/1.773 | 12.236/13.497 | 0.103 | 0.1244 |
| vulkan/gradient-grid/512/area | 3.135/4.127 | 0.723/1.037 | 4.337 | 0.7700 |
| vulkan/dynamic-curves/512/area | 0.996/1.271 | 11.505/14.934 | 0.087 | 1.2429 |
| vulkan/blurred-rrect/512/area | 0.905/1.029 | 2.086/2.220 | 0.434 | 2.6623 |
| vulkan/winding-overlap/512/area | 0.762/1.028 | 3.896/4.183 | 0.196 | 0.6714 |
| vulkan/glyph-unhinted/256/balanced | 1.054/1.325 | 0.585/0.756 | 1.802 | 0.7242 |
| vulkan/glyph-hinted/256/balanced | 0.628/0.782 | 0.681/1.082 | 0.922 | 0.6516 |
| vulkan/glyph-emboldened/256/balanced | 0.851/0.907 | 0.809/0.922 | 1.052 | 0.7552 |
| vulkan/compound/256/balanced | 0.598/0.954 | 0.223/0.293 | 2.681 | 0.3160 |
| vulkan/gradient-grid/256/balanced | 2.562/2.942 | 0.419/0.580 | 6.117 | 1.5217 |
| vulkan/dynamic-curves/256/balanced | 0.690/0.927 | 0.889/1.097 | 0.777 | 4.1132 |
| vulkan/blurred-rrect/256/balanced | 0.597/0.730 | 0.739/0.859 | 0.808 | 2.9588 |
| vulkan/winding-overlap/256/balanced | 0.634/0.830 | 0.246/0.300 | 2.577 | 1.1470 |
| vulkan/glyph-unhinted/512/balanced | 0.942/1.201 | 0.776/0.946 | 1.212 | 0.2800 |
| vulkan/glyph-hinted/512/balanced | 0.937/1.063 | 0.811/1.192 | 1.155 | 0.2887 |
| vulkan/glyph-emboldened/512/balanced | 1.126/3.411 | 1.005/1.168 | 1.120 | 0.3816 |
| vulkan/compound/512/balanced | 1.262/1.773 | 0.561/0.843 | 2.250 | 0.1604 |
| vulkan/gradient-grid/512/balanced | 3.135/4.127 | 0.927/1.003 | 3.381 | 0.7700 |
| vulkan/dynamic-curves/512/balanced | 0.996/1.271 | 1.392/2.114 | 0.716 | 1.6564 |
| vulkan/blurred-rrect/512/balanced | 0.905/1.029 | 2.524/2.986 | 0.359 | 2.6623 |
| vulkan/winding-overlap/512/balanced | 0.762/1.028 | 0.449/0.510 | 1.697 | 0.9442 |
| vulkan/glyph-unhinted/256/reference | 1.054/1.325 | 2.095/2.649 | 0.503 | 0.1891 |
| vulkan/glyph-hinted/256/reference | 0.628/0.782 | 2.109/2.412 | 0.298 | 0.1523 |
| vulkan/glyph-emboldened/256/reference | 0.851/0.907 | 2.328/2.579 | 0.366 | 0.2347 |
| vulkan/compound/256/reference | 0.598/0.954 | 1.924/2.037 | 0.311 | 0.2635 |
| vulkan/gradient-grid/256/reference | 2.562/2.942 | 2.826/3.037 | 0.907 | 1.4578 |
| vulkan/dynamic-curves/256/reference | 0.690/0.927 | 1.655/1.877 | 0.417 | 3.3100 |
| vulkan/blurred-rrect/256/reference | 0.597/0.730 | 4.533/6.167 | 0.132 | 2.9802 |
| vulkan/winding-overlap/256/reference | 0.634/0.830 | 1.783/1.871 | 0.356 | 0.8002 |
| vulkan/glyph-unhinted/512/reference | 0.942/1.201 | 4.576/6.719 | 0.206 | 0.0893 |
| vulkan/glyph-hinted/512/reference | 0.937/1.063 | 3.948/5.277 | 0.237 | 0.0921 |
| vulkan/glyph-emboldened/512/reference | 1.126/3.411 | 4.299/7.538 | 0.262 | 0.1051 |
| vulkan/compound/512/reference | 1.262/1.773 | 3.738/4.690 | 0.338 | 0.1250 |
| vulkan/gradient-grid/512/reference | 3.135/4.127 | 5.226/5.512 | 0.600 | 0.7748 |
| vulkan/dynamic-curves/512/reference | 0.996/1.271 | 4.515/5.851 | 0.221 | 1.2401 |
| vulkan/blurred-rrect/512/reference | 0.905/1.029 | 16.196/17.547 | 0.056 | 2.6569 |
| vulkan/winding-overlap/512/reference | 0.762/1.028 | 3.421/4.157 | 0.223 | 0.6043 |
| gles/glyph-unhinted/256/area | 0.943/1.309 | 1.195/1.378 | 0.790 | 0.0835 |
| gles/glyph-hinted/256/area | 1.129/2.129 | 1.145/1.695 | 0.986 | 0.0806 |
| gles/glyph-emboldened/256/area | 1.096/1.755 | 2.130/5.225 | 0.514 | 0.1245 |
| gles/compound/256/area | 1.015/3.050 | 3.524/4.491 | 0.288 | 0.2625 |
| gles/gradient-grid/256/area | 3.200/4.596 | 0.999/1.476 | 3.204 | 1.5217 |
| gles/dynamic-curves/256/area | 1.178/1.684 | 3.235/5.102 | 0.364 | 3.3125 |
| gles/blurred-rrect/256/area | 0.952/2.850 | 1.361/1.530 | 0.700 | 2.9588 |
| gles/winding-overlap/256/area | 1.529/3.134 | 1.993/2.899 | 0.767 | 0.8677 |
| gles/glyph-unhinted/512/area | 1.440/2.674 | 1.245/2.751 | 1.156 | 0.0382 |
| gles/glyph-hinted/512/area | 1.798/2.645 | 1.095/1.919 | 1.642 | 0.0406 |
| gles/glyph-emboldened/512/area | 1.575/2.341 | 1.881/2.897 | 0.837 | 0.0601 |
| gles/compound/512/area | 1.282/2.563 | 12.888/17.445 | 0.099 | 0.1244 |
| gles/gradient-grid/512/area | 4.521/6.106 | 1.643/2.545 | 2.752 | 0.7700 |
| gles/dynamic-curves/512/area | 1.420/2.004 | 10.851/18.623 | 0.131 | 1.2429 |
| gles/blurred-rrect/512/area | 1.265/2.343 | 4.350/4.472 | 0.291 | 2.6623 |
| gles/winding-overlap/512/area | 1.261/1.699 | 5.676/7.873 | 0.222 | 0.6714 |
| gles/glyph-unhinted/256/balanced | 0.943/1.309 | 0.959/2.926 | 0.984 | 0.7242 |
| gles/glyph-hinted/256/balanced | 1.129/2.129 | 0.929/2.351 | 1.216 | 0.6516 |
| gles/glyph-emboldened/256/balanced | 1.096/1.755 | 1.338/1.710 | 0.819 | 0.7552 |
| gles/compound/256/balanced | 1.015/3.050 | 0.607/1.413 | 1.672 | 0.3160 |
| gles/gradient-grid/256/balanced | 3.200/4.596 | 0.997/2.496 | 3.208 | 1.5217 |
| gles/dynamic-curves/256/balanced | 1.178/1.684 | 1.668/2.063 | 0.706 | 4.1132 |
| gles/blurred-rrect/256/balanced | 0.952/2.850 | 1.497/1.808 | 0.636 | 2.9588 |
| gles/winding-overlap/256/balanced | 1.529/3.134 | 0.650/0.963 | 2.351 | 1.1470 |
| gles/glyph-unhinted/512/balanced | 1.440/2.674 | 1.182/1.585 | 1.218 | 0.2800 |
| gles/glyph-hinted/512/balanced | 1.798/2.645 | 1.174/1.446 | 1.531 | 0.2887 |
| gles/glyph-emboldened/512/balanced | 1.575/2.341 | 1.558/3.029 | 1.011 | 0.3816 |
| gles/compound/512/balanced | 1.282/2.563 | 0.923/1.395 | 1.388 | 0.1604 |
| gles/gradient-grid/512/balanced | 4.521/6.106 | 2.025/2.370 | 2.232 | 0.7700 |
| gles/dynamic-curves/512/balanced | 1.420/2.004 | 2.027/2.632 | 0.701 | 1.6564 |
| gles/blurred-rrect/512/balanced | 1.265/2.343 | 3.672/4.926 | 0.345 | 2.6623 |
| gles/winding-overlap/512/balanced | 1.261/1.699 | 0.845/1.169 | 1.492 | 0.9442 |
| gles/glyph-unhinted/256/reference | 0.943/1.309 | 2.117/3.366 | 0.446 | 0.1891 |
| gles/glyph-hinted/256/reference | 1.129/2.129 | 1.688/2.678 | 0.669 | 0.1523 |
| gles/glyph-emboldened/256/reference | 1.096/1.755 | 2.162/5.390 | 0.507 | 0.2347 |
| gles/compound/256/reference | 1.015/3.050 | 1.744/3.690 | 0.582 | 0.2635 |
| gles/gradient-grid/256/reference | 3.200/4.596 | 4.035/5.219 | 0.793 | 1.4578 |
| gles/dynamic-curves/256/reference | 1.178/1.684 | 2.106/3.523 | 0.559 | 3.3100 |
| gles/blurred-rrect/256/reference | 0.952/2.850 | 6.642/9.425 | 0.143 | 2.9802 |
| gles/winding-overlap/256/reference | 1.529/3.134 | 1.693/2.845 | 0.903 | 0.8002 |
| gles/glyph-unhinted/512/reference | 1.440/2.674 | 3.111/4.341 | 0.463 | 0.0893 |
| gles/glyph-hinted/512/reference | 1.798/2.645 | 3.128/4.522 | 0.575 | 0.0921 |
| gles/glyph-emboldened/512/reference | 1.575/2.341 | 4.258/6.980 | 0.370 | 0.1051 |
| gles/compound/512/reference | 1.282/2.563 | 3.405/4.562 | 0.376 | 0.1250 |
| gles/gradient-grid/512/reference | 4.521/6.106 | 9.016/10.034 | 0.501 | 0.7748 |
| gles/dynamic-curves/512/reference | 1.420/2.004 | 5.699/7.873 | 0.249 | 1.2401 |
| gles/blurred-rrect/512/reference | 1.265/2.343 | 20.279/22.462 | 0.062 | 2.6569 |
| gles/winding-overlap/512/reference | 1.261/1.699 | 2.851/3.699 | 0.442 | 0.6043 |

round17 Balanced 的 48 项中 Jadix 有 36 项 P50 较低；只报告本机有限数据，未做重复试验置信区间，也不是全场景结论。AA 与保留缓存工作不同，逐项误差和计时须一起阅读。

![round17 规范化场景像素对照：Vello/Jadix Area/差异放大](../crates/jadix_2d/evidence/round17/pixel-comparison.png)

源数据：[vello-comparison.json](../crates/jadix_2d/evidence/round17/vello-comparison.json)。有限场景数据不代表全面超越；完整 API 清单中的缺口不因该基准消失。

### 重复 nonzero 轮廓的独立覆盖参考

同一 thin rectangle 重复两次不改变 nonzero 填充区域。Python f64 直接计算矩形与像素交面积，不调用双方几何/栅格实现。这里只验证 alpha coverage，不是所有复杂路径的正确性证明。

| 后端/尺寸/渲染器 | ink alpha MAE (0..255) | max alpha delta |
|---|---|---|
| dx12/256/vello | 80.3764 | 81.7500 |
| dx12/512/vello | 61.9597 | 102.0000 |
| dx12/256/jadix-area | 0.2382 | 0.2500 |
| dx12/512/jadix-area | 0.2514 | 0.5000 |
| dx12/256/jadix-balanced | 25.8729 | 66.9375 |
| dx12/512/jadix-balanced | 25.0958 | 45.9000 |
| dx12/256/jadix-reference | 6.5708 | 6.7500 |
| dx12/512/jadix-reference | 6.1681 | 6.5000 |
| vulkan/256/vello | 80.3764 | 81.7500 |
| vulkan/512/vello | 61.9597 | 102.0000 |
| vulkan/256/jadix-area | 0.2382 | 0.2500 |
| vulkan/512/jadix-area | 0.2514 | 0.5000 |
| vulkan/256/jadix-balanced | 25.8729 | 66.9375 |
| vulkan/512/jadix-balanced | 25.0958 | 45.9000 |
| vulkan/256/jadix-reference | 6.5708 | 6.7500 |
| vulkan/512/jadix-reference | 6.1681 | 6.5000 |
| gles/256/vello | 80.3764 | 81.7500 |
| gles/512/vello | 61.9597 | 102.0000 |
| gles/256/jadix-area | 0.2382 | 0.2500 |
| gles/512/jadix-area | 0.2514 | 0.5000 |
| gles/256/jadix-balanced | 25.8729 | 66.9375 |
| gles/512/jadix-balanced | 25.0958 | 45.9000 |
| gles/256/jadix-reference | 6.5708 | 6.7500 |
| gles/512/jadix-reference | 6.1681 | 6.5000 |

完整参考：[winding-oracle.json](../crates/jadix_2d/evidence/round17/winding-oracle.json)。

### 30k 真正亚像素矩形重跑

round17 双方统一0.8×0.8逻辑像素；旧参考曾用1px对应 Jadix 原先的最小1px钳制。旧结果不可与新几何直接混算，失败证据保留。当前 Jadix Area 的 CPU/GPU误差以及所有 Vello失败均按实际记录。

| 后端/渲染器 | exit | 秒 |
|---|---|---|
| dx12/vello | 1 | 13.54 |
| dx12/jadix-area | 0 | 2.32 |
| vulkan/vello | 0 | 0.79 |
| vulkan/jadix-area | 0 | 1.37 |
| gles/vello | 0 | 1.25 |
| gles/jadix-area | 0 | 1.21 |

命令与状态：[large-scene-runs.json](../crates/jadix_2d/evidence/round17/large-scene-runs.json)。两测量帧只作有限压力/画质验证，不用于性能排名。

## round18：几何交集与重复 clip 的独立参考

32个不重叠细矩形、256²/512²、三个原生后端。Vello 采用两个相同 push_clip_layer；Jadix 显式两次 Intersection 后一次 fill。直接 Python f64 矩形交集/像素面积参考，不调用双方栅格代码。这里只比较 alpha coverage，准备路径和缓存工作不同，不做速度排名。

| 后端/尺寸/实现 | ink alpha MAE /255 | max alpha delta /255 |
|---|---|---|
| dx12/256/vello | 62.20625 | 79.25000 |
| dx12/256/jadix-area | 0.12465 | 0.25000 |
| dx12/512/vello | 66.93704 | 101.00000 |
| dx12/512/jadix-area | 0.16944 | 0.50000 |
| vulkan/256/vello | 62.20625 | 79.25000 |
| vulkan/256/jadix-area | 0.12465 | 0.25000 |
| vulkan/512/vello | 66.93704 | 101.00000 |
| vulkan/512/jadix-area | 0.16944 | 0.50000 |
| gles/256/vello | 62.20625 | 79.25000 |
| gles/256/jadix-area | 0.12465 | 0.25000 |
| gles/512/vello | 66.93704 | 101.00000 |
| gles/512/jadix-area | 0.16944 | 0.50000 |

[原始 oracle 与限制](../crates/jadix_2d/evidence/round18/clip-oracle.json)。新 API 解决显式几何集合，不会自动改变原 clip_path mask 或修复任意重叠 draw 的采样相关性。

## round19：整场景采样与预滤波缩小的独立参考

新增公开入口：`Image2D::with_mipmaps`、`mipmapped_region`、`sample_linear_with_footprint`、`ImageSampling::Trilinear/Anisotropic`、`Scene2D::sampled`、`SampledScene2D::rasterize/prepare_gpu`、`PreparedSampledSceneGpu::render`。默认 API 路径不自动切换为高成本超采样。

32²/64²，三个原生后端。接缝场景为两个在 x=12.25 相接的纯色不透明矩形；缩小场景为383×191黑白棋盘，参考为 Python f64 对原始 texel 面积积分，颜色解码到 linear-premultiplied 后比较。Jadix 分别使用整场景4×4样本和显式 Trilinear mip；Vello 使用 ImageQuality::High，分别运行 Area/Msaa8/Msaa16。算法、缓存和滤波带宽不同，不以这些运行的时间作速度排名。主误差包含颜色域合同差异：固定 Vello fine.wgsl 对 encoded image_atlas texel 做数值插值，Jadix 在 linear-premultiplied 域预滤波；不能把 gamma 差异都解释为混叠。原始 JSON 的 filter_domain_rgb_mae_255 另按各自滤波域计算，以区分缩小振荡与传递函数差异。

| 后端/尺寸/场景/实现 | linear premul MAE /255 | RMSE (0..255) | max alpha delta /255 |
|---|---|---|---|
| dx12/32/sampling-seams/jadix | 0.00618 | 0.06641 | 0.00000 |
| dx12/32/sampling-seams/vello-area | 1.01236 | 7.15742 | 48.00000 |
| dx12/32/sampling-seams/vello-msaa8 | 1.01236 | 7.15742 | 48.00000 |
| dx12/32/sampling-seams/vello-msaa16 | 1.01236 | 7.15742 | 48.00000 |
| dx12/32/sampling-minify/jadix | 0.58787 | 0.67882 | 0.00000 |
| dx12/32/sampling-minify/vello-area | 54.34299 | 62.74988 | 0.00000 |
| dx12/32/sampling-minify/vello-msaa8 | 54.34299 | 62.74988 | 0.00000 |
| dx12/32/sampling-minify/vello-msaa16 | 54.34299 | 62.74988 | 0.00000 |
| dx12/64/sampling-seams/jadix | 0.00309 | 0.04696 | 0.00000 |
| dx12/64/sampling-seams/vello-area | 0.50618 | 5.06106 | 48.00000 |
| dx12/64/sampling-seams/vello-msaa8 | 0.50618 | 5.06106 | 48.00000 |
| dx12/64/sampling-seams/vello-msaa16 | 0.50618 | 5.06106 | 48.00000 |
| dx12/64/sampling-minify/jadix | 0.58787 | 0.68110 | 0.00000 |
| dx12/64/sampling-minify/vello-area | 54.34299 | 62.74990 | 0.00000 |
| dx12/64/sampling-minify/vello-msaa8 | 54.34299 | 62.74990 | 0.00000 |
| dx12/64/sampling-minify/vello-msaa16 | 54.34299 | 62.74990 | 0.00000 |
| vulkan/32/sampling-seams/jadix | 0.00618 | 0.06641 | 0.00000 |
| vulkan/32/sampling-seams/vello-area | 1.01236 | 7.15742 | 48.00000 |
| vulkan/32/sampling-seams/vello-msaa8 | 1.01236 | 7.15742 | 48.00000 |
| vulkan/32/sampling-seams/vello-msaa16 | 1.01236 | 7.15742 | 48.00000 |
| vulkan/32/sampling-minify/jadix | 0.58787 | 0.67882 | 0.00000 |
| vulkan/32/sampling-minify/vello-area | 71.06526 | 82.95311 | 0.00000 |
| vulkan/32/sampling-minify/vello-msaa8 | 71.06526 | 82.95311 | 0.00000 |
| vulkan/32/sampling-minify/vello-msaa16 | 71.06526 | 82.95311 | 0.00000 |
| vulkan/64/sampling-seams/jadix | 0.00309 | 0.04696 | 0.00000 |
| vulkan/64/sampling-seams/vello-area | 0.50618 | 5.06106 | 48.00000 |
| vulkan/64/sampling-seams/vello-msaa8 | 0.50618 | 5.06106 | 48.00000 |
| vulkan/64/sampling-seams/vello-msaa16 | 0.50618 | 5.06106 | 48.00000 |
| vulkan/64/sampling-minify/jadix | 0.58787 | 0.68110 | 0.00000 |
| vulkan/64/sampling-minify/vello-area | 51.66982 | 65.96254 | 0.00000 |
| vulkan/64/sampling-minify/vello-msaa8 | 51.66982 | 65.96254 | 0.00000 |
| vulkan/64/sampling-minify/vello-msaa16 | 51.66982 | 65.96254 | 0.00000 |
| gles/32/sampling-seams/jadix | 0.00618 | 0.06641 | 0.00000 |
| gles/32/sampling-seams/vello-area | 1.01236 | 7.15742 | 48.00000 |
| gles/32/sampling-seams/vello-msaa8 | 1.01236 | 7.15742 | 48.00000 |
| gles/32/sampling-seams/vello-msaa16 | 1.01236 | 7.15742 | 48.00000 |
| gles/32/sampling-minify/jadix | 0.58787 | 0.67882 | 0.00000 |
| gles/32/sampling-minify/vello-area | 71.06526 | 82.95311 | 0.00000 |
| gles/32/sampling-minify/vello-msaa8 | 71.06526 | 82.95311 | 0.00000 |
| gles/32/sampling-minify/vello-msaa16 | 71.06526 | 82.95311 | 0.00000 |
| gles/64/sampling-seams/jadix | 0.00309 | 0.04696 | 0.00000 |
| gles/64/sampling-seams/vello-area | 0.50618 | 5.06106 | 48.00000 |
| gles/64/sampling-seams/vello-msaa8 | 0.50618 | 5.06106 | 48.00000 |
| gles/64/sampling-seams/vello-msaa16 | 0.50618 | 5.06106 | 48.00000 |
| gles/64/sampling-minify/jadix | 0.58787 | 0.68110 | 0.00000 |
| gles/64/sampling-minify/vello-area | 51.66982 | 65.96254 | 0.00000 |
| gles/64/sampling-minify/vello-msaa8 | 51.66982 | 65.96254 | 0.00000 |
| gles/64/sampling-minify/vello-msaa16 | 51.66982 | 65.96254 | 0.00000 |

![round19：相接色块与 NPOT 高频缩小，对照独立面积参考](../crates/jadix_2d/evidence/round19/sampling-comparison.png)

[完整原始数据](../crates/jadix_2d/evidence/round19/sampling-oracle.json)、[命令及状态](../crates/jadix_2d/evidence/round19/sampling-reference-runs.json)、[API/限制/跨平台测试](2d-round19.md)。这里的有限画质优势不能外推为全 API、所有几何或全平台超越。

round19 时 mip 链由调用者显式准备；其 GPU 外部纹理注册只产生 level 0（该缺口已由下述 round20 补齐）。源矩形需要 `mipmapped_region` 独立预滤波，否则按 base bilinear 限域。16-tap 各向异性是有界主轴积分，非 EWA。整场景网格最多64样本/像素，可能漏掉比采样间距更细的几何；FP16中间结果和256..1024MiB预算有明确上限。Metal/mobile、OS HDR 实际呈现、全 CSS color 和所有字体/网格资源混合压力仍未证明。

## round20：GPU-only mip、Gaussian EWA 与大 compound 路径

本轮补齐 GPU 外部图像的独立区域预滤波，并增加 CPU/GPU `ImageSampling::Ewa`。`register_texture_region` / `override_image_region` 显式选择源区域和 `MipmapLimits`；`set_registered_texture_mipmaps` 配置已有句柄；`PathGpu::prepare_sampled_scene` 将注册表冻结到整场景准备对象。源纹理不读回，颜色/alpha 转换后在 FP16 mip 链中逐层面积预滤波。脏更新重建，热帧复用；mask 保持单层。

同色非图像 Area/点模式大路径使用16×16像素空间列表，保留一次像素合成。6400三角形 compound 与整场景采样已在三原生后端通过独立 f64 面积/CPU参考。每列表4096三角形、262144输入三角形、1048560几何texel、约67M work 仍是显式预算；密集重叠或巨大跨域三角形仍可能失败。

Gaussian EWA 的实际 NPOT 坐标、协方差和归一化支持最多17×17/层、最多两层；省略/截断 mip 时支持范围按8 texel钳制。它与 Vello High 的 Mitchell Bicubic、box 参考和有限主轴采样具有不同滤波带宽，不能仅凭一个误差指标宣布全方面胜出。

[完整 API、合同、验证与资源限制](2d-round20.md)、[本轮结构化摘要](../crates/jadix_2d/evidence/round20/summary.json)。

### 斜向缩小的明确参考

127×65图像，Repeat/Reflect，相同 affine，32²/64²、三后端。独立 Python f64 对 affine 像素 footprint/source texel 单元裁切作面积积分，另记录32×32 stratified交叉检查与质量守恒误差；这里是距离指定 linear-light box 参考的误差，Gaussian/tent/encoded bicubic 的合同差异全部计入。时间与资源不能由像素误差推导。

| 后端/尺寸/实现 | linear RGB MAE (0..255) | RMSE (0..255) | max (0..255) |
|---|---|---|---|
| dx12/32/Trilinear | 3.09738 | 10.78639 | 83.81000 |
| dx12/32/Anisotropic | 1.29981 | 4.46884 | 36.61277 |
| dx12/32/Ewa | 1.20661 | 3.80243 | 27.41892 |
| dx12/32/vello-area | 26.31850 | 44.00509 | 126.02980 |
| dx12/32/vello-msaa8 | 26.31850 | 44.00509 | 126.02980 |
| dx12/32/vello-msaa16 | 26.31850 | 44.00509 | 126.02980 |
| vulkan/32/Trilinear | 3.09738 | 10.78639 | 83.81000 |
| vulkan/32/Anisotropic | 1.29981 | 4.46884 | 36.61277 |
| vulkan/32/Ewa | 1.20661 | 3.80243 | 27.41892 |
| vulkan/32/vello-area | 27.25288 | 51.07859 | 143.41210 |
| vulkan/32/vello-msaa8 | 27.25288 | 51.07859 | 143.41210 |
| vulkan/32/vello-msaa16 | 27.25288 | 51.07859 | 143.41210 |
| gles/32/Trilinear | 3.09738 | 10.78639 | 83.81000 |
| gles/32/Anisotropic | 1.29981 | 4.46884 | 36.61277 |
| gles/32/Ewa | 1.20661 | 3.80243 | 27.41892 |
| gles/32/vello-area | 27.25288 | 51.07859 | 143.41210 |
| gles/32/vello-msaa8 | 27.25288 | 51.07859 | 143.41210 |
| gles/32/vello-msaa16 | 27.25288 | 51.07859 | 143.41210 |
| dx12/64/Trilinear | 2.93476 | 10.49538 | 83.81000 |
| dx12/64/Anisotropic | 1.28528 | 4.40079 | 36.92618 |
| dx12/64/Ewa | 1.22700 | 3.80513 | 27.41892 |
| dx12/64/vello-area | 26.50189 | 44.26775 | 131.05039 |
| dx12/64/vello-msaa8 | 26.50189 | 44.26775 | 131.05039 |
| dx12/64/vello-msaa16 | 26.50189 | 44.26775 | 131.05039 |
| vulkan/64/Trilinear | 2.93476 | 10.49538 | 83.81000 |
| vulkan/64/Anisotropic | 1.28528 | 4.40079 | 36.92618 |
| vulkan/64/Ewa | 1.22700 | 3.80513 | 27.41892 |
| vulkan/64/vello-area | 27.39817 | 51.25331 | 143.55463 |
| vulkan/64/vello-msaa8 | 27.39817 | 51.25331 | 143.55463 |
| vulkan/64/vello-msaa16 | 27.39817 | 51.25331 | 143.55463 |
| gles/64/Trilinear | 2.93476 | 10.49538 | 83.81000 |
| gles/64/Anisotropic | 1.28528 | 4.40079 | 36.92618 |
| gles/64/Ewa | 1.22700 | 3.80513 | 27.41892 |
| gles/64/vello-area | 27.39817 | 51.25331 | 143.55463 |
| gles/64/vello-msaa8 | 27.39817 | 51.25331 | 143.55463 |
| gles/64/vello-msaa16 | 27.39817 | 51.25331 | 143.55463 |

![斜向缩小：Vello High、Jadix三滤波及独立box参考](../crates/jadix_2d/evidence/round20/oblique-comparison.png)

[完整原始结果、独立参考交叉检查与合同](../crates/jadix_2d/evidence/round20/oblique-oracle.json)。Metal/mobile、OS HDR 实际呈现、完整 CSS 语法、所有资源混合压力以及 Vello compute ABI 兼容仍未证明；公开 API 枚举完整不代表全 API 动态等价。

## round21：四项对应入口、DX12 初始化和可选 compute

上一轮四个 `missing` 项现在均有公开用途对应入口：初始化线程和 wgpu cache 分类为用途对应；CPU 执行和场景估算仍保留 `partial`，因为它们分别采用完整 CPU 光栅化与 Jadix payload 规划，而非 Vello compute coarse/fine 与 bump ABI。消除 `missing` 标签不代表消除所有语义差异。

| 项目 | 当前实现与边界 |
|---|---|
| CPU 模式 | `SceneRenderer2D` 执行 CPU clip/filter/composite，再上传线性预乘 FP16 并输出 straight/premultiplied。CPU surface 预算默认512MiB、硬上限1GiB；GPU-only 图像需显式 CPU fallback。 |
| 初始化线程 | `RendererOptions::num_init_threads` 控制实际所需材质的有界线程批次，`warm_up` 可主动预热；WebAssembly 串行。 |
| 管线缓存 | caller-owned same-device `wgpu::PipelineCache` 传入 Jadix 自有路径、Area、layer/filter/import/mip/sample-resolve 管线。Vulkan 数据读回实测；DX12 不保证可持久化，独立 `GpuMesh` 不归该配置管理。 |
| 场景估算 | `SceneMemoryEstimate` 检查整数溢出，报告几何、vertex upload、Area列表、去重图像、保守 layer/target 字节及工作量上界。不是总RSS/VRAM峰值或 Vello bump 编码。 |
| DX12 首次使用 | Area 先路由再初始化，只建需要的材质；普通图像模块移除 EWA 函数体，EWA 首次使用单向升级对应 PSO。避免标准/Area 双重无用初始化；没有降低滤波质量。 |
| compute feature | `jadix` / `jadix_2d` / `jadix_renderer` 转发至 `jadix_wgpu/compute`。DX12 backend 不再隐式开启。wgpu 包自身默认 feature 保持上游兼容，工作区依赖仍 default-features=false。 |
| compute 执行 | direct/indirect buffer/storage-texture dispatch，GPU-only NPOT mip compute；WebGL render-pass fallback。CPU/GPU 图像能力不要求 compute。 |

关闭 compute 时两个安全公开入口 `Device::create_compute_pipeline`、`CommandEncoder::begin_compute_pass` 不可用；描述符/handle 类型及后端内部仍保留，不能称为完整后端剥离或安全隔离。Cargo feature 是依赖图并集，任何依赖显式启用都会在同一包实例生效。真实执行还依赖 requested-device limits 和 adapter downlevel flags。

[本轮详细合同与验证](2d-round21.md)、[结构化验收](../crates/jadix_2d/evidence/round21/summary.json)、[feature 正反编译](../crates/jadix_2d/evidence/round21/compute-feature-contract.json)。四项公开入口已补齐，但 Metal/mobile 实机、OS HDR 呈现、完整 CSS 语法和所有资源混合高水位仍不在现有证据范围。

### dx12 初始化阶段观测（当前 round24）

| 同设备新 renderer | 构造 ms | 首次纯色 ms | 首次 bilinear 图案 ms | 首次 EWA 图案 ms |
|---|---:|---:|---:|---:|
| 0 | 37.472 | 79.778 | 118.998 | 1664.326 |
| 1 | 15.783 | 20.959 | 3.004 | 3.238 |

包含验证、提交和GPU等待；没有清空系统/驱动缓存，不是纯编译计时，不能据此声称全场景冷启动倍率或超过 Vello。

### vulkan 初始化阶段观测（当前 round24）

| 同设备新 renderer | 构造 ms | 首次纯色 ms | 首次 bilinear 图案 ms | 首次 EWA 图案 ms |
|---|---:|---:|---:|---:|
| 0 | 15.241 | 21.683 | 2.377 | 2.079 |
| 1 | 16.947 | 18.798 | 1.830 | 1.905 |

包含验证、提交和GPU等待；没有清空系统/驱动缓存，不是纯编译计时，不能据此声称全场景冷启动倍率或超过 Vello。

### gles 初始化阶段观测（当前 round24）

| 同设备新 renderer | 构造 ms | 首次纯色 ms | 首次 bilinear 图案 ms | 首次 EWA 图案 ms |
|---|---:|---:|---:|---:|
| 0 | 17.579 | 29.695 | 6.168 | 79.848 |
| 1 | 16.435 | 21.628 | 5.422 | 6.473 |

包含验证、提交和GPU等待；没有清空系统/驱动缓存，不是纯编译计时，不能据此声称全场景冷启动倍率或超过 Vello。

## round22：compute 命令与资源 API 的逐项对应

新增 `gpu::low_level`（需要 compute feature），覆盖 ResourceId、BufferProxy、ImageProxy、ResourceProxy/BufferRange、BindType、ShaderId、Command、Recording 和实际 Engine 执行。原先 low_level 的统一“无兼容 ABI”说明已逐项替换为入口、执行语义和具体限制，FullShaders/Render 算法阶段仍保留差异。

上传采用 encoder staging copy，保留 `upload → dispatch → upload → indirect → download` 的实际顺序；读回是命令位置快照，资源存续至显式释放。检查 ID/descriptor 冲突、use-after-free、range 对齐/长度、图像区域、binding schema、工作组和 resource/staging/encoder峰值预算；无效 recording 预检不分配、不提交。原生 GPU validation 可同步返回错误；浏览器仍需外层 awaited error scope，不能将同步返回当作异步验证成功。

`Engine::add_compute_shader` 和 `add_render_shader` 接受调用者 WGSL，PSO 按需编译并缓存；同一 command stream 可交错 compute 与无 vertex-buffer 的 vertex-index render draw。支持 FP16 storage 图像、局部上传、image readback（含显式 padded stride）和现有 Canvas GPU import。不是引入 Vello 运行时或复制其内部 buffers。

`low_level::Render` 把 CPU scene preparation、可取走的 CapturedScene、调用者 encoder 的 record_fine 和保留 FP16 out_image 分开；它接受 Scene2D，不接受 Vello Encoding/Resolver。FullShaders 的 pathtag scans、coarse tile commands、GPU bump allocator 和 native fine_msaa8/16 不伪装成已兼容：对应行逐项写明 native 方案与未实现的特定算法。

DX12 再增加 nearest/bilinear 专用 shader tier，去掉 Bicubic/Lanczos/mip/EWA 重建循环；高阶采样和 EWA 单向升级对应材质，混合场景保持最强已需 tier。首次普通图案不承担完整重建编译；完整质量矩阵及历史图像继续作为回归门槛。

[本轮实现、预算与实际验收](2d-round22.md)、[结构化证据](../crates/jadix_2d/evidence/round22/summary.json)。Native low-level 命令用途覆盖不等于 Vello 内部 shader-buffer ABI 兼容，未把算法差异改标为虚假的等价。

## round23：最新稳定版对照与后续增强

2026-11-06 通过 crates.io 官方 API 实时查询，Vello 最新稳定版为0.11.0（2026-10-02发布），Vello Hybrid0.2.0、Vello CPU0.3.0是不同包。本轮实际链接发布版 Vello0.11.0/upstream wgpu30，未修改其渲染源码；[原始版本查询](../crates/jadix_2d/evidence/round23/vello-registry.json)与锁文件一并保留。

新增同输入、同阶段的三个后端对照，每实现/后端三个全新进程，交替执行顺序；每进程同设备新建两个 renderer，Area、单初始化线程，DX12均FXC。原始统一白图/NPOT/斜向图案文件由两实现读取，同样3200矩形compound输入。分别记录构造、CPU准备、render+wait、读回和整个进程CPU/峰值RSS。

Vello会在构造阶段初始化更多PSO；Jadix延迟初始化，所以“构造+首次纯色”与整个进程都列出。High是Vello High对Jadix EWA，滤波带宽/色域/显式mip不同，不能当作严格同算法速度排名。旧Jadix GPU-only FP16 mip、HDR、sampled-scene整套测试没有对应的Vello公开Scene执行组合，不能拿旧80.98秒直接除以本表的进程时间。

### 同输入混合进程：三次观测中位数

| 后端 | 实现 | 构造+首次纯色 ms | 混合进程 wall s | 进程CPU s | 峰值RSS MiB |
|---|---|---:|---:|---:|---:|
| dx12 | jadix | 363.32 | 8.4388 | 8.1875 | 425.38 |
| dx12 | vello | 4488.78 | 5.2948 | 4.9062 | 647.78 |
| vulkan | jadix | 33.49 | 0.6376 | 0.4844 | 329.60 |
| vulkan | vello | 91.23 | 0.7126 | 0.4531 | 580.77 |
| gles | jadix | 48.05 | 0.6770 | 0.5469 | 86.87 |
| gles | vello | 357.35 | 1.0859 | 0.3906 | 262.32 |

### 同阶段 render+GPU wait：首次与热帧中位数

| 后端 | 实现 | 阶段 | 同设备renderer序号 | 首次 ms | 其余帧中位 ms | CPU准备中位 ms |
|---|---|---|---:|---:|---:|---:|
| dx12 | jadix | solid | 0 | 327.8916 | 0.4824 | 0.0083 |
| dx12 | jadix | solid | 1 | 20.9561 | 0.6406 | 0.0052 |
| dx12 | jadix | bilinear | 0 | 145.1856 | 0.8357 | 0.1408 |
| dx12 | jadix | bilinear | 1 | 2.5537 | 0.8794 | 0.1408 |
| dx12 | jadix | high | 0 | 6433.9769 | 0.8603 | 0.1339 |
| dx12 | jadix | high | 1 | 3.7621 | 0.9495 | 0.1367 |
| dx12 | jadix | npot-lifecycle | 0 | 609.7165 | 0.8493 | 0.1555 |
| dx12 | jadix | npot-lifecycle | 1 | 3.0635 | 0.9336 | 0.1745 |
| dx12 | jadix | compound | 0 | 3.2118 | 3.0328 | 6.3075 |
| dx12 | jadix | compound | 1 | 3.2640 | 2.7611 | 6.5256 |
| dx12 | jadix | oblique | 0 | 2.3039 | 1.8706 | 0.6583 |
| dx12 | jadix | oblique | 1 | 2.3945 | 1.8324 | 0.7371 |
| dx12 | vello | solid | 0 | 44.5956 | 1.2522 | 0.0021 |
| dx12 | vello | solid | 1 | 37.3668 | 1.4640 | 0.0033 |
| dx12 | vello | bilinear | 0 | 2.0585 | 1.2807 | 0.1221 |
| dx12 | vello | bilinear | 1 | 1.8657 | 1.1282 | 0.1252 |
| dx12 | vello | high | 0 | 1.1339 | 1.1575 | 0.1093 |
| dx12 | vello | high | 1 | 1.1535 | 1.1834 | 0.1119 |
| dx12 | vello | npot-lifecycle | 0 | 1.2052 | 1.1529 | 0.1098 |
| dx12 | vello | npot-lifecycle | 1 | 1.0922 | 1.1773 | 0.1183 |
| dx12 | vello | compound | 0 | 1.8475 | 1.5795 | 0.4516 |
| dx12 | vello | compound | 1 | 1.9883 | 1.6284 | 0.4580 |
| dx12 | vello | oblique | 0 | 1.8677 | 1.1483 | 0.1283 |
| dx12 | vello | oblique | 1 | 2.0929 | 1.1397 | 0.1267 |
| vulkan | jadix | solid | 0 | 18.4696 | 0.2709 | 0.0050 |
| vulkan | jadix | solid | 1 | 17.4151 | 0.3155 | 0.0049 |
| vulkan | jadix | bilinear | 0 | 1.5512 | 0.4268 | 0.1247 |
| vulkan | jadix | bilinear | 1 | 1.3913 | 0.3833 | 0.1250 |
| vulkan | jadix | high | 0 | 1.6363 | 0.3349 | 0.1089 |
| vulkan | jadix | high | 1 | 1.5235 | 0.3286 | 0.1172 |
| vulkan | jadix | npot-lifecycle | 0 | 1.8293 | 0.4357 | 0.1547 |
| vulkan | jadix | npot-lifecycle | 1 | 1.5841 | 0.3883 | 0.1431 |
| vulkan | jadix | compound | 0 | 2.8621 | 2.6345 | 6.0977 |
| vulkan | jadix | compound | 1 | 3.0292 | 2.5739 | 6.0497 |
| vulkan | jadix | oblique | 0 | 1.4192 | 1.2408 | 0.6466 |
| vulkan | jadix | oblique | 1 | 1.2584 | 1.2382 | 0.6240 |
| vulkan | vello | solid | 0 | 67.9695 | 0.6676 | 0.0023 |
| vulkan | vello | solid | 1 | 44.4049 | 0.6018 | 0.0017 |
| vulkan | vello | bilinear | 0 | 0.9492 | 0.7456 | 0.1248 |
| vulkan | vello | bilinear | 1 | 0.8565 | 0.6098 | 0.1374 |
| vulkan | vello | high | 0 | 0.6927 | 0.6639 | 0.1116 |
| vulkan | vello | high | 1 | 0.6289 | 0.5680 | 0.1197 |
| vulkan | vello | npot-lifecycle | 0 | 0.6090 | 0.6355 | 0.1133 |
| vulkan | vello | npot-lifecycle | 1 | 0.5767 | 0.5479 | 0.1146 |
| vulkan | vello | compound | 0 | 1.2246 | 0.9673 | 0.4737 |
| vulkan | vello | compound | 1 | 0.9843 | 1.0902 | 0.4883 |
| vulkan | vello | oblique | 0 | 1.0498 | 0.5562 | 0.1342 |
| vulkan | vello | oblique | 1 | 1.1153 | 0.5893 | 0.1356 |
| gles | jadix | solid | 0 | 29.4276 | 0.8665 | 0.0056 |
| gles | jadix | solid | 1 | 23.8150 | 0.7929 | 0.0054 |
| gles | jadix | bilinear | 0 | 5.3308 | 1.4207 | 0.1464 |
| gles | jadix | bilinear | 1 | 5.3959 | 1.2587 | 0.1290 |
| gles | jadix | high | 0 | 6.8660 | 1.3044 | 0.1345 |
| gles | jadix | high | 1 | 6.7317 | 1.2755 | 0.1369 |
| gles | jadix | npot-lifecycle | 0 | 6.9884 | 1.4319 | 0.1789 |
| gles | jadix | npot-lifecycle | 1 | 6.2543 | 1.4361 | 0.1800 |
| gles | jadix | compound | 0 | 4.4810 | 3.8622 | 6.7681 |
| gles | jadix | compound | 1 | 3.7274 | 3.6675 | 6.6603 |
| gles | jadix | oblique | 0 | 2.8597 | 2.4990 | 0.7494 |
| gles | jadix | oblique | 1 | 2.6752 | 2.5335 | 0.7313 |
| gles | vello | solid | 0 | 298.2539 | 0.9900 | 0.0023 |
| gles | vello | solid | 1 | 268.6856 | 0.9323 | 0.0022 |
| gles | vello | bilinear | 0 | 2.1153 | 1.0108 | 0.1368 |
| gles | vello | bilinear | 1 | 1.9026 | 1.0328 | 0.1437 |
| gles | vello | high | 0 | 1.0189 | 1.0396 | 0.1205 |
| gles | vello | high | 1 | 1.1280 | 1.0444 | 0.1434 |
| gles | vello | npot-lifecycle | 0 | 1.7057 | 0.9802 | 0.1275 |
| gles | vello | npot-lifecycle | 1 | 1.0863 | 1.0478 | 0.1371 |
| gles | vello | compound | 0 | 2.2861 | 1.5616 | 0.4654 |
| gles | vello | compound | 1 | 2.1190 | 1.6056 | 0.2627 |
| gles | vello | oblique | 0 | 1.5389 | 1.0031 | 0.1431 |
| gles | vello | oblique | 1 | 1.8004 | 1.0591 | 0.1547 |

### 斜向图案：独立线性box参考距离

| 后端 | 实现 | linear RGB MAE (0..255) | RMSE | max |
|---|---|---:|---:|---:|
| dx12 | jadix | 1.20702 | 3.80242 | 27.41897 |
| dx12 | vello | 26.31789 | 44.00506 | 126.02994 |
| vulkan | jadix | 1.20702 | 3.80242 | 27.41897 |
| vulkan | vello | 27.25355 | 51.07860 | 143.41211 |
| gles | jadix | 1.20702 | 3.80242 | 27.41897 |
| gles | vello | 27.25355 | 51.07860 | 143.41211 |

![当前Vello High、Jadix EWA和独立参考](../crates/jadix_2d/evidence/round23/matched-oblique.png)

此参考对源texel单元与affine像素footprint作f64裁切积分，检查面积守恒；这里只评价距离指定linear box参考的差距。Gaussian EWA/encoded bicubic不是相同滤镜；没有从误差推导全API画质胜出。

[完整对照结果](../crates/jadix_2d/evidence/round23/matched-summary.json)、[18个独立进程观测](../crates/jadix_2d/evidence/round23/matched-runs.json)、[输入/二进制/来源哈希](../crates/jadix_2d/evidence/round23/matched-manifest.json)。

`Scene2D::reset/reset_and_resize/rerecord` 补充mutable handle的用途对应，并保持已冻结克隆/父场景快照；`Engine::import_buffer/import_image` 接入同设备外部资源，执行前检查实际usage。EWA专用shader不包含Cubic/Lanczos/anisotropic分支，遇到两类高阶采样共存时按能力位集合升级完整shader；旧采样数学和kind=8不变。

[round23完整合同、未解决差异与实测限制](2d-round23.md)、[最终结构化验收](../crates/jadix_2d/evidence/round23/summary.json)。

## round24：DX12 编译回归定位、命令包与矩形采样

三个新DX12进程在修改前首次High均超过6秒。保留原始HLSL直接调用系统FXC确认单个fs_area_pattern约6秒；loop提示、prefer-flow和重复调用合并未改善，均未保留。有效改动为整数寻址改单次unsigned remainder并恢复负数Euclidean域、统一mip bilinear的零级与其他级路径；纯色Area入口直接计算纯色，避免编译不使用的gradient/Gaussian通用路径。没有改变编译器选项或改用低质量滤镜。

`Engine::warm_up` 只预编译有效Recording引用的PSO，不分配命令资源、不上传、不提交；`Engine::record` 返回带command buffer/downloads/stats的`RecordedExecution`，允许调用者在同一提交中插入前后GPU命令。编码成功后资源逻辑状态前进；须按record顺序提交，丢弃命令包后需重置相关资源。原execute仍是record+submit便捷入口，浏览器仍需要awaited error scope。

`Scene2D::sampled_grid`支持每轴1/2/4/8，覆盖总1/2/4/8/16/32/64采样；CPU和GPU保持每样本clip/composite后再resolve，Gaussian、offset、morphology、convolution按XY各自缩放。2x4和4x2为实际8样本用途，不伪装为Vello硬件MSAA8或fine compute ABI。

### 当前同场景进程观测：每实现/后端三个新进程

| 后端 | 实现 | 构造+首次纯色 ms | 进程wall s | 进程CPU s | 峰值RSS MiB |
|---|---|---:|---:|---:|---:|
| dx12 | jadix | 113.44 | 3.2305 | 3.0312 | 413.35 |
| dx12 | vello | 4924.59 | 5.8022 | 5.4844 | 647.77 |
| vulkan | jadix | 35.34 | 0.7131 | 0.5469 | 329.85 |
| vulkan | vello | 89.59 | 0.7310 | 0.4219 | 580.74 |
| gles | jadix | 45.57 | 0.6338 | 0.4688 | 87.95 |
| gles | vello | 355.84 | 1.0181 | 0.3906 | 261.95 |

### 当前阶段延迟：与进程wall分别保留

| 后端 | 实现 | 阶段 | renderer序号 | 首次 ms | 热帧中位 ms | CPU准备中位 ms |
|---|---|---|---:|---:|---:|---:|
| dx12 | jadix | solid | 0 | 76.3598 | 0.5356 | 0.0050 |
| dx12 | jadix | solid | 1 | 18.9140 | 0.6435 | 0.0052 |
| dx12 | jadix | bilinear | 0 | 126.4325 | 0.8287 | 0.1298 |
| dx12 | jadix | bilinear | 1 | 2.4016 | 0.8304 | 0.1305 |
| dx12 | jadix | high | 0 | 1687.5931 | 0.7420 | 0.1358 |
| dx12 | jadix | high | 1 | 2.9445 | 0.8194 | 0.1266 |
| dx12 | jadix | npot-lifecycle | 0 | 361.4621 | 0.8493 | 0.1710 |
| dx12 | jadix | npot-lifecycle | 1 | 2.7134 | 0.8897 | 0.1628 |
| dx12 | jadix | compound | 0 | 3.2113 | 2.9018 | 6.3620 |
| dx12 | jadix | compound | 1 | 2.8856 | 2.5349 | 6.1227 |
| dx12 | jadix | oblique | 0 | 2.2595 | 1.6819 | 0.6658 |
| dx12 | jadix | oblique | 1 | 1.9554 | 1.6649 | 0.6209 |
| dx12 | vello | solid | 0 | 36.2617 | 1.6241 | 0.0033 |
| dx12 | vello | solid | 1 | 44.4194 | 1.4521 | 0.0038 |
| dx12 | vello | bilinear | 0 | 1.8173 | 1.5379 | 0.1612 |
| dx12 | vello | bilinear | 1 | 1.8266 | 1.4439 | 0.1450 |
| dx12 | vello | high | 0 | 1.6692 | 1.5775 | 0.1901 |
| dx12 | vello | high | 1 | 1.7430 | 1.8310 | 0.1936 |
| dx12 | vello | npot-lifecycle | 0 | 1.6000 | 1.3159 | 0.1360 |
| dx12 | vello | npot-lifecycle | 1 | 1.3087 | 1.5074 | 0.1538 |
| dx12 | vello | compound | 0 | 2.0450 | 2.0138 | 0.4385 |
| dx12 | vello | compound | 1 | 1.9612 | 2.4500 | 0.5482 |
| dx12 | vello | oblique | 0 | 1.5961 | 1.1543 | 0.1392 |
| dx12 | vello | oblique | 1 | 2.9056 | 1.8982 | 0.1874 |
| vulkan | jadix | solid | 0 | 19.9609 | 0.3160 | 0.0057 |
| vulkan | jadix | solid | 1 | 18.4257 | 0.3121 | 0.0056 |
| vulkan | jadix | bilinear | 0 | 1.6399 | 0.4420 | 0.1387 |
| vulkan | jadix | bilinear | 1 | 1.7315 | 0.3866 | 0.1460 |
| vulkan | jadix | high | 0 | 1.7488 | 0.3408 | 0.1211 |
| vulkan | jadix | high | 1 | 1.7059 | 0.3983 | 0.1375 |
| vulkan | jadix | npot-lifecycle | 0 | 1.8421 | 0.4789 | 0.1748 |
| vulkan | jadix | npot-lifecycle | 1 | 1.5695 | 0.3933 | 0.1590 |
| vulkan | jadix | compound | 0 | 2.8960 | 2.6840 | 6.9834 |
| vulkan | jadix | compound | 1 | 3.2637 | 2.6059 | 6.9616 |
| vulkan | jadix | oblique | 0 | 1.5842 | 1.3818 | 0.7164 |
| vulkan | jadix | oblique | 1 | 1.6650 | 1.3841 | 0.7437 |
| vulkan | vello | solid | 0 | 65.4832 | 0.6062 | 0.0022 |
| vulkan | vello | solid | 1 | 40.8940 | 0.5996 | 0.0020 |
| vulkan | vello | bilinear | 0 | 1.2082 | 0.7602 | 0.1419 |
| vulkan | vello | bilinear | 1 | 0.9493 | 0.6041 | 0.1427 |
| vulkan | vello | high | 0 | 0.7416 | 0.7401 | 0.1210 |
| vulkan | vello | high | 1 | 0.6282 | 0.5539 | 0.1246 |
| vulkan | vello | npot-lifecycle | 0 | 0.7506 | 0.6020 | 0.1305 |
| vulkan | vello | npot-lifecycle | 1 | 0.5490 | 0.6037 | 0.1206 |
| vulkan | vello | compound | 0 | 1.2853 | 0.9230 | 0.4573 |
| vulkan | vello | compound | 1 | 1.1633 | 0.9761 | 0.4903 |
| vulkan | vello | oblique | 0 | 0.8678 | 0.6236 | 0.1391 |
| vulkan | vello | oblique | 1 | 0.7937 | 0.5939 | 0.1486 |
| gles | jadix | solid | 0 | 26.8125 | 0.8365 | 0.0059 |
| gles | jadix | solid | 1 | 21.1250 | 0.7658 | 0.0051 |
| gles | jadix | bilinear | 0 | 5.3016 | 1.2474 | 0.1407 |
| gles | jadix | bilinear | 1 | 5.2673 | 1.0918 | 0.1343 |
| gles | jadix | high | 0 | 6.3410 | 1.2269 | 0.1309 |
| gles | jadix | high | 1 | 6.3902 | 1.1948 | 0.1332 |
| gles | jadix | npot-lifecycle | 0 | 5.8583 | 1.3355 | 0.1638 |
| gles | jadix | npot-lifecycle | 1 | 5.9030 | 1.2125 | 0.1595 |
| gles | jadix | compound | 0 | 4.0442 | 3.2307 | 6.5157 |
| gles | jadix | compound | 1 | 3.4793 | 3.1922 | 6.2028 |
| gles | jadix | oblique | 0 | 2.4997 | 2.2466 | 0.6767 |
| gles | jadix | oblique | 1 | 2.3277 | 2.2063 | 0.6509 |
| gles | vello | solid | 0 | 293.4104 | 1.1823 | 0.0030 |
| gles | vello | solid | 1 | 256.6411 | 0.9475 | 0.0021 |
| gles | vello | bilinear | 0 | 2.0555 | 1.0956 | 0.1398 |
| gles | vello | bilinear | 1 | 1.2517 | 0.9287 | 0.1272 |
| gles | vello | high | 0 | 1.3258 | 1.0931 | 0.1410 |
| gles | vello | high | 1 | 1.1673 | 1.0014 | 0.1371 |
| gles | vello | npot-lifecycle | 0 | 1.1866 | 1.0800 | 0.1399 |
| gles | vello | npot-lifecycle | 1 | 1.4049 | 0.9762 | 0.1179 |
| gles | vello | compound | 0 | 2.6696 | 1.6720 | 0.2383 |
| gles | vello | compound | 1 | 2.1454 | 1.5240 | 0.2092 |
| gles | vello | oblique | 0 | 1.9410 | 1.0068 | 0.1366 |
| gles | vello | oblique | 1 | 1.2977 | 0.9601 | 0.1268 |

三后端同原始输入、Area、单初始化线程、每设备两个renderer、交替三个进程；High滤镜仍分别为Vello High与Jadix EWA，不是严格同算法吞吐排名。缓存未清，不将三个样本外推所有设备。GPU-only HDR/mip/sampled-scene套件另外记录，不与公共六场景混算。

[当前完整原始阶段与资源数据](../crates/jadix_2d/evidence/round24/matched-summary.json)、[输入/二进制/源码身份](../crates/jadix_2d/evidence/round24/matched-manifest.json)。

本轮108张Jadix公共场景PNG及480张历史PNG逐字节不变。未修改的Vello二进制有12张DX12/Vulkan compound输出跨进程/轮次出现最大8级通道差，原因未定位，不计作Jadix回归，也不用于精确像素等价结论。单像素/条带目标现在支持五种AA和采样输出；共72个尺寸/alpha/layer/HDR组合通过。

[round24实现、回归与性能合同](2d-round24.md)、[最终验收](../crates/jadix_2d/evidence/round24/summary.json)。

## 可复现性与一手来源

`python benchmarks/vello-api/scan.py` 从原始 rustdoc 枚举；`python benchmarks/vello-api/report.py` 生成本文，未分类 Vello 自有项会使脚本失败。当前源码与 JSON 的 SHA-256 保留于 evidence/round24 和 inventory（round20 数据作为明确标注的历史参考）；历史画质/性能表仍明确链接 round17。不同 target/features 需要重生成，不能将当前清单外推未来版本。

- [Vello 0.11.0 官方文档](https://docs.rs/vello/0.11.0/vello/)
- [Scene](https://docs.rs/vello/0.11.0/vello/struct.Scene.html)
- [Renderer](https://docs.rs/vello/0.11.0/vello/struct.Renderer.html)
- [DrawGlyphs](https://docs.rs/vello/0.11.0/vello/struct.DrawGlyphs.html)
- [Color 0.3.3](https://docs.rs/color/0.3.3/color/)
- [Moxcms 0.8.1](https://docs.rs/moxcms/0.8.1/moxcms/)
- [CSS Color 4 gamut mapping](https://www.w3.org/TR/css-color-4/#css-gamut-mapping)
