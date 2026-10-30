# jadix_hot_reload_cli

[English](README.md) | [简体中文](README.zh.md)

Development watcher and template patch sender for Jadix.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.** Local archives and manifests do not prove registry availability.

## Installation

Use the version available in the target registry:

```sh
cargo install jadix_hot_reload_cli --version "=0.1.0"
jadix-hot-reload --help
```

Before publication, use the source workspace. Consumers use one registry graph without copying private path/git/patch overrides. Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation and contributions

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers usage, migration, testing and releases. Preserve test discovery and keep private module tests separate from public integration contracts. Update both README languages when changing package setup.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.
