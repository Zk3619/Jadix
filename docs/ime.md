# IME 光标区与组字（WP-1.1）

对应实现计划 [ENGINEERING_IMPLEMENTATION_PLAN.md](ENGINEERING_IMPLEMENTATION_PLAN.md) **WP-1.1**。

## 目标

1. **preedit 稳定上屏**：中日韩组字预览正确；Commit 不丢字、不插错位；有选区时组字开始先删选区。  
2. **`set_ime_cursor_area` 随 ScrollArea / HiDPI 缩放更新**：候选窗贴近 caret，不卡在字段左上角或滚动前位置。  
3. **可观测**：`RUI_IME_DIAG=1` 日志 + `rui::app::take_last_ime_*` 断言接口。

## 代码锚点

| 能力 | 路径 |
|------|------|
| Preedit / Commit / Disabled | [`input_controller.rs`](../crates/rui/src/app/input_controller.rs) |
| 光标区几何 + diag | [`ime_support.rs`](../crates/rui/src/app/ime_support.rs) |
| 每帧刷新 cursor area | [`window_events.rs`](../crates/rui/src/app/window_events.rs)（redraw 后） |
| Win 组字定位 | [`ime_support.rs`](../crates/rui/src/app/ime_support.rs)：`IMR`/Imm 锚组字起点；**仅滚动帧**候选 HWND ΔY；锚点 Y clamp 到视口（滚出窗外贴顶/底，**不失焦**）；程序内/OS 失焦：preedit 折入 value 但去掉音节分隔 `'`；结束组字用 Disabled 排空 OS 回声（非定时器）；不建系统 caret |
| 滚动跟随几何 | 优先本帧 `text_nodes` 视觉坐标（`painted`）；焦点盒 − `scroll_acc` 为回退；`scroll:` 视口始终计入 offset |
| Input / Textarea 信号 | [`input.rs`](../crates/rui/src/components/input.rs)、[`textarea.rs`](../crates/rui/src/components/textarea.rs) |

`input:` 与 `textarea:` 共用同一套 value / preedit / caret / ime_anchor 信号键族。

## 自动化回归（库测）

```powershell
cargo test -p rui --lib ime_support
```

覆盖：

- `physical_ime_cursor_area` 随 scroll 视觉坐标与 `ui_scale` 变化  
- `prepare_compose_anchor` 删选区  
- `apply_ime_commit` 中段插入（含 CJK）  
- snapshot / commit 记录 API

## 手工验收（desktop_smoke #3–#4）

在 **Windows** 开发机（也可 macOS/Linux 对照）执行：

### 前置

```powershell
$env:RUI_IME_DIAG = "1"
$env:RUST_LOG = "rui=info,warn"
cargo run -p gallery
# 推荐：滚到「2) Input」卡片；整页在 root-scroll 内，便于测 #4
```

切换系统 IME 为**中文**（微软拼音 / 搜狗等任选其一；日文/韩文可另测）。

### #3 组字上屏不丢字

| 步骤 | 期望 |
|------|------|
| 点击 Gallery / 示例中的 **Input** | 焦点环可见；IME 允许 |
| 输入拼音如 `nihao` → 选词上屏「你好」 | 预编辑可见；上屏后字段值为「你好」；无缺字、无拉丁残留 |
| 在已有文本中间插入组字 | 插入点正确，不覆盖无关字符 |
| 先拖选一段英文再组字上屏 | 选区被替换为汉字，不残留选区原文 |

日志中应出现类似：

```text
RUI_IME_DIAG Enabled fid=input:...
RUI_IME_DIAG Preedit ...
RUI_IME_DIAG commit rest=... text="你好" ...
RUI_IME_DIAG cursor fid=... phys=(...) scale=...
```

### #4 滚动后候选窗不漂移

| 步骤 | 期望 |
|------|------|
| 将 Input 放在可滚动区域（或使用含 Input 的 ScrollArea 示例） | — |
| 聚焦 Input，开始组字（候选窗出现） | 候选窗靠近 caret |
| **保持组字**，滚动 ScrollArea | 候选窗跟随字段；焦点保持；滚动应仍可接受 |
| 继续滚到字段**完全离开**窗口客户区 | **不失焦**；组字继续；候选停在视口顶/底缘不再跟出窗外（日志可有 `offscreen-clamp`） |
| 点击其他输入框 / 其他组件（程序内失焦） | 强制结束组字：未上屏 preedit **折入** value，但去掉音节分隔 `'`；已上屏保留；不整框清空 |
| 失焦 / 切走窗口 | 同上：preedit 折入（去 `'`）；已上屏保留 |
| 改变 Windows 显示缩放（100%↔150%）后重开示例再测 | `scale` 日志变化；候选窗仍对齐（允许 ±数 px） |

失败时：附 `RUI_IME_DIAG` 日志、OS/IME 名称、是否 `RUI_FORCE_SOFTWARE`，开 issue 标签 `smoke` / `ime`。

## 断言 API（可选脚本 / 集成）

```rust,ignore
use rui::app::{peek_last_ime_cursor_snapshot, take_last_ime_commit};

// 真窗口跑一帧后：
let snap = peek_last_ime_cursor_snapshot().expect("ime cursor");
assert!(snap.physical_h >= 1);
assert!(snap.ui_scale > 0.0);
// 组字 Commit 后：
let (rest, text) = take_last_ime_commit().expect("commit");
assert!(!text.is_empty());
```

## 通过标准（WP-1.1 Done）

- [x] 库测 `ime_support` 绿  
- [x] 手工：Win 上 smoke **#3** 通过（2026-08-09，Kingdom / Win 10.0.26200；中文）  
- [x] 手工：Win 上 smoke **#4** 通过（2026-08-10；候选跟随 + 滚出视口贴边不失焦）  
- [ ] （可选）日文或韩文任测一组字路径  

签署栏见 [desktop_smoke.md](desktop_smoke.md)。
