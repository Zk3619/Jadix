# jadix_components

[English](README.md) | [简体中文](README.zh.md)

原生产品控件，函数、Builder 与可选 RSX 共用实现；只在需要时启用 macros。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

## 安装

在应用中添加依赖：

```toml
[dependencies]
jadix_components = "=0.1.0"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：`gpu`，保留完整产品行为；RSX 继续通过 `macros` 显式选择。禁用默认 features 后按需选择宏与主题，使用[便携样例](../../examples/portable-contract/README.md)所示接口。便携写法投影为强类型描述，由 [jadix_native 平台宿主](../jadix_native/README.md)创建系统／DOM 控件并声明能力。登记子集含基本控件、Dialog、基本组合 Select 以及 Popover Builder／组合 RSX；Popover 由 DOM 与 Android PopupWindow 宿主支持，UIKit 明确拒收。完整依赖关系以 [Cargo.toml](Cargo.toml) 为准，存在开关不代表平台行为已完成验证。

## 文档

便携 Tabs 支持 `tabs` Builder、`TabsList`／`TabsTrigger`／`TabsContent` 组合 RSX，
共用受控值、惰性面板、水平／垂直方向键、Home／End、禁用项跳过与原生焦点请求。
非活动面板卸载原生控件，保留 Element 与业务信号，重新激活时用新原生身份恢复文本。
条件可见性变更等待当前已投影文本的组合输入结束；通用 portal 与其他任意结构删除不由此策略保护。
跨面板原生选区／撤销、单一 Tab 停靠、专用 Tab／TabPanel 辅助技术 provider、主题变体
及物理 IME 验收仍未完成。`native-tabs` 提供 Windows 实际产品入口。

GPU 专属控件、文档、媒体、动画与缩放预览开关会同时选择 GPU 组件接口及对应运行能力；仅启用宏与主题仍可使用不依赖 GPU／窗口后端的便携接口。

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。
