# jadix_layout

[English](README.md) | [简体中文](README.zh.md)

CSS 尺寸、逻辑单位及可复用 Grid 轨道算法，不提供应用运行时。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

## 安装

在应用中添加依赖：

```toml
[dependencies]
jadix_layout = "=0.1.0"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 文档

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。

共享 CPU 呈现布局位于 `presentation::PresentationLayout`：NodeStyle 弹性尺寸、宿主提供并缓存的叶节点测量、浮层几何。native 保留 NativeLayout 别名，TUI 提供 grapheme 字符格测量；不引入 OS 视图、GPU 或窗口 App 运行时。

PresentationLayout 跨快照保留 Taffy 身份、子列表和计算缓存；样式、内容与结构变化仅使相关节点失效，环境 revision 变化清理布局与测量缓存。测量和几何全部成功后才发布 frames。LayoutStats 新增引擎节点创建、复用、移除、更新及子列表替换计量。release_layout_cache() 只释放 Taffy 存储，保留有界宿主测量与调用方状态。输入验证、同步和 frame 收集仍扫描全树。
