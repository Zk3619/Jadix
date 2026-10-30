# Jadix

[English](README.md) | [简体中文](README.zh.md)

Rust 原生 UI 框架，提供响应式状态、GPU 绘制与可选声明式宏。
使用普通 Rust Builder 构建应用，按需加入独立产品库，通过 Cargo feature 选择能力。

> **Alpha · 尚未发布。** API 仍在演进。清单版本是待发布版本，不代表 crates.io 已可安装。各平台、架构的验证程度不同，已验证配置及待完成的宿主、打包和设备工作见[构建指南](docs/BUILD.zh.md)。

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

待相关版本实际发布后，Windows 应用可采用：

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

该 Builder 配置不需要 UI 过程宏。发布前使用工作区示例，不将本仓私有 path／git／patch 复制进普通应用。

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

`vendor/` 中的维护叉采用明确的 Jadix 包名。保留其许可并使用依赖图中的版本，不能直接用相似名称的上游包替换。

## 文档与贡献

- [项目文档索引](docs/README.md)：维护中的权威使用说明。
- [编译与打包](docs/BUILD.zh.md)（[English](docs/BUILD.md)）：平台前置条件、架构、构建命令、验证与已知限制。
- [Feature](docs/FEATURES.md)、[状态管理](docs/state-management.md)、[异步集成](docs/async-architecture.md)、[远程资源](docs/remote-assets.md)。
- [组件作者接口](docs/component-authoring.md)、[桌面窗口](docs/desktop-windows.md)、[应用图标](docs/application-icons.md)、[WebView](docs/webview.md)、[DevTools](docs/devtools.md)。
- [迁移说明](docs/migration.md)、[测试规范](docs/testing.md)、[发布流程](docs/releasing.md)。

`docs/` 仅放项目权威说明，可执行示例和配置放在 `examples/`。
大型私有测试归属对应 Rust 模块，公开契约放在 crate 的 `tests/`，Python 契约放在 `scripts/tests/`。
开发审计与原始证据单独维护，不作为构建输入。贡献时保留测试发现入口并同步两种 README；问题反馈附系统、后端、工具链和最小复现。

## 许可证

Jadix 自有框架代码采用 [Apache-2.0](LICENSE)。依赖与维护叉保留各自的许可及 NOTICE。
