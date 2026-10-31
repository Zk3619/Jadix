# jadix_wgpu

[English](README.md) | [简体中文](README.zh.md)

wgpu GPU API 门面的 Jadix 维护分发，服务于固定版本的 Jadix 依赖图。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

上游源码基线：`wgpu 30.0.0`。

## 安装

在应用中添加依赖：

```toml
[dependencies]
wgpu = { package = "jadix_wgpu", version = "=0.1.0" }
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：`std`, `parking_lot`, `dx12`, `metal`, `gles`, `vulkan`, `wgsl`, `webgpu`。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 维护叉与来源

本包单独以 Jadix 名称分发，不表示上游发布或背书。使用兼容的 Jadix 依赖图并保留原署名。

上游：https://github.com/gfx-rs/wgpu。

## 详细用法

原有[包使用指南](GUIDE.md)保留 API、平台及来源细节，可能使用其他语言。

## 文档

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

MIT OR Apache-2.0。保留 [LICENSE.APACHE](LICENSE.APACHE), [LICENSE.MIT](LICENSE.MIT) 及依赖的来源声明。

## 发布边界

Core、HAL、公共类型、Naga 桥接及修改后的 Naga 以模块随本包一起发布；平台 feature 辅助包收进本包配置。底层集成使用 `wgpu::hal`、`wgpu::naga`，不再依赖独立的 `jadix_wgpu_*`／`jadix_naga` 包。原始许可和补丁记录保留在 `upstream/`。

使用工作区锁定的 `web-sys 0.3.91` 构建 WebGPU/WebGL 时，设置 `RUSTFLAGS="--cfg web_sys_unstable_apis"` 以启用其 `VideoFrame` 绑定。较新的 `web-sys` 版本可能已直接公开这些绑定。
