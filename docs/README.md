# Jadix 项目文档

本目录提供 Jadix 的使用指南、配置契约、平台接入和 API 参考。可执行示例与配置位于 `examples/`。

| 文档 | 内容 |
| --- | --- |
| [编译与打包](BUILD.zh.md) / [English](BUILD.md) | 六平台架构、SDK、release 构建、签名安装与问题排查 |
| [Crate 清单](CRATES.zh.md) / [English](CRATES.md) | 27 个库 crate 的职责、版本、依赖关系与维护实现 |
| [Feature](FEATURES.md) | 默认值、可选能力与后端 |
| [状态管理](state-management.md) | Signal、Store 与 Owner |
| [异步与网络](async-architecture.md) | 执行器、取消、背压及运行时适配 |
| [远程资源](remote-assets.md) | 字节缓存与鉴权／失效边界 |
| [组件作者接口](component-authoring.md) | Builder、宏、共享样式与宿主接入 |
| [样式与对齐](styling.md) | Flex/Grid、文本与绘制边界 |
| [桌面窗口](desktop-windows.md) | 全屏、实例与生命周期 |
| [DevTools](devtools.md) | Inspector、Sources 与 Debug 热更 |
| [应用图标](application-icons.md) | 生成与平台接入；[配置示例](../examples/config/app-icons.toml) |
| [WebView](webview.md) | 系统 WebView 接入与限制 |
| [迁移说明](migration.md) | 产品拆分、feature、宏路径与维护叉 |

包内手册：[动画](../crates/jadix/MOTION.md)、[缩放预览](../crates/jadix/SCALE_PREVIEW.md)、[主题](../crates/jadix/THEMES.md)、[文档编辑](../crates/jadix/DOCUMENT_EDITING.md)、[文本视图](../crates/jadix/TEXT_DOCUMENT_VIEWS.md)。
