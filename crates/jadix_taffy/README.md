# jadix_taffy

[English](README.md) | [简体中文](README.zh.md)

A flexible UI layout library.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

Upstream source baseline: `taffy 0.15.0`.

## Installation

Add the package to your application:

```toml
[dependencies]
taffy = { package = "jadix_taffy", version = "=0.1.0" }
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `std`, `taffy_tree`, `flexbox`, `flexbox_balance`, `grid`, `block_layout`, `float_layout`, `calc`, `content_size`, `detailed_layout_info`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Maintenance fork

This is a separately named Jadix distribution, not an upstream release or endorsement. Use the compatible Jadix dependency graph and preserve original attribution.

Upstream: https://github.com/DioxusLabs/taffy.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

MIT. Preserve [LICENSE](LICENSE) and dependency notices.
