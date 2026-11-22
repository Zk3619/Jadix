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

`virtualization::UniformGrid` composes two gap-aware fixed axes without a per-row,
per-column or per-cell index. Geometry, cell origins, atomic two-axis alignment
and bounded rectangular mount windows use O(1) time and storage. Size/offset/gap
arrays use `[x, y]`; constructor counts are `(rows, columns)`. `GridViewport`
separates actual visible ranges from overscan in `UniformGridWindow`. A zero
count empties both axes; zero viewport dimensions mount no cells. Capacity is
checked on the cell product, including integer overflow, before allocation.
Variable extents and pinned rows/columns remain the separate keyed `VirtualGrid`.

`UniformGrid::window_with_scroll_ahead` and its `UniformWrap` counterpart add
signed pixel prediction to the base mount window, capped at one viewport on each
scrolling axis. They retain the actual viewport and base overscan; prediction
exceeding capacity falls back to the base window, while an oversized base still
fails. Wrap capacity counts real items in partial rows. Both queries stay O(1)
without per-cell allocation and accept the shared `ListScrollAhead` output.

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

`shortcut::KeyChord` parses one accelerator stroke and preserves its bound symbol
and modifier order for keycap labels. `CmdOrCtrl` stays portable in serialized
bindings and resolves for the current host when displayed or dispatched. Shared
menus accept `MenuItem::key_chord`; Kbd groups accept the same model. Characters
require Ctrl, Alt or Super; F1–F24 may stand alone. Input is bounded to 64 bytes;
duplicate modifiers, whitespace and malformed keys are rejected. Keyboard-layout
translation, multi-stroke sequences, focus scopes and event consumption remain
host responsibilities.

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

`VirtualAxis::window_in_parent` and `window_with_focus_in_parent` extend measured
row mounting to an ancestor's viewport, retaining the signed local offset and
full content extent. Overscan starts at the clipped intersection; a completely
offscreen or zero-sized intersection mounts only explicitly pinned/focused rows
and focus neighbors. All window variants count distinct mounted rows against
capacity, including overlap between visible, overscanned, pinned and focus rows.
Focus merging reuses the sorted mount indices without copying keys. Adapters
still own ancestor discovery, content placement and application of scroll offsets.

`VirtualGrid::window_in_parent` applies the same parent geometry to measured rows
and columns. Empty axes and zero-area intersections suppress ordinary mounting
on both axes; explicit pins remain. Capacity bounds each distinct axis index set
and their cell product. Windows retain a contiguous range plus outlying pins
during planning, checking the product before allocating either mount vector.
Single-axis windows also use this plan, avoiding tree nodes for visible rows.
This is shared geometry; rendering and ancestor scroll integration belong to adapters.

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.


`virtualization::UniformWrap` lays out a flat dataset in as many equal columns as
fit the available width and minimum item width. Heights and gaps are fixed;
a narrow viewport gets one narrower column. Its O(1) item/cell mapping rejects
padding cells in the final row, and its row window checks the actual item budget.
Save `anchor`, rebuild for the new width, then `restore_anchor` to preserve that
item's row and pixel depth (clamped at the end). Keep the original item anchor
across successive reflows until the user scrolls to avoid cumulative row drift.
This shared model has no renderer or native dependency.


`ListStickyHeaders` normalizes section indices once. `UniformList::sticky_header`
and `VirtualAxis::sticky_header` return the active row and a nonnegative displacement
from its natural content position, including next-header push-off and content-end
clamping. Signed viewport insets support overlapping chrome. Queries allocate
nothing: O(log headers) for uniform rows, O(log headers * log rows) for measured
rows. Indices outside the dataset are ignored; zero viewports return no pin.

`ListScrollPosition::remap_keys` preserves keyed row identity across mutations,
falling forward to the next surviving old row, then backward, with zero depth
when the anchor was removed. No survivor returns None. `UniformList::anchor`
includes trailing gaps; both uniform and measured axes expose `restore_position`
with half-open row and viewport clamping. Key-index ownership stays with callers.

`ListContent` projects fixed or measured row geometry into complete scrolling
content with leading/trailing insets. Navigation, anchor restoration, viewability
and sticky headers use the same physical coordinates; slot-only viewports contain
no rows. Short alignment includes the insets, viewport-fraction thresholds retain
the full viewport denominator, and the final sticky row stops before the footer.
The adapter allocates nothing and keeps each underlying query's complexity.

`ListContentAlign::padding(content, viewport)` distributes unused main-axis space
at the start, center or end without allocations. Start preserves natural extent;
Center and End fill the viewport, including empty data. Both extents must be finite
and nonnegative. Alignment is independent of scroll navigation and end following.

`ListOrder` projects dataset indices and half-open ranges to forward or reversed
visual order on either axis. Both mappings are their own inverse, validate bounds
(including empty ranges), and use constant space without a reversed collection.

`ListScrollAhead` predicts a signed, directional mount extension from synchronized
physical offsets using O(1) work and storage. It caps prediction at one viewport,
switches direction immediately, retains windows at rest, and discards teleport,
resize or hidden-viewport history. Call `reset` for data revisions or anchor/navigation
corrections. It does not change actual visibility or schedule frames.

`ParentViewport` projects a region's natural start and an ancestor's physical
scroll offset/extent into signed local coordinates. Uniform and measured lists,
`ListContent`, `UniformGrid` and `UniformWrap` expose parent queries: fully offscreen
regions have no visible rows/cells and grid/wrap overscan mounts nothing; partial
intersections retain the full parent viewport for overlap thresholds. Navigation
returns a parent-space offset clamped to the parent's complete content, while
edge loading uses parent distances and disarms after passing an edge. Slotted
content keeps physical coordinates and the final sticky row stops before its
footer; completely offscreen content has no pin. Queries allocate nothing and
retain O(1) uniform / O(log rows) measured geometry costs. Adapters must supply
the correct content origin, route scrolling, and update it after layout changes;
automatic ancestor discovery and layout spacers are not provided by this geometry
model. Parent queries do not enable end following
or reverse the dataset; call `ListOrder` explicitly when mapping logical edges.

`ParentViewport::preserve_change` compensates a changed row/slot interval only
when its old extent was entirely above the viewport and the region's start has
scrolled past. Partially visible changes show normally. The returned snapshot
clamps to the parent's full range; feed it through each change in one layout,
then apply `parent_offset()` to the scroll owner. Pass the parent content extent
AFTER each change, including surrounding page content. `VirtualAxis::measure_in_parent`
uses this rule in O(log rows), including changes in a region already above the
viewport. `splice_in_parent` applies it to keyed range replacements, retaining
unaffected measurements and spacing; `splice` exposes the same atomic replacement
without anchoring. Splices rebuild in O((n + k) log(n + k)) and O(n + k) space;
append remains the incremental option. Invalid edits leave the axis unchanged.
These APIs provide compensation geometry; adapters still apply parent scrolling.
