# jadix_hot_reload_cli

[English](README.md) | [简体中文](README.zh.md)

Debug 文件监听与静态模板补丁发送命令；运行时接收是独立 Debug feature，不支持的 Rust 变更需重编译。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；待发布版本 `0.1.0`，尚未发布。** 本地归档和清单不代表 registry 已可安装。

## 安装

待对应版本实际发布后：

```sh
cargo install jadix_hot_reload_cli --version "=0.1.0"
jadix-hot-reload --help
```

发布前使用源码工作区。普通应用从同一 registry 解析依赖图，不复制本仓私有 path／git／patch；平台专用包仍需对应 SDK。

## Feature 与边界

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 详细用法

原有[包使用指南](GUIDE.md)保留 API、平台及来源细节，可能使用其他语言。

## 文档与贡献

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)说明用法、迁移、测试和发布。保留测试发现入口，私有模块测试与公开契约分层；包配置变化同步两种 README。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。
