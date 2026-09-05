# OS 平台层（`crate::platform`）现状与目标结构

> **文档地位**：`crates/rui/src/platform` 的**领域规范**（「是什么 / 缺什么 / 该怎么长」）。  
> **执行源**：巩固期排期仍以 [`AUTHORITY.md`](../AUTHORITY.md) 为准；本文**不**单独开 WP 编号。  
> **事实来源**：`crates/rui/src/platform/`、`app/window_chrome.rs`、`app/ime_support.rs`、`renderer/font_loader.rs`、`utils/clipboard.rs`、`app/wgpu_backend.rs`；Cargo `[target.'cfg(windows)'.dependencies]`。  
> **审计日期**：2026-09-06。  
> **关联**：窗口 chrome 验收见 [`multi_window.md`](../ops/multi_window.md)；IME 见 [`ime.md`](../ops/ime.md)；栈预算见 [`stack_and_element_model.md`](stack_and_element_model.md)（ADR-10）；字体路径见 [`font_strategy.md`](font_strategy.md)。

---

## 0. 一句话结论

`platform` **并非完全没有**条件编译——`mod.rs` 已按 OS 声明子模块并在 facade 内 dispatch——但当前形态更像是 **「Windows 窗口/chrome 插件 + macOS/Linux blur stub」**，**不是**可扩展的跨平台抽象层：无 desktop / mobile / embedded 分层，macOS / Linux 能力严重不对称，大量 OS 逻辑仍散落在 `app/` 与 `renderer/` 各处。

---

## 1. 现状目录与条件编译

### 1.1 当前源码树

```text
crates/rui/src/platform/
├── mod.rs              # 统一 facade + OS dispatch
├── windows/            # 完整实现（DWM、滚动、内存、PE stack helpers）
│   ├── window.rs
│   ├── process.rs
│   ├── scroll.rs
│   └── stack.rs
├── macos/mod.rs        # 仅 winit set_blur（≈15 行）
└── linux/mod.rs        # 仅 winit set_blur（≈15 行）
```

### 1.2 已有的 `#[cfg]`

入口 [`platform/mod.rs`](../../crates/rui/src/platform/mod.rs) 按 OS 声明子模块：

| 子模块 | 条件 |
|--------|------|
| `windows` | `#[cfg(target_os = "windows")]` |
| `macos` | `#[cfg(target_os = "macos")]` |
| `linux` | `#[cfg(target_os = "linux")]` |

各公开 API 在同一文件内分支，例如：

| API | Windows | macOS / Linux | 其它 |
|-----|---------|---------------|------|
| `decorate_window_attributes` | `windows::…` | 透传 attrs | 同左 |
| `apply_created_window` | DWM / backdrop | `set_blur` | `set_blur` fallback |
| `process_private_mb` | Win32 `GetProcessMemoryInfo` | **恒 0.0** | **恒 0.0** |
| `system_scroll_lines` | `SPI_GETWHEELSCROLLLINES` | **硬编码 3.0** | **硬编码 3.0** |

`windows-sys` 仅在 Cargo `[target.'cfg(windows)'.dependencies]` 中声明；非 Windows 目标不会编译 `windows/` 子树。

### 1.3 仍存在的问题

| 问题 | 说明 |
|------|------|
| 子模块内部无文件级 `cfg` | `windows/*.rs` 完全依赖父模块 `pub mod windows` 的 gate；IDE 里目录始终可见，易误判为「未做条件编译」 |
| `cfg` 写法不一致 | `mod.rs` 混用 `windows` 简写与 `target_os = "linux"`；`font_loader.rs` 对 Linux 用 `all(unix, not(target_os = "macos"))` |
| 非 Windows 多为 stub | 见上表；与 [`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) §1「三平台真窗口冒烟」仍有差距 |
| 无平台**类别**分层 | 只有 OS 扁平目录，无 `desktop/` / `mobile/` / `embedded/` |
| 无移动 / 嵌入式目标 | 无 `android` / `ios` / `wasm32` 等子树或 fallback 策略 |

---

## 2. `platform` 当前职责边界

### 2.1 已覆盖（在 `platform/` 内）

| 能力 | 路径 | 说明 |
|------|------|------|
| 窗口属性装饰 | `windows/window.rs` · `mod.rs` | 无边框圆角、DWM backdrop、composition swapchain |
| 创建后 Win32 补丁 | `windows/window.rs` | DWM 扩展框、桌面 punch-through、backdrop 重设 |
| 进程私有内存 MB | `windows/process.rs` · `mod.rs` | 供 [`tree_stats`](../../crates/rui/src/core/tree_stats.rs) 诊断 |
| 系统滚轮行数 | `windows/scroll.rs` · `mod.rs` | 供 [`scroll_support`](../../crates/rui/src/app/scroll_support.rs) |
| 可选 PE `/STACK` 常量 | `windows/stack.rs` · `mod.rs` | **应用 opt-in**；框架不 emit；见 ADR-10、[`stack_and_element_model.md`](stack_and_element_model.md) |
| macOS / Linux blur | `macos/mod.rs` · `linux/mod.rs` | `Window::set_blur(true)` |

调用链：`WindowOptions` → [`window_chrome.rs`](../../crates/rui/src/app/window_chrome.rs) → `platform::decorate_window_attributes` / `apply_created_window`。

### 2.2 仍散落在 crate 其它模块（**未**归入 `platform`）

| 模块 | OS 相关职责 | 应否迁入 `platform` |
|------|-------------|---------------------|
| [`app/ime_support.rs`](../../crates/rui/src/app/ime_support.rs) | Windows IME（Imm / IMR / 候选 HWND）大块 Win32 | **是**（`platform/os/windows/ime.rs` 或 `platform/desktop/ime.rs`） |
| [`renderer/font_loader.rs`](../../crates/rui/src/renderer/font_loader.rs) | 各 OS 字体目录与系统族解析 | **是**（`platform/os/*/font_paths.rs`） |
| [`utils/clipboard.rs`](../../crates/rui/src/utils/clipboard.rs) | macOS vs 其它剪贴板 | **是** |
| [`app/wgpu_backend.rs`](../../crates/rui/src/app/wgpu_backend.rs) | macOS Metal 优先排序 | **可选**（图形栈偏好，可留 `app/` 或 `platform/desktop/gpu.rs`） |

结论：`platform` 名不副实——窗口 chrome 与滚动/内存有集中点，**原生桌面可信行为**（IME、字体、剪贴板）仍与业务层混杂。

---

## 3. 与工程文档的落差

[`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) §1 要求 **Win / macOS / Linux 三平台**在 IME、焦点、剪贴板等路径上「行为一致到可写冒烟清单」。当前 `platform` 层：

- **Windows**：窗口 chrome + 滚动 + 内存有实质实现；
- **macOS / Linux**：blur only；`process_private_mb` / `system_scroll_lines` 为 stub；
- **IME / 字体 / 剪贴板**：实现不在 `platform`，三平台对称性需在 [`ime.md`](../ops/ime.md)、[`font_strategy.md`](font_strategy.md) 等手册单独验收。

---

## 4. 目标目录结构（重构方向）

以下为**建议形态**，非已落地代码。变更须配合 WP 与冒烟清单，不得静默大搬家。

```text
platform/
├── mod.rs                    # 对外统一 facade（保持 crate::platform::* 稳定）
├── common/                   # 全平台共享常量（如 UI_STACK_RESERVE）
├── desktop/
│   ├── mod.rs                # desktop 默认实现 + trait
│   ├── window.rs             # decorate / apply_created 的 desktop 默认
│   ├── scroll.rs             # 可复用 fallback（如默认 3 行）
│   └── memory.rs             # 可选：Unix `/proc` 等
├── mobile/                   # #[cfg(any(target_os = "android", target_os = "ios"))]
│   ├── mod.rs
│   └── window.rs
├── embedded/                 # 特定 triple / no_std 钩子（远期）
│   └── mod.rs
└── os/                       # OS 特化 override
    ├── windows/
    │   ├── mod.rs
    │   ├── window.rs         # 自现 windows/window.rs
    │   ├── process.rs
    │   ├── scroll.rs
    │   └── stack.rs
    ├── macos/
    ├── linux/
    ├── android/
    └── ios/
```

### 4.1 Facade dispatch 示意

```rust,ignore
// 伪代码：统一 cfg 风格，避免 mod.rs 内重复分支
#[cfg(feature = "desktop")]
mod desktop;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod os;

pub(crate) fn system_scroll_lines() -> f32 {
    os::current::system_scroll_lines()
}
```

### 4.2 迁移优先级（建议）

| 阶段 | 内容 | 风险 |
|------|------|------|
| P0 | 统一 `cfg` 写法；补 macOS/Linux `system_scroll_lines` / `process_private_mb` 真实实现或文档化「已知 stub」 | 低 |
| P1 | 建立 `desktop/` + `os/` 骨架；**不**改公开 API | 低 |
| P2 | 迁入 `ime_support` Win32 块 → `platform/os/windows/` | 中（WP-1.1 回归） |
| P3 | 迁入 `font_loader` OS 路径 → `platform/os/*` | 中（golden / 字体 smoke） |
| P4 | `mobile/` / `embedded/` 占位 + feature gate | 高（新目标 triple） |

---

## 5. 与 ADR-10 / 栈预算的关系

[`stack_and_element_model.md`](stack_and_element_model.md) 冻结：**框架不 emit PE `/STACK`**；深遍历走堆 DFS + `with_ui_build_stack`。  
`platform/windows/stack.rs` 仅提供 `EXE_STACK_RESERVE` / `msvc_link_arg()` / `gnu_link_arg()` 供**下游二进制**在自有 `build.rs` opt-in——属于 `platform` 职责，与 OS chrome 并列，**不应**与 `desktop/` 窗口逻辑混为一谈。

---

## 6. 代码锚点速查

| 符号 / 路径 | 用途 |
|-------------|------|
| `platform::UI_STACK_RESERVE` | 8 MiB 建议值（常量，全平台可见） |
| `platform::windows::EXE_STACK_RESERVE` | Windows 专用 re-export（仅 `cfg(windows)`） |
| `platform::decorate_window_attributes` | 创建前 `WindowAttributes` 补丁 |
| `platform::apply_created_window` | 创建后 OS 窗口补丁 |
| `platform::process_private_mb` | 诊断用私有内存 MB |
| `platform::system_scroll_lines` | 滚轮「行」换算 |
| [`WindowOptions`](../../crates/rui/src/app/window_chrome.rs) | 跨平台选项面；Win32 特化字段在 options 上 |

---

## 7. 相关文档

| 文档 | 关系 |
|------|------|
| [`stack_and_element_model.md`](stack_and_element_model.md) | ADR-10；`platform/windows/stack.rs` opt-in |
| [`multi_window.md`](../ops/multi_window.md) | 无边框 / blur / DWM 验收 |
| [`ime.md`](../ops/ime.md) | IME 实现仍在 `app/ime_support.rs` |
| [`font_strategy.md`](font_strategy.md) · [`font.md`](font.md) | 字体路径仍在 `renderer/font_loader.rs` |
| [`size_and_memory.md`](../ops/size_and_memory.md) | 内存事实；Windows Private Bytes 与 `process_private_mb` 互补 |
| [`desktop_smoke.md`](../ops/desktop_smoke.md) | 三平台冒烟清单 |
| [`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) | §1 工程级「可信原生桌面」门槛 |
