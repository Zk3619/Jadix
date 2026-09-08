# 已知缺口 / 产品愿望（非正式 tracker）

## 产品/工程

- caret 的 520ms WaitUntil 等类似的强制等待是否为兜底策略？这种强制等待兜底在工程项目中似乎不应存在

## 性能

- 渲染管线优化：不当的绘制调用（Draw Call）批次管理将显著降低帧率。应遵循“合并相同状态绘制指令”原则。
- 纹理与资源管理：频繁创建销毁纹理或 GPU 缓冲区将引发驱动级开销。需实现资源池（Resource Pool）进行复用。
- 极端内存约束：在仅 300KB 内存的嵌入式环境中，需严格监控堆分配。关键措施包括：1. 使用 no_std 或极度谨慎的堆分配策略。2. 预计算并存储 UI 布局，避免运行时动态计算。3. 压缩或使用索引颜色（Indexed Color）的位图资源。

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
- solid-dnd/sortablejs 拖拽、拖拽排序及切换父级、拖拽时的浮层、碰撞检测等 — **WP-DRAG.1 ☑** [`drag.md`](components/drag.md)
- orientation="vertical"  自动检测屏幕方向 auto、始终水平、始终垂直
- 浮层编排壳：**anchored_overlay**（Trigger / Point / Unanchored）与 **viewport_modal**（DialogFade / SheetSlide）均已落地 + Gallery。[overlay_shells.md](../components/overlay_shells.md)
- 无任何动画、阴影、模糊等特殊效果的性能模式
- 是否应将 lucide icon / lucide-animated 图标拆成独立 crate 发布

## Bugs

- sidebar 展开/收起时常发生卡顿、突变的问题，因为动画期间每帧都在因布局重算而全树重建
- 程序启动时首屏白屏的问题
- 程序启动后点击最大化后，内容没有正确放大而是维持原大小，在额外的空间仅填补底色的问题
