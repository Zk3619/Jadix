# jadix_components

[English](README.md) | [简体中文](README.zh.md)

Jadix native UI components with optional RSX authoring.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

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
