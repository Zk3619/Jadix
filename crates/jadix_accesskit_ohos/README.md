# jadix_accesskit_ohos

[English](README.md) | [简体中文](README.zh.md)

Pinned AccessKit OpenHarmony adapter backport for Jadix.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

## Installation

Add the package to your application:

```toml
[dependencies]
jadix_accesskit_ohos = "=0.1.0"
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `api-15`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

MIT OR Apache-2.0. Preserve [LICENSE-APACHE](LICENSE-APACHE), [LICENSE-MIT](LICENSE-MIT), [LICENSE.chromium](LICENSE.chromium) and dependency notices.
