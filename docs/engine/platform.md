# OS 平台层（`crate::platform`）现状与目标结构

> **文档地位**：`crates/rui/src/platform` 的**领域规范**（巩固期 desktop **P0–P3d** facade：职责边界、搬家纪律、`cfg` 分发）。§4 `mobile/` / `embedded/` 草图与 §12 后续项**不在**本期冻结范围。  
> **冻结状态**：**已冻结**（2026-09-10；库测 `platform::` / `font_loader` 绿 + Win Gallery P2 IME / P3 字体·剪贴板 §8–§9 视觉签收 2026-09-08～09）。纠错（行号、测试命令、Gallery 步骤）可改；**策略变更**（facade API、`cfg` 边界、OS 表搬家纪律、GPU 候选序、剪贴板 I/O 层）须改本文并跑 §9.6 / §10 / §11 自动化，禁止在 `app/` 或 `renderer/` 静默新增 Win32 块。  
> **执行源**：巩固期排期仍以 [`AUTHORITY.md`](../AUTHORITY.md) 为准；本文**不**单独开 WP 编号。P4 / 移动·鸿蒙见 AUTHORITY **B4** 与本文 §13。  
> **事实来源**：`crates/rui/src/platform/`、`app/window_chrome.rs`、`app/ime_support.rs`、`renderer/font_loader.rs`、`utils/clipboard.rs`、`app/state.rs`（`acquire_adapter`）、`app/glass_gpu.rs`、`app/hmr_log.rs`；Cargo `[target.'cfg(windows)'.dependencies]` / `[target.'cfg(target_os = "macos")'.dependencies]`。  
> **日期**：2026-09-10（治理层冻结签署）；**P0 / P1**：2026-09-08；**P2 IME Win32 迁入**：2026-09-08；**P3 字体路径 / 剪贴板**：2026-09-09（Win 视觉同日签收）；**P3c GPU 后端排序**：2026-09-09；**P3d 控制台 VT / 本地时钟**：2026-09-09。  
> **关联**：窗口 chrome 验收见 [`multi_window.md`](../ops/multi_window.md)；IME 见 [`ime.md`](../ops/ime.md)；栈预算见 [`stack_and_element_model.md`](stack_and_element_model.md)（ADR-10）；字体路径见 [`font_strategy.md`](font_strategy.md)；剪贴板依赖见 [`crate_deps.md`](../ops/crate_deps.md)；GPU 顺序见 [`size_and_memory.md`](../ops/size_and_memory.md) §8.1（ADR-3）；色域探测远期见 [`color_advanced.md`](color_advanced.md) P3-B。

---

## 0. 一句话结论

`platform` 已是 **desktop 默认 + `os/` override** 的 facade（公开 `crate::platform::*` 未改）：窗口 chrome、滚轮行数、进程内存、**Windows IME 驱动**、**系统字体路径**、**剪贴板纯文本 I/O + Cmd/Ctrl**、**主路径 GPU 后端排序**、**控制台 ANSI / 本地时钟**均在 `platform/`。仍**不是**完整跨平台抽象——无 mobile / embedded（巩固期不做，见 §13）。组字策略与几何仍在 [`ime_support.rs`](../../crates/rui/src/app/ime_support.rs)；字体懒加载 / bundled 仍在 [`font_loader.rs`](../../crates/rui/src/renderer/font_loader.rs)；透明次窗 GPU 仍在 [`glass_gpu.rs`](../../crates/rui/src/app/glass_gpu.rs)；HMR 行格式 / `NO_COLOR` 仍在 [`hmr_log.rs`](../../crates/rui/src/app/hmr_log.rs)。文本撤销/恢复不在本层（见 §9.8）。

---

## 1. 现状目录与条件编译

### 1.1 当前源码树

```text
crates/rui/src/platform/
├── mod.rs                 # 统一 facade（crate::platform::*）
├── common.rs              # UI_STACK_RESERVE
├── font_script.rs         # SystemFontScript（懒加载桶，无 fontdb）
├── desktop/               # 桌面默认：blur、滚轮 3 行、Unix 内存、winit IME、Unix 字体、Ctrl 剪贴板、GL 优先 GPU
│   ├── mod.rs
│   ├── window.rs
│   ├── scroll.rs
│   ├── memory.rs
│   ├── ime.rs             # 非 Win：仅 `set_ime_cursor_area`
│   ├── font_paths.rs      # Unix/*BSD 目录 + DejaVu/Noto
│   ├── clipboard.rs       # clipboard-rs 纯文本 + Ctrl
│   ├── gpu.rs             # 不透明 GL 优先；合成器 Vulkan/DX12 再 GL；`RUI_GPU_BACKEND`
│   └── console.rs         # Unix `localtime_r`；ANSI 默认已开（VT 由 Windows 覆盖）
└── os/                    # OS override
    ├── mod.rs             # `current` → windows | macos | linux
    ├── windows/           # DWM、SPI 滚轮、Private Bytes、PE /STACK、Imm/IMR、Segoe/msyh；GPU 转发 desktop
    │   ├── mod.rs
    │   ├── window.rs
    │   ├── process.rs
    │   ├── scroll.rs
    │   ├── stack.rs
    │   ├── ime.rs         # Imm / IMR subclass / 候选 HWND ΔY
    │   ├── font_paths.rs  # C:\Windows\Fonts
    │   └── console.rs     # VT + GetLocalTime
    ├── macos/
    │   ├── mod.rs         # chrome/IME 转发 desktop
    │   ├── font_paths.rs  # /System/Library/Fonts + PingFang / SFNS
    │   ├── clipboard.rs   # Cmd（Super）覆盖 Ctrl
    │   └── gpu.rs         # 仅 Metal
    └── linux/mod.rs       # 转发 desktop（含 Unix 字体 + Ctrl 剪贴板 + GPU 序 + 时钟）
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
| `apply_ime_cursor_area` 等 | Imm / IMR / 候选 HWND | winit `set_ime_cursor_area` | 同左 | 同左 |
| `system_font_dirs` / eager+lazy 文件名 | `C:\Windows\Fonts` + Segoe/msyh | `/System/Library/Fonts` + PingFang | `/usr/share/fonts` + DejaVu/Noto | Unix 表（desktop） |
| `clipboard_*` / `edit_shortcut_modifiers` | Ctrl + clipboard-rs | **Cmd** + clipboard-rs | Ctrl + clipboard-rs | Ctrl |
| `gpu_backend_candidates` | 不透明 **GL** 优先；合成器 Vulkan/DX12 再 GL | **仅 Metal**（忽略 compositor 标志） | 同 Windows 序 | 同 desktop |
| `enable_ansi_stderr` / `local_hms` | VT + `GetLocalTime` | Unix `localtime_r`；ANSI 已开 | 同 macOS | Unix 表或 stub |

`windows-sys` 仅在 Cargo `[target.'cfg(windows)'.dependencies]`；`libc` 仅 macOS（Mach `task_info`）。非对应目标不编译该子树。

### 1.3 仍存在的问题

| 问题 | 说明 |
|------|------|
| 字体 / 剪贴板 OS 表已迁入 | P3 ☑ 2026-09-09；策略仍在 `font_loader` / `utils/clipboard` facade |
| `font_loader` Linux cfg 覆盖 *BSD | **已迁**：Unix 表在 `desktop/font_paths.rs`，非 `target_os = "linux"` 独有 |
| 无平台**类别** feature | `desktop/` 是源码分层，不是 Cargo `feature = "desktop"` |
| 无色域 / HDR 探测 | `probe_display` 属 [`color_advanced.md`](color_advanced.md) P3-B，**不是**本文当前阶段（§12） |
| 无移动 / 嵌入式目标 | 无 `android` / `ios` / `wasm32` 子树；巩固期 **不建空目录**（AUTHORITY B4；§13） |

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
| IME OS 驱动 | `os/windows/ime.rs` · `desktop/ime.rs` | Win：Imm / `WM_IME_REQUEST` / 滚动帧候选 HWND ΔY；其它：winit cursor area。组字策略仍在 `app/ime_support.rs` |
| 系统字体目录 / 文件名 | `os/*/font_paths.rs` · `desktop/font_paths.rs` | 只提供路径表；`fontdb` 加载仍在 `font_loader` |
| 剪贴板纯文本 + 修饰键 | `desktop/clipboard.rs` · `os/macos/clipboard.rs` | `clipboard-rs` I/O；macOS Cmd，其它 Ctrl。公开路径仍是 `utils::clipboard` |
| 主路径 GPU 后端排序 | `desktop/gpu.rs` · `os/macos/gpu.rs` | ADR-3：不透明 GL 优先；合成器 Vulkan/DX12；macOS Metal。`RUI_GPU_BACKEND` 提到队首 |
| 控制台 ANSI + 本地时钟 | `desktop/console.rs` · `os/windows/console.rs` | Win：VT + `GetLocalTime`；Unix：`localtime_r`。12h 格式与 `NO_COLOR` 仍在 `hmr_log` |

调用链：`WindowOptions` → [`window_chrome.rs`](../../crates/rui/src/app/window_chrome.rs) → `platform::decorate_window_attributes` / `apply_created_window`。IME：[`window_events.rs`](../../crates/rui/src/app/window_events.rs) → `ime_support`（几何 / preedit）→ `platform::apply_ime_cursor_area`。

### 2.2 仍散落在 crate 其它模块（**未**归入 `platform`）

| 模块 | OS 相关职责 | 应否迁入 `platform` |
|------|-------------|---------------------|
| [`app/ime_support.rs`](../../crates/rui/src/app/ime_support.rs) | 光标几何、preedit 折入、诊断；**Win32 已迁出** | 策略留 `app/`；OS 驱动已在 `os/windows/ime.rs` |
| [`renderer/font_loader.rs`](../../crates/rui/src/renderer/font_loader.rs) | 懒加载、`ScriptNeeds`、bundled、`RUI_FONT_DIR` | **策略留下**；OS 路径表已在 `font_paths.rs`（P3a ☑） |
| [`utils/clipboard.rs`](../../crates/rui/src/utils/clipboard.rs) | 粘贴净化、MIME stub、`edit_shortcut_letter`、撤销快捷键识别 | **策略留下**；I/O + Cmd/Ctrl 已在 `platform`（P3b ☑）；公开 facade 未改 |
| [`app/state.rs`](../../crates/rui/src/app/state.rs) `acquire_adapter` | 按候选表建 instance / surface / adapter；WARP 兜底 | **策略留下**；候选表已是 `platform::gpu_backend_candidates`（P3c ☑） |
| [`app/glass_gpu.rs`](../../crates/rui/src/app/glass_gpu.rs) | 透明次窗：Vulkan（+ 可选 DX12），**无 GL** | **留下**：与主路径候选表不同；禁止合并 |
| [`app/hmr_log.rs`](../../crates/rui/src/app/hmr_log.rs) | 12h 时间戳格式、`NO_COLOR` / `FORCE_COLOR`、HMR 文案 | **策略留下**；VT / 本地时钟已在 `platform`（P3d ☑） |

结论：窗口 chrome / 滚动 / 内存 / Windows IME 驱动 / **系统字体路径** / **剪贴板 OS I/O** / **主路径 GPU 序** / **控制台 VT 与本地时钟** 已集中。P3 **不改字体策略与剪贴板语义**；P3c **不改** adapter 获取循环与 glass 候选表；P3d **不改** HMR 行外观。

---

## 3. 与工程文档的落差

[`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) §1 要求 **Win / macOS / Linux 三平台**在 IME、焦点、剪贴板等路径上「行为一致到可写冒烟清单」。当前 `platform` 层：

- **Windows**：窗口 chrome + SPI 滚轮 + Private Bytes + **Imm/IMR IME 驱动**；
- **macOS**：blur + Mach RSS；滚轮惯例 3 行；IME = winit cursor area；
- **Linux**：blur + `/proc` 私有 RSS；KDE `WheelScrollLines` 或 3 行；IME = winit cursor area；
- **字体 / 剪贴板**：OS 表在 `platform`（P3 ☑，Win 视觉 2026-09-09 签）；策略见 [`font_strategy.md`](font_strategy.md)。IME 行为验收仍以 [`ime.md`](../ops/ime.md) / [`desktop_smoke.md`](../ops/desktop_smoke.md) #3–#4 为准（§8）；P3 视觉见 §9.7 / §9.9。

macOS `process_private_mb` 报的是 **RSS**（`mach_task_basic_info.resident_size`），不是 Windows Private Bytes 的严格同构；Linux 用 `smaps_rollup` 的 Private_Clean+Dirty，更接近专用集。诊断日志不要跨 OS 横比绝对值。

---

## 4. 目标目录结构（重构方向）

P0 / P1 / P2 / **P3** / **P3c** / **P3d** 已落到 §1.1。下文 `mobile/` / `embedded/` 是远期草图，**巩固期不建**（§13）。变更须配合 WP 与冒烟清单，不得静默大搬家。

```text
platform/
├── mod.rs                    # 对外统一 facade（保持 crate::platform::* 稳定）
├── common.rs                 # 全平台共享常量（UI_STACK_RESERVE）☑
├── desktop/                  # ☑
│   ├── mod.rs
│   ├── window.rs
│   ├── scroll.rs
│   ├── memory.rs
│   ├── ime.rs
│   ├── font_paths.rs         # ☑ P3a：Unix/*BSD 目录 + DejaVu/Noto 文件名
│   ├── clipboard.rs          # ☑ P3b：clipboard-rs 封装 + Ctrl 修饰键
│   ├── gpu.rs                # ☑ P3c：不透明 GL 优先；合成器 Vulkan/DX12 再 GL
│   └── console.rs            # ☑ P3d：Unix `localtime_r`；VT 由 Windows 覆盖
├── mobile/                   # 远期草图；巩固期 **不建**（AUTHORITY B4）
│   ├── mod.rs
│   └── window.rs
├── embedded/                 # 特定 triple / no_std 钩子（远期）
│   └── mod.rs
└── os/                       # ☑ Windows / macOS / Linux；android / ios 仍缺且本期不建
    ├── windows/
    │   ├── font_paths.rs     # ☑ P3a：C:\Windows\Fonts + Segoe / msyh / Malgun…
    │   └── console.rs        # ☑ P3d：VT + GetLocalTime
    ├── macos/
    │   ├── font_paths.rs     # ☑ P3a：/System/Library/Fonts + PingFang / SFNS…
    │   ├── clipboard.rs      # ☑ P3b：Cmd（Super）覆盖 desktop Ctrl
    │   └── gpu.rs            # ☑ P3c：仅 Metal
    ├── linux/                # 转发 desktop（memory / scroll / 字体 / GPU 序）
    ├── android/              # 远期；不建空目录
    └── ios/
```

### 4.1 Facade dispatch

桌面目标走 `os::current::*`；其余走 `desktop::*`。不引入 Cargo `feature = "desktop"`（避免与现有 lean-default 特征矩阵纠缠）。

### 4.2 迁移优先级

| 阶段 | 内容 | 风险 | 状态 |
|------|------|------|------|
| P0 | 统一 `cfg` 写法；补 macOS/Linux `system_scroll_lines` / `process_private_mb` | 低 | **☑ 2026-09-08** |
| P1 | 建立 `desktop/` + `os/` 骨架；**不**改公开 API | 低 | **☑ 2026-09-08** |
| P2 | 迁入 `ime_support` Win32 块 → `platform/os/windows/` | 中（WP-1.1 回归） | **☑ 2026-09-08** |
| P3a | 迁入 `font_loader` **系统目录 + 文件名表** → `platform/os/*/font_paths.rs` | 中（golden / 字体 smoke） | **☑ 2026-09-09** |
| P3b | 迁入剪贴板 **OS I/O + Cmd/Ctrl**；`utils/clipboard` 保持公开 facade | 中（跨应用粘贴 / 修饰键） | **☑ 2026-09-09** |
| P3c | 迁入主路径 **GPU 后端排序** → `desktop/gpu.rs` + `os/macos/gpu.rs` | 低（顺序不变；adapter 循环仍在 `state.rs`） | **☑ 2026-09-09** |
| P3d | 迁入 **控制台 VT + 本地时钟**；`hmr_log` 只保留 12h 格式与 `NO_COLOR` | 低（行外观不变） | **☑ 2026-09-09** |
| P4 | `mobile/` / `embedded/` 占位 + 新 triple | 高 | **巩固期不做**（AUTHORITY B4；§13） |
| — | 文本撤销/恢复、换 `arboard`、色域探测、合并 `glass_gpu` 候选表 | — | **不是本层当前阶段**（§9.8 / §12） |

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
| `platform::apply_ime_cursor_area` | 组字锚点交给 OS（Win Imm/IMR；其它 winit） |
| `platform::sync_ime_focus_owner` | 焦点场切换；Win 上通过回调折入 preedit |
| [`WindowOptions`](../../crates/rui/src/app/window_chrome.rs) | 跨平台选项面；Win32 特化字段在 options 上 |
| `platform::system_font_dirs` / `resolve_system_font` | 系统字体目录与「文件名 → 已存在路径」 |
| `platform::eager_ui_font_files` / `lazy_script_font_files` | 启动 UI 面 / 懒加载脚本文件名表 |
| `platform::clipboard_set_text` / `clipboard_get_text` | OS 剪贴板纯文本；公开路径仍是 `utils::clipboard` |
| `platform::edit_shortcut_modifiers` | macOS Cmd / 其它 Ctrl |
| `platform::gpu_backend_candidates` | 主路径 wgpu 后端顺序 + `RUI_GPU_BACKEND`；glass 窗不用此表 |
| `platform::enable_ansi_stderr` / `local_hms` | Win VT；本地时:分:秒。公开调用仍在 `hmr_log` |

---

## 7. 相关文档

| 文档 | 关系 |
|------|------|
| [`stack_and_element_model.md`](stack_and_element_model.md) | ADR-10；`platform/os/windows/stack.rs` opt-in |
| [`multi_window.md`](../ops/multi_window.md) | 无边框 / blur / DWM 验收 |
| [`ime.md`](../ops/ime.md) | WP-1.1；几何/preedit 在 `ime_support`；Win32 驱动在 `os/windows/ime.rs` |
| [`font_strategy.md`](font_strategy.md) · [`font.md`](font.md) | 策略/诊断冻结；系统路径在 `platform` `font_paths.rs` |
| [`size_and_memory.md`](../ops/size_and_memory.md) | 内存事实；`process_private_mb` 三桌面可诊断；禁全量 `load_system_fonts`；ADR-3 GPU 序见 §8.1 |
| [`desktop_smoke.md`](../ops/desktop_smoke.md) | 三平台冒烟清单；P2 见 §8，P3 见 §9.7 |
| [`crate_deps.md`](../ops/crate_deps.md) | `clipboard-rs` 审计；P3b **不**换成 `arboard` |
| [`color_advanced.md`](color_advanced.md) | `probe_display` 属该文 P3-B，不是本文当前阶段（§12） |
| [`text_undo_redo.md`](../plans/text_undo_redo.md) | 文本撤销设计；**不是** `platform` P3（§9.8） |
| [`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) | §1 工程级「可信原生桌面」门槛 |

---

## 8. P2 迁移后如何验证（视觉 + 标准）

P2 **不改组字语义**，只搬家 Win32 驱动。验收以 WP-1.1 为准：自动化绿 + Gallery 真窗口手工 #3 / #4。细则与诊断字段见 [`ime.md`](../ops/ime.md)；总表见 [`desktop_smoke.md`](../ops/desktop_smoke.md)。

### 8.1 自动化（每次改 `platform/os/windows/ime.rs` 或 `ime_support` 必跑）

```powershell
cargo test -p rui --lib ime_support
cargo test -p rui --lib platform::
```

**通过**：`ime_support` 库测全绿（preedit 折入、commit 中段插入、光标区随 scroll/`ui_scale`、选区删除）。这只证明几何/策略，**不能**代替候选窗视觉。

### 8.2 视觉冒烟（Windows 真窗口，必做）

```powershell
$env:RUI_IME_DIAG = "1"
$env:RUST_LOG = "rui=info,warn"
cargo run -p gallery --release
```

切到系统 **中文 IME**（微软拼音或搜狗）。Gallery 滚到 **「2) Input」**（整页在 root-scroll 内，便于 #4）。

| 编号 | 操作 | 通过标准（看见什么） | 失败长什么样 |
|------|------|----------------------|--------------|
| **#3 组字上屏** | 点 Input，打 `nihao` 选词「你好」 | 预编辑跟 caret；上屏后字段恰为「你好」，无缺字、无拉丁残留 | 丢字、插入点跳到开头/末尾、拼音残留 |
| **#3 中段插入** | 已有 `abc`，caret 放在 `b\|c` 再组字 | 汉字插在 b 与 c 之间 | 整段被替换或插到串尾 |
| **#3 选区替换** | 拖选一段英文再组字上屏 | 选区被汉字替换，不残留原文 | 选区还在或中英叠在一起 |
| **#4 候选跟随** | 聚焦 Input，组字中**保持候选窗**，滚动页面 | 候选窗贴 caret，焦点环不丢 | 候选卡在组字开始时的屏幕位置 |
| **#4 滚出视口** | 继续滚到 Input **完全离开**窗口 | **不失焦**；组字继续；候选停在视口顶/底缘，不跟出窗外 | 失焦、组字被取消、候选飞出客户区 |
| **#4 换框 / 切走窗口** | 点另一个 Input，或 Alt-Tab 走焦再回来 | 未上屏 preedit **折入** value 但去掉音节 `'`；已上屏汉字保留 | 整框被清空，或 `'` 留在字段里 |
| **HiDPI** | 显示缩放 100% ↔ 150% 后重开再测 #3/#4 | 候选仍对齐 caret（允许 ±数 px）；日志 `scale=` 变化 | 候选明显偏到字段左上角 |

日志（`RUI_IME_DIAG=1`）应出现：`Enabled` / `Preedit` / `commit` / `cursor`；滚动时允许 `scroll-latch` / `scroll-follow-y` / `offscreen-clamp`。**不应**每帧刷屏 `EnumWindows` 式 relocate。

### 8.3 次窗（回归，防 HWND 绑错）

Gallery → 浮层 → Secondary window（或导航 Router → Open secondary）。在**次窗** Input 上重复 #3：候选窗必须跟次窗 caret，不能出现在主窗。

### 8.4 签署

- [x] `cargo test -p rui --lib ime_support` 绿（P2 代码迁入后，2026-09-08）
- [x] Win 真窗口 #3 / #4（与 [`desktop_smoke.md`](../ops/desktop_smoke.md) 同机签署；P2 迁入后重签 **2026-09-08**，不沿用 2026-08 旧签字代替）
- [ ] （可选）次窗 IME；日文或韩文一组字路径

失败时附：`RUI_IME_DIAG` 片段、OS/IME 名称、是否 `RUI_FORCE_SOFTWARE`，标签 `smoke` / `ime`。

---

## 9. P3 迁入（代码 + Win 视觉已签）

P3 **只搬家**，不改字体策略、不改剪贴板语义、不换依赖。对照 P2：Win32 驱动搬走、组字策略留下。自动化见 §9.6；Gallery 视觉步骤见 §9.7，签署见 §9.9。

### 9.1 目标与非目标

| 做 | 不做 |
|----|------|
| 系统字体**目录**与**文件名表**进 `platform/`；`font_loader` 只问「这条路径在不在」再 `fontdb::load_font_file` | 搬 `ScriptNeeds` / `ensure_coverage_for_text` / bundled `include_bytes!` / `apply_default_families` |
| 剪贴板 **get/set 纯文本** + **Cmd vs Ctrl** 进 `platform/`；`utils/clipboard` 保持公开 facade | 换 `arboard`；实现 HTML/URI MIME；搬 `sanitize_pasted_line` / `edit_shortcut_letter` |
| 保留 *BSD：Unix 字体目录不得收成仅 `target_os = "linux"` | 新建 `mobile/`（§13）；合并 `glass_gpu` 候选表；实现 `probe_display` |
| 公开路径稳定：`crate::platform::windows::*`、`crate::utils::clipboard::*` | **撤销/恢复改造**（历史栈、`text-undo` feature、Owner 持有、rope） |

### 9.2 字体路径：迁什么 / 留什么

今日 OS 分歧全部写在 [`font_loader.rs`](../../crates/rui/src/renderer/font_loader.rs) 的 `#[cfg]` 块里。P3a 抽出**数据**，不抽出 **fontdb / cosmic**。

| 符号（今日） | 迁入 | 留下 |
|--------------|------|------|
| `try_load_system_font` 里的目录列表 | `system_font_dirs() → Vec<PathBuf>` | `is_file` + `db.load_font_file` 仍在 loader |
| `load_eager_system_ui_fonts` 文件名 | `eager_ui_font_files() → &'static [&'static str]` | 循环与日志 |
| `load_lazy_{cjk,hangul,thai,emoji,symbols,arabic,devanagari}` 文件名 | `lazy_*_font_files() → &'static [&'static str]`（每脚本一组） | 码点扫描、`attempted`、bundled 优先、`RUI_LOAD_SYSTEM_*` |
| `font_search_dirs` / `find_font_file` | **不迁** | 应用旁路：`RUI_FONT_DIR`、exe `fonts/` / `assets/`、crate `assets/`——这是**打包**，不是 OS |
| `create_font_system` / `LazyFontState` | **不迁** | 启动只装 UI 面 + 懒加载，契约在冻结的 [`font_strategy.md`](font_strategy.md) |

建议 facade（crate 内 `pub(crate)`，不必进 prelude）：

```text
platform::system_font_dirs() -> Vec<PathBuf>
platform::resolve_system_font(file_name: &str) -> Option<PathBuf>
  // 在 system_font_dirs 下 join，返回第一个 is_file 的路径
platform::eager_ui_font_files() -> &'static [&'static str]
platform::lazy_script_font_files(script) -> &'static [&'static str]
```

`font_loader::try_load_system_font` 变成：`resolve_system_font(name).map(|p| try_load_font_path(db, &p, state))`。

**文件名表必须逐 OS 抄现状，禁止「顺手改候选」**（改候选 = 策略变更，须改 `font_strategy.md`）：

| 桶 | Windows | macOS | Unix（含 *BSD，今日 `all(unix, not(macos))`） |
|----|---------|-------|------|
| Eager UI | `segoeui.ttf` 及 Light/Semilight/Semibold/Bold/Black；`consola.ttf`；Cascadia；**`times.ttf` / `georgia.ttf`（及 Bold）** | `SFNS*` / Helvetica / Arial；**Times / Georgia / NewYork** | DejaVu Sans/Mono；**DejaVu Serif / Liberation Serif / Noto Serif** |
| CJK | `msyh.ttc` → `msyhbd.ttc` → `simhei.ttf` → `simsun.ttc` | `PingFang.ttc` → Hiragino Sans GB → STHeiti Light → AppleSDGothicNeo | NotoSansCJK `.ttc` 若干路径 → `wqy-microhei.ttc` |
| Hangul | `malgun*.ttf` → `gulim.ttc` | AppleSDGothicNeo / AppleGothic | NotoSansCJK / NanumGothic |
| Thai | Leelawadee UI / Tahoma | Thonburi / Ayuthaya | NotoSansThai / TlwgTypo |
| Emoji | `seguiemj.ttf` | `Apple Color Emoji.ttc` | NotoColorEmoji 若干路径 |
| Symbols | `seguisym.ttf` / `seguili.ttf` | Apple Symbols / Arial Unicode | DejaVuSans |
| Arabic | tradbdo / trado / arial | Supplemental/Arial / GeezaPro | DejaVuSans / LiberationSans |
| Devanagari | `Nirmala.ttc` / `mangal.ttf` | Devanagari Sangam MN / KohinoorDevanagari | NotoSansDevanagari |

目录：

| OS | 根目录（按序） |
|----|----------------|
| Windows | `C:\Windows\Fonts` |
| macOS | `/System/Library/Fonts`、`…/Supplemental`、`/Library/Fonts` |
| Unix | `/usr/share/fonts`、`/usr/local/share/fonts`、`…/truetype`、`…/opentype` |

相对路径（`truetype/noto/…`、`Supplemental/Arial.ttf`）是「相对上述根」的 join，搬家时不要改成绝对路径猜盘符。

### 9.3 *BSD cfg 纪律

今日注释写明：Linux 侧用 `all(unix, not(macos))` 是为了 **FreeBSD / NetBSD 等也走 `/usr/share/fonts`**，不是漏写 `target_os = "linux"`。

P3a 落地时：

1. **默认表放 `desktop/font_paths.rs`**（Unix 目录 + DejaVu/Noto 文件名）。
2. `os/linux` **转发 desktop**（与今日 `memory` / `scroll` / IME 同构）。
3. **禁止**把 Unix 表改成 `#[cfg(target_os = "linux")]` 而让 *BSD 变成「无系统字体、只靠 bundled」。
4. `os/windows` / `os/macos` 覆盖目录与文件名；facade 仍走 `os::current`。

`platform/` 内继续统一 `target_os = "windows" | "macos" | "linux"`；**仅字体 Unix 回退**允许 desktop 里保留 `all(unix, not(macos))`。这是有意的例外，写进 `desktop/font_paths.rs` 模块文档。

### 9.4 剪贴板：OS I/O vs 编辑策略

今日 [`utils/clipboard.rs`](../../crates/rui/src/utils/clipboard.rs) 混了三层。P3b 只抽最底一层。

| 符号 | 层 | P3b |
|------|----|-----|
| `set_text` / `get_text`（`clipboard-rs`） | OS I/O | **迁入** `platform/desktop/clipboard.rs`；macOS 无独立 API 时可共用实现 |
| `shortcut_modifiers`（Cmd vs Ctrl） | OS 快捷键惯例 | **迁入**；`os/macos` 覆盖为 `super_key()` |
| `sanitize_pasted_line` | 单行 Input 策略 | 留 `utils/` |
| `mime` / `set_with_mime` / `get_with_mime` / `set_html` | 公开占位（未知 MIME 退纯文本） | 留 `utils/`；**不**在 P3 做 CF_HTML / NSPasteboard 类型 |
| `edit_shortcut_letter` | 键盘布局 | 留 `utils/` |
| `is_undo_shortcut` / `is_redo_shortcut` | 修饰键 + Z/Y | **可**改为调用 `platform` 的修饰键，文件仍放 `utils/`；**不**因此改造历史栈 |

公开 API **保持** `crate::utils::clipboard::{set_text, get_text, shortcut_modifiers, …}`。内部改为转发 `crate::platform::…`。调用方 [`input_controller.rs`](../../crates/rui/src/app/input_controller.rs)、[`label.rs`](../../crates/rui/src/components/label.rs) 不改导入路径。

依赖：继续 `clipboard-rs`（已关 `image` feature，见 [`size_and_memory.md`](../ops/size_and_memory.md)）。换成 `arboard` 或 Win32 `OpenClipboard` 薄封装是 [`crate_deps.md`](../ops/crate_deps.md) 的独立项，**不要夹在 P3 搬家里**。

### 9.5 落地顺序（代码 PR 时）

1. **P3a 先于 P3b**（字体回归面更大：golden + Gallery 字体卡 + 懒加载 CJK）。
2. 先加 `platform` 模块与 facade，**再**删 `font_loader` 里的 `cfg` 表；中间不得双份表分叉。
3. 不改 `font_strategy.md` 冻结项（默认栈、synthetic bold、不 embed Inter、启动禁 `FontSystem::new()` 全量扫描）。
4. 一次 PR 只做 a 或 b；不要和 IME / 撤销 / GPU 搅在一起。

### 9.6 自动化（代码迁入后必跑）

```powershell
cargo test -p rui --lib font_loader
cargo test -p rui --lib platform::
cargo test -p rui --lib clipboard
```

**通过**：

- `ensure_coverage_injects_cjk_into_each_font_system` 仍绿（两份 `FontSystem` 各自注入 CJK，不因全局 `loaded_paths` 短路）。
- `platform::` 原有 `process_private_mb` / `system_scroll_lines` 仍绿。
- 若 P3b 给 `shortcut_modifiers` 加测：非 macOS 认 Ctrl、macOS 认 Super（`cfg` 分支，CI 各跑各的 host）。

**不能代替视觉**：系统是否真有 `msyh.ttc` / PingFang / Noto，库测在缺字的 CI 镜像上可能跳过或只断言「调用不 panic」。Windows 开发机以 Gallery 为准。

### 9.7 视觉冒烟（P3 代码合入后）

与 P2 一样：自动化绿 ≠ 用户看见的字和剪贴板对。下列在 **同一台已签过 P2 的 Win 机**上重走；macOS / Linux 列仍按 [`desktop_smoke.md`](../ops/desktop_smoke.md) 开放。

```powershell
cargo run -p gallery --release
```

**字体（P3a）** — 策略未改，看见的应与迁入前同类（允许 hinting/DPI 既有差，不允许缺族、豆腐盒增多、启动 RSS 暴涨）：

| 编号 | 操作 | 通过标准 | 失败长什么样 |
|------|------|----------|--------------|
| **F1 启动族** | 侧栏 **文本** → **7) 字体族 / 字重 / 行高覆盖（验收）** | `font-sans` / `serif` / `mono` 可辨；字重阶梯仍在；与 [`font_strategy.md`](font_strategy.md) Gallery 步骤一致 | 西文回成宋体/点阵；等宽列与 sans 无差别 |
| **F2 懒加载 CJK** | Input 打 `你好` / `日本語`（系统 IME 或粘贴） | 汉字出现，**不是** tofu `□`；日志可有 `loaded font file: …msyh` 一类，**不应**出现 `RUI_LOAD_SYSTEM_FONTS=1` 全量扫描 | 方框、空白、或启动瞬间多映射数百 MB 字体 |
| **F3 启动内存** | 冷启动 Gallery，看 `RUI_TREE_STATS` / 任务管理器 | 与迁入前同量级；**禁止**为「省事」改回 `db.load_system_fonts()` | Private Bytes 无故多出一两百 MiB |
| **F4 HiDPI** | 100% ↔ 150% 重开 F1 | 字形仍在；不要求与 Web 像素对齐（见 [`font.md`](font.md)） | 字消失或回退到错误族 |

**剪贴板（P3b）** — Gallery **2) Input** / Textarea；组字中快捷键仍应被 IME 抢先（与今日一致）：

| 编号 | 操作 | 通过标准 | 失败长什么样 |
|------|------|----------|--------------|
| **C1 复制粘贴** | 选一段 ASCII，Ctrl+C，再 Ctrl+V 到同框或另一 Input | 选区被替换为剪贴板文本 | 无反应、粘成空、插入点跳到开头 |
| **C2 跨应用** | 从记事本复制中文，粘进 Input；再从 Input 复制到记事本 | 双向都是同一段字 | 只能框内循环、出不去 OS 剪贴板 |
| **C3 剪切 / 全选** | Ctrl+A、Ctrl+X，再 Ctrl+V | 先清空再还原 | 密码框若能复制出明文 → 失败（密码须拦 C/X） |
| **C4 单行净化** | 从外部贴带换行的两行到 **单行** Input | 换行被丢掉，仍是一行 | 字段变成两行或插入 `\r` |
| **C5 修饰键** | Win/Linux 用 **Ctrl**；macOS 用 **Cmd**（若有 Mac 机） | 对侧修饰键不触发（Win 上 Cmd 等价键不应当成复制） | Win 上只有 Super+C 才复制 |
| **C6 组字中** | 中文 IME 组字未上屏时按 Ctrl+V | **不**把拼音/候选当选区剪贴；与 [`ime.md`](../ops/ime.md) 一致 | 预编辑被剪贴板打乱 |

### 9.8 明确不做：撤销与恢复

`is_undo_shortcut` / `is_redo_shortcut` 已在 `utils/clipboard.rs`；`input_controller` 在 `text-undo` feature 下调用 `text_edit_history::{undo,redo}_field`（无 feature 时仍消费 Z/Y）。那是**文本编辑能力**，不是 OS chrome。P0–P4 已落地（rope feature 按该计划 §9.3 **未开**），见 [`text_undo_redo.md`](../plans/text_undo_redo.md)。

P3 **不得**夹带：

- `text-undo` Cargo feature、Owner 持有历史、RAII 事务、rope / piece table（见 [`text_undo_redo.md`](../plans/text_undo_redo.md)）；
- 改合并窗口、`is_internal_sync`、废弃 thread-local 历史 Map；
- 把撤销栈塞进 `platform/`（它不是系统调用栈，也不是 Imm/DWM 一类 OS 驱动）。

若 P3b 让 `shortcut_modifiers` 搬家，撤销**快捷键识别**可以跟着修饰键走；**历史结构与语义**见 [`text_undo_redo.md`](../plans/text_undo_redo.md)（P0–P4 已落地；`text-undo-rope` 未开）。

### 9.9 签署

- [x] `cargo test -p rui --lib font_loader` 与 `platform::` 绿（P3 代码迁入后，2026-09-09）
- [x] Win Gallery F1–F3（同机签收 **2026-09-09**，先于后续 P3c 推进）
- [x] Win Gallery C1–C4、C6（同上）
- [ ] （可选）macOS C5 Cmd；Linux F2 Noto/WenQuanYi

失败时附：OS、是否缺系统字体包、`RUI_FONT_DIR` / `RUI_LOAD_SYSTEM_FONTS`、标签 `smoke` / `font` / `clipboard`。

---

## 10. P3c GPU 后端排序（已落地）

P3c **只搬家**主窗口 `acquire_adapter` 用的候选表，不改顺序、不改 `RUI_GPU_BACKEND` 语义、不碰 glass 窗。

| 做 | 不做 |
|----|------|
| OS 相关后端序列进 `platform/`：desktop 默认 GL/Vulkan/DX12；macOS 仅 Metal | 改 ADR-3（不透明窗仍 GL 优先） |
| `RUI_GPU_BACKEND`（`gl` / `dx12` / `vulkan` / `metal`）提到队首，其余仍兜底 | 未编译的后端 abort；换 wgpu 版本 |
| facade `gpu_backend_candidates(compositor_alpha)` | 把 [`glass_gpu.rs`](../../crates/rui/src/app/glass_gpu.rs) 的 Vulkan/+DX12、**无 GL** 表并进来 |
| 删 `app/wgpu_backend.rs` | 搬 `acquire_adapter` 循环、WARP/llvmpipe 兜底、`MemoryHints` |

调用：[`state.rs`](../../crates/rui/src/app/state.rs) `acquire_adapter` → `platform::gpu_backend_candidates`。glass 路径继续用自己的 `backend_candidates()`（GL 无 PreMultiplied alpha，见 [`size_and_memory.md`](../ops/size_and_memory.md) §8.1）。

自动化：

```powershell
cargo test -p rui --lib platform::
```

**通过**：Windows 不透明首项 `gl`；合成器在默认 feature 下首项 `gl`（无 vulkan/dx12）或 feature 开启时 `vulkan`/`dx12`；macOS 恒为 `metal`。不能代替真机选后端；Gallery 启动日志 `gpu backend:` 应与迁入前同类。

---

## 11. P3d 控制台 VT 与本地时钟（已落地）

P3d **只搬家** `hmr_log` 里的 OS 系统调用，不改 Vite 风格行、不改 12 小时制、不引入 `chrono`。

| 做 | 不做 |
|----|------|
| Windows：`ENABLE_VIRTUAL_TERMINAL_PROCESSING` + `GetLocalTime` → `os/windows/console.rs` | 换 `windows-sys` Console feature、改 FFI 语义 |
| Unix（含 *BSD）：`localtime_r` → `desktop/console.rs`（不得收成仅 `linux`） | 用 `chrono` / `time` crate |
| facade `enable_ansi_stderr` / `local_hms` | 搬 `NO_COLOR` / `FORCE_COLOR` / `format_12h` / HMR 文案 |

调用：[`hmr_log.rs`](../../crates/rui/src/app/hmr_log.rs) `emit` → `platform::enable_ansi_stderr` + `local_hms`。UTC 回退仍在 `hmr_log`（OS 时钟不可用时）。

自动化：`cargo test -p rui --lib platform::`（`local_hms` 时分秒落在 24h/60/60 内；`enable_ansi_stderr` 可重复调用）。不能代替「stderr 是否真出颜色」——无 TTY / `NO_COLOR` 时本来就不着色。

---

## 12. 后续但不属于本层当前阶段

| 项 | 放哪 | 为何不是现在 |
|----|------|-------------|
| `probe_display`（色域 / EDR） | [`color_advanced.md`](color_advanced.md) **P3-B**（该文自己的阶段名，勿与本文 P3 混淆） | 未实现；默认像素须不变；绑 `DisplayTransform` 而非 `font_paths` |
| `mobile/` / `embedded/` | 本文 §13；AUTHORITY **B4** | 巩固期明确不做移动/鸿蒙一等公民 |
| 换 `clipboard-rs` → `arboard` / Win32 | [`crate_deps.md`](../ops/crate_deps.md) | 依赖手术，不与搬家同 PR |
| 文本撤销/恢复深化 | [`text_undo_redo.md`](../plans/text_undo_redo.md) | §9.8 |
| 合并 `glass_gpu` 与主路径候选表 | 留 `app/glass_gpu.rs` | 合成器必须避开 GL；与 ADR-3 主路径正交 |

---

## 13. P4 `mobile/` / `embedded/`（巩固期不做）

[`AUTHORITY.md`](../AUTHORITY.md)：**巩固期明确不做**「移动 / 鸿蒙一等公民」；backlog **B4**。本文把目标结构画在 §4，是为了以后搬家时 facade 仍稳定，**不是**本期任务。

本期纪律：

1. **不建**空的 `platform/mobile/`、`os/android/`、`os/ios/`、`embedded/`（空树会让人以为有 hook）。
2. **不**新增 Cargo `feature = "desktop"`（与 lean-default 特征矩阵纠缠）。
3. 公开路径继续只保证桌面：`crate::platform::*`、`crate::platform::windows::*`（`cfg(target_os = "windows")`）。
4. 若将来开新 triple：先写该 OS 的 override（window / IME / 字体 / 剪贴板 / GPU），facade 仍 `os::current` 或按 `target_os` 增加分支；禁止在 `app/` 再堆一套 `cfg(android)`。

---

