#![allow(non_snake_case)]

use ui_runtime::authoring::{BaseComponentBuilder, SlotMap};
use ui_runtime::core::tree::Element;

pub mod board_drag;

pub mod runtime {
    pub use ui_runtime::{app, authoring, core, style};
}

#[ui_runtime::component(crate_path = "runtime")]
pub fn ExternalLeaf<Value: Clone + Default + ToString>(
    value: Value,
    children: Option<Element>,
) -> Element {
    Element::new()
        .text(value.to_string())
        .children(Element::unpack_fragment_children(children))
}

#[ui_runtime::component(crate_path = "runtime")]
pub fn ExplicitBase(
    value: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    let builder = BaseComponentBuilder::new(Element::new().text(value));
    base_builder.map_or_else(
        || Element::new().text("no base"),
        |style| style(builder).finish(),
    )
}

#[ui_runtime::component(reactive, crate_path = "runtime")]
pub fn ReactiveLeaf(id: String, value: ui_runtime::core::reactivity::Signal<String>) -> Element {
    let _ = id;
    Element::new().text(value.get())
}

pub struct SurfaceBuilder {
    slots: SlotMap,
}

pub fn surface() -> SurfaceBuilder {
    SurfaceBuilder {
        slots: SlotMap::default(),
    }
}

impl SurfaceBuilder {
    ui_runtime::impl_slot_map_methods!();
    ui_runtime::impl_overlay_content_slot!("content");

    pub fn finish(self) -> Element {
        self.slots.paint("content", Element::new())
    }
}

#[ui_runtime::component(crate_path = "runtime")]
pub fn SlotLeaf(children: Option<Element>) -> Element {
    Element::new().children(Element::unpack_fragment_children(children))
}

ui_runtime::impl_simple_slot_builder!(LeafBuilder, leaf, SlotLeaf, SlotLeafProps);

thread_local! {
    static DRAG_POSITION: std::cell::Cell<Option<[f32; 2]>> = const { std::cell::Cell::new(None) };
}

pub fn start_external_drag() {
    use ui_runtime::utils::drag::routing::{self, DragHandlers, ScrollPolicy};
    DRAG_POSITION.with(|position| position.set(Some([0.0, 0.0])));
    routing::register(
        module_path!(),
        DragHandlers::new(
            |x, y| {
                DRAG_POSITION.with(|position| position.set(Some([x, y])));
                true
            },
            || {
                routing::unregister(module_path!());
                DRAG_POSITION.with(|position| position.set(None));
            },
        )
        .scroll_policy(ScrollPolicy::BlockPan),
    );
}

pub fn external_drag_position() -> Option<[f32; 2]> {
    DRAG_POSITION.with(|position| position.get())
}

#[doc(hidden)]
pub use ui_runtime as __runtime;

pub mod product {
    use ui_runtime::core::reactivity::Signal;
    use ui_runtime::core::tree::Element;

    pub fn progress_scope(
        id: String, value: Option<Signal<f32>>, initial: f32, orientation: String,
        children: impl FnOnce() -> Element,
    ) -> Element {
        let mut root = Element::new().child(children());
        root.set_attr("external-progress", id);
        root.set_attr("orientation", orientation);
        root.set_attr("value", value.map_or(initial, |value| value.peek()).to_string());
        root
    }
}

#[macro_export]
macro_rules! ui {
    ($($tokens:tt)*) => {
        $crate::__runtime::__ui_with_runtime!(
            $crate::__runtime; components = $crate::product; $($tokens)*
        )
    };
}
