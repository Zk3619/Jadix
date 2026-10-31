# jadix_clipboard

[English](README.md) | [简体中文](README.zh.md)

Cross-platform clipboard for Jadix (text, html, rtf, files, optional image). Fork of clipboard-rs aligned to workspace Windows/objc pins.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.1`.**

## Installation

Add the package to your application:

```toml
[dependencies]
jadix_clipboard = "=0.1.1"
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE), [NOTICE](NOTICE) and dependency notices.
