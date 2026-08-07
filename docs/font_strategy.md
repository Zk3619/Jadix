# 字体策略（Stage 8）

短计划：UI 默认西文用 **Inter**（或平台 UI 字体），CJK / emoji / 符号仍走懒加载回退。实现入口见 [`font_loader.rs`](../crates/rui/src/renderer/font_loader.rs)。

## 原则

1. **禁止**启动时 `FontSystem::new()` 全量 `load_system_fonts()`（Windows 上数百 MB RSS）。
2. **启动只装**：UI sans（Inter 嵌入或系统 Segoe UI / SF Pro 近亲）+ 所需字重面 + Lucide。
3. **按需打开**：CJK / Hangul / Thai / emoji / symbols / Arabic 等由 `ensure_coverage_for_text` 按码点打开。

## Inter 打包计划

| 步骤 | 内容 |
|------|------|
| 嵌入面 | Regular 400 + Medium 500 + SemiBold 600 + Bold 700（可变字体则单文件 + wght 轴） |
| Feature | `bundled-inter`（可选）：`include_bytes!` 进二进制；默认关闭以控体积 |
| 运行时目录 | 未嵌入时从 `RUI_FONT_DIR` / 可执行文件旁 `fonts/Inter-*.ttf` 加载 |
| 回退链 | Inter → Segoe UI / SF Pro Text / system UI → 懒加载 CJK |
| 测宽对齐 | 布局 [`install_shared_font_system`](../crates/rui/src/text/measure.rs) 与渲染共用同一 `FontSystem` |

## 与现有加载器的关系

`font_loader` 已实现「启动瘦栈 + 脚本桶懒加载」。本策略只固定 **UI 主族优先 Inter**，不改变 CJK/emoji 路径；字重无精确面时仍就近选面（不做 CSS 式伪粗），详见 [font.md](font.md)。
