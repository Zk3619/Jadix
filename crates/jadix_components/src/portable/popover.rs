use super::{button, Base, Element, Signal};
use jadix::portable::authoring::SlotChrome;
use jadix::portable::core::reactivity::{
    component_root, create_context, current_owner, provide_context, use_context, with_owner,
    Context,
};
pub use jadix::portable::PopoverOpenChangeReason;
pub use jadix::presentation::PopoverPosition;
use jadix::{portable::PopoverBinding, presentation::PopoverPlacement};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct PopoverCtx {
    pub open: Signal<bool>,
    pub id: String,
    pub trigger_fid: Signal<String>,
}

type OpenChangeHandler = Rc<dyn Fn(bool, PopoverOpenChangeReason)>;
type CancelHandler = Rc<dyn Fn(PopoverOpenChangeReason)>;
type PositionHandler = Rc<dyn Fn(Option<PopoverPosition>)>;
type LifecycleHandler = Rc<dyn Fn()>;

#[derive(Clone, Default)]
struct OpenChanges {
    controlled: bool,
    handler: Option<OpenChangeHandler>,
    cancel: Option<CancelHandler>,
    position: Option<PositionHandler>,
    opened: Option<LifecycleHandler>,
    closed: Option<LifecycleHandler>,
}

impl OpenChanges {
    fn request(&self, open: Signal<bool>, next: bool, reason: PopoverOpenChangeReason) {
        if !self.controlled {
            open.set(next);
        }
        if let Some(handler) = &self.handler {
            handler(next, reason);
        }
    }
}

fn popover_context() -> Context<(PopoverCtx, OpenChanges)> {
    thread_local! { static CONTEXT: Context<(PopoverCtx, OpenChanges)> = create_context(); }
    CONTEXT.with(|context| *context)
}

pub fn use_popover() -> Option<PopoverCtx> {
    use_context(popover_context()).map(|(context, _)| context)
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
    let mut builder = popover(id);
    builder.open = open;
    builder.child(body)
}

pub struct PopoverBuilder {
    id: String,
    open: Option<Signal<bool>>,
    default_open: bool,
    changes: OpenChanges,
}
pub fn popover(id: impl Into<String>) -> PopoverBuilder {
    PopoverBuilder {
        id: id.into(),
        open: None,
        default_open: false,
        changes: OpenChanges::default(),
    }
}
impl PopoverBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self.changes.controlled = false;
        self
    }
    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }

    /// Use the supplied signal as the source of truth. Apply requests by setting the signal.
    pub fn controlled_open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self.changes.controlled = true;
        self
    }

    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Observe trigger presses and allowed dismissal requests.
    pub fn on_open_change(
        mut self,
        callback: impl FnMut(bool, PopoverOpenChangeReason) + 'static,
    ) -> Self {
        let owner = current_owner();
        let callback = RefCell::new(callback);
        self.changes.handler = Some(Rc::new(move |next, reason| {
            let invoke = || (callback.borrow_mut())(next, reason);
            if let Some(owner) = owner {
                with_owner(owner, invoke);
            } else {
                invoke();
            }
        }));
        self
    }

    /// Observe cancellation attempts, including when the popover stays open.
    pub fn on_cancel(mut self, callback: impl FnMut(PopoverOpenChangeReason) + 'static) -> Self {
        let owner = current_owner();
        let callback = RefCell::new(callback);
        self.changes.cancel = Some(Rc::new(move |reason| {
            let invoke = || (callback.borrow_mut())(reason);
            if let Some(owner) = owner {
                with_owner(owner, invoke);
            } else {
                invoke();
            }
        }));
        self
    }

    /// Observe resolved AppKit placement and anchor visibility changes.
    pub fn on_position_change(
        mut self,
        callback: impl FnMut(Option<PopoverPosition>) + 'static,
    ) -> Self {
        let owner = current_owner();
        let callback = RefCell::new(callback);
        self.changes.position = Some(Rc::new(move |position| {
            let invoke = || (callback.borrow_mut())(position);
            if let Some(owner) = owner {
                with_owner(owner, invoke);
            } else {
                invoke();
            }
        }));
        self
    }

    /// Observe an opening committed by the AppKit host.
    pub fn on_open(mut self, callback: impl FnMut() + 'static) -> Self {
        self.changes.opened = Some(popover_lifecycle_handler(callback));
        self
    }

    /// Observe a closing committed by AppKit while the panel node is retained.
    pub fn on_close(mut self, callback: impl FnMut() + 'static) -> Self {
        self.changes.closed = Some(popover_lifecycle_handler(callback));
        self
    }

    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        let Self {
            id,
            open,
            default_open,
            changes,
        } = self;
        component_root(|| {
            let open = open.unwrap_or_else(|| {
                jadix::portable::use_scoped_signal(&format!("{id}__popover_open"), || default_open)
            });
            let trigger_fid =
                jadix::portable::use_scoped_signal(&format!("{id}__popover_trigger_fid"), || {
                    format!("{id}__popover_trigger")
                });
            let context = PopoverCtx {
                open,
                id: id.clone(),
                trigger_fid,
            };
            provide_context(popover_context(), (context, changes));
            let mut root = body();
            if root.template_id.is_none() {
                root.template_id = Some(format!("{id}__popover"));
            }
            root
        })
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
    let (context, changes) =
        use_context(popover_context()).expect("PopoverTrigger requires Popover");
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
        changes.request(open, !open.peek(), PopoverOpenChangeReason::TriggerPress);
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
    pub margin: Option<u16>,
    pub close_on_escape: bool,
    pub close_on_outside_click: bool,
    pub enter_duration: Option<u16>,
    pub exit_duration: Option<u16>,
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
            margin: None,
            close_on_escape: true,
            close_on_outside_click: true,
            enter_duration: None,
            exit_duration: None,
            panel: SlotChrome::default(),
            strategy: None,
            width: None,
            lock_page_scroll: false,
            scroll_freeze: None,
        }
    }
}

impl PopoverContentOpts {
    #[doc(hidden)]
    pub fn native_panel_options(
        mut self,
        margin: Option<u16>,
        close_on_escape: bool,
        close_on_outside_click: bool,
        enter_duration: Option<u16>,
        exit_duration: Option<u16>,
    ) -> Self {
        self.margin = margin;
        self.close_on_escape = close_on_escape;
        self.close_on_outside_click = close_on_outside_click;
        self.enter_duration = enter_duration;
        self.exit_duration = exit_duration;
        self
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
    let (context, changes) =
        use_context(popover_context()).expect("PopoverContent requires Popover");
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
    let open = context.open;
    let opened = changes.opened.clone();
    let closed = changes.closed.clone();
    let cancel = changes.cancel.clone();
    let position = changes.position.clone();
    let mut element = Element::new()
        .id(format!("{}__popover_content", context.id))
        .w(width.and_then(Result::ok).unwrap_or(288.0))
        .p(16.0)
        .gap(8.0)
        .bg([255; 4])
        .on_popover_dismiss(move |reason| changes.request(open, false, reason));
    if let Some(cancel) = cancel {
        element = element.on_popover_cancel(move |reason| cancel(reason));
    }
    if let Some(position) = position {
        element = element.on_popover_position(move |geometry| position(geometry));
    }
    if opened.is_some() || closed.is_some() {
        element = element.on_popover_lifecycle(move |open| {
            if let Some(callback) = if open { &opened } else { &closed } {
                callback();
            }
        });
    }
    element.descriptor.accessible_name = Some(context.id.clone());
    element.descriptor.style.popover_margin = opts.margin;
    element.descriptor.style.popover_close_on_escape = (!opts.close_on_escape).then_some(false);
    element.descriptor.style.popover_enter_ms = opts.enter_duration;
    element.descriptor.style.popover_exit_ms = opts.exit_duration;
    element.popover = Some(PopoverBinding {
        anchor: context.trigger_fid,
        open: context.open,
        placement: placement.unwrap_or_default(),
        gap: if gap_valid {
            opts.side_offset as u16
        } else {
            0
        },
        close_on_outside_click: opts.close_on_outside_click,
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

fn popover_lifecycle_handler(callback: impl FnMut() + 'static) -> LifecycleHandler {
    let owner = current_owner();
    let callback = RefCell::new(callback);
    Rc::new(move || {
        let invoke = || (callback.borrow_mut())();
        if let Some(owner) = owner {
            with_owner(owner, invoke);
        } else {
            invoke();
        }
    })
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
