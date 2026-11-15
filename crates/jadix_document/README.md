# jadix_document

[English](README.md) | [简体中文](README.zh.md)

GUI-independent text snapshots, document height indexing and optional diffs.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

## Installation

Add the package to your application:

```toml
[dependencies]
jadix_document = "=0.1.0"
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

The optional `checkpoint` feature exposes CPU-only `checkpoint::{Checkpoint, CheckpointStore}` for checksummed, bounded, generation-checked application storage. Both `jadix_native` and `jadix_tui` consume the same implementation; it adds no GUI, renderer or syntax engine. Existing locks and generation conflicts fail explicitly; it does not claim power-loss durability.

With `markdown`, `Document::markdown_with_raw_html(source, false)` displays raw HTML tags and blocks as literal text while preserving Markdown and autolinks outside HTML blocks. The policy survives edits and is shared by document views and editors. HTML block contents remain literal; this option is not an HTML sanitizer.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.
