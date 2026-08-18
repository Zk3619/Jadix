# 已知缺口 / 产品愿望（非正式 tracker）

> **非权威**。巩固期 Backlog 以 [AUTHORITY.md](AUTHORITY.md) §7 为准；缺陷请开 issue。本文仅作随手备忘。

## Components

## Style

- animation / transform 产品化加深

## Capabilities

- 跨平台文件读写深化（选择器已有 `utils/file_dialog`）
- 网络能力
- ~~无边框窗口~~（`WindowOptions::frameless` + 自定义标题栏 `start_window_drag`）
- ~~透明 / 模糊窗口~~（`transparent` / `blur`；Win11 Acrylic）

## Bugs

所以：不是「环要每帧闪」，而是 Input 选了「不 rebuild + 每帧 peek 对齐整段 chrome」；环只是这条 pass 里的一项；闪烁光标才是需要周期性 redraw 的那部分。

若以后要拆，可以把「环/边框」改成 focus 变化时局部 dirty，但那是架构优化，不是当前 bug。
