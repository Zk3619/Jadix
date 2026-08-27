# Glyphon / 文本管线核对

> 从属巩固期文本轨道（WP-2.1）。早期选型随笔已归档：[archive/test.md](archive/test.md)（**非**测试计划）。中期结论：**坚持 glyphon**；长期关注 parley/swash/vello。

基于当前实现 [crates/rui/src/renderer/text.rs](../crates/rui/src/renderer/text.rs) 与变换文字三模式的核对结论：

| 建议项 | 现状 |
|--------|------|
| `FontSystem` / `TextAtlas` 归属 | 由 `RuiTextRenderer` 持有，随 `AppState::new` 创建并复用，非每帧 `new` |
| 文本脏检查 | 布局路径在 `layout_tree` / `prepare_text` 侧按内容、样式与几何变化更新缓冲；高帧率场景继续优先 `peek`/`set_silent` |
| **变换文字三模式** | [`GlyphRasterMode`](../crates/rui/src/renderer/text_xform.rs)：`Coverage`（轴对齐 UI，glyphon + Nearest）· `Bake`（sticky 超采样 RGBA + Linear/mip 贴图）· `Msdf`（覆盖率→单通道 SDF 贴图 + FS `median`/`pxRange`/`smoothstep`，自由缩放） |
| 批处理 | Coverage 走 glyphon；Bake/Msdf 走 `TexturedDraw`（与壳同 `world_linear`）；禁止 Bake 与主 `text_renderer` 共用 prepare |
| DPI / scale | `AppState::ui_scale` 与 `scale_instances`、surface 尺寸联动；换屏或 `ScaleFactorChanged` 会标记布局脏 |
| **3D 文字** | `bake_text_texture_key_for_3d` + `scene3d().label(..)`：mat4 投影 billboard 角点 → 屏幕 AABB → textured mat2 VS；见 Gallery Scene3d |
| Vello | [`reference/vello-main`](../reference/vello-main) 仅对照矢量/Canvas；**不**替换 glyphon。窄矢量字形见 `utils/text_vector_spike.rs`（后置 ADR） |

### Bake vs Msdf vs Vector vs Vello

```
shaping (cosmic)
 ├─ Coverage — idle UI, Nearest atlas
 ├─ Bake     — sticky transform / transition / spin (0° included)
 ├─ Msdf     — scale outside ~0.5–2 or 3D label (SDF atlas, not glyphon ContentType yet)
 ├─ Vector   — spike only (outline → PathCommand → RUI_GPU_2D)
 └─ Vello    — reference; Canvas/SVG layer later, never UI text backend
```

后续优化（非阻塞）：Bake RT 体积与淘汰；真 multi-channel MSDF 图集（当前为单通道 SDF 打包进 RGB，shader 已用 `median3` 兼容）；billboard 非仿射透视翘曲。
