# Glyphon / 文本管线核对

> 从属巩固期文本轨道（WP-2.1 **已冻结**，见 [text_align.md](text_align.md)）。早期选型随笔已归档：[archive/test.md](../archive/test.md)（**非**测试计划）。中期结论：**坚持 glyphon**；长期关注 parley/swash/vello。**换栈须 ADR，不得静默破坏 `text_align_*`。**

基于当前实现 [crates/rui/src/renderer/text.rs](../../crates/rui/src/renderer/text.rs) 与变换文字三模式的核对结论：

| 建议项 | 现状 |
|--------|------|
| `FontSystem` / `TextAtlas` 归属 | 由 `RuiTextRenderer` 持有，随 `AppState::new` 创建并复用，非每帧 `new` |
| 文本脏检查 | 布局路径在 `layout_tree` / `prepare_text` 侧按内容、样式与几何变化更新缓冲；高帧率场景继续优先 `peek`/`set_silent` |
| **变换文字三模式** | [`GlyphRasterMode`](../../crates/rui/src/renderer/text_xform.rs)：`Coverage`（轴对齐 UI，上游 glyphon + Nearest）· `Bake`（sticky 超采样 RGBA + `TexturedDraw` mat2）· `Msdf`（覆盖率→三通道 MSDF + FS `median`/`smoothstep`，自由缩放） |
| 批处理 | Coverage 走上游 glyphon（无 `TextArea.transform`）；Bake/Msdf 走 `TexturedDraw`（与壳同 `world_linear`）；rich spans 走 `ensure_text_xform_texture_spans` |
| DPI / scale | `AppState::ui_scale` 与 `scale_instances`、surface 尺寸联动；换屏或 `ScaleFactorChanged` 会标记布局脏 |
| **3D 文字** | `bake_text_texture_key_for_3d` + `scene3d().label(..)`：[`project_billboard_warp`](../../crates/rui/src/renderer/text_xform.rs) 投四角 + clip `w` → textured VS 透视校正 UV；见 Gallery Scene3d |
| Vello | [`reference/vello-main`](../../reference/vello-main) 仅对照矢量/Canvas；**不**替换 glyphon。窄矢量字形见 `utils/text_vector_spike.rs`（后置 ADR） |

### Bake vs Msdf vs Vector vs Vello

```
shaping (cosmic)
 ├─ Coverage — idle UI, Nearest atlas
 ├─ Bake     — sticky transform / transition / spin (0° included)
 ├─ Msdf     — scale outside ~0.5–2 or 3D label (3-channel MSDF atlas)
 ├─ Vector   — spike only (outline → PathCommand → RUI_GPU_2D)
 └─ Vello    — reference; Canvas/SVG layer later, never UI text backend
```

## Bake RT 体积与淘汰（2026-09-10）

[`text_texture.rs`](../../crates/rui/src/utils/text_texture.rs) CPU 缓存不再按 HashMap 任意首键丢条目、也不再把整库冷图回填 GPU：

| 项 | 契约 |
|----|------|
| CPU 帽 | 128 条 **且** 16 MiB RGBA；超帽 LRU 丢**非本帧** key |
| 本帧钉住 | `ensure_*` 只 `touch`（不 clone 位图）；[`flush_text_xform_textures`](../../crates/rui/src/utils/text_texture.rs) 只回填本帧 needed 且 GPU 已淘汰的 key |
| GPU | 仍走 [`TexturePool`](../../crates/rui/src/renderer/resources.rs) LRU；`ResourceKind::TextXform` |
| 诊断 | `RUI_TEXTURE_STATS=1` 行含 `xform_cpu=n/bytes` |
| 测 | `cargo test -p rui --lib text_xform_cpu_cache` |

## 三通道 MSDF（2026-09-10）

覆盖率图 → 轮廓边按梯度角 120° 分箱，每条边着 **两个** 通道（Chlumsky 边对；直边 `median(d,d,far)=d`，尖角丢掉绕远的那路）。打包进 `rui_text_msdf_v2:` RGB；FS 仍 `median3`。无边时退回单通道 SDF（R=G=B）。**不是** glyphon `ContentType::Mask` 图集（仍是每叶一张 bake）。

测：`cargo test -p rui --lib text_texture_msdf_channels_diverge`

## Billboard 透视翘曲（2026-09-10）

`TexturedInstanceInput.flags` 3/4 = RGBA/MSDF warp：四角像素 + clip `w`，VS 写 `clip = (ndc * w, w)` 让 GPU 做透视校正 UV。不再把投影四边形压成屏幕 AABB。

测：`cargo test -p rui --lib billboard_warp_perspective_tapers`

## RGBA 四通道？

**能，但是 MTSDF，不是第四条 MSDF 色。** Chlumsky 的四通道格式是 **RGB = 三通道 MSDF，A = 常规真 SDF**（描边 / 光晕 / 小字回退）。当前打包 `rui_text_msdf_v2:` 的 A 固定 255，FS 只用 `median3(RGB)`。不要把 A 当成第四个 MSDF 分箱——会破坏 median 与预乘混合。需要描边或小字 SDF 回退时再 bump 前缀落地；不挡 Beta。

## Gallery

- **文本**：`MsdfMultichannelShowcase` — Coverage / Bake / `scale-[3]` 三通道 MSDF（「永角 Ag」尖角）。
- **绘制**：`DrawingBillboardShowcase` — 默认 pitch vs 大俯仰；label 四角 + clip `w`。需 `cargo run -p gallery --features scene3d`。
- 布局「rotate + translate」3b 口是「三通道 MSDF」。

## 何时做：glyphon 共享 atlas / scene3d depth

两项都不挡 W4 / 0.1。触发条件：

| 项 | 现在 | 开工条件 | 挂哪 |
|----|------|----------|------|
| MSDF 字种打进 glyphon 共享 atlas | 每叶一张 `rui_text_msdf_v2:` bake，不是 `ContentType` | 独立 MSDF 叶打满 TexturePool，或真要字种级 MSDF（与 Coverage 分 pipeline、cache key） | 新 ADR；glyphon 管线加深。Beta 后 |
| label 写入 scene3d depth | RT 上的 2D 合成 pass，无 depth attachment | 多 mesh / 相机绕到字后面，需要与立方体正确遮挡 | WP-3.13 加深或 B16 3D 件产品化时 |

共享 atlas 的计划勾选（`msdf-gen-atlas`）指的是**每叶 bake**，不是 glyphon `ContentType`。开 ADR 前不要改上游 atlas。
