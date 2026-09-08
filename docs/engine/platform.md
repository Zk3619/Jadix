# OS 平台层（`crate::platform`）现状与目标结构

> **文档地位**：`crates/rui/src/platform` 的**领域规范**（「是什么 / 缺什么 / 该怎么长」）。  
> **执行源**：巩固期排期仍以 [`AUTHORITY.md`](../AUTHORITY.md) 为准；本文**不**单独开 WP 编号。  
> **事实来源**：`crates/rui/src/platform/`、`app/window_chrome.rs`、`app/ime_support.rs`、`renderer/font_loader.rs`、`utils/clipboard.rs`、`app/wgpu_backend.rs`；Cargo `[target.'cfg(windows)'.dependencies]` / `[target.'cfg(target_os = "macos")'.dependencies]`。  
> **审计日期**：2026-09-06；**P0 / P1 落地**：2026-09-08。  
> **关联**：窗口 chrome 验收见 [`multi_window.md`](../ops/multi_window.md)；IME 见 [`ime.md`](../ops/ime.md)；栈预算见 [`stack_and_element_model.md`](stack_and_element_model.md)（ADR-10）；字体路径见 [`font_strategy.md`](font_strategy.md)。

---

## 0. 一句话结论

`platform` 已是 **desktop 默认 + `os/` override** 的 facade（公开 `crate::platform::*` 未改）：窗口 chrome、滚轮行数、进程内存在三桌面 OS 上均有真实实现。仍**不是**完整跨平台抽象——无 mobile / embedded 分层；IME / 字体 / 剪贴板仍散落在 `app/` 与 `renderer/`。

---

## 1. 现状目录与条件编译

### 1.1 当前源码树

```text
crates/rui/src/platform/
├── mod.rs                 # 统一 facade（crate::platform::*）
├── common.rs              # UI_STACK_RESERVE
├── desktop/               # 桌面默认：blur、滚轮 3 行、Unix 内存
│   ├── mod.rs
│   ├── window.rs
│   ├── scroll.rs
│   └── memory.rs
└── os/                    # OS override
    ├── mod.rs             # `current` → windows | macos | linux
    ├── windows/           # DWM、SPI 滚轮、Private Bytes、PE /STACK
    │   ├── mod.rs
    │   ├── window.rs
    │   ├── process.rs
    │   ├── scroll.rs
    │   └── stack.rs
    ├── macos/mod.rs       # 转发 desktop（Mach RSS 在 desktop/memory）
    └── linux/mod.rs       # 转发 desktop（/proc + KDE WheelScrollLines）
```

公开 re-export：`#[cfg(target_os = "windows")] pub use os::windows` → 仍为 `crate::platform::windows::{EXE_STACK_RESERVE, msvc_link_arg, gnu_link_arg}`。

### 1.2 已有的 `#[cfg]`

入口 [`platform/mod.rs`](../../crates/rui/src/platform/mod.rs) 与 `os/` **统一**写 `target_os = "windows" | "macos" | "linux"`（不再混用 `windows` 简写）。

| 子模块 | 条件 |
|--------|------|
| `os::windows` | `#[cfg(target_os = "windows")]` |
| `os::macos` | `#[cfg(target_os = "macos")]` |
| `os::linux` | `#[cfg(target_os = "linux")]` |
| `desktop` | 全目标编译（非桌面 triple 走默认；Windows 上部分符号仅作 SPI 回退） |

各公开 API 经 facade → `os::current`（桌面）或 `desktop`（其它）：

| API | Windows | macOS | Linux | 其它 |
|-----|---------|-------|-------|------|
| `decorate_window_attributes` | DWM / 圆角 / swapchain | 透传 attrs | 透传 attrs | 透传 |
| `apply_created_window` | DWM / backdrop | `set_blur` | `set_blur` | `set_blur` |
| `process_private_mb` | `GetProcessMemoryInfo` PrivateUsage | Mach `resident_size`（RSS） | `/proc/self/smaps_rollup` Private_*，回退 `RssAnon` | **0.0** |
| `system_scroll_lines` | `SPI_GETWHEELSCROLLLINES` | 工具包惯例 **3.0**（无 SPI） | KDE `WheelScrollLines`，否则 **3.0** | **3.0** |

`windows-sys` 仅在 Cargo `[target.'cfg(windows)'.dependencies]`；`libc` 仅 macOS（Mach `task_info`）。非对应目标不编译该子树。

### 1.3 仍存在的问题

| 问题 | 说明 |
|------|------|
| IME / 字体 / 剪贴板未迁入 | 见 §2.2；P2–P3 |
| `font_loader` Linux cfg 覆盖 *BSD | `all(unix, not(macos))` **有意保留**（`/usr/share/fonts`），P3 抽 `font_paths` 时再按 OS 拆 |
| 无平台**类别** feature | `desktop/` 是源码分层，不是 Cargo `feature = "desktop"` |
| 无移动 / 嵌入式目标 | 无 `android` / `ios` / `wasm32` 子树（P4） |

---

## 2. `platform` 当前职责边界

### 2.1 已覆盖（在 `platform/` 内）

| 能力 | 路径 | 说明 |
|------|------|------|
| 窗口属性装饰 | `os/windows/window.rs` · `desktop/window.rs` | Win：无边框圆角、DWM backdrop、composition swapchain；其它：透传 |
| 创建后 OS 补丁 | 同上 | Win：DWM 扩展框、punch-through；macOS/Linux：`set_blur` |
| 进程私有内存 MB | `os/windows/process.rs` · `desktop/memory.rs` | 供 [`tree_stats`](../../crates/rui/src/core/tree_stats.rs) 等诊断；三桌面 OS 均非 stub |
| 系统滚轮行数 | `os/windows/scroll.rs` · `desktop/scroll.rs` | 供 [`scroll_support`](../../crates/rui/src/app/scroll_support.rs) |
| 可选 PE `/STACK` 常量 | `os/windows/stack.rs` | **应用 opt-in**；框架不 emit；见 ADR-10、[`stack_and_element_model.md`](stack_and_element_model.md) |

调用链：`WindowOptions` → [`window_chrome.rs`](../../crates/rui/src/app/window_chrome.rs) → `platform::decorate_window_attributes` / `apply_created_window`。

### 2.2 仍散落在 crate 其它模块（**未**归入 `platform`）

| 模块 | OS 相关职责 | 应否迁入 `platform` |
|------|-------------|---------------------|
| [`app/ime_support.rs`](../../crates/rui/src/app/ime_support.rs) | Windows IME（Imm / IMR / 候选 HWND）大块 Win32 | **是**（`platform/os/windows/ime.rs` 或 `platform/desktop/ime.rs`） |
| [`renderer/font_loader.rs`](../../crates/rui/src/renderer/font_loader.rs) | 各 OS 字体目录与系统族解析 | **是**（`platform/os/*/font_paths.rs`） |
| [`utils/clipboard.rs`](../../crates/rui/src/utils/clipboard.rs) | macOS vs 其它剪贴板 | **是** |
| [`app/wgpu_backend.rs`](../../crates/rui/src/app/wgpu_backend.rs) | macOS Metal 优先排序 | **可选**（图形栈偏好，可留 `app/` 或 `platform/desktop/gpu.rs`） |

结论：窗口 chrome / 滚动 / 内存已集中；**原生桌面可信行为**里 IME、字体、剪贴板仍与业务层混杂。

---

## 3. 与工程文档的落差

[`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) §1 要求 **Win / macOS / Linux 三平台**在 IME、焦点、剪贴板等路径上「行为一致到可写冒烟清单」。当前 `platform` 层：

- **Windows**：窗口 chrome + SPI 滚轮 + Private Bytes；
- **macOS**：blur + Mach RSS；滚轮无系统 SPI，使用 Cocoa/GTK/Qt 惯例 3 行；
- **Linux**：blur + `/proc` 私有 RSS；KDE `WheelScrollLines` 可读，否则 3 行；
- **IME / 字体 / 剪贴板**：实现不在 `platform`，三平台对称性需在 [`ime.md`](../ops/ime.md)、[`font_strategy.md`](font_strategy.md) 等手册单独验收。

macOS `process_private_mb` 报的是 **RSS**（`mach_task_basic_info.resident_size`），不是 Windows Private Bytes 的严格同构；Linux 用 `smaps_rollup` 的 Private_Clean+Dirty，更接近专用集。诊断日志不要跨 OS 横比绝对值。

---

## 4. 目标目录结构（重构方向）

P0 / P1 已落到 §1.1。下文为**后续**形态（mobile / embedded 仍未建）。变更须配合 WP 与冒烟清单，不得静默大搬家。

```text
platform/
├── mod.rs                    # 对外统一 facade（保持 crate::platform::* 稳定）
├── common.rs                 # 全平台共享常量（UI_STACK_RESERVE）☑
├── desktop/                  # ☑
│   ├── mod.rs
│   ├── window.rs
│   ├── scroll.rs
│   └── memory.rs
├── mobile/                   # #[cfg(any(target_os = "android", target_os = "ios"))]
│   ├── mod.rs
│   └── window.rs
├── embedded/                 # 特定 triple / no_std 钩子（远期）
│   └── mod.rs
└── os/                       # ☑ Windows / macOS / Linux；android / ios 仍缺
    ├── windows/
    ├── macos/
    ├── linux/
    ├── android/
    └── ios/
```

### 4.1 Facade dispatch

桌面目标走 `os::current::*`；其余走 `desktop::*`。不引入 Cargo `feature = "desktop"`（避免与现有 lean-default 特征矩阵纠缠）。

### 4.2 迁移优先级

| 阶段 | 内容 | 风险 | 状态 |
|------|------|------|------|
| P0 | 统一 `cfg` 写法；补 macOS/Linux `system_scroll_lines` / `process_private_mb` | 低 | **☑ 2026-09-08** |
| P1 | 建立 `desktop/` + `os/` 骨架；**不**改公开 API | 低 | **☑ 2026-09-08** |
| P2 | 迁入 `ime_support` Win32 块 → `platform/os/windows/` | 中（WP-1.1 回归） | |
| P3 | 迁入 `font_loader` OS 路径 → `platform/os/*` | 中（golden / 字体 smoke） | |
| P4 | `mobile/` / `embedded/` 占位 + feature gate | 高（新目标 triple） | |

---

## 5. 与 ADR-10 / 栈预算的关系

[`stack_and_element_model.md`](stack_and_element_model.md) 冻结：**框架不 emit PE `/STACK`**；深遍历走堆 DFS + `with_ui_build_stack`。  
`platform/os/windows/stack.rs` 仅提供 `EXE_STACK_RESERVE` / `msvc_link_arg()` / `gnu_link_arg()` 供**下游二进制**在自有 `build.rs` opt-in——属于 `platform` 职责，与 OS chrome 并列，**不应**与 `desktop/` 窗口逻辑混为一谈。公开路径仍是 `platform::windows::*`。

---

## 6. 代码锚点速查

| 符号 / 路径 | 用途 |
|-------------|------|
| `platform::UI_STACK_RESERVE` | 8 MiB 建议值（常量，全平台可见） |
| `platform::windows::EXE_STACK_RESERVE` | Windows 专用 re-export（仅 `cfg(target_os = "windows")`） |
| `platform::decorate_window_attributes` | 创建前 `WindowAttributes` 补丁 |
| `platform::apply_created_window` | 创建后 OS 窗口补丁 |
| `platform::process_private_mb` | 诊断用私有内存 / RSS MB |
| `platform::system_scroll_lines` | 滚轮「行」换算 |
| [`WindowOptions`](../../crates/rui/src/app/window_chrome.rs) | 跨平台选项面；Win32 特化字段在 options 上 |

---

## 7. 相关文档

| 文档 | 关系 |
|------|------|
| [`stack_and_element_model.md`](stack_and_element_model.md) | ADR-10；`platform/os/windows/stack.rs` opt-in |
| [`multi_window.md`](../ops/multi_window.md) | 无边框 / blur / DWM 验收 |
| [`ime.md`](../ops/ime.md) | IME 实现仍在 `app/ime_support.rs` |
| [`font_strategy.md`](font_strategy.md) · [`font.md`](font.md) | 字体路径仍在 `renderer/font_loader.rs` |
| [`size_and_memory.md`](../ops/size_and_memory.md) | 内存事实；`process_private_mb` 三桌面可诊断 |
| [`desktop_smoke.md`](../ops/desktop_smoke.md) | 三平台冒烟清单 |
| [`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) | §1 工程级「可信原生桌面」门槛 |
