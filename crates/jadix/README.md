# jadix

[English](README.md) | [简体中文](README.zh.md)

Pure-Rust native UI framework with fine-grained reactivity and optional static UI macros.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.** Local archives and manifests do not prove registry availability.

## Installation

Use the version available in the target registry:

```toml
[dependencies]
jadix = { version = "0.1.0", default-features = false, features = ["backend-dx12"] }
```

Before publication, use the source workspace. Consumers use one registry graph without copying private path/git/patch overrides. Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `macros`, `backend-dx12`, `backend-gles`, `backend-metal`, `window-materials`, `theme-shadcn`, `animation-base`, `native-clipboard`, `images`, `file-persistence`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Optional bundled fonts

Registry archives contain source and notices, not the large Noto font collection. Default builds use system/runtime font discovery. Runtime font files can be supplied through `JADIX_FONT_DIR`.

To embed fonts with `bundled-cjk`, `bundled-cjk-serif`, `bundled-emoji` or `bundled-symbols`, set the **build-time** `JADIX_BUNDLED_FONT_DIR` to your licensed font directory. Required names are respectively `NotoSansCJK.ttc`; `NotoSerifCJK.ttc`; both `NotoColorEmoji_WindowsCompatible.ttf` and `NotoColorEmoji.ttf`; or `NotoSansSymbols-wght.ttf`, `NotoSansSymbols2-Regular.ttf`, `NotoSansMath-Regular.ttf` and `NotoMusic-Regular.ttf`. `bundled-fonts` requires all of them. Source checkouts can use the existing package `assets` directory as the default. Missing requested files fail the build with an actionable error; no network download occurs during the build. Preserve the font licenses and [NOTICE](NOTICE) in your application distribution.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation and contributions

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers usage, migration, testing and releases. Preserve test discovery and keep private module tests separate from public integration contracts. Update both README languages when changing package setup.

## License

Apache-2.0. Preserve [LICENSE](LICENSE), [NOTICE](NOTICE) and dependency notices.
