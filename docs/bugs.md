# 已知缺口 / 产品愿望（非正式 tracker）

> **非权威**。巩固期 Backlog 以 [AUTHORITY.md](AUTHORITY.md) §7 为准；缺陷请开 issue。本文仅作随手备忘。

## Components

## Style

- animation / transform 产品化加深

## Capabilities

- 跨平台文件读写深化（选择器已有 `utils/file_dialog`）
- 网络能力
- 无边框窗口
- 透明 / 模糊窗口
- 托盘
- svg / canvas 标签等公开自绘能力 → **已升格**：规范 [`drawing.md`](drawing.md)；执行 [AUTHORITY.md](AUTHORITY.md) WP-3.11 / 3.12 / 3.13；创意库 **B16** canvas-ui（包住子树）· **B17** [react-bits](https://github.com/DavidHDev/react-bits)（文字/背景/动画 UI）
- 多窗口

## Bugs

多窗口下，次窗组件的鼠标样式一直来回闪烁，说明事件仍被传递到了主窗，只是阻断了主窗的样式变化和事件处理；另外，次窗的下拉没有正确显示在其触发器的下方
