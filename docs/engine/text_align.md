# 测宽 ≡ glyphon 分行（WP-2.1）

对应 [ENGINEERING_IMPLEMENTATION_PLAN.md](../archive/ENGINEERING_IMPLEMENTATION_PLAN.md) **WP-2.1**。

## 契约

| 角色 | API |
|------|-----|
| 布局测宽权威 | [`prepare`](../../crates/rui/src/text/measure.rs) → [`layout_lines`](../../crates/rui/src/text/measure.rs) / [`layout`](../../crates/rui/src/text/measure.rs) |
| 绘制 soft-wrap 对照 | [`glyphon_soft_wrap_line_count`](../../crates/rui/src/text/measure.rs)（与 measure 同 family / weight / shaping） |
| Chip / Mention | [`prepare_rich`](../../crates/rui/src/text/measure.rs)：原子段 + `padding_x`；不断中段 |

**容差**

- 行宽：[`LINE_WIDTH_EPS`](../../crates/rui/src/text/measure.rs) = **1.0** 逻辑 px，另允许约 **1×font_size** 的行尾空白/`break_after` 越界（wrap 检查在空白段之前完成）。
- 行数：在「仅于 break opportunity 软折」用例上，`layout_lines.len` 与 glyphon `layout_runs` **精确相等**；超宽单段的字形级折行仍由绘制侧负责（`layout_lines` 保一行）。

**无字体**

- `fontdb` 无 face 时走启发式宽度，不 panic（见 `text_align_no_font_heuristic_no_panic`）。

**VirtualList**

- 固定字号行高：[`item_height_for_text`](../../crates/rui/src/utils/virtual_list.rs) / `VirtualListBuilder::item_height_from_font`。
- 滚动换窗：keyed 增量（复用仍可见行的 `template_id` / `layout_node`，仅创建进入窗口的行、销毁离开的行）；`ReactiveSync` + `peek` 偏移（滚动用 `set_silent`）。

## 自动化

```powershell
cargo test -p rui --lib text_align_
```

## Gallery 视觉验收

```powershell
cargo run -p gallery
```

侧栏 **文本** → 卡片 **WP-2.1) 测宽 ≡ glyphon 分行**：

1. ASCII / CJK 窄框：多行软折，框内无横向溢出。
2. Chip + Mention：整段原子（Chip 文案不中段断开），窄宽下在原子前折行。
3. VirtualList：首屏 `virtual row 000`；滚动后连续出现新行（无半屏空白）；重叠行应复用而非整窗重建；行高 21px；滚动条常显。

## 已知边界

- 全链路 paint（`prepare_text`）仍对整串再 shape；与 measure 共享的是 soft-wrap **窗口宽度**，不是逐行 buffer。Chip 的 pad 计入 measure 宽度、不计入 glyphon glyph advances——chip 用例以 `layout_lines` 原子性为准，不以 glyphon run 数对照 chip pad。
- Gallery 窄框里超宽英文单词可能出现字形级断词（如 `alignmen`/`t`）；measure soft-wrap 仍按 break opportunity，绘制侧可再切。
