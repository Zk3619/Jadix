# Jadix 项目文档

本目录仅维护项目权威使用说明、配置契约、架构边界和贡献规范。
可执行示例与配置归 `examples/`；研究审计、阶段台账和原始证据另行保存。正常消费不依赖另行检出的开发文档仓。

| 文档 | 内容 |
| --- | --- |
| [编译与打包](BUILD.zh.md) / [English](BUILD.md) | 六平台架构、SDK、release 构建、签名安装与问题排查 |
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
| [测试规范](testing.md) | 归属、发现入口与执行条件 |
| [发布流程](releasing.md) | 归档、双语文档、权限与真实仓外验证 |

包内手册：[动画](../crates/jadix/MOTION.md)、[缩放预览](../crates/jadix/SCALE_PREVIEW.md)、[主题](../crates/jadix/THEMES.md)、[文档编辑](../crates/jadix/DOCUMENT_EDITING.md)、[文本视图](../crates/jadix/TEXT_DOCUMENT_VIEWS.md)。
