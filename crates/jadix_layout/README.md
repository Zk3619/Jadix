# jadix_layout

[English](README.md) | [简体中文](README.zh.md)

CSS dimensions, logical units and grid track algorithms.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

## Installation

Add the package to your application:

```toml
[dependencies]
jadix_layout = "=0.1.0"
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.

CPU-only shared presentation layout now lives in `presentation::PresentationLayout`: NodeStyle flex sizing, cached host-provided leaf measurements and overlay geometry. Native hosts re-export it as NativeLayout; terminal hosts supply grapheme cell measurement. This does not import OS views, GPUs or a windowed application runtime.

PresentationLayout retains Taffy identity, child links and computation caches across snapshots. Style/content/structure changes invalidate affected nodes; environment revision changes clear layout and measurement caches. Frames publish only after successful measurement and geometry validation. `LayoutStats` exposes actual engine creation/reuse/removal/update and child-list changes. `release_layout_cache()` frees Taffy storage while retaining bounded host measurements and caller state. Validation, synchronization and frame collection still scan the full input tree.
