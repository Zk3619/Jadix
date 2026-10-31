# jadix_glyphon

[English](README.md) | [简体中文](README.zh.md)

glyphon GPU 文本绘制的 Jadix 维护分发，服务于固定版本的 Jadix 依赖图。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

上游源码基线：`glyphon 0.12.0`。

## 安装

在应用中添加依赖：

```toml
[dependencies]
glyphon = { package = "jadix_glyphon", version = "=0.1.0" }
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 维护叉与来源

本包单独以 Jadix 名称分发，不表示上游发布或背书。使用兼容的 Jadix 依赖图并保留原署名。

上游：https://github.com/grovesNL/glyphon。

## 文档

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

MIT OR Apache-2.0 OR Zlib。保留 [LICENSE-APACHE](LICENSE-APACHE), [LICENSE-MIT](LICENSE-MIT), [LICENSE-ZLIB](LICENSE-ZLIB) 及依赖的来源声明。
