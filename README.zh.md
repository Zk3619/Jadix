# Jadix

[English](README.md) | [简体中文](README.zh.md)

Rust 原生 UI 框架，提供响应式状态、GPU 绘制与可选声明式宏。
使用普通 Rust Builder 构建应用，按需加入独立产品库，通过 Cargo feature 选择能力。

> **Alpha。** API 仍在演进。平台要求、SDK 配置与应用打包方式见[构建指南](docs/BUILD.zh.md)。

## 核心能力

- 原生窗口、Flex/Grid 布局、文本塑形与 GPU 绘制，不依赖浏览器 UI 层。
- Rust Builder 与可选 `ui!` / `#[component]` 宏共用运行时。
- Signal、Store、Owner 作用域及保留元素树支持响应式更新。
- 控件、Lucide 图标和高层拖拽按需依赖，框架不反向依赖产品。
- 图形后端、动画、文档、无障碍、运行时及网络适配均有明确配置边界。
- Debug 工具可检查普通元素、调整样式、热更已支持的静态声明；任意 Rust 业务逻辑仍需重编译。

## 从源码运行

安装近期稳定版 Rust 与目标平台 SDK，以各包的 `rust-version` 为工具链下限。
Windows 需要 MSVC 和 Windows SDK；其他平台需准备对应窗口系统与图形依赖。

```sh
git clone https://github.com/Zk3619/Jadix.git
cd Jadix
cargo run -p gallery
```

[构建指南](docs/BUILD.zh.md) 覆盖 Windows、macOS、Linux、Android、iOS 和 OHOS，说明架构选择、SDK 准备、release 构建、打包签名、设备/模拟器验证及常见问题。Android/OHOS 提供独立的 ARM64/x86_64 构建，OHOS 进一步区分手机与桌面配置；iOS 尚需实现 Gallery 应用宿主。

## 最小应用

Windows 应用的依赖配置：

```toml
[dependencies]
jadix = { version = "0.1.0", default-features = false, features = ["backend-dx12"] }
```

```rust,no_run
use jadix::prelude::*;

fn main() {
    jadix::run(|| Element::new().child(text("Hello, Jadix")));
}
```

该 Builder 配置不需要 UI 过程宏。

## 按职责选择依赖

| 库 | 职责 |
| --- | --- |
| [jadix](crates/jadix/README.zh.md) | 运行时、元素、布局、绘制、窗口与可选开发工具 |
| [jadix_components](crates/jadix_components/README.zh.md) | 原生产品控件，可选宏 |
| [jadix_lucide](crates/jadix_lucide/README.zh.md) | 静态图标与按需动画 |
| [jadix_dnd](crates/jadix_dnd/README.zh.md) | 高层拖拽产品 |
| [jadix_core](crates/jadix_core/README.zh.md) | 无 UI 依赖的 Owner、Signal 和 Store |
| [jadix_animation](crates/jadix_animation/README.zh.md) | 动画时钟、算法、时间线及可选路径运算 |
| [jadix_document](crates/jadix_document/README.zh.md) | 文档快照与索引 |
| [jadix_tasks](crates/jadix_tasks/README.zh.md)、[jadix_net](crates/jadix_net/README.zh.md)、[jadix_tokio](crates/jadix_tokio/README.zh.md) | 任务、网络契约与可选 Tokio 适配 |

窗口、GPU、文本与布局维护分叉统一位于 `crates/`，包名分别为 `jadix_winit`、`jadix_wgpu`、`jadix_cosmic_text`、`jadix_glyphon`、`jadix_taffy`。保留上游许可并使用依赖图中的版本，不能直接用相似名称的上游包替换。`vendor/` 目录只保留对照基准使用的 GPUI 补丁依赖。

## 文档

- [项目文档索引](docs/README.md)：维护中的权威使用说明。
- [Crate 清单](docs/CRATES.zh.md)（[English](docs/CRATES.md)）：各包职责、依赖关系与维护实现。
- [编译与打包](docs/BUILD.zh.md)（[English](docs/BUILD.md)）：平台前置条件、架构、构建命令、验证与已知限制。
- [Feature](docs/FEATURES.md)、[状态管理](docs/state-management.md)、[异步集成](docs/async-architecture.md)、[远程资源](docs/remote-assets.md)。
- [组件作者接口](docs/component-authoring.md)、[桌面窗口](docs/desktop-windows.md)、[应用图标](docs/application-icons.md)、[WebView](docs/webview.md)、[DevTools](docs/devtools.md)。
- [API 迁移](docs/migration.md)：导入路径、feature 选择与资源配置。

`docs/` 仅放项目权威说明，可执行示例和配置放在 `examples/`。
问题反馈请附系统、后端、工具链和最小复现。

## 许可证

Jadix 自有框架代码采用 [Apache-2.0](LICENSE)。依赖与维护叉保留各自的许可及 NOTICE。
