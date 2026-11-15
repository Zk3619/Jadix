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

## Host environment

`environment::{EnvironmentHost, Environment, EnvironmentSnapshot}` provides validated, root-local preferences over the existing Store/Memo runtime. Create the writer under a reactive owner, pass its readonly environment through Context, and commit a full snapshot with `apply(expected_revision, next)`. A stale writer or disposed owner returns an error; identical snapshots do not advance revision. `select` returns an equality-filtered readonly Memo. Projection computations may rerun on sibling changes; unchanged results do not rerun downstream effects.

Preferences include locale/language order, a tagged IANA/Windows/offset time-zone identifier, optional hour cycle, ISO weekday, measurement/temperature units, light/dark appearance, reduced motion, lifecycle and a 0.5–4.0 host text multiplier. Unknown OS preferences remain None. `from_json` accepts at most 16 KiB with strict fields and validation; it checks identifier shape and budgets, not CLDR/TZDB membership. Time-zone metadata does not modify process TZ or format dates; font_scale is not display DPI.

The optional `system-environment` feature reads Windows preferences through Win32, and Linux process locale/time-zone metadata plus optional GNOME GSettings. Other targets or disabled configurations return Unsupported. Windows zone keys retain their Windows identity; its text multiplier remains 1.0. Linux reads GNOME color-scheme, clock format, animation and text scaling when available; unknown desktop values and independent temperature preference remain None. POSIX TZ expressions are not mislabelled as ISO offsets. No setting is written and no process locale/TZ is changed.

`SystemEnvironmentWatcher::start(notify)` owns one worker and one pending latest snapshot. Windows uses an invisible top-level window for settings/theme/time/display broadcasts; Linux uses a GSettings monitor plus inotify on `/etc`. Identical snapshots are filtered. The callback must only wake the host; drain `take()` before blocking, then publish via EnvironmentHost on the UI thread, preserving host lifecycle. `close`/Drop stop and join the worker, clear pending data and release owned handles/processes. Linux needs the GNOME schema and an available settings session for live desktop notifications; monitor termination reports an error. Each settings-read subprocess has a two-second deadline. No renderer or VM dependency is introduced.

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

The renderer-independent `virtualization::VirtualAxis::append` atomically validates a batch
of `(key, extent)` rows before appending, retaining existing measured heights and
scroll anchors without scanning old keys. Invalid or duplicate rows and capacity
overflow leave the axis unchanged. Ordered lookup and Fenwick updates cost amortized
O(k log(N+k)); a Vec growth can still copy old storage on an individual append.
The shared API is exercised by the core `virtual_axis_append` release example and
`jadix_native`'s real Windows `windows_virtual_append` integration test. These compare
against Jadix full replacement, not NSH or complete SDK/GPU end-to-end O(k).

Virtual lists can use `VirtualAxis::window_with_focus` to keep the focused row and
its previous/next rows mounted for sequential keyboard navigation. Call `reveal`
when focus moves to scroll the row into view. The focused window still observes the
caller-provided mount capacity; it does not define host-specific keyboard or
screen-reader navigation policy.

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.
