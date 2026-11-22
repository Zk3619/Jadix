# jadix_components

[English](README.md) | [简体中文](README.zh.md)

Jadix native UI components with optional RSX authoring.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

VirtualList, VirtualGrid and VirtualWrap resample count/data-key changes made by item render callbacks before publishing viewport and load-edge notifications. Grid and wrap also resample dynamic cell sizes. Invalidated callbacks stop the old window immediately; up to four attempts run per synchronization, then another frame is requested. Prefer pure render callbacks, and change `data_key` when editing items without changing their count. This guard also covers VirtualList's initial rows and parent scroll adapters.

## Installation

Add the package to your application:

```toml
[dependencies]
jadix_components = "=0.1.0"
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default feature: `gpu`, preserving the complete component product. RSX remains optional through `macros`. Disable default features and select macros and a theme for portable components; see the [portable example](../../examples/portable-contract/README.md). Portable authoring projects typed descriptors; [jadix_native platform hosts](../jadix_native/README.md) provide system/DOM controls and define supported capabilities. The subset includes basic controls, Dialog, basic compound Select and Popover Builder/compound RSX. Popover has DOM and Android PopupWindow hosts; UIKit rejects it. See [Cargo.toml](Cargo.toml) for the full graph. A feature flag does not establish platform runtime validation.

## Documentation

GPU Command and Combobox Builders accept `.query_signal(Signal<String>)`, also with
`.child(...)`, in addition to their existing `command_query` / `combobox_query`
controllers. The signal seeds committed input text on mount; later external writes
apply before filtering/rendering when the field is unfocused and IME preedit is
empty. Local edits, clear/session resets and batched local writes take precedence
over pending external text. The binding belongs to the component owner and survives
temporary input unmounts. Searchless Command lists retain independent typeahead. Combobox
keeps text separate from selected IDs: closing single-select publishes the selected
label; closing multi-select clears the query. This does not start async work or
change focus; use `filterable(false)` with externally supplied ranked results.

VirtualGrid accepts `.row_key_fn(...)` and `.column_key_fn(...)` independently. Stable keys retain mounted cell wrappers through insertions and sorting, while render callbacks receive current coordinates. Key indices use O(rows + columns) storage and rebuild only on count/data-key changes, never per scroll or per logical cell. Duplicate keys fail closed, including outside the viewport. `.maintain_visible_content_position(false)` disables the default keyed-axis edit anchoring; explicit navigation wins, and parent/hidden grids keep owner offsets. Unkeyed axes keep coordinate identity.

GPU `virtual_grid(id)` mounts the visible rectangle, per-axis overscan and live
interaction owners within its cell budget:

```rust,ignore
virtual_grid("sheet")
    .row_count(100_000).column_count(1_000)
    .cell_size(100.0, 28.0).cell_gap(2.0, 2.0).height(400.0)
    .render_cell(|row, column| Element::new().text(format!("{row}:{column}")))
```

`row_count_fn`, `column_count_fn`, `cell_size_fn` and `data_key_fn` are polled
without rebuilding the outer view. Coordinates retain mounted cell state during
scroll and metric/count changes; change `data_key_fn` after content edits or
reorder to refresh visible content. `VirtualGridHandle::scroll_to_cell` accepts
`[x, y]` alignment/extra arrays and waits for measured viewport dimensions;
`scroll_to_end` jumps both axes. Actual visible geometry is available through
`viewport()` or `on_viewport_change`. Gap geometry and total extents are shared
with renderer-free `jadix_core::virtualization::UniformGrid`.
The default mount budget is 16,384 cells; `max_mounted_cells` changes it. Invalid
metrics or an exceeded budget display an empty body with `data-virtual-grid-error`
on the body; valid geometry recovers automatically. Capacity failures retain the
scroll extent. This is a fixed-cell GPU Builder foundation: business-key reorder
retention, variable sizes, frozen panes, grid keyboard/selection semantics, RSX
and native descriptor hosts are not included.


VirtualGrid and VirtualWrap support `.use_parent_scroll()`. Grid binds the nearest registered ancestor independently on both axes; wrap borrows the vertical owner and follows its own container width. Both contribute full natural size and navigate actual owners across surrounding content. Hidden or unbound navigation stays queued. Parent mode disables wrap anchoring, end following and spare-space alignment; prediction, retention, slots, viewability and load edges keep their budgets and full-parent semantics.

GPU Collapsible and Accordion content accept an optional physical reveal spring:
`.content_with(|c| c.reveal_spring(SpringPhysics::default_css()).text("Body"))`
(on an Accordion item), or `reveal_spring={spring}` on the RSX content tag.
Import `SpringPhysics` from `jadix_components::style`. Omit it to retain keyframes.
For recursive TreeView, configure `.variant("collapsible").reveal_spring(spring)`
or `<TreeView variant="collapsible" reveal_spring={spring}>`. All branches,
including descendants created on expansion, inherit it; flat virtualization is unchanged.
With `animation-base`, reversals retain velocity and closing content stays mounted
until the spring settles. The height clip stays within the natural content extent.
Reduced motion or disabled animations finish immediately; invalid physics uses
the default, with a 20-second safety ceiling for extreme configurations.

Static selectable text supports double-click word selection and triple-click
hard-paragraph selection, followed by dragging in either direction to extend
by that unit. Word boundaries follow Unicode, preserving combining marks and
emoji sequences; CRLF is excluded from paragraph selections. This applies
within one selectable text leaf; selection across leaves and touch token
selection are not provided by this interaction.

`text_view(id).highlights(ranges)` and
`diff_view(id, diff).highlights(original_ranges, modified_ranges)` accept
`TextDecoration` values from `jadix::utils::text_annotations`. Each range uses
absolute UTF-8 byte offsets in its source document, before tab expansion.
Foreground/background colors and underline/strikethrough preserve text metrics
and selectable source text. Later colors win when ranges overlap. Highlights
are bound to the initial document snapshot; replacing it clears them. Rebuild
the view with ranges for the new snapshot to apply new highlights.

DiffView also highlights inline changes automatically in unified and split modes.
The shared `jadix_document::DiffDocument` computes absolute source-byte ranges once,
comparing grapheme clusters within positionally paired replacement lines. CRLF and
unchanged text keep their source coordinates. Explicit highlights override automatic
backgrounds; `.inline_changes(false)` disables automatic highlighting (RSX:
`inline_changes`). New snapshots refresh automatic ranges. Refinement is bounded
per pair and per snapshot; oversized pairs retain whole-line highlighting, reported
by `has_coarse_inline_changes()`. Unpaired insertions/deletions stay whole-line changes.

With `syntax-highlighting`, both builders accept `.language("rs")` (RSX: `language`).
Each complete source is parsed on a shared document-core worker, preserving
multiline syntax and source-byte coordinates. Views show plain text first and
refresh visible rows when the latest result arrives. Syntax colors compose with inline
diff backgrounds and explicit decorations; explicit styles take precedence.
Snapshot replacement and light/dark changes refresh syntax. Pending snapshots
replace older work; cancellation and unmount discard obsolete results without
waiting for parsing. Line indexing runs on the UI thread; parsing is not incremental.
The shared parser falls back to plain text above 2 MiB per source or 64 KiB per line.
Without the feature, the same API preserves plain source text.

With `document`, `document_view(id).on_link_hover(handler)` exposes painted inline
link transitions for an application-owned preview. RSX accepts `on_link_hover`
as a `DocumentLinkHoverHandler`. Each `DocumentLinkHover` carries the view ID,
an optional destination and window logical pointer coordinates. Repeated moves
within one link are deduplicated; plain text, pointer leave/press and unmount
dismiss an active preview. Two views of one document keep independent state.
Use unique view IDs when sharing a handler. Activation remains `.on_link(...)`.
Preview content, keyboard interaction and refresh under a stationary pointer
after scrolling or document changes are application responsibilities.

Tooltip and HoverCard builders expose `.open_delay(300)` and `.close_delay(0)`;
numeric values are milliseconds. The shared opening default is 300ms and zero
opens immediately. RSX uses `openDelay` and `closeDelay` on the root or trigger.

Portable Tabs support Builder and compound RSX through `tabs`, `TabsList`,
`TabsTrigger` and `TabsContent`. Controlled values, lazy panels, horizontal/vertical
arrow navigation, Home/End, disabled-item skipping and native focus requests share
one projection. Inactive panels unmount their native controls; retained Elements
and signals restore business text with fresh native identities on reactivation.
Conditional visibility changes wait for active projected text composition to
finish. This does not protect arbitrary tree removal or Dialog close. Windows
maps TabList/TabItem/TabPanel roles and selected state to UIA and retains one
native tab stop among the triggers; Tab leaves the list for the active panel.
Native selection/undo history across panel remount, other hosts' tab traversal,
theme variants and physical IME acceptance remain separate work. `native-tabs`
is the actual Windows example.

Portable Collapsible supports `collapsible(...).trigger(...).content(...)` and
compound `<Collapsible>` / `<CollapsibleTrigger>` / `<CollapsibleContent>` RSX.
Controlled or default open state, root/trigger disabling, expanded descriptor
updates and lazy content use the shared projection. Content builds once, then
unmounts native controls when closed and restores business signals with fresh
native identities. Conditional collapse waits for projected composition to
commit. `.exclusive(true).group("name")` / RSX equivalents coordinate siblings
created under the same authoring owner: the first initially open item wins,
later controlled opens close its peers, and a closed group may have no winner.
The same group name under another owner is independent; incomplete exclusive/group
options fail before native publication. Animated height, focus restoration and native undo across collapse are not
implemented. `native-collapsible` runs the Windows product; expanded
metadata does not establish an ExpandCollapse accessibility pattern or actual
screen-reader/physical IME acceptance.

Portable Accordion supports `accordion(id).item(value, title, body)` and compound
`<Accordion>` / `<AccordionItem>` / `<AccordionTrigger>` / `<AccordionContent>`.
`value_signal` / RSX `value` control a `Signal<Vec<String>>`; `default_value`
initializes uncontrolled selection. Single selection is the default; `multiple`
allows several open panels, and every mode allows all panels to close.
Root, item and trigger disabling are combined. Up/Down wrap over enabled
headers; Home/End focus the first/last enabled header without changing selection.
Activation toggles selection. Hosts must bind `navigation_nodes` and apply
`take_focus_request`. Content uses the same lazy/remount/composition policy as
Collapsible. Duplicate/empty item values and invalid initial selection are
rejected. The Windows `native-collapsible` product probes actual BUTTON focus,
disabled-header skipping, single selection and collapse-all. Multi-selection
and composition have shared projection tests. Hosts can use `snapshot_with_focus`
with their current focused NodeId and then apply `take_focus_request`: removal of
a focused disclosure subtree returns focus to its nearest visible, unique,
enabled trigger. Nested closures fall back to the outer trigger. Existing focus
outside the removed subtree and pending navigation take precedence. Composition
defers both removal and restoration. The Windows product verifies real EDIT to
BUTTON restoration and preserves focus on an activated alternative header.
Mobile execution, animations and ExpandCollapse accessibility patterns remain pending.

GPU-only control, document, media, animation and scale-preview features select
the GPU component surface together with their runtime capability. Macros and
themes alone keep the portable surface available without GPU/window dependencies.

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.


GPU `virtual_wrap(id)` adapts flat data to the measured viewport width:

```rust,ignore
let handle = VirtualGridHandle::default();
virtual_wrap("gallery").handle(handle.clone())
    .item_count(100_000).minimum_item_width(120.0)
    .item_height(80.0).item_gap(8.0, 8.0).height(400.0)
    .render_item(|index| Element::new().text(format!("Item {index}")));
// Once mounted: handle.scroll_to_item(index, ListScrollAlign::Nearest, 0.0).
```

Columns share the available width; a narrower viewport uses one narrower column
with horizontal scrolling disabled. Mounted item indices retain state across
column changes. The first visible item's row and depth survive repeated reflows
until scrolling or navigation establishes a new anchor, subject to end clamping.
`item_count_fn`, `item_height_fn`, and `data_key_fn` support updates; change the
data key after edits or reordering. `overscan` counts rows, and the mount budget
counts real items, excluding final-row padding. `item_origin` and item navigation
use flat indices through the shared `UniformWrap` geometry. Reuses `VirtualGrid`'s
bounded renderer and handle. Fixed heights only; selection/keyboard policy, RSX
and native hosts remain separate work.

For mutable datasets, `.item_key_fn(|index| stable_key(index))` retains mounted
wrapper identity by unique business key and preserves the leading item and depth
after edits. Deleted anchors choose the next surviving old item, then the previous
one, at its start. Explicit navigation wins. Disable data-edit anchoring with
`.maintain_visible_content_position(false)`; width reflow still preserves position.
Keys use O(item_count) storage and are scanned only when count or data key changes;
scrolling and unchanged frames do not scan the dataset. Change `data_key_fn` after
same-count edits. Moved items and revised data re-render their contents using the
current index; wrapper attributes and template identity survive, while descendant
state requires authored stable IDs. Duplicate keys expose `DuplicateKey` through
`data-virtual-grid-error` and an empty window; fix the keys and advance the data key
to recover. Index-based mode remains allocation-free with respect to dataset size.

VirtualWrap also accepts `.viewability(ListViewability::ItemFraction(0.75))`,
`.minimum_view_time(Duration::from_millis(100))`, and
`.on_viewable_items_change(|change| { /* current/previous flat indices */ })`.
Policies measure vertical overlap; complete items always qualify. Shared
`UniformWrap::viewable_items` returns an O(1) flat range excluding row gaps,
overscan, prediction and final-row padding. Continuous dwell uses the same
VirtualGrid and VirtualWrap also retain mounted focus, valid text selection and portal owners. Retained cells follow current geometry and stable business keys without changing viewport reports or scrolling. The requested window has priority within `max_mounted_cells`; offscreen owners use only the remaining capacity. Hidden/invalid geometry and removed identities release owners.

viewport-bounded tracker and node-owned deadline scheduler as VirtualList, with
no timer/observer unless a callback is configured and no idle wake. Count/data-key
changes, invalid geometry and hidden/offscreen suspension restart dwell; reflow
retains time only for indices that still qualify. Events use data indices even
with business keys; ancestor clipping and overlay occlusion are not subtracted.

VirtualList and VirtualWrap share `ListLoadEdges` in `jadix_core` for page requests:
`.on_load_more(callback)` and `.on_load_more_at_start(callback)` use actual viewport
geometry, excluding overscan/prediction. `.load_more_threshold(viewports)` defaults
to 1 for both edges; `.load_more_at_start_threshold(viewports)` overrides only the
logical start, independently of setter order. Zero waits for the edge; invalid
values retain the current setting. Reversed lists apply thresholds to dataset
order on either axis. Each approach requests once; count/data-key changes rearm
nearby edges. A measured empty viewport requests its first page only at the end.
Missing viewport/invalid wrap geometry/capacity errors suppress requests and rearm
on recovery. Callbacks can read the current handle and update data; applications
own loading, exhaustion and retry guards. These hooks do not start network work.


VirtualWrap supports `.initial_scroll_at_end()`, `.follow_end(bool)` and
`.follow_end_threshold(px)` with the same opt-in intent as VirtualList. The initial
snap waits for successful, nonempty measured layout; handle navigation wins and
settles that initial request. Following compares the current user offset with the
last successful visible extent before key/column/row-height restoration. Appends,
column reflow, item-height and viewport changes then keep an end-anchored reader at
the new end; leaving the threshold suspends following until returning. Idle sync
near the threshold does not snap. Invalid thresholds retain the setting; a later
`.follow_end(false)` disables following. Empty/hidden/error layouts cannot consume
the initial request. Hidden/error layouts do not replace the last successful
follow geometry. Failed layouts do not publish scroll corrections.
The policy adds constant state and no scan, timer or additional mounted items.

VirtualWrap accepts `.content_align(ListContentAlign::Start | Center | End)` for rows
shorter than the viewport. Center/End fill its height, including empty data; Start
keeps the natural extent. Shared `UniformWrap::with_content_align` applies the same
physical coordinates to item/handle origins, navigation, visibility, prediction and
content size. Reapply the shared geometry option after a viewport resize; the
component does this automatically. This policy does not enable initial end
navigation or following. Configure those separately with the APIs above.


VirtualWrap supports full-width scrolling `.header(height, content)`,
`.footer(height, content)` and `.empty(height, content)`. Empty content appears
between the header and footer only with zero data items. Heights include desired
spacing; `item_gap` applies only between data rows/columns. Up to three authored
nodes retain identity across data changes, hidden viewports and error recovery;
they are outside data indices, viewability callbacks and the mounted-cell budget.
Shared `UniformWrap::with_content([before, after], viewport, align)` keeps total
size, navigation, anchors, prediction and following in physical coordinates.
`content()` exposes geometry including alignment padding; `grid()` stays natural.
Alignment applies to the complete content. Slots are parked until the viewport
is measured and while geometry is invalid. An empty wrap retains its available
width for full-width authored content, with no data rows or columns.


VirtualList `.use_parent_scroll()` embeds full-length content in the nearest registered
same-axis scroll ancestor, including through orthogonal scroll containers. It uses
natural layout origins, signed local offsets, the full ancestor viewport for
visibility/dwell/loading, and ancestor-space handle/focus navigation. Offscreen
lists mount no ordinary overscan rows; explicit retained rows remain mounted.
Measurements compensate only rows wholly above the viewport; whole-list changes
above it also compensate. Inverted ordering, initial/follow-end and key-reorder
anchoring are disabled. A missing or hidden ancestor defers navigation; a window
viewport is not an implicit fallback. The builder's fixed viewport height and
private scrollbar settings do not apply in this mode.

VirtualList automatically retains mounted rows owning keyboard focus, a valid
nonempty selectable-text range, or a declared portal (including its source slot
after hoisting). These sparse rows keep their natural positions without changing
scrolling or visibility callbacks, and are released when interaction ends. Stable
keys resolve moved rows; removed keys and invalid selections do not pin. Content
fingerprint changes still rebuild row content; automatic retention does not make
unkeyed dataset replacement preserve identity. The shared
`jadix::utils::virtual_list::subtree_needs_virtual_retention` query is also available
to other virtualization adapters. Work follows mounted subtrees; unchanged keyed
rows use their cached index, measured moves use the axis map, and uniform-key
moves share a single dataset scan.

VirtualList `.retained_indices_fn(|| vec![overlay_host, selection_endpoint])`
keeps explicit data rows mounted outside the viewport without scrolling to them.
It is polled each frame; duplicate and invalid indices are ignored. Disjoint
spacers preserve natural positions and full content size, mounting only requested
outliers plus the ordinary window and active sticky header. Retention also works
on either axis, with measured extents and inverted ordering. Visibility and dwell
callbacks still use the viewport. Callers update retained indices when data moves;
`item_key_fn` preserves surviving mounted nodes. Release indices when their state
is no longer needed; this does not automatically discover open overlays or
selection endpoints and intentionally imposes no hidden retention limit.

VirtualList supports `.sticky_header_indices([0, 20, 50])` on either axis, with
fixed or measured row extents. `.sticky_header_offset(px)` (or its `_fn` variant)
sets a signed viewport inset. Only the active header is additionally mounted;
next-header push-off and content-end clamping reuse shared geometry. Changing
translation retains row nodes and does not call `render_item`. List-owned wrappers
preserve authored child styles and natural row measurement. Dynamic indices via
`sticky_header_indices_fn` refresh only on count/data-key changes: update the data
key after section changes. Handle `sticky_header()` reports the last pin; visibility
and dwell callbacks still describe natural overlap, without overlay occlusion.

With `item_key_fn`, VirtualList preserves the leading keyed row and depth by
default across prepends, deletions and reorders, on both axes and in measured
mode. Deleted anchors choose the next surviving old row, then the predecessor,
at its start; full replacement keeps pixels subject to end clamping. Use
`.maintain_visible_content_position(false)` to retain pixel position during key
mutations. Same-count edits require a new `data_key`. Explicit navigation and
`follow_end` take priority. Unchanged frames reuse keys; only mutation builds
the temporary lookup. Uniform geometry remains O(1), with O(N) key storage when
keys are configured. Viewport/motion offsets rebase together.

VirtualList `.header(extent, node)`, `.footer(extent, node)` and `.empty(extent, node)`
add scrolling content on either axis, with fixed or measured rows. Each extent is
the full main-axis size including desired slot spacing; `item_gap` only separates
data rows. Supply content that fits that size. Empty content appears between the
slots only when there are no rows and retains its node while parked. Inverted
lists reverse the header/footer positions together with the dataset. Slots remain
mounted (at most three), outside row measurements and reuse counters; jumps,
anchors, viewability and sticky rows share their complete scrolling coordinates.
The last sticky row stops before the footer. Slot-only viewports report no rows.
Automatic slot measurement and implicit inter-slot gaps are not provided.

VirtualList `.content_align(ListContentAlign::Center)` or `End` aligns short content
on either axis, for fixed and measured rows. It reuses spacers and mounted rows as
the viewport or measurements change. `row_offset` includes leading padding; its
`index == count` sentinel and `viewport().total_extent` include both ends. Empty
aligned content fills the viewport. Visibility/position callbacks still describe
rows, and short aligned headers stay at their natural position until the configured
sticky inset reaches them. The default Start preserves natural extent.
`.follow_end_threshold(10.0)` enables following when the user was within ten logical
units of the previous end; scrolling farther away suspends it. Invalid distances
retain the configuration. `.follow_end(false)` disables it; `.follow_end(true)`
retains its threshold (zero by default). Alignment, following and
`.initial_scroll_at_end()` are independent. Combine End alignment, initial end and
following for a chat list. Explicit navigation continues to take priority.

VirtualList `.inverted()` places logical item 0 at the bottom/right, opens there,
and end-aligns short or empty content. It supports both axes, measured extents,
gaps, focus and handle navigation. Render/key/focus callbacks, viewport ranges,
viewability membership and loading edges retain dataset indices; the position
callback reports the visually leading row and depth from its top/left edge.
Offsets, jump alignment and `scroll_to_end()` retain visual coordinates; end is
bottom/right. Stable keys preserve the leading visible key during mutations.
Without authored keys, fixed-size lists preserve end distance in O(1) space;
measured lists reuse the existing measurement index with logical index identities.
Same-count data edits still need a new data key. End pinning defaults on and can
be disabled with `follow_end(false)`; unkeyed fixed-size end-distance retention
still applies. Explicit jumps take priority. Sticky headers are disabled in this
mode. Inversion is fixed at construction and does not transform authored children.

VirtualList `.scroll_ahead(true)` optionally adds up to one viewport of directional
pre-rendering on top of `.overscan(n)`. Both axes, measured rows and inverted lists
use the shared `ListScrollAhead` heuristic; stationary syncs retain mounted rows.
Dataset revisions, resizing, anchoring and jumps rebase prediction. Visibility,
viewability and load-more callbacks still use the actual viewport. This bounds the
extra pixel span, not the count of arbitrarily small measured rows. Disabled by default.

VirtualGrid and VirtualWrap also accept `.scroll_ahead(true)`, disabled by default.
Grid prediction follows each axis independently; wrap prediction is vertical.
Both reuse the shared core predictor and bounded window queries. Prediction
exceeding `max_mounted_cells` falls back to the normal window so visible cells
remain mounted. Overscan itself still counts against that budget. Width/height,
metrics/count changes, data revisions, anchor corrections and navigation reset
history; idle syncs retain nodes. Handles and viewport callbacks report actual
visibility, excluding prediction. Wrap reflow keeps its existing item anchor.
