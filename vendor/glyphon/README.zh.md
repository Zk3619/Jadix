# jadix_glyphon

[English](README.md) | [简体中文](README.zh.md)

glyphon GPU 文本绘制的 Jadix 维护分发，服务于固定版本的 Jadix 依赖图。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；待发布版本 `0.12.0`，尚未发布。** 本地归档和清单不代表 registry 已可安装。

## 安装

待对应版本实际发布后：

```toml
[dependencies]
glyphon = { package = "jadix_glyphon", version = "=0.12.0" }
```

发布前使用源码工作区。普通应用从同一 registry 解析依赖图，不复制本仓私有 path／git／patch；平台专用包仍需对应 SDK。

## Feature 与边界

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 维护叉与来源

本包单独以 Jadix 名称分发，不表示上游发布或背书。使用兼容的 Jadix 依赖图并保留原署名。

上游：https://github.com/grovesNL/glyphon。

## 文档与贡献

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)说明用法、迁移、测试和发布。保留测试发现入口，私有模块测试与公开契约分层；包配置变化同步两种 README。

## 许可证

MIT OR Apache-2.0 OR Zlib。保留 [LICENSE-APACHE](LICENSE-APACHE), [LICENSE-MIT](LICENSE-MIT), [LICENSE-ZLIB](LICENSE-ZLIB) 及依赖的来源声明。
