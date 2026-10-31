# jadix_window_materials

[English](README.md) | [简体中文](README.zh.md)

原生窗口材质集成，平台效果依赖相应系统 API，不承诺所有平台行为相同。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

## 安装

在应用中添加依赖：

```toml
[dependencies]
jadix_window_materials = "=0.1.0"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 文档

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。
