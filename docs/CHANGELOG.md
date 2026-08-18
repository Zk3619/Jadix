# Changelog（草案）

> **地位**：SemVer / 实验面草稿（WP-2.5 任务③）。正式发布前以 [`AUTHORITY.md`](AUTHORITY.md) WP-4.1 为准重写。  
> **日期**：2026-08-13

## Unreleased

### Experimental（未进 SemVer 承诺面）

| API | 路径 | 说明 |
|-----|------|------|
| `create_computed` | `core/reactivity.rs` | **WP-R.9 ☑**：Computed 相；先于 `bind_*` / 用户 `create_effect`；非 `RenderEffect` 类型 |
| `reactivity_debug_snapshot` / `dump_reactivity_graph` / `ReactivityDebugSnapshot` | `core/reactivity.rs` | **WP-R.8 ☑**：运行时信号图计数 + 边列表诊断钩子；非 DevTools UI |
| `Trigger` / `create_trigger` | `core/reactivity.rs` | 零载荷通知原语（Solid `createTrigger`）；每次 `notify()` 无条件触发订阅者 |
| `start_transition` | `core/reactivity.rs` | 帧级延迟批（Solid `startTransition` 类比）；复用 deferred-fn 队列 |
| `Store` / `create_store` / `Store::field` / `store_field` / `store_select` / `Store::produce` | `core/store.rs` | **WP-R.3 ☑**：`field.set` 兄变不重跑弟 compute；`store_select`+`produce` 仍为整根过渡 |
| `create_resource` / `Resource` / `suspense` | `app/resource.rs` | 桌面语义；无 SSR |
| `error_boundary` / `throw_error` / `throw_resource_error` | `app/error_boundary.rs` | Resource Err→EB 须**显式** `throw_resource_error`（巩固期不自动桥） |
| `define_chart` / marks / `ChartTheme` / `custom_mark`；polar 族；`scale_log`/`scale_time`；`.tooltip`/`.crosshair`；预设 `pie_chart` / `area_chart`；`ChartDatum.time`；path 核光栅贴图 | `components/chart/` · `primitives/path.rs` | WP-3.2 grammar（对照 TanStack，不搬 D3）；RSX `<Chart>`/`BarChart`…；Donut=`inner_radius`；禁区≠可加深见 `drawing.md` §8；未进 SemVer |
| `tray_icon` / `TrayIcon` / `request_exit` / `system_tray_native` | `app/tray.rs` · `ui_bridge.rs` | WP-3.6；默认 stub；`system-tray` 才有原生副作用 |
| `virtual_masonry` / `VirtualMasonryBuilder` | `components/virtual_masonry.rs` · `utils/virtual_masonry.rs` | WP-3.10；变高瀑布流虚拟化；`item_height` + 测宽缓存；未进 SemVer |

### Added

- WP-3.6：可选 feature `system-tray` 接入 `tray-icon`；菜单点击经 `run_on_ui_thread`；[`request_exit`](../crates/rui/src/app/ui_bridge.rs) 退出事件循环；默认仍 stub。手册 [`tray.md`](tray.md)。
- WP-3.8：稳定面 [`component_variants.md`](component_variants.md)（Button 词汇 + 差异表 + 受控命名 + 双书写审计）；未知 variant/size → default；Input RSX `value`；Toast success/warning 走 `chart-*` token。
- WP-3.9：Form helpers 校验路径（仍 `experimental`）：`FormState::set_field_error` / `clear_*` / `validate`；`form_field` 错误文案与 Input invalid 环同源；Gallery「表单」卡。点击回调须 clone `FormState`，勿在事件里 `use_form()`。
- WP-3.10：VirtualMasonry（仍 `experimental`）：最短列优先装箱 + 视口虚拟化；行高 `item_height(index, col_w)` / `height_from_cache`；Gallery「数据」千级卡。

- Command：三种表面——行内 `<Command>`、VS Code 式顶部居中 `<CommandOverlay>`、居中模态 `<CommandDialog>`（复用 Dialog，不改 `dialog.rs`）。Builder `.open_signal` 默认浮层，`.dialog()` 走 Dialog。Overlay/Dialog 宿主为 `0×0` portal 锚点（打开后 reconcile 仍保持 0×0，避免在 HStack 里撑破布局）；TooltipContent 同样用 `0×0` 锚点。

- WP-3.2：Chart **grammar**（`define_chart` + cartesian/polar marks + `ChartTheme`/`MarkStyle`）；`scale_log`/`scale_time`（Rust 专用，非 JS D3）；`.tooltip(true)` / `.crosshair(true)`（钉在 pointer-down）；预设薄封装；path 核光栅；RSX `<Chart>`/`<BarChart>`…；`ChartDatum.series`/`time`。Gallery「数据」含组合图、时间轴、log、radial。
- Chart 修：path 贴图竖直翻转对齐 TexturedQuad（面积填下方、柱从基线向上）；RSX `h` 只约束绘图区；饼/环近心缝 + 选中外扩描边；`selected_signal` + tooltip overlay 可响应。
- Chart 点/命中：dot 改为 path 圆画在折线之上（中心对齐顶点）；折线/面积不再按 datum[0] 整组变淡；`HitPathEntry.bounds` 用可视盒（ScrollArea 内饼/环/tooltip 可点）；radial_line 命中加距离上限，不再抢走 radial_bar。
- Chart 交互：选中点仅 +1px；再点同一 datum / 点空白取消选中；tooltip 随选中编进同一 compile 树（复用 `tooltip_bubble`，百分比定位，不再把底画进被拉伸的 path 贴图）。靠近右/上缘时左右/上下翻转并夹 `max_w`，避免 `overflow: hidden` 裁切。预设 Line/Area/Bar/Pie 默认 `.tooltip(true)`。
- Chart 点：viewBox→叶宽非等比时，闭合正圆按 `min(sx,sy)` 保半径，避免折线/面积选中点被压成椭圆。
- WP-3.3：独立 [`Calendar`]（`caption_layout` label/dropdown，ISO `YYYY-MM-DD`，方向键）；Date/TimePicker 复用并补 Enter/Esc；ColorPicker HSV+色相/透明度+hex/rgb/hsl（对照 Reader）；Command ↑↓ 高亮 + Enter；RSX `captionLayout` / `valueSignal` / `showAlpha` 宏别名。
- Calendar / DatePicker `caption_layout=dropdown` 与 `kind=month` 年月选择复用 [`Select`]（Aria Month/Year picker）；Date/Time/Color 浮层改为 `floating_portal_anchor` + `scroll:` 视口视觉锚点，滚动不再错位。
- 浮层嵌套：Popover 内 Menu（DatePicker caption Select）可叠层；关闭 Popover 一并关掉其上 Select。`max_h` 随视口可回升；DatePicker 浮层 `w-fit`（`min_w=0`）+ Calendar `min_w=17.5rem`。
- Popover / Date·Time·Color `strategy=fixed`：钉在触发器下方，滚动不挤压高度。Select `scroll_selected`（`none`/`start`/`center`，默认 none）；打开后用布局真实 `content_h` 把当前项滚到视口中线（不再在菜单 effect 里订滚动信号，避免整树重建/悬停闪烁）。Calendar 六周等分列宽；非本月日 muted 可点并切月。
- TimePicker：三个 Combobox 选时分秒（限 0–23 / 0–59）；`datetime-local` 日历下 `Separator` + 时间行。行内 Calendar 下拉可重复打开。DateTime：日历与 `Separator` 间距 `pt 1rem`；HMS→val / val→HMS 分向同步；浮层用 `isolate_reads`（勿 `untrack`，否则 Calendar/Combobox 无法注册 anchor，浮层空壳）。Combobox/Select **实例根** `with_child_owner`（Trigger+下拉同属该实例 owner；`#[component]` 不会自动开 scope）。Combobox 打开时 `active`/滚动对齐当前 value（如 `"09"` → index 9）。Calendar 非本月日 `muted-foreground`；日网格占位固定 `17rem` + `overflow:hidden`。
- Chart 交互按实例隔离：未传 `selected_signal` 时每图自带选中/pin 信号（不再共用 `chart_pin`）；饼/环选中改为 TanStack `radialArc` 式外半径加大 + 同色描边，不平移扇区。
- B2：`merge_props` / `merge_props!` / `split_props!` / `children()` / `resolve_children`（Rust 语义：后源 `Some` 覆盖、fragment 展平；非 JS Proxy）。`#[component]` `*Props` impl `MergeProps`。
- WP-R.7：`ui!` 宏降级漏绑回归（`isolate_reads` / `#[component(reactive)]`）；`<For each={…get()} key>`（含 `fallback`）走 keyed For；`<Match key>` / `<MatchArm>`/`<When>` RSX；岛内叶子与未列 attr 亦可 `bind_*`；`with()` 视为 tracked read（`peek()` 故意不订）；`Element::text(sig)` / `|| String` 经 `IntoNodeText` 自动 bind。gallery 扩 `ReactiveCountChip`。
- WP-R.9：`flush_pending` 分相 Computed → `bind_*` → 用户 `create_effect`（I-12）；可选 [`create_computed`](../crates/rui/src/core/reactivity.rs)。同帧、顺序固定，无 SSR tick。
- WP-R.3：`Store::field` / `store_field` 路径触发器；兄字段 `set` 不重跑弟 compute（I-8）。`store_select`+`produce` 仍为整根过渡。
- WP-2.5：`throw_resource_error`；Resource `from_signal` / `set_error`；`produce` 切片契约测；Resource Err→EB e2e 测。
- WP-2.9：Builder `.border_box()`；utility `box-border` / `box-content`；命中/裁剪库测；测宽叶审计见 `docs/box_model.md` 附录 A。
- 盒模型单位：布局期真 `vw`/`vh`/`dvh`、局部 `em`、`calc(A ± B)`（含 `calc(100% - Npx)` 相对父 content box）；逻辑 `*-inline`/`*-block` 双值；`row-gap`/`column-gap`。
- B15 分边 border：`border_left` / `border_top_width` / `borderWidth="4px 8px"`（1–4 值）；Taffy 分边；均匀 SDF、非均匀四边条绘制。
- 窗口缩放 fast 布局会重烘焙 `vw`/`vh`/`em`/`calc`，视口单位随窗口实时变化。

### Notes

- 稳定面控件 API 仍见 inventory / COMPONENT_LIBRARY；本文件只标反应性异步与 Store 实验边界。
- 盒模型默认仍为 BorderBox；禁止静默改默认。见 [`box_model.md`](box_model.md)。
- **W2 退出（2026-08-13）** → Wave 3。公开自绘面（`<svg>` / `<canvas>` / 2D·3D）与未来创意库见 [`drawing.md`](drawing.md)：B16 canvas-ui · B17 react-bits；执行 WP-3.11–3.13，尚未进 SemVer。
