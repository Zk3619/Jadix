# Changelog（草案）

> **地位**：SemVer / 实验面草稿（WP-2.5 任务③）。正式发布前以 [`AUTHORITY.md`](AUTHORITY.md) WP-4.1 为准重写。  
> **日期**：2026-08-13

## Unreleased

### Experimental（未进 SemVer 承诺面）

| API | 路径 | 说明 |
|-----|------|------|
| `Store` / `create_store` / `store_select` / `Store::produce` | `core/store.rs` | `produce` ≡ `update`；无 proxy 路径触发器（R-WP-3） |
| `create_resource` / `Resource` / `suspense` | `app/resource.rs` | 桌面语义；无 SSR |
| `error_boundary` / `throw_error` / `throw_resource_error` | `app/error_boundary.rs` | Resource Err→EB 须**显式** `throw_resource_error`（巩固期不自动桥） |
| `define_chart` / marks / `ChartTheme` / `custom_mark`；polar 族；`scale_log`/`scale_time`；`.tooltip`/`.crosshair`；预设 `pie_chart` / `area_chart`；`ChartDatum.time`；path 核光栅贴图 | `components/chart/` · `primitives/path.rs` | WP-3.2 grammar（对照 TanStack，不搬 D3）；RSX `<Chart>`/`BarChart`…；Donut=`inner_radius`；禁区≠可加深见 `drawing.md` §8；未进 SemVer |

### Added

- WP-3.2：Chart **grammar**（`define_chart` + cartesian/polar marks + `ChartTheme`/`MarkStyle`）；`scale_log`/`scale_time`（Rust 专用，非 JS D3）；`.tooltip(true)` / `.crosshair(true)`（钉在 pointer-down）；预设薄封装；path 核光栅；RSX `<Chart>`/`<BarChart>`…；`ChartDatum.series`/`time`。Gallery「数据」含组合图、时间轴、log、radial。
- Chart 修：path 贴图竖直翻转对齐 TexturedQuad（面积填下方、柱从基线向上）；RSX `h` 只约束绘图区；饼/环近心缝 + 选中外扩描边；`selected_signal` + tooltip overlay 可响应。
- Chart 点/命中：dot 改为 path 圆画在折线之上（中心对齐顶点）；折线/面积不再按 datum[0] 整组变淡；`HitPathEntry.bounds` 用可视盒（ScrollArea 内饼/环/tooltip 可点）；radial_line 命中加距离上限，不再抢走 radial_bar。
- Chart 交互：选中点仅 +1px；再点同一 datum / 点空白取消选中；tooltip 随选中编进同一 compile 树（复用 `tooltip_bubble`，百分比定位，不再把底画进被拉伸的 path 贴图）。靠近右/上缘时左右/上下翻转并夹 `max_w`，避免 `overflow: hidden` 裁切。预设 Line/Area/Bar/Pie 默认 `.tooltip(true)`。
- Chart 点：viewBox→叶宽非等比时，闭合正圆按 `min(sx,sy)` 保半径，避免折线/面积选中点被压成椭圆。
- Chart 交互按实例隔离：未传 `selected_signal` 时每图自带选中/pin 信号（不再共用 `chart_pin`）；饼/环选中改为 TanStack `radialArc` 式外半径加大 + 同色描边，不平移扇区。
- WP-2.5：`throw_resource_error`；Resource `from_signal` / `set_error`；`produce` 切片契约测；Resource Err→EB e2e 测。
- WP-2.9：Builder `.border_box()`；utility `box-border` / `box-content`；命中/裁剪库测；测宽叶审计见 `docs/box_model.md` 附录 A。
- 盒模型单位：布局期真 `vw`/`vh`/`dvh`、局部 `em`、`calc(A ± B)`（含 `calc(100% - Npx)` 相对父 content box）；逻辑 `*-inline`/`*-block` 双值；`row-gap`/`column-gap`。
- B15 分边 border：`border_left` / `border_top_width` / `borderWidth="4px 8px"`（1–4 值）；Taffy 分边；均匀 SDF、非均匀四边条绘制。
- 窗口缩放 fast 布局会重烘焙 `vw`/`vh`/`em`/`calc`，视口单位随窗口实时变化。

### Notes

- 稳定面控件 API 仍见 inventory / COMPONENT_LIBRARY；本文件只标反应性异步与 Store 实验边界。
- 盒模型默认仍为 BorderBox；禁止静默改默认。见 [`box_model.md`](box_model.md)。
- **W2 退出（2026-08-13）** → Wave 3。公开自绘面（`<svg>` / `<canvas>` / 2D·3D）与未来创意库见 [`drawing.md`](drawing.md)：B16 canvas-ui · B17 react-bits；执行 WP-3.11–3.13，尚未进 SemVer。
