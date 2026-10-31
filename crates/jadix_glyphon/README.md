# jadix_glyphon

[English](README.md) | [简体中文](README.zh.md)

Fast, simple 2D text rendering for wgpu.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

Upstream source baseline: `glyphon 0.12.0`.

## Installation

Add the package to your application:

```toml
[dependencies]
glyphon = { package = "jadix_glyphon", version = "=0.1.0" }
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Maintenance fork

This is a separately named Jadix distribution, not an upstream release or endorsement. Use the compatible Jadix dependency graph and preserve original attribution.

Upstream: https://github.com/grovesNL/glyphon.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

MIT OR Apache-2.0 OR Zlib. Preserve [LICENSE-APACHE](LICENSE-APACHE), [LICENSE-MIT](LICENSE-MIT), [LICENSE-ZLIB](LICENSE-ZLIB) and dependency notices.
