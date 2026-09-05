# 已知缺口 / 产品愿望（非正式 tracker）

> **已归档（2026-09-05）**。非正式 scratch，**不是**权威 backlog。巩固期 Backlog 以 [`AUTHORITY.md`](../AUTHORITY.md) §7 为准；缺陷请开 issue。  
> 下文后半混入了前端动画科普，仅作历史备忘，勿当规范。

## Components

## Style

- animation / transform 产品化加深

## Capabilities

- 跨平台文件读写深化（选择器已有 `utils/file_dialog`）
- ~~透明 / 模糊窗口~~ 参考 DWMBlurGlass 优化
- tailwindcss 全量语法支持
- 并考虑：增加 popup_window 能力，为桌面端实现可超出视口的独立菜单/弹窗/通知等能力
- 平台能力特化
- 缺的主要是 steps / jump-* 以及 scroll/view `animation-timeline` 驱动（`animation` / `@keyframes` MVP 已落地：name/duration/timing/delay/iteration/direction/fill-mode/play-state；composition/range 仅存档）
- solid-dnd/sortablejs 拖拽、拖拽排序及切换父级、拖拽时的浮层、碰撞检测等 — 设计拆分见 [overlay_shells.md](../components/overlay_shells.md) §9
- orientation="vertical"  自动检测屏幕方向 auto、始终水平、始终垂直
- 浮层编排壳：**anchored_overlay**（Trigger / Point / Unanchored）与 **viewport_modal**（DialogFade / SheetSlide）均已落地 + Gallery。[overlay_shells.md](../components/overlay_shells.md)
- 无任何动画、阴影、模糊等特殊效果的性能模式
- 是否应将 lucide icon / lucide-animated 图标拆成独立 crate 发布

## Bugs

- sidebar 展开/收起时常发生卡顿、突变的问题
- 程序启动时首屏白屏的问题
- transition 按像素的滑出：中间被跳过。Sheet/Drawer 开/关改走 `@keyframes` + `translate_pct` 只换了产品路径，没有修 `transition` 本身：`transition: translate` 仍只插值像素、进度走 `Instant`（空闲后下一帧可落到终态）、ease-out 把可见行程压在开头。拖拽松手吸附等仍走这条路径。由此引出拖拽松手吸附行为也存在bug：观察到吸附一次后再次拖拽，drawer瞬间塌陷导致与鼠标之间被隔开大量间距从而导致drawer顶部被压倒视口底部边缘
