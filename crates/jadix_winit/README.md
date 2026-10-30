# jadix_winit

[English](README.md) | [简体中文](README.zh.md)

Jadix's Winit facade fork with a unified OpenHarmony backend.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.31.0-beta.3-jadix.1`.** Local archives and manifests do not prove registry availability.

## Installation

Use the version available in the target registry:

```toml
[dependencies]
jadix_winit = "=0.31.0-beta.3-jadix.1"
```

Before publication, use the source workspace. Consumers use one registry graph without copying private path/git/patch overrides. Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `x11`, `wayland`, `wayland-dlopen`, `wayland-csd-adwaita`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation and contributions

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers usage, migration, testing and releases. Preserve test discovery and keep private module tests separate from public integration contracts. Update both README languages when changing package setup.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.

## Distribution

Core types and all platform implementations ship as modules in this one package. There are no separately released `jadix_winit_*` backend packages. Original source identities, licenses and native patch notes are included under `upstream/`.
