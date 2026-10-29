# Jadix

[English](README.md) | [简体中文](README.zh.md)

A Rust-native UI framework with reactive state, GPU rendering, and optional declarative macros.
Use ordinary Rust builders, add product libraries independently, and select capabilities with Cargo features.

> **Alpha · unreleased.** APIs are evolving. Manifest versions are prepared versions, not proof of availability on crates.io. Native validation currently covers Windows / DX12; platform source code does not establish validation on other targets.

## Features

- Native windowing, Flex/Grid layout, text shaping, and GPU rendering without a browser UI layer.
- Rust builders and optional `ui!` / `#[component]` macros share the same runtime.
- Signals, stores, ownership scopes, and retained element trees support reactive updates.
- Components, Lucide icons, and high-level drag and drop are separate product dependencies.
- Graphics backends, animation, documents, accessibility, and runtime/network adapters are explicit capabilities.
- Debug tools inspect ordinary elements, edit styles, and hot-reload supported static declarations. Arbitrary Rust logic requires recompilation.

## Run from source

Install a recent stable Rust toolchain and the SDKs for your target. Respect each package's `rust-version`.
Windows builds need MSVC and the Windows SDK; other platforms require their own windowing and graphics dependencies.

```sh
git clone https://github.com/Zk3619/Jadix.git
cd Jadix
cargo run -p gallery
```

## Minimal application

After the prepared versions are published, a Windows application can use:

```toml
[dependencies]
jadix = { version = "0.1.0", default-features = false, features = ["backend-dx12"] }
```

```rust,no_run
use jadix::prelude::*;

fn main() {
    jadix::run(|| Element::new().child(text("Hello, Jadix")));
}
```

This builder configuration does not require UI procedural macros. Until publication, run the workspace examples rather than copying private path/git/patch overrides into applications.

## Choose a library

| Library | Responsibility |
| --- | --- |
| [jadix](crates/jadix/README.md) | Runtime, elements, layout, rendering, windows, optional development tools |
| [jadix_components](crates/jadix_components/README.md) | Native product controls with optional macros |
| [jadix_lucide](crates/jadix_lucide/README.md) | Static icons and opt-in animation |
| [jadix_dnd](crates/jadix_dnd/README.md) | High-level drag and drop |
| [jadix_core](crates/jadix_core/README.md) | UI-independent ownership, signals, and stores |
| [jadix_animation](crates/jadix_animation/README.md) | Animation clocks, algorithms, timelines, optional path operations |
| [jadix_document](crates/jadix_document/README.md) | Document snapshots and indexing |
| [jadix_tasks](crates/jadix_tasks/README.md), [jadix_net](crates/jadix_net/README.md), [jadix_tokio](crates/jadix_tokio/README.md) | Task contracts, network contracts, and the optional Tokio adapter |

The `vendor/` tree contains explicitly named Jadix maintenance forks. Preserve their licenses and use the versions selected by the graph; similarly named upstream packages are not interchangeable.

## Documentation and contributions

- [Documentation index](docs/README.md): maintained project instructions; most detailed guides are currently in Chinese.
- [Features](docs/FEATURES.md), [state](docs/state-management.md), [async integration](docs/async-architecture.md), [remote assets](docs/remote-assets.md).
- [Components](docs/component-authoring.md), [windows](docs/desktop-windows.md), [icons](docs/application-icons.md), [WebView](docs/webview.md), [DevTools](docs/devtools.md).
- [Migration](docs/migration.md), [testing](docs/testing.md), and [release process](docs/releasing.md).

`docs/` contains authoritative project documentation; executable examples and configuration live in `examples/`.
Keep large private tests with their owning Rust modules, public contracts in crate `tests/`, and Python contracts in `scripts/tests/`.
Development audit records and raw evidence are maintained separately and are not build inputs.
When contributing, preserve test discovery and update both README languages. Include OS, backend, toolchain, and a minimal reproduction in bug reports.

## License

Jadix-owned framework code uses [Apache-2.0](LICENSE). Dependencies and maintenance forks retain their own licenses and notices.
