# Changelog（草案）

> **地位**：SemVer / 实验面草稿（WP-2.5 任务③）。正式发布前以 [`AUTHORITY.md`](AUTHORITY.md) WP-4.1 为准重写。  
> **日期**：2026-08-13

## Unreleased

- **2026-09-07 音视频槽 MVP**：[`engine/media.md`](engine/media.md) 分层落地 WP-M.1/M.2/M.3/M.6。同键 `write_texture`；[`queue_media_frame`](../crates/rui/src/media/stream.rs)（RGBA 默认；`image-stream` 时 NV12/I420/P010 → BT.709）；Gallery「绘制」合成色条（1×1 canvas 时钟已修）。无 FFmpeg、无播放器 crate。
- **2026-09-06 对照**：[`EUI_NEO_VISUAL.md`](comparison/EUI_NEO_VISUAL.md) §9 补 RUI × EUI × GPUI / gpui-component **视觉 / 2.5D / 动画**三角；[`GPUI_ECOSYSTEM_COMPARISON.md`](comparison/GPUI_ECOSYSTEM_COMPARISON.md) §6.5；动效手册附录 §21。不改冻结面、不新开 WP。
- **2026-09-06 平台层文档**：新增 [`platform.md`](engine/platform.md)——`crate::platform` 现状（OS 级 `cfg`、Windows 特化 vs macOS/Linux stub）、散落 OS 逻辑清单与 desktop/mobile/embedded 目标结构；更新 AUTHORITY / ENGINEERING / multi_window / ime / stack 等引用。

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
| `queue_media_frame` / `MediaFrame` / `PixelLayout` / `TextureColorSpace` | `media/stream.rs` | **B18 / WP-M.2–M.3**：动态纹理槽；`image-stream` 开 YUV；不解码、不播放；HDR 后置 |
| `is_remote_src` / `set_remote_asset_fetcher` / `fetch_remote_bytes` / `RemoteAssetFetcher` | `media/remote.rs` | **remote-asset**：远程 `src` 入口；HTTP 由应用注入；默认关 |
| `WindowOptions::devtools` / `rui::app::devtools::*` | `app/devtools/` · `app/runtime.rs` | **B1**：Inspector（Elements + Layout + Reactivity + 样式/CSSOM 编辑）；`devtools` feature |
| `virtual_masonry` / `VirtualMasonryBuilder` | `components/virtual_masonry.rs` · `utils/virtual_masonry.rs` | WP-3.10；变高瀑布流虚拟化；`item_height` + 测宽缓存；未进 SemVer |

- **2026-09-05 三层动画模型**：引擎 `Timing` + spring 积分；声明式 `transition: … spring()`；编程式 `Animation` 句柄（`run` / `seek` / `set_rate` / `on_finish`）。丢掉 DocumentTimeline、Promise、编程面 composite。手册 [`animation_and_motion.md`](engine/animation_and_motion.md) 第二部分 §11.5。
- **2026-09-05 gallery wgpu 管线**：`inst_id` 从 VertexOutput loc 15 打进 loc 14 `extra_and_id`，避开 `max_inter_stage_shader_variables`（0..=14）。
- **2026-09-05 动画扩面收进冻结面**：编程式句柄（`Animation`/`KeyframeEffect`）、`linear()` intern、`clip-path: path()`/`url(#id)`、`offset-path`、`transform-origin` 动画（槽 60）、`extra_shadows` 列表 lerp、两层 `GpuClipPoly` AND。手册 §1.2 / §1.3。
- **2026-09-05 动画域已冻结**：[`animation_and_motion.md`](engine/animation_and_motion.md) 第二部分追上实现；`clip-path` transition 与 keyframes 共用 `ClipPath::interpolate`；独立 `scale`/`rotate` `transition-property`；色 transition 走同一 `lerp_text_rgba`。CI `anim_freeze`。
- **2026-09-05 overflow 圆角 clip**：子级裁剪走 SDF 圆角盒（与填充同一 `sd_round_box`），不再用 16 顶点折线切成八角。Gallery「布局 / 文本」增加 `writing-mode` 五值卡片。
- **2026-09-05 ANIM-6 延伸**：完整 `clip-path`（`polygon` / `circle` / `ellipse` 命中 + SDF even-odd）、具名 `scroll-timeline` / `view-timeline` + `animation-range`、`calc-size()` 求值器（`size` 关键字）。
- **2026-09-05 ANIM-6**：`grid-template-rows/columns` 等长 `fr` 列表为通用 C 层动画；`animation-composition` add/accumulate；`interpolate-size: allow-keywords`；`scroll()`/`view()` timeline；`background-position` / `clip-path: inset()`。
- **2026-09-05 文档收口**：`docs/` 根目录只留权威规范 + changelog；手册进 `docs/components` / `engine` / `ops` / `comparison`；冻结计划与 scratch 进 `docs/archive`。Gallery 产品面走查后 inventory 升 `verified`（SemVer 0.1 名单不自动扩大，见 `COMPONENT_LIBRARY.md` §5.2.1）。
- Timeline 默认圆点 20px、图标按圆点 70% 缩放（约 14px），避免指示过小。
- **2026-09-05 ADR-10 第 1 档**：`Element` 改为 `Box<ElementInner>` 堆句柄（`size_of` = 指针）；TLS 回收卸载的 `ElementInner`（保留树对应 GPUI arena，**不是**每帧 bump）；`stack_budget` 断言句柄契约与 slab 回收。
- **2026-09-05 ADR-10 第 2 档**：每帧 `sync_class_variants_subtree` / `tick_active_css_transitions_subtree` / `class_variant_hover_needs_frame` 改为堆 DFS（LTR `peer` 语义不变）；layout collect/build/cascade 先前已迭代。见 [`stack_and_element_model.md`](engine/stack_and_element_model.md)。
- **2026-09-05 ADR-10 第 3 档**：`with_ui_build_stack` 改为 GPUI `stacksafe` 同构（**64 KiB 红区 / 1 MiB 段**），打在用户闭包边界（`run_effect` / `component_root` / 每条 UI 任务 / file-drop）与 Taffy `compute_layout`；click/key/touch/redraw 不再整段 grow。HMR `app_builder` 仍显式包。overlay `template_id` 查找与副窗 portal 探测改为堆 DFS。见 [`stack_and_element_model.md`](engine/stack_and_element_model.md)。
- **2026-09-05 ADR-10**：保持上游 taffy（不 fork）；Taffy 内部递归 + 深链帽 128 为接受差距。组件壳 find / 每帧 subtree sync（含 Button hover、Sidebar rail / icon chrome）改堆 DFS；Floem 式扁平 `paint_order` **不做**（`OverlayPaintSplit` 已覆盖 stacking）。规范**已冻结**。
- **2026-09-05 sticky-bake**：`hover:scale-*` / `hover:rotate-*` / `hover:skew-*`（及 `group-hover:`）在静止帧即 sticky Bake，避免指针进入时 Coverage→Bake 跳变（Bubble 表情芯片 / Twemoji）。Toast / Dialog 的内联 `transition: transform` + 程序化 scale **仍不** sticky。可选 `rui_sticky_xform=1`。

- 图形栈升到 glyphon **0.12** / wgpu **30** / cosmic-text **0.19**（`swash` 0.2 / `fontdb` 0.23 / `ttf-parser` 0.25 / `pollster` 0.4）。wgpu 30 起 DX12 可正确映射 compositor alpha；玻璃窗仍优先独立 Vulkan 预乘交换链，`backend-dx12` 时再试 DX12。手册 [`multi_window.md`](ops/multi_window.md)。
- WP-3.6：可选 feature `system-tray` 接入 `tray-icon`；菜单点击经 `run_on_ui_thread`；[`request_exit`](../crates/rui/src/app/ui_bridge.rs) 退出事件循环；默认仍 stub。手册 [`tray.md`](ops/tray.md)。
- 窗口 chrome：[`WindowOptions`](../crates/rui/src/app/window_chrome.rs)（`frameless` / `transparent` / `blur`）+ [`open_secondary_window_with`](../crates/rui/src/app/multi_window.rs) / [`run_with`](../crates/rui/src/app.rs)；自定义标题栏 [`start_window_drag`](../crates/rui/src/app/window_chrome.rs)。手册 [`multi_window.md`](ops/multi_window.md)。
- WP-3.8：稳定面 [`component_variants.md`](components/component_variants.md)（Button 词汇 + 差异表 + 受控命名 + 双书写审计）；未知 variant/size → default；Input RSX `value`；Toast success/warning 走 `chart-*` token。
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
- WP-2.9：Builder `.border_box()`；utility `box-border` / `box-content`；命中/裁剪库测；测宽叶审计见 `docs/engine/box_model.md` 附录 A。
- 盒模型单位：布局期真 `vw`/`vh`/`dvh`、局部 `em`、`calc(A ± B)`（含 `calc(100% - Npx)` 相对父 content box）；逻辑 `*-inline`/`*-block` 双值；`row-gap`/`column-gap`。
- B15 分边 border：`border_left` / `border_top_width` / `borderWidth="4px 8px"`（1–4 值）；Taffy 分边；均匀 SDF、非均匀四边条绘制。
- 窗口缩放 fast 布局会重烘焙 `vw`/`vh`/`em`/`calc`，视口单位随窗口实时变化。

- **2026-09-06 字体手册冻结**：Gallery **文本** 卡「7) 字体族 / 字重 / 行高覆盖」走查通过；[`font_strategy.md`](engine/font_strategy.md) + [`font.md`](engine/font.md) **已冻结**（默认栈 / VF / CSS 匹配 / 伪粗 / 懒加载；Inter 应用级 `fonts/`，核心不做 `bundled-inter`）。
- **2026-09-06 动画手册全文冻结**：[`animation_and_motion.md`](engine/animation_and_motion.md) 第一部分（分层 ADR）+ 第二部分（CSS 子集）**已冻结**；`PropertyId` 64 槽满；CI `anim_freeze` 20 项（reduced-motion、fill-mode、Specified 不漂移）。
- **2026-09-06 动画文档合并**：[`animation_and_motion.md`](engine/animation_and_motion.md) 吸收原 CSS 冻结子集；旧独立文件已删除。
- **2026-09-06 主题文档拆分**：新建 [`theme.md`](engine/theme.md)（DesignTokens、明暗、`ThemeOverrides`/`theme.toml`、多预设）；[`color.md`](engine/color.md) 聚焦色空间/混色/`semantic_tone`。
- **2026-09-06 颜色与主题覆盖实现**：[`theme_override.rs`](../crates/rui/src/style/theme_override.rs)（`load_theme_overrides` / `install_theme_overrides` / `compose_theme`）；CI `color_freeze`；`check-token-colors` 脚本（未接入 CI，WP-STY.1 阴影未令牌化会误报）。
- **2026-09-06 WP-OVL.1**：无头壳 [`utils/viewport_modal`](../crates/rui/src/utils/viewport_modal.rs) — underlay + 视口舞台 + `ModalSurfaceMotion`；RSX `<ViewportModal>` / `<ViewportModalContent>`；Gallery `ViewportModalShowcase`（DialogFade RSX + SheetSlide Builder）。不进 prelude。存量 Dialog/Sheet 薄包装仍归 WP-OVL.4。DialogFade 对齐 Dialog 的 `{id}__modal_zoom`，否则遮罩停在 opacity 0、退场 ExitGate 完不成。SheetSlide 入场须传入视口滑行距离；退场重建须重新种面板 transform，否则遮罩淡出后面板停在贴边、卸树后右上角残留。局部 `Portal mount=` 跟宿主裁剪/滚动，不按根 portal 逃逸。
- **2026-09-05 WP-OVL.2 / WP-OVL.3**：无头壳 [`utils/anchored_overlay`](../crates/rui/src/utils/anchored_overlay.rs) — Trigger / Point / Unanchored 三种定位；RSX `<AnchoredOverlay>` / `<AnchoredOverlayContent>`；Point 用 `Signal<[f32;2]>` + `drag_handle`。Gallery「浮层 / 反馈」`AnchoredOverlayShowcase`。不进 prelude。
- **2026-09-05 Unanchored 开合**：去掉多余视口舞台；身份 `ATTR_FLOATING` + 开关 `ATTR_FLOATING_TRIGGER`（`focusId` / `.trigger_id`），避免外点关闭与 toggle 对打。作者 `fixed` / inset 相对窗口即可。

### Notes

- 稳定面控件 API 仍见 inventory / COMPONENT_LIBRARY；本文件只标反应性异步与 Store 实验边界。
- 盒模型默认仍为 BorderBox；禁止静默改默认。见 [`box_model.md`](engine/box_model.md)。
- **W2 退出（2026-08-13）** → Wave 3。公开自绘面（`<svg>` / `<canvas>` / 2D·3D）与未来创意库见 [`drawing.md`](drawing.md)：B16 canvas-ui · B17 react-bits；执行 WP-3.11–3.13，尚未进 SemVer。
