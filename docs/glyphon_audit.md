# Glyphon / 文本管线核对

> 从属巩固期文本轨道（WP-2.1）。早期选型随笔已归档：[archive/test.md](archive/test.md)（**非**测试计划）。中期结论：**坚持 glyphon**；长期关注 parley/swash/vello。

基于当前实现 [crates/rui/src/renderer/text.rs](../crates/rui/src/renderer/text.rs) 的快速核对结论：

| 建议项 | 现状 |
|--------|------|
| `FontSystem` / `TextAtlas` 归属 | 由 `RuiTextRenderer` 持有，随 `AppState::new` 创建并复用，非每帧 `new` |
| 文本脏检查 | 布局路径在 `layout_tree` / `prepare_text` 侧按内容、样式与几何变化更新缓冲；高帧率场景继续优先 `peek`/`set_silent` |
| SDF | 依赖 `glyphon`/`wgpu` 配置；若需强制 SDF，应在 `TextAtlas`/`Render` 初始化参数中显式对齐 `glyphon` 文档 |
| 批处理 | 文本通过 `glyphon` 的 `TextRenderer` 在 render pass 中提交；与矩形实例批处理分离，属预期结构 |
| DPI / scale | `AppState::ui_scale` 与 `scale_instances`、surface 尺寸联动；换屏或 `ScaleFactorChanged` 会标记布局脏 |

后续优化（非阻塞）：监控 Atlas 体积、在极大动态文本场景下评估异步 shaping（见 archive/test.md 所述后台线程方案）。
