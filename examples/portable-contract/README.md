# Portable component contract

This windowless example uses the same Rust page module through Builder and `ui!`,
with the existing theme and reactive Owner. It snapshots the registered portable
components, measures leaves through a host callback, runs Taffy outer layout and
commits to the retained native reference model.

Run `cargo run -p jadix_portable_contract`. Its component dependency disables
default features and enables only macros and the theme, so this example does not
link a GPU renderer, window system or JavaScript VM. The `gpu` feature remains
enabled by default for ordinary Jadix applications.

The measurement callback is a deterministic stand-in for platform typography.
The example does not create Android, UIKit or DOM widgets. See the crate README
for lifecycle and measurement obligations when writing a platform adapter.

The Select contract tests also cover basic compound RSX authoring:

```rust,ignore
ui! {
    <Select id="fruit" value={value} placeholder="Choose fruit">
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
            <SelectItem value="apple">"Apple"</SelectItem>
            <SelectItem value="blocked" disabled=true>"Unavailable"</SelectItem>
        </SelectContent>
    </Select>
}
```

These slots become one native Select descriptor; they do not create a custom
popup view tree. Labels must be static text, and numeric trigger chrome may be
specified through w/h or a manual base closure. Styled popup content, grouped
items and compound root default/disabled/open flags are rejected. The options
Builder supports default values and disabled controls. The separate native-controls
sample exercises this compound primary control with platform adapters.

Popover supports Builder scopes and compound RSX with retained native controls:

```rust,ignore
ui! {
    <Popover id="profile" open={open}>
        <PopoverTrigger><Button>"Edit profile"</Button></PopoverTrigger>
        <PopoverContent side="bottom" align="start" sideOffset={4} width="280">
            <PopoverHeader><PopoverTitle>"Profile"</PopoverTitle></PopoverHeader>
            <Input value={value} placeholder="Name" />
        </PopoverContent>
    </Popover>
}
```

The content resolves its actual trigger after projection, builds on first open,
and keeps its controls across close/reopen. popover_close uses the owning context.
The portable contract checks typed placement, revisioned dismissal, lazy retention,
custom trigger identity and unsupported-option rejection. Width accepts numeric/px
strings, sideOffset whole u16 units, and panel.base supplies numeric chrome. Classes,
scroll locks, rem widths and unsupported strategies are rejected. DOM provides the
native HTML Popover host; Android/UIKit remain unsupported. Run the isolated Web
sample with examples/native-controls/build-web.py --popover --release.
