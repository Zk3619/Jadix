# jadix_wgpu

[English](README.md) | [简体中文](README.zh.md)

Cross-platform, safe, pure-rust graphics API.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

Upstream source baseline: `wgpu 30.0.0`.

## Installation

Add the package to your application:

```toml
[dependencies]
wgpu = { package = "jadix_wgpu", version = "=0.1.0" }
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `std`, `parking_lot`, `dx12`, `metal`, `gles`, `vulkan`, `wgsl`, `webgpu`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Maintenance fork

This is a separately named Jadix distribution, not an upstream release or endorsement. Use the compatible Jadix dependency graph and preserve original attribution.

Upstream: https://github.com/gfx-rs/wgpu.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

MIT OR Apache-2.0. Preserve [LICENSE.APACHE](LICENSE.APACHE), [LICENSE.MIT](LICENSE.MIT) and dependency notices.

## Distribution

Core, HAL, shared types, the Naga bridge and the modified Naga implementation ship as modules in this one package. Platform feature helpers are folded into the package manifest and build configuration. Use `wgpu::hal` and `wgpu::naga` for the supported low level integrations. Original licenses and patch notes are included under `upstream/`.

When building WebGPU/WebGL with the workspace's locked `web-sys 0.3.91`, set `RUSTFLAGS="--cfg web_sys_unstable_apis"` to expose its `VideoFrame` bindings. Later `web-sys` releases may expose these bindings without that flag.
