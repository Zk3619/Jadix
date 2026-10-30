# jadix_winit_wayland

[English](README.md) | [简体中文](README.zh.md)

Winit wayland 平台层的 Jadix 维护分发；普通应用通过 jadix_winit 门面选择兼容版本，不直接绑定内部后端。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.31.0-beta.3-jadix.1`。** 本地归档和清单不代表 registry 已可安装。

## 安装

使用目标 registry 中可用的对应版本：

```toml
[dependencies]
winit_wayland = { package = "jadix_winit_wayland", version = "=0.31.0-beta.3-jadix.1" }
```

发布前使用源码工作区。普通应用从同一 registry 解析依赖图，不复制本仓私有 path／git／patch；平台专用包仍需对应 SDK。

## Feature 与边界

默认开关：`dlopen`, `csd-adwaita`。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 维护叉与来源

本包单独以 Jadix 名称分发，不表示上游发布或背书。使用兼容的 Jadix 依赖图并保留原署名。

上游：https://github.com/Zk3619/Jadix。

## 文档与贡献

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)说明用法、迁移、测试和发布。保留测试发现入口，私有模块测试与公开契约分层；包配置变化同步两种 README。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。
