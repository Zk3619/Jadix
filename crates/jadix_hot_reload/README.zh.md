# jadix_hot_reload

[English](README.md) | [简体中文](README.zh.md)

框架无关的热更协议、静态模板表示与接收端契约；不是文件监听命令，不解释任意 Rust。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.1`。**

## 安装

在应用中添加依赖：

```toml
[dependencies]
jadix_hot_reload = "=0.1.1"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 详细用法

原有[包使用指南](GUIDE.md)保留 API、平台及来源细节，可能使用其他语言。

## 文档

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。

## 可选开发命令行

```sh
cargo install jadix_hot_reload --version "=0.1.1" --features cli --bin jadix-hot-reload
jadix-hot-reload --help
```
