# ScalePreview

`ScalePreview` is a reusable component built on framework-level scaled-viewport
capabilities, not a layout primitive or a DevTools-only feature. Both layers are
available in normal and release builds without enabling `devtools`.

## Layering

- `core::scaled_viewport`: configure logical dimensions, transforms, clipping and
  local portal routing on an existing `Element`. DevTools consumes this layer
  directly and supplies its own frame and toolbar.
- `components::scale_preview`: `ScalePreview`, `ScalePreviewBuilder` and
  `scale_preview` assemble a ready-made frame and reserve its scaled footprint.
- `primitives`: atomic layout/rendering building blocks such as Box and stacks;
  it does not own the preview component.

The same distinction applies to scrolling: `core::scroll_viewport` and
`core::scrollbar` own activation, signals, geometry and paint/input contracts;
`components::scroll_viewport` and `components::scrollbar` own their builders,
RSX tags, appearance and theme defaults. Neither core module re-exports the UI
builders. Existing `components` and `prelude` entry points remain unchanged.

The viewport lays out its children at a logical width and height. The surrounding
layout reserves only the scaled footprint; painting, text and pointer hit testing
use the same transform. Default portals stay inside the preview and inherit its
scale and clipping. Explicit `Portal mount` targets remain unchanged. Multiple
and nested previews have separate portal hosts. Responsive utility classes use
the logical viewport width rather than the window width.

```rust,ignore
use jadix::prelude::*;

let preview: Element = scale_preview(390.0, 844.0, 0.5)
    .child(page)
    .into();
```

The equivalent tag syntax is:

```rust,ignore
ui! {
    <ScalePreview viewport_width={390.0} viewport_height={844.0} scale={0.5}>
        <Box w="100%" h="100%">"Preview content"</Box>
    </ScalePreview>
}
```

`scale` defaults to `1.0` in tag syntax. The accepted range is `0.01..=8.0`.
Non-finite or non-positive scales fall back to `1.0`; invalid viewport dimensions
fall back to `390 x 844`. To fit a preview into an available area, pass
`(available_width / viewport_width).min(available_height / viewport_height)`.
DevTools uses the same viewport and portal implementation, adding its toolbar,
device presets, fit calculation and drag handles.

This is a live layout preview, not a screenshot, OS/device emulator or a native
child-window scaling API.

To reuse only the framework capability, configure an existing viewport:

```rust,ignore
use jadix::core::scaled_viewport::configure_viewport;
use jadix::prelude::Element;

let mut viewport = Element::new().child(page);
configure_viewport(&mut viewport, [390.0, 844.0], 0.5, "my-preview-portals");
```

This lower-level function expects finite positive dimensions and scale, plus a
portal mount id unique in the tree. The caller controls the outer frame and its
layout footprint; the component handles those details automatically.
