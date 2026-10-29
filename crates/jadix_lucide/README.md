# jadix_lucide

[English](README.md) | [简体中文](README.zh.md)

Lucide static icons, animated playback and recipes for Jadix.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; prepared version `0.1.0`, not yet published.** Local archives and manifests do not prove registry availability.

## Installation

After the prepared version is published:

```toml
[dependencies]
jadix_lucide = "=0.1.0"
```

Before publication, use the source workspace. Consumers use one registry graph without copying private path/git/patch overrides. Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `ui`, `paths`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Three ways to use icons

1. Static UI icons: call `install()` before creating the window, then use `icon_named("check")` or `<Icon name="check" />`.
2. Animated icons: opt into `animated`; add `svg` for SVG frame rendering and `animation-path` only when path interpolation is needed.
3. Standalone SVG sources: copy individual files from [`assets/svg`](assets/svg), or call `svg_document("check")` with `default-features = false, features = ["paths"]`. This data-only mode does not depend on Jadix/GPU. The 1,703 canonical SVGs come from this package's frozen path catalog; legacy font aliases are not a promise of additional SVG geometry. `unlock` resolves to `lock-open` in the lookup API.

SVG sources use the 24×24 view box, `fill="none"` and `stroke="currentColor"`. You can edit SVG `fill` and `stroke` for suitable closed shapes. Lucide is designed as an outline set: open paths, holes and overlapping strokes do not automatically become a coherent solid icon. Font glyph color changes the outlined glyph; it cannot fill its interior. For a dependable solid silhouette, provide a purpose-designed SVG. Preserve [NOTICE](NOTICE) when copying sources.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation and contributions

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers usage, migration, testing and releases. Preserve test discovery and keep private module tests separate from public integration contracts. Update both README languages when changing package setup.

## License

Apache-2.0. Preserve [LICENSE](LICENSE), [NOTICE](NOTICE) and dependency notices.
