# jadix_hot_reload

[English](README.md) | [简体中文](README.zh.md)

Framework-neutral hot-reload protocol and receiver.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.1`.**

## Installation

Add the package to your application:

```toml
[dependencies]
jadix_hot_reload = "=0.1.1"
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.

## Optional development CLI

```sh
cargo install jadix_hot_reload --version "=0.1.1" --features cli --bin jadix-hot-reload
jadix-hot-reload --help
```
