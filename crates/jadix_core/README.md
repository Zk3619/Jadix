# jadix_core

[English](README.md) | [简体中文](README.zh.md)

Reactive ownership, signals and stores without a UI or renderer.

The experimental `presentation` module also provides shared typed node/commit,
event, measurement and rectangular hybrid geometry contracts. The GPU facade
re-exports the same module as `jadix::presentation`; windowless consumers can
use it directly without adding rendering dependencies.

`tree::Tree` provides ordered expansion, visible rows, keyboard navigation,
range/additive selection and bounded lazy loading with tree-specific generation
receipts. Collapsing an ancestor cancels outstanding descendant loads. Its
stable business keys feed `virtualization::VirtualAxis`; rendering, native
TreeItem providers, platform key bindings and asynchronous I/O belong to hosts.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

## Installation

Add the package to your application:

```toml
[dependencies]
jadix_core = "=0.1.0"
```

Platform-specific packages require the corresponding SDK.

## Features and scope

The renderer-independent `dock` model owns stable business pane keys, tab stacks,
split ratios and floating surface bounds. Layout mutations validate a candidate
before publication, collapse empty stacks, keep the main window nonempty and
support bounded undo/redo. Drag tokens bind to one owner, generation and revision;
cancelled, foreign or superseded tokens cannot move a pane. Versioned JSON
restoration validates ownership, uniqueness, depth, windows and byte budgets.
Defaults are 128 panes, eight windows, depth 32, 32 history entries and 256 KiB
of layout data. Documents, native handles and actual pointer capture belong to
the consuming product; see the Native controls Dock sample.

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.
