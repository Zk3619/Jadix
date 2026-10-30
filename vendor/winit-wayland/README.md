# jadix_winit_wayland

[English](README.md) | [简体中文](README.zh.md)

Winit's Wayland backend.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.31.0-beta.3-jadix.1`.** Local archives and manifests do not prove registry availability.

## Installation

Use the version available in the target registry:

```toml
[dependencies]
winit_wayland = { package = "jadix_winit_wayland", version = "=0.31.0-beta.3-jadix.1" }
```

Before publication, use the source workspace. Consumers use one registry graph without copying private path/git/patch overrides. Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `dlopen`, `csd-adwaita`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Maintenance fork

This is a separately named Jadix distribution, not an upstream release or endorsement. Use the compatible Jadix dependency graph and preserve original attribution.

Upstream: https://github.com/Zk3619/Jadix.

## Documentation and contributions

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers usage, migration, testing and releases. Preserve test discovery and keep private module tests separate from public integration contracts. Update both README languages when changing package setup.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.
