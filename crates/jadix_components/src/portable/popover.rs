use super::{button, Base, Element, Signal};
use crate::authoring::SlotChrome;
use crate::core::reactivity::{
    component_root, create_context, provide_context, use_context, Context,
};
use jadix::{presentation::PopoverPlacement, PopoverBinding};
use std::sync::OnceLock;

#[derive(Clone)]
pub struct PopoverCtx {
    pub open: Signal<bool>,
    pub id: String,
    pub trigger_fid: Signal<String>,
}

fn popover_context() -> Context<PopoverCtx> {
    static CONTEXT: OnceLock<Context<PopoverCtx>> = OnceLock::new();
    *CONTEXT.get_or_init(create_context::<PopoverCtx>)
}

pub fn use_popover() -> Option<PopoverCtx> {
    use_context(popover_context())
}
pub fn popover_close() {
    if let Some(context) = use_popover() {
        context.open.set(false);
    }
}
pub fn popover_open() {
    if let Some(context) = use_popover() {
        context.open.set(true);
    }
}

pub fn popover_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = id.into();
    component_root(|| {
        let open = open
            .unwrap_or_else(|| crate::use_scoped_signal(&format!("{id}__popover_open"), || false));
        let trigger_fid = crate::use_scoped_signal(&format!("{id}__popover_trigger_fid"), || {
            format!("{id}__popover_trigger")
        });
        provide_context(
            popover_context(),
            PopoverCtx {
                open,
                id: id.clone(),
                trigger_fid,
            },
        );
        let mut root = body();
        if root.template_id.is_none() {
            root.template_id = Some(format!("{id}__popover"));
        }
        root
    })
}

pub struct PopoverBuilder {
    id: String,
    open: Option<Signal<bool>>,
}
pub fn popover(id: impl Into<String>) -> PopoverBuilder {
    PopoverBuilder {
        id: id.into(),
        open: None,
    }
}
impl PopoverBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }
    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        popover_scope(self.id, self.open, body)
    }
}

#[derive(Default)]
pub struct PopoverTriggerProps {
    pub id: String,
    pub children: Option<Element>,
    pub base: Base,
}
#[allow(non_snake_case)]
pub fn PopoverTrigger(props: PopoverTriggerProps) -> Element {
    let context = use_popover().expect("PopoverTrigger requires Popover");
    let mut element = props
        .children
        .unwrap_or_else(|| Element::new().text("Open"));
    while element
        .attrs
        .get("jadix_fragment")
        .is_some_and(|value| value == "1")
        && element.children.len() == 1
    {
        element = element.children.pop().unwrap();
    }
    let id = if props.id.is_empty() {
        element
            .template_id
            .clone()
            .unwrap_or_else(|| format!("{}__popover_trigger", context.id))
    } else {
        props.id
    };
    if !matches!(
        element.descriptor.content,
        jadix::presentation::NodeContent::Button { .. }
    ) {
        element = button(&id).child(element).into();
    }
    element.template_id = Some(id.clone());
    let previous = element.on_click.take();
    let open = context.open;
    element = element.on_click(move || {
        if let Some(previous) = &previous {
            (previous.borrow_mut())();
        }
        open.set(!open.peek());
    });
    if let Some(base) = props.base {
        element = base(element);
    }
    context
        .trigger_fid
        .set(element.template_id.clone().unwrap_or(id));
    element
}

pub struct PopoverContentOpts {
    pub side: String,
    pub align: String,
    pub side_offset: f32,
    pub panel: SlotChrome,
    pub strategy: Option<String>,
    pub width: Option<String>,
    pub lock_page_scroll: bool,
    pub scroll_freeze: Option<jadix::app::portal_overlay::ScrollFreeze>,
}
impl Default for PopoverContentOpts {
    fn default() -> Self {
        Self {
            side: "bottom".into(),
            align: "center".into(),
            side_offset: 4.0,
            panel: SlotChrome::default(),
            strategy: None,
            width: None,
            lock_page_scroll: false,
            scroll_freeze: None,
        }
    }
}

pub fn popover_side_align_to_placement(side: &str, align: &str) -> Option<PopoverPlacement> {
    use PopoverPlacement::*;
    let side = side.trim().to_ascii_lowercase();
    let align = align.trim().to_ascii_lowercase();
    let (side, align) = side.split_once('-').unwrap_or((&side, &align));
    match (side, align) {
        ("top", "start") => Some(TopStart),
        ("top", "center") => Some(Top),
        ("top", "end") => Some(TopEnd),
        ("bottom", "start") => Some(BottomStart),
        ("bottom", "center") => Some(Bottom),
        ("bottom", "end") => Some(BottomEnd),
        ("left", "start") => Some(LeftStart),
        ("left", "center") => Some(Left),
        ("left", "end") => Some(LeftEnd),
        ("right", "start") => Some(RightStart),
        ("right", "center") => Some(Right),
        ("right", "end") => Some(RightEnd),
        _ => None,
    }
}

pub fn popover_content(opts: PopoverContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let context = use_popover().expect("PopoverContent requires Popover");
    let placement = popover_side_align_to_placement(&opts.side, &opts.align);
    let gap_valid = opts.side_offset.is_finite()
        && opts.side_offset >= 0.0
        && opts.side_offset <= f32::from(u16::MAX)
        && opts.side_offset.fract() == 0.0;
    let width = opts.width.as_deref().map(|value| {
        value
            .trim()
            .strip_suffix("px")
            .unwrap_or(value.trim())
            .parse::<f32>()
    });
    let width_valid = width.as_ref().is_none_or(|value| {
        value
            .as_ref()
            .is_ok_and(|value| value.is_finite() && *value >= 0.0)
    });
    let mut element = Element::new()
        .id(format!("{}__popover_content", context.id))
        .w(width.and_then(Result::ok).unwrap_or(288.0))
        .p(16.0)
        .gap(8.0)
        .bg([255; 4]);
    element.descriptor.accessible_name = Some(context.id.clone());
    element.popover = Some(PopoverBinding {
        anchor: context.trigger_fid,
        open: context.open,
        placement: placement.unwrap_or_default(),
        gap: if gap_valid {
            opts.side_offset as u16
        } else {
            0
        },
        close_on_outside_click: true,
    });
    if placement.is_none()
        || !gap_valid
        || !width_valid
        || opts.lock_page_scroll
        || opts
            .scroll_freeze
            .is_some_and(|value| value != jadix::app::portal_overlay::ScrollFreeze::None)
        || opts
            .strategy
            .as_deref()
            .is_some_and(|value| value != "flip")
    {
        element.set_attr("jadix_native_unsupported", "Popover options");
    }
    element.deferred_children = Some(Box::new(body));
    opts.panel.apply(element)
}

macro_rules! popover_slot {
    ($props:ident, $component:ident, $slot:literal, $size:expr) => {
        #[derive(Default)]
        pub struct $props {
            pub children: Option<Element>,
            pub base: Base,
        }
        #[allow(non_snake_case)]
        pub fn $component(props: $props) -> Element {
            let mut element = Element::new().gap(8.0).text_size($size).flex_shrink(0.0);
            element.set_attr("data-slot", $slot);
            if let Some(child) = props.children {
                element = element.child(child);
            }
            match props.base {
                Some(base) => base(element),
                None => element,
            }
        }
    };
}
popover_slot!(PopoverHeaderProps, PopoverHeader, "popover-header", 16.0);
popover_slot!(PopoverTitleProps, PopoverTitle, "popover-title", 18.0);
popover_slot!(
    PopoverDescriptionProps,
    PopoverDescription,
    "popover-description",
    14.0
);
