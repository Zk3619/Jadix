# Jadix 应用接入手册

> 此目录保留应用使用／集成文档；项目状态、执行和设计统一从 [development-docs](../development-docs/README.md) 进入。本页不声明所有平台均已验收。

| 文档 | 内容与边界 |
| --- | --- |
| [WebView](webview.md) | 系统 WebView 接入、平台要求、生命周期及实际验证限制 |
| [异步与网络](async-architecture.md) | TaskScope、owner 桥与外部适配；设计项和已有接口分开 |
| [桌面窗口](desktop-windows.md) | 桌面作者 API／行为；开发期契约补充见 [窗口规范](../development-docs/ops/native_window_and_menu.md) |
| [DevTools 与热重载](devtools.md) | Gallery 使用与限制；内部机制见 [开发工具手册](../development-docs/ops/devtools.md) |
| [ScalePreview](../crates/jadix/SCALE_PREVIEW.md) | 框架级逻辑视口缩放；支持非 DevTools 场景与独立浮层宿主 |
| [应用图标与托盘](application-icons.md) | 产品集成；配置示例 [app-icons.example.toml](app-icons.example.toml) |
| [自定义组件作者接口](component-authoring.md) | 框架共享样式／槽位、组件宏的显式运行时路径与兼容边界 |
| [Feature 配置清单](FEATURES.md) | 已实现能力、默认值与核心保留范围；开发期候选/拆分/测试与文档治理见 development-docs |

源码、包或 feature 变化时同步使用示例；拆分迁移按 R2／R4 的实际发布入口更新，不能预写尚不存在的 import 为当前可用。
