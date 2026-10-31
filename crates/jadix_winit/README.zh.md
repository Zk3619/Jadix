# jadix_winit

[English](README.md) | [简体中文](README.zh.md)

Jadix 窗口门面与统一平台后端依赖图，包含 OpenHarmony；内部后端通常由门面传递选择。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.31.0-beta.3-jadix.1`。**

## 安装

在应用中添加依赖：

```toml
[dependencies]
jadix_winit = "=0.31.0-beta.3-jadix.1"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：`x11`, `wayland`, `wayland-dlopen`, `wayland-csd-adwaita`。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 详细用法

原有[包使用指南](GUIDE.md)保留 API、平台及来源细节，可能使用其他语言。

## 文档

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。

## 发布边界

核心类型和各平台后端以内部模块随本包一起发布，不再独立发布 `jadix_winit_*` 后端包。原始来源、许可证和平台补丁记录保留在 `upstream/`。
