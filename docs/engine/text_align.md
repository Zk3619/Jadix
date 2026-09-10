# 测宽 ≡ glyphon 分行（WP-2.1）

> **文档地位**：布局测宽与 glyphon soft-wrap **对齐契约**（非 CSS `text-align` 属性；后者见 `TextStyle::align` / utility `text-*`）。  
> **冻结状态**：**已冻结**（2026-09-10；库测 `text_align_*` 绿 + Gallery **文本 → WP-2.1** 视觉验收）。纠错（行号、测试命令、Gallery id）可改；**策略变更**（容差、`LINE_WIDTH_EPS`、Chip 原子性、VirtualList 行高公式）须改本文并跑 `cargo test -p rui --lib text_align_`，禁止静默改 measure/paint 对齐。  
> **执行源**：WP-2.1 已退出（[`AUTHORITY.md`](../AUTHORITY.md)，2026-08-13）。本文不再单开 WP。  
> **实现锚点**：[`text/measure.rs`](../../crates/rui/src/text/measure.rs)（`prepare` / `layout_lines` / `layout` / `glyphon_soft_wrap_line_count` / `prepare_rich`）· [`utils/virtual_list.rs`](../../crates/rui/src/utils/virtual_list.rs)（`item_height_for_text`）· [`crate_tests/text/align.rs`](../../crates/rui/src/crate_tests/text/align.rs) · Gallery [`text.rs`](../../examples/gallery/src/pages/text.rs)（`TextAlignShowcase`）。  
> **日期**：2026-09-10（治理层冻结签署；功能已于 2026-08-10 Win 开发机签署，见 [`archive/ENGINEERING_IMPLEMENTATION_PLAN.md`](../archive/ENGINEERING_IMPLEMENTATION_PLAN.md) WP-2.1）。

交叉：[`font_strategy.md`](font_strategy.md) · [`font.md`](font.md) · [`glyphon_audit.md`](glyphon_audit.md)（长期栈选型，**未冻结**）· [`damage.md`](damage.md)

---

## 0. 一句话

布局期 [`layout_lines`](../../crates/rui/src/text/measure.rs) 的 soft-wrap **行数与行宽**须与 glyphon 绘制侧 soft-wrap **可测对齐**；Chip/Mention 为 measure 原子段；VirtualList 固定字号行高由 [`item_height_for_text`](../../crates/rui/src/utils/virtual_list.rs) 断言。超宽不可折单词的字形级断行仍由绘制侧负责。

---

## 1. 契约

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

---

## 2. 自动化

```powershell
cargo test -p rui --lib text_align_
# CI：`.github/workflows/ci.yml` step「Text measure contract (freeze)」
```

库测覆盖（命名前缀 `text_align_*`）：ASCII/CJK soft-wrap ≡ glyphon；硬换行 + 软折；Chip/Mention 原子段；行宽容差；空 fontdb 启发式；VirtualList 行高 21px（14×1.5）与 `visible_item_range`。

---

## 3. Gallery 视觉验收

```powershell
cargo run -p gallery
```

侧栏 **文本** → 卡片 **WP-2.1) 测宽 ≡ glyphon 分行**：

1. ASCII / CJK 窄框：多行软折，框内无横向溢出。
2. Chip + Mention：整段原子（Chip 文案不中段断开），窄宽下在原子前折行。
3. VirtualList：首屏 `virtual row 000`；滚动后连续出现新行（无半屏空白）；重叠行应复用而非整窗重建；行高 21px；滚动条常显。

---

## 4. 已知边界（冻结面内）

- 全链路 paint（`prepare_text`）仍对整串再 shape；与 measure 共享的是 soft-wrap **窗口宽度**，不是逐行 buffer。Chip 的 pad 计入 measure 宽度、不计入 glyphon glyph advances——chip 用例以 `layout_lines` 原子性为准，不以 glyphon run 数对照 chip pad。
- Gallery 窄框里超宽英文单词可能出现字形级断词（如 `alignmen`/`t`）；measure soft-wrap 仍按 break opportunity，绘制侧可再切。

---

## 5. 已冻结（2026-09-10）

与 [`box_model.md`](box_model.md) 同标准：**纠错可改；策略变更须改本文并跑 `text_align_*`；开放项已写入 §4。**

WP-2.1 退出：库测 + Gallery 视觉验收 ☑（Win 开发机 2026-08-10；治理层冻结 2026-09-10）。**不重开 WP**；换 parley/vello 等长期栈见 [`glyphon_audit.md`](glyphon_audit.md)，须另开 ADR，不得静默破坏本契约。

---

*修订时同步 [`AUTHORITY.md`](../AUTHORITY.md) WP-2.1 备注；争议以 measure + 本文契约为准。*
