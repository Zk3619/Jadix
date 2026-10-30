# jadix_clipboard

[English](README.md) | [简体中文](README.zh.md)

与 Jadix Windows／Objective-C 依赖栈对齐的跨平台剪贴板维护叉，支持文本、HTML、RTF、文件及可选图像。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.1`。** 本地归档和清单不代表 registry 已可安装。

## 安装

使用目标 registry 中可用的对应版本：

```toml
[dependencies]
jadix_clipboard = "=0.1.1"
```

发布前使用源码工作区。普通应用从同一 registry 解析依赖图，不复制本仓私有 path／git／patch；平台专用包仍需对应 SDK。

## Feature 与边界

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 文档与贡献

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)说明用法、迁移、测试和发布。保留测试发现入口，私有模块测试与公开契约分层；包配置变化同步两种 README。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE), [NOTICE](NOTICE) 及依赖的来源声明。
