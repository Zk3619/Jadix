# 已知缺口 / 产品愿望（非正式 tracker）

> **非权威**。巩固期 Backlog 以 [AUTHORITY.md](AUTHORITY.md) §7 为准；缺陷请开 issue。本文仅作随手备忘。

## Components

- 参考 @D:\Projects\Rust\Reader\Reader\src\components\ui\color-picker.tsx 调整 color-picker：1.将颜色类型切换与颜色值输入放在一行，左侧使用 select.rs 选择色值类型，右侧使用 input.rs 负责输入
2.默认变体不再区分是否支持透明度
3.参考 https://github.com/xbmlz/solid-color  设计并创建一些多样的颜色选择器

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
