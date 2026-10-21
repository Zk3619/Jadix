# Jadix - Rust Native UI Framework

> **GitHub Description (建议 / Suggested)**
>
> - 中文：基于 `winit + wgpu + taffy + glyphon` 的 Rust 原生 UI 框架，提供类 RSX 与纯 Rust Builder 双书写面、细粒度响应式与开发期热重载。
> - English: A Rust-native UI framework built on `winit + wgpu + taffy + glyphon`, featuring dual authoring (RSX-like `ui!` and pure-Rust builders), fine-grained reactivity, and dev-time hot reload.

---

## 项目简介 | Overview

**中文**  
Jadix 是一个纯 Rust 原生 UI 工作区，目标是在不依赖高层 GUI 框架的前提下，构建现代化、可组合、可扩展的跨平台 UI 引擎。项目采用分层架构、信号驱动更新、ShadCN 风格组件体验与开发期热重载等设计目标，并已在实际代码中落地了可运行的核心链路。

**English**  
Jadix is a pure Rust-native UI workspace aimed at building a modern, composable, and extensible cross-platform UI engine without relying on heavyweight high-level GUI frameworks. It targets layered architecture, signal-driven updates, a ShadCN-like component experience, and dev-time hot reload, with working implementations already present in code.

---

## 当前状态 | Current Status

- **中文**：当前阶段为 **Alpha / 实验性**，核心渲染、布局、文本、组件与宏链路可运行，API 仍在快速迭代。  
- **English**: The project is currently **Alpha / experimental**. Core rendering, layout, text, components, and macro pipelines are functional, while APIs are still evolving quickly.

---

## 核心特性 | Key Features

- **分层架构 | Layered architecture**  
  基于 `winit`（窗口/事件）、`wgpu`（GPU 渲染）、`taffy`（Flex/Grid 布局）、`glyphon`（文本）构建。

- **声明式 UI · 双书写面 | Dual authoring**  
  - **中文**：`ui!` 类 RSX 标签与 `button()` / `*Builder` 纯 Rust 链式 API **同等一等公民**，可互嵌；不强制类 HTML 标签。  
  - **English**: RSX-like `ui!` tags and pure-Rust `*Builder` APIs are **both first-class** and nestable; HTML-like tags are optional.

- **组件宏 | Component macro**  
  提供 `#[component]`，支持类似前端组件化的复用；函数体可用标签或 Builder 构建子树。

- **细粒度响应式 | Fine-grained reactivity**  
  通过 `use_signal` 驱动状态更新，减少不必要重绘。

- **保留与即时混合 | Retained and immediate regions**
  默认保留树与 Signal 叶级更新；`immediate_region` 在所属窗口的请求帧重述动态子树。`jadix::prelude` 导出 L0–L4 失效 API：`request_frame`、`node(...).mark_paint/mark_layout`、`rebuild_all`、`bust_caches`。L3 保留文本与字形缓存，L4 显式重建；停止续帧后可回到空闲。详见[混合帧模型](development-docs/engine/hybrid_frame_model.md)。
  Retained trees and signal bindings are the default. Immediate regions restate dynamic subtrees on requested frames; declaration rebuilds preserve expensive caches, while explicit cache invalidation resets text resources.

- **组件与样式能力 | Components and styling**  
  内置 `Button`、`Input`、`Label`、`ScrollArea`、`Scrollbar` 等组件，支持类 utility 的样式属性写法。

- **国际化 | i18n**
  `src/i18n` 整模块收进 `--features i18n`（与 `a11y` 相同）。应用文案用两参数 `tr_default("save", "Save")`；Jadix 组件用 `tr_default_zh(..., "Previous", "上一页")` 带 en-US/zh-CN。其它语言只开 feature、改 `i18n.toml` / `locales/<tag>.toml` 覆盖即可。关 feature 时返回英文源字符串。无障碍桥接需 `--features a11y`。
  The `i18n` module is feature-gated. Apps call two-arg `tr_default`. Jadix chrome ships en-US/zh-CN via `tr_default_zh`. More locales are TOML overlays. AccessKit is `--features a11y`.

- **开发期热重载 | Dev-time hot reload**  
  `jadix_hot_reload_cli` 监听 `crates/` 与 `examples/` 变更，通过本地 TCP 通知运行时刷新模板与资源。

---

## 技术栈 | Tech Stack

- `winit` - windowing and input
- `wgpu` - GPU rendering backend
- `taffy` - layout engine (Flexbox/Grid)
- `glyphon` / `cosmic-text` - text shaping and rendering
- `accesskit` - accessibility tree (`a11y` feature; OS adapter pending winit 0.31)
- `syn` / `quote` / `proc-macro2` - macro infrastructure

Versions are pinned once in the root `[workspace.dependencies]` table. Member crates use `{ workspace = true }`. Adding or bumping a crate follows `development-docs/ops/crate_deps.md`; licenses and duplicate versions are gated by [`deny.toml`](deny.toml) (`cargo deny check licenses bans sources`).

---

## 工作区结构 | Workspace Structure

```text
jadix/
├─ crates/
│  ├─ jadix/                  # 核心运行时与组件系统 | Core runtime and components
│  ├─ jadix_macros/           # #[component] / ui! 宏 | Proc-macro crate
│  ├─ jadix_style_tokens/     # utility 真源 | Shared style tokens
│  ├─ jadix_hot_reload_cli/   # 热重载监听器 | Hot reload watcher
│  └─ jadix_window_state/   # 可选：窗口几何 TOML 持久化 | Optional window-state crate
└─ examples/
   ├─ gallery/              # 组件与能力展示 | Component gallery
   ├─ calculator/           # 体积 / 内存基线 | Size/memory baseline
   ├─ gpu_probe/            # winit+wgpu 驱动探针 | Driver-floor probe
   └─ harmonyos/            # OpenHarmony Ability 入口 | OHOS Ability host
```

---

## 快速开始 | Quick Start

### 1) 环境要求 | Prerequisites

- Rust stable toolchain (`rustup`)
- 支持 `wgpu` 的图形驱动 / A GPU driver compatible with `wgpu`

### 2) 运行示例 | Run the demo

```bash
cargo run -p gallery
# Optional wgpu compute (blur / histogram / particles / backdrop-filter):
# cargo run -p gallery --features compute
```

性能采样是框架的可选 `jadix/performance` feature，默认关闭。Gallery 默认启用其 `performance` feature，负责采样调度、显示文案和监控浮层；框架提供窗口呈现请求计数、CPU 耗时及进程资源原始数据。Windows 支持 CPU/GPU/内存采样，其他平台的不可用资源计数返回 `None`。窗口呈现请求频率不代表显示器扫描输出频率，GPU 和内存资源数据按进程统计。

Gallery 的「布局」/「文本」分区包含可调页宽、栏高和障碍物的 [Pretext 式文字环绕与连续分栏](examples/gallery/src/pages/text_flow.rs) 示例。

桌面窗口支持平台全屏快捷键、单例/多开控制和主窗关闭后独立存活的次窗；配置与生命周期契约见[桌面窗口手册](docs/desktop-windows.md)。

### 3) 启用开发期热重载（可选） | Enable dev hot reload (optional)

在一个终端运行示例，在另一个终端运行监听器：  
Run the demo in one terminal, then run watcher in another:

```bash
cargo run -p jadix_hot_reload_cli -- .
```

默认会连接到 `127.0.0.1:39091`。  
By default, it connects to `127.0.0.1:39091`.

---

## Ubuntu 24.04 arm64 / 树莓派 5 交叉编译 | Cross-compilation

目标为 **64 位 Ubuntu 24.04 Desktop**，Rust target 为 `aarch64-unknown-linux-gnu`。
请按环境选择一种方式；下面的命令均在仓库根目录运行。首次构建建议至少预留 10GB 磁盘空间，内存紧张时使用 `-j 2`。空间不足时可在仓库根目录执行 `cargo clean` 清理构建产物；需保留的基线可执行文件和验收日志先保存到 `target` 之外，清理后重新构建。

| 方式 | 前置条件 | 适用场景 |
|---|---|---|
| [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) | Rust、Zig、cargo-zigbuild | Windows / macOS / Linux；可指定最低 glibc 版本，无需 Docker |
| [cross](https://github.com/cross-rs/cross) | Rust、Docker / Podman 的 Linux 容器环境 | 用容器统一交叉工具链；Windows 建议在 WSL2 中运行并启用 Docker 集成 |
| Ubuntu / WSL2 + GCC | Linux Rust 工具链、GNU arm64 交叉编译器 | 已有 Ubuntu 构建环境，或需要自行配置目标系统库 |

2026-10-16 的验证包使用 **cargo-zigbuild 0.23.0 + Zig 0.16.0 + Rust 1.95.0** 构建，
已检查 AArch64 ELF、最高 glibc 符号要求为 2.39，并在 QEMU 中完成加载至事件循环创建的无桌面启动检查。
`cross` 和 GCC 是可选步骤，本次未执行完整构建；树莓派实机图形验证仍待完成。

### A) cargo-zigbuild

安装 [Zig](https://ziglang.org/download/) 并加入 PATH，然后执行：

```sh
cargo install cargo-zigbuild --locked
rustup target add aarch64-unknown-linux-gnu
cargo zigbuild -j 2 -p gallery --release --locked \
  --target aarch64-unknown-linux-gnu.2.39 \
  --features runtime-logging,jadix/backend-vulkan
```

`.2.39` 指定 Ubuntu 24.04 对应的 glibc 版本；产物目录仍为
`target/aarch64-unknown-linux-gnu/release/gallery`。PowerShell 请将多行命令写成一行，或把行尾 `\` 换为反引号。
Zig 提供 C 工具链和 libc；若启用需要额外系统开发库的 feature（如 `system-tray`），仍需准备 arm64 库和 pkg-config sysroot。

### B) cross

启动 Docker / Podman 后执行：

```sh
cargo install cross --git https://github.com/cross-rs/cross --locked
cross build -j 2 -p gallery --release --locked \
  --target aarch64-unknown-linux-gnu \
  --features runtime-logging,jadix/backend-vulkan
```

`cross` 自动使用目标架构的构建镜像。glibc 基线由镜像决定；如果另加原生库，
需在自定义 cross 镜像中安装 **arm64** 开发包。此方式不声称固定使用 Ubuntu 24.04 sysroot，发布前应检查产物的 glibc 要求。

### C) Ubuntu 24.04 / WSL2 + GCC

以下命令在 Ubuntu shell 中运行（WSL 也需安装 Linux 版 Rust）：

```sh
sudo apt-get update
sudo apt-get install -y gcc-aarch64-linux-gnu g++-aarch64-linux-gnu pkg-config
rustup target add aarch64-unknown-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++ \
cargo build -j 2 -p gallery --release --locked \
  --target aarch64-unknown-linux-gnu \
  --features runtime-logging,jadix/backend-vulkan
```

不要用宿主机 x86_64 的库冒充 arm64 库，也不要仅设置 `PKG_CONFIG_ALLOW_CROSS=1` 绕过检查。
当前 Gallery 默认依赖以动态加载 / Rust 实现为主；增加原生依赖后，应为目标架构配置 `PKG_CONFIG_LIBDIR` / `PKG_CONFIG_SYSROOT_DIR`。

### 打包与树莓派运行 | Package and run

三种方式产物均为 `target/aarch64-unknown-linux-gnu/release/gallery`；设置 `CARGO_TARGET_DIR` 时以该目录为准。
一起分发以下文件，不要复制 Windows 的窗口状态文件：

```text
gallery-ubuntu24.04-arm64/
├── gallery
├── gallery.desktop           # examples/gallery/gallery.desktop
├── theme.toml                # examples/gallery/theme.toml
├── styles.toml               # examples/gallery/styles.toml
├── assets/firefox-logo.svg   # examples/gallery/assets/firefox-logo.svg
├── LICENSE
└── NOTICE
```

在树莓派 5 的 Ubuntu 24.04 **图形桌面终端**中安装运行库并启动：

```sh
sudo apt-get update
sudo apt-get install -y libegl1 libgles2 libgl1-mesa-dri mesa-vulkan-drivers \
  libvulkan1 libwayland-client0 libwayland-cursor0 libwayland-egl1 \
  libxkbcommon0 libxkbcommon-x11-0 libx11-6 libxcursor1 libxi6 libxrandr2 \
  fonts-noto-cjk fonts-noto-color-emoji
chmod +x ./gallery
RUST_LOG=info ./gallery 2>&1 | tee gallery.log
# 分别验证 GLES 与 Vulkan：
JADIX_GPU_BACKEND=gl ./gallery
JADIX_GPU_BACKEND=vulkan ./gallery
# X11 / Wayland 对照需相应桌面会话可用：
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET ./gallery  # X11，保留 DISPLAY
env -u DISPLAY ./gallery                           # Wayland，保留 WAYLAND_DISPLAY
```

字体使用目标系统字体，Lucide 图标已内嵌。文件选择器依赖桌面的 XDG portal 服务。
Gallery 默认在右下角显示实时性能 overlay（FPS、帧间隔、CPU 帧耗时、布局与文字准备耗时），
每 500 ms 更新；点击“性能 / FPS”收起，`JADIX_PERF_OVERLAY=0` 完全关闭采样。
CPU 耗时包含获取表面和提交的等待，并非 GPU 执行时间；静止时显示 Idle，不强制连续渲染。
Windows release 的启动与最大化重启回归见 [scripts/startup-bench.md](scripts/startup-bench.md)。
交叉编译成功表示二进制可构建；树莓派 GPU、窗口、输入法和剪贴板仍需在实机验证。

### 树莓派上的帧率、首次启动与桌面名称

60 FPS 是绘制上限，每帧预算为 16.67 ms；约 23 ms 的帧间隔对应约 43 FPS。
性能面板的 CPU 数字是主线程一帧的墙钟耗时，包含驱动等待；layout/text 属于其中的阶段，不能再叠加。
`top` 默认以单核为 100%，四核 Pi 5 上的 102% 约为一个核心的工作量。

在设备图形桌面终端分别运行 GLES 和 Vulkan 对照（构建时启用 `jadix/backend-vulkan`）：

```sh
JADIX_GPU_BACKEND=gl JADIX_RENDER_PROFILE=1 RUST_LOG=info ./gallery 2>&1 | tee gallery-gl.log
JADIX_GPU_BACKEND=vulkan JADIX_RENDER_PROFILE=1 RUST_LOG=info ./gallery 2>&1 | tee gallery-vulkan.log
```

两次均在基础页以相同窗口尺寸滚动约 10 秒后退出。比较日志中的 adapter 名称、
`render profile` 的 layout/text/encode/acquire/present，以及 `startup stage=…` 的耗时。
启动阶段分别记录设备、表面、各管线、字体/文字渲染器和共享文字资源；
`startup primary_ui_mounted_ms` 记录窗口创建至首次 UI 挂载的总耗时。
它们不等同于显示器首帧可见时间。首次与第二次启动应分别保留日志。

`inxi` 的 `llvmpipe` 表示该 OpenGL 会话使用软件渲染，不能据此确认 Gallery 的 Vulkan adapter。
以 Gallery 的 `gpu backend` / `software adapter` 日志为准；Pi 硬件渲染通常显示 V3D/V3DV。
自动选择会先尝试各后端的硬件 adapter，再接受软件 adapter；`JADIX_GPU_BACKEND`
调整尝试顺序，不能强制不存在的硬件驱动出现。全为软件渲染时检查当前会话的
`/dev/dri` 访问与 `LIBGL_ALWAYS_SOFTWARE`、`MESA_LOADER_DRIVER_OVERRIDE` 等环境覆盖。

桌面显示名称由应用 ID 和 desktop entry 关联，独立于 `top` 中的进程名。
Wayland 未显式配置 ID 时使用可执行文件名 `gallery`。在分发目录执行以下操作安装入口
（二进制和分发目录应保留在此位置）：

```sh
mkdir -p "$HOME/.local/share/applications" "$HOME/.local/bin"
ln -sfn "$(pwd)/gallery" "$HOME/.local/bin/gallery"
cp gallery.desktop "$HOME/.local/share/applications/gallery.desktop"
```

确保 `$HOME/.local/bin` 位于桌面会话的 PATH。随后从应用菜单启动 **Jadix Gallery**。
已有窗口可能需要关闭后重新启动才能更新名称。

Ubuntu 智能拼音的候选文字按系统提供的 preedit 实时显示；`Preedit(_, None)` 隐藏光标，
提供光标位置时恢复光标，真正的 Commit 事件才写入已提交值。使用
`JADIX_IME_DIAG=1 RUST_LOG=jadix=debug ./gallery` 可记录事件；该日志包含输入内容，分享前请脱敏。
可用 `file gallery`、`readelf -d gallery`、`readelf --version-info gallery` 检查 AArch64 架构、动态依赖和 glibc 要求。

---

## 最小示例 | Minimal Example

```rust
#![allow(non_snake_case)]

use jadix::component;
use jadix::core::reactivity::use_signal;
use jadix::ui;

#[component]
fn Counter() -> jadix::core::tree::Element {
    let count = use_signal("count", || 0);

    ui! {
        <VStack gap="0.5rem" p="1rem">
            <Label>{ format!("Count: {}", count.get()) }</Label>
            <Button on_click={move || count.set(count.get() + 1)}>
                "Increment"
            </Button>
        </VStack>
    }
}

fn main() {
    jadix::app::run(|| {
        ui! { <Counter /> }
    });
}
```

同一棵树也可用 **Builder（无类 HTML 标签）** 拼装，并与 `ui!` 互嵌，例如：

```rust
use jadix::components::button;
use jadix::core::tree::Element;
use jadix::primitives::r#box::vstack;
use jadix::ui;

fn toolbar() -> Element {
    Element::from(
        vstack().gap("0.5rem").child(
            // Builder 子树嵌进布局；亦可反过来在 ui! 里写 { Element::from(button(…)) }
            Element::from(button("save").variant("outline")),
        ),
    )
}

// 标签树中嵌入 Builder 产物：
// ui! { <VStack>{ toolbar() }</VStack> }
```

---

## 细粒度原语（阶段 3/4）| Fine-grained Primitives (Stage 3/4)

Builder 文本入口直接接收 Signal 或闭包；`text(count)`、`TextBuilder::new(count)` 与 `label("id").text(count)` 都走叶子绑定。`get()` 会订阅当前 effect/Memo，但把其结果传给普通 Builder 不会另建叶子绑定；`ui!` 可以将 tracked 表达式自动降为叶子。Memo / StoreField 用闭包：`text(move || memo.get().to_string())`。

```rust
use jadix::prelude::*;

let count = create_signal(0);
let row: Element = vstack()
    .child(text(count))
    .child(text(move || format!("Count: {}", count.get())))
    .child(button("increment").text("+").on_click(move || count.update(|n| *n += 1)))
    .into();
```

以上代码放在 `app().mount(...)` 的构建闭包或组件体中。匿名叶子通过 Owner、调用位置和构建内序号自动隔离，同址 helper/固定顺序循环无需手写叶子 ID；可重排列表仍用业务 key。Store 整值 `set/update/produce` 比较已登记字段的投影，只通知变化路径；字段写入只检查相关路径。每次 Store 写入合批根与路径通知，多字段写入可再包 `batch`。完整契约与成本说明见[响应式手册](development-docs/ops/reactivity_and_hot_reload.md)。

- **`reactive!`**：为一段子树创建独立 effect，并通过稳定 anchor 做子树级 reconcile。  
  Use `reactive!(id, || ui! { ... })` to localize updates to a subtree.
- **`create_memo`**：派生只读值，依赖变化时重算；读者通过 `.get()` 订阅 memo 输出（见 `crates/jadix/src/core/reactivity.rs`）。
- **`show`**：条件渲染辅助（`when == false` 时返回空节点）。  
  Conditional rendering helper that returns an empty element when false.
- **`<For>` / `for_keyed!` / `for_keyed`**：列表渲染；推荐标签式 `<For>`，需要显式 key 时传 `key={...}`。  
  List rendering with optional explicit keys (`key={...}`) for stable reconciliation.

```rust
use jadix::core::reactivity::use_signal;
use jadix::ui;

fn FineGrainedDemo() -> jadix::core::tree::Element {
    let selected = use_signal("selected", || "default".to_string());
    let items = vec!["A", "B", "C"];

    ui! {
        <VStack gap="0.5rem">
            <Reactive id="demo:panel">
                <VStack gap="0.5rem">
                    <Show when={selected.get() != "default"}>
                        <Label>"Changed"</Label>
                    </Show>
                    <For each={items.clone()} key={*item}>
                        <Label>{ *item }</Label>
                    </For>
                </VStack>
            </Reactive>
        </VStack>
    }
}
```

---

## 架构目标 | Architecture Goals

网络采用与媒体相同的外部适配边界：独立 `jadix_net` 定义 HTTP 请求/响应、命令/事件及状态契约，只依赖本地 std-only `jadix_tasks`；可选 `net` / `net-json` 提供 UI 线程回投与 owner 生命周期桥接。实际网络、TLS、运行时及原生取消由应用的第三方库适配器负责，不内置 Reqwest。见[网络适配契约](crates/jadix_net/README.md)。

异步采用同步 UI 核心与可插拔后台执行：`TaskScope` 统一 owner 取消、Future/阻塞任务、窗口作用域恢复和有界事件订阅；资源加载复用既有 Resource/Suspense。执行器、协议后端与共享资产缓存的实现边界见[异步架构](docs/async-architecture.md)与[公共任务契约](crates/jadix_tasks/README.md)。

媒体 UI 提供无默认控件的 Audio / Video 原语，以及 shadcn 风格的 AudioPlayer / VideoPlayer 和可组合控制零件。独立 jadix_media 仅定义标准库契约；解码、设备输出、播放控制执行、时钟与音画同步由外部库负责。双 API、接入示例与生命周期见[媒体组件文档](crates/jadix_media/README.md)。Gallery 绘图页可查看受控状态与命令演示。

- **中文**
  - 保留模式内部实现 + 声明式组件 API。
  - 信号驱动的局部更新（子树 reconcile + 按需 `request_redraw`）；整帧 GPU 绘制（脏区 / scissor 局部重绘尚未落地）。
  - 面向现代 UI 的组件体系与样式表达。
  - 持续提升开发体验（热重载、可维护宏系统、示例完善）。

- **English**
  - Retained internals with a declarative component API.
  - Signal-driven partial updates (subtree reconcile + on-demand `request_redraw`); full-frame GPU present (dirty-region / scissor redraw is not implemented yet).
  - Modern component system and style expression.
  - Better developer experience (hot reload, maintainable macros, richer examples).

---

## 路线图（简版） | Roadmap (Short)

- 完善组件分层与复用（primitives/components patterns）
- 加强宏能力与错误提示质量
- 优化布局与渲染热路径性能
- 扩展示例覆盖面
- 持续推进无障碍与跨平台细节

---

## 组件作者约定 | Component authoring

主题支持 shadcn、Microsoft Fluent、Google MD3 和 Liquid Glass 四种风格，覆盖颜色、排版、尺寸、圆角、阴影、组件 slot 及玻璃光学材质。配置与优先级见[主题系统](crates/jadix/THEMES.md)，Gallery 的主题工作台可实时切换和调参。

新增 `#[component]`、改 RSX 属性或宏 `own_prop` 时，遵循 [docs/COMPONENT_LIBRARY.md](docs/COMPONENT_LIBRARY.md) §1（属性分层与改名）。CSS 短名（`w` / `h` / `class`）留给根节点 chrome；领域测量用 `plot_h` / `col_w` / `rail_w` 这类专名。

---

## 贡献与反馈 | Contributing & Feedback

- **中文**：欢迎提交 Issue / PR 讨论 API、性能与组件设计。当前版本迭代较快，建议先在 Issue 中对齐方向。  
- **English**: Issues and PRs are welcome for API, performance, and component design discussions. Since the project is evolving quickly, opening an issue first is recommended.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE). Bundled fonts and icon assets are listed in [NOTICE](NOTICE).

Dependency licenses, bans, and crate sources are audited with [`deny.toml`](deny.toml) (`cargo deny check licenses bans sources`). Version policy lives in `development-docs/ops/crate_deps.md`.
