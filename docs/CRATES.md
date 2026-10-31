# Crate inventory

[English](CRATES.md) | [简体中文](CRATES.zh.md)

Jadix contains **27 library crates** under `crates/`. The tables describe package responsibilities and application usage. Examples, Gallery applications, and bundled implementation modules are separate from these libraries.

See [BUILD.md](BUILD.md) for platform setup and packaging, and [FEATURES.md](FEATURES.md) for feature selection.

## Application libraries — 4 packages

| Package / version | Source and package guide | Responsibility and usage |
| --- | --- | --- |
| `jadix` · `0.1.0` | [crates/jadix](../crates/jadix/README.md) | Application facade: elements, reactive UI, layout, text, rendering, windows, and optional documents, media, accessibility, and development tools. Builder APIs work without procedural macros; `macros` enables `ui!` and component authoring. Select graphics backends for the target platform. |
| `jadix_components` · `0.1.0` | [crates/jadix_components](../crates/jadix_components/README.md) | Native controls, forms, navigation, overlays, and data presentation, with optional document/code controls. Depends on Jadix; related features forward framework capabilities. Default features are empty; `macros` is optional. |
| `jadix_lucide` · `0.1.0` | [crates/jadix_lucide](../crates/jadix_lucide/README.md) | Lucide icon data, UI integration, and opt-in animation/morphing. Defaults to `ui` and `paths`; `animated`, `svg`, and `animation-path` enable additional support. |
| `jadix_dnd` · `0.1.0` | [crates/jadix_dnd](../crates/jadix_dnd/README.md) | High-level drag-and-drop UI: drag sources, targets, and coordinated interactions. Builder use does not require the optional `macros` feature. |

## Core, documents, and development — 9 packages

| Package / version | Source and package guide | Responsibility and usage |
| --- | --- | --- |
| `jadix_core` · `0.1.0` | [crates/jadix_core](../crates/jadix_core/README.md) | Ownership, signals, stores, and fine-grained reactivity without a window or renderer, suitable for shared state and non-UI logic. |
| `jadix_animation` · `0.1.0` | [crates/jadix_animation](../crates/jadix_animation/README.md) | Animation timing, springs, inertia, scalar players, and timelines. Defaults to `animation-base`; extended algorithms and path operations are optional. |
| `jadix_layout` · `0.1.0` | [crates/jadix_layout](../crates/jadix_layout/README.md) | CSS-like dimensions, logical units, and grid-track algorithms: reusable layout primitives alongside the Taffy layout engine. |
| `jadix_style_tokens` · `0.1.0` | [crates/jadix_style_tokens](../crates/jadix_style_tokens/README.md) | Shared Tailwind/Uno-style utility-token catalogue and variant parsing, keeping runtime and macro styling vocabulary consistent. |
| `jadix_theme` · `0.1.0` | [crates/jadix_theme](../crates/jadix_theme/README.md) | Theme palettes, design tokens, color parsing, and validated overrides. Defaults to `theme-shadcn`; Fluent, Material 3, and Liquid Glass are optional. |
| `jadix_document` · `0.1.0` | [crates/jadix_document](../crates/jadix_document/README.md) | GUI-independent text snapshots, document height indexing, and document operations. Markdown, editing, math/science, diff, and syntax are feature-gated. Document semantics live here; framework/components supply native presentation and interaction. |
| `jadix_lsp` · `0.1.0` | [crates/jadix_lsp](../crates/jadix_lsp/README.md) | GUI-independent language-server transport and UTF-16 text edits. Applications configure the language server; this is not a bundled server implementation. |
| `jadix_macros` · `0.1.0` | [crates/jadix_macros](../crates/jadix_macros/README.md) | Compile-time authoring, including `ui!` and `#[component]`, usually selected through product `macros` features. Builder-only applications need not depend on it directly. |
| `jadix_hot_reload` · `0.1.1` | [crates/jadix_hot_reload](../crates/jadix_hot_reload/README.md) | Framework-neutral hot-reload protocol and receiver. Optional `syntax` adds syntax handling; `cli` includes the `jadix-hot-reload` executable and watcher dependencies. |

## Rendering, adapters, and system integration — 9 packages

| Package / version | Source and package guide | Responsibility and usage |
| --- | --- | --- |
| `jadix_renderer` · `0.1.0` | [crates/jadix_renderer](../crates/jadix_renderer/README.md) | GPU compute kernels, backdrop-blur pipelines, graphics compatibility support, and emoji classification used by the framework. A rendering support library, not the application facade. |
| `jadix_media` · `0.1.0` | [crates/jadix_media](../crates/jadix_media/README.md) | Backend-neutral playback contracts for Jadix and external adapters. A playback contract does not install a decoder or platform player. |
| `jadix_tasks` · `0.1.0` | [crates/jadix_tasks](../crates/jadix_tasks/README.md) | Runtime-neutral cancellation, execution, and event contracts for asynchronous adapters, without choosing an executor. |
| `jadix_net` · `0.1.0` | [crates/jadix_net](../crates/jadix_net/README.md) | Backend-neutral networking contracts and optional `json` support. Applications integrate a concrete network implementation. |
| `jadix_tokio` · `0.1.0` | [crates/jadix_tokio](../crates/jadix_tokio/README.md) | Bounded Tokio execution adapter for `jadix_tasks`, enabling Tokio without imposing it on the core state and task contracts. |
| `jadix_clipboard` · `0.1.1` | [crates/jadix_clipboard](../crates/jadix_clipboard/README.md) | Maintained clipboard-rs fork for text, HTML, RTF, files, and optional images, aligned with Jadix platform dependency versions. Preserve upstream notices; runtime support depends on the OS. |
| `jadix_window_materials` · `0.1.0` | [crates/jadix_window_materials](../crates/jadix_window_materials/README.md) | Platform-neutral backdrop choices and material tuning, with optional Windows composition integration. Native effects depend on host support. |
| `jadix_window_state` · `0.1.0` | [crates/jadix_window_state](../crates/jadix_window_state/README.md) | TOML persistence of desktop window state for winit/Jadix applications. Defaults to `winit`, with optional `jadix` integration. Keep this desktop dependency out of Android/iOS/OHOS mobile application dependency sections. |
| `jadix_accesskit_ohos` · `0.1.0` | [crates/jadix_accesskit_ohos](../crates/jadix_accesskit_ohos/README.md) | Pinned OpenHarmony AccessKit adapter backport for native accessibility. Defaults to `api-15`, with optional `api-24`; requires the matching OHOS SDK and host integration. |

## Maintained platform and text forks — 5 packages

| Package / version | Source and package guide | Responsibility and maintenance boundary |
| --- | --- | --- |
| `jadix_winit` · `0.31.0-beta.3-jadix.1` | [crates/jadix_winit](../crates/jadix_winit/README.md) | Maintained window/event-loop implementation. Core types, common support, and platform backends are modules in this single package, selected by features and target configuration. |
| `jadix_wgpu` · `0.1.0` | [crates/jadix_wgpu](../crates/jadix_wgpu/README.md) | Maintained GPU implementation including Jadix backend/shader changes. GPU core, HAL, types, bridge support, and modified Naga are bundled modules. Rust library name: `wgpu`, with `wgpu::hal`, `wgpu::wgc`, `wgpu::wgt`, and `wgpu::naga` entry points. |
| `jadix_cosmic_text` · `0.1.0` | [crates/jadix_cosmic_text](../crates/jadix_cosmic_text/README.md) | Multiline text shaping/layout fork with Jadix alignment and paragraph-direction changes, normally consumed through the framework text stack. |
| `jadix_glyphon` · `0.1.0` | [crates/jadix_glyphon](../crates/jadix_glyphon/README.md) | GPU text rendering fork with CPU atlas mirroring, batched dirty uploads, glyph-residency tracking, and backend/shader adaptations. Uses the compatible cosmic-text and wgpu forks. |
| `jadix_taffy` · `0.1.0` | [crates/jadix_taffy](../crates/jadix_taffy/README.md) | Flexbox/Grid layout engine fork with Jadix invalidation and intrinsic-cache changes. Distinct from the units/grid primitives in `jadix_layout`. |

Maintained window, GPU, text, and layout implementations live under `crates/`. The patched GPUI implementations in `vendor/` are used only by comparison benchmarks. Upstream baselines, licenses, and attribution are recorded in [vendor/RELEASE_FORKS.json](../vendor/RELEASE_FORKS.json); unmodified dependencies retain their upstream package identities.
