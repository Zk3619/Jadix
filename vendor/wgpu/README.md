# jadix_wgpu

[English](README.md) | [简体中文](README.zh.md)

Cross-platform, safe, pure-rust graphics API.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `30.0.0`.** Local archives and manifests do not prove registry availability.

## Installation

Use the version available in the target registry:

```toml
[dependencies]
wgpu = { package = "jadix_wgpu", version = "=30.0.0" }
```

Before publication, use the source workspace. Consumers use one registry graph without copying private path/git/patch overrides. Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `std`, `parking_lot`, `dx12`, `metal`, `gles`, `vulkan`, `wgsl`, `webgpu`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Maintenance fork

This is a separately named Jadix distribution, not an upstream release or endorsement. Use the compatible Jadix dependency graph and preserve original attribution.

Upstream: https://github.com/gfx-rs/wgpu.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation and contributions

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers usage, migration, testing and releases. Preserve test discovery and keep private module tests separate from public integration contracts. Update both README languages when changing package setup.

## License

MIT OR Apache-2.0. Preserve [LICENSE.APACHE](LICENSE.APACHE), [LICENSE.MIT](LICENSE.MIT) and dependency notices.
