# jadix

The shared environment contract is available through jadix::environment with or without GPU features. Its optional platform field separates the runtime name from the physical operating system; older JSON without that field remains accepted. Enable system-environment for supported desktop OS sampling. The [WeChat Rust sample](../../examples/miniprogram/README.md) uses the same core environment, layout and controlled events through the native adapter.

The primary Rust UI facade owns shared UI behavior; `jadix_native` is a system-view adapter and `jadix_shell` is an optional script/distribution layer.

[English](README.md) | [简体中文](README.zh.md)

Pure-Rust native UI framework with fine-grained reactivity and optional static UI macros.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

## Installation

Add the package to your application:

```toml
[dependencies]
jadix = { version = "0.1.0", default-features = false, features = ["backend-dx12"] }
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: `macros`, `backend-dx12`, `backend-gles`, `backend-metal`, `window-materials`, `theme-shadcn`, `animation-base`, `native-clipboard`, `images`, `file-persistence`. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Optional bundled fonts

Registry archives contain source and notices, not the large Noto font collection. Default builds use system/runtime font discovery. Runtime font files can be supplied through `JADIX_FONT_DIR`.

To embed fonts with `bundled-cjk`, `bundled-cjk-serif`, `bundled-emoji` or `bundled-symbols`, set the **build-time** `JADIX_BUNDLED_FONT_DIR` to your licensed font directory. Required names are respectively `NotoSansCJK.ttc`; `NotoSerifCJK.ttc`; both `NotoColorEmoji_WindowsCompatible.ttf` and `NotoColorEmoji.ttf`; or `NotoSansSymbols-wght.ttf`, `NotoSansSymbols2-Regular.ttf`, `NotoSansMath-Regular.ttf` and `NotoMusic-Regular.ttf`. `bundled-fonts` requires all of them. Source checkouts can use the existing package `assets` directory as the default. Missing requested files fail the build with an actionable error; no network download occurs during the build. Preserve the font licenses and [NOTICE](NOTICE) in your application distribution.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE), [NOTICE](NOTICE) and dependency notices.

Shared keyed virtualization is available as `jadix::virtualization` with or without
GPU/default features. `VirtualAxis::append` preserves existing keys and measured
heights, validating each new batch before publication. It uses the same
`jadix_core` implementation for Rust GPU, native and character consumers; no
shell VM is required. Its amortized O(k log(N+k)) update cost does not imply that
SDK publication, layout, GPU collection or host output is O(k).
