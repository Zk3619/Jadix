use super::{button, Base, Element, Signal};
use jadix::portable::authoring::SlotChrome;
use jadix::portable::core::reactivity::{
    component_root, create_context, provide_context, use_context, Context,
};
use jadix::presentation::NodeContent;
use std::sync::OnceLock;

#[derive(Clone)]
pub struct DialogCtx {
    pub open: Signal<bool>,
    pub id: String,
}

fn dialog_context() -> Context<DialogCtx> {
    static CONTEXT: OnceLock<Context<DialogCtx>> = OnceLock::new();
    *CONTEXT.get_or_init(create_context::<DialogCtx>)
}

pub fn use_dialog() -> Option<DialogCtx> {
    use_context(dialog_context())
}

pub fn dialog_open() {
    if let Some(context) = use_dialog() {
        context.open.set(true);
    }
}

pub fn dialog_close() {
    if let Some(context) = use_dialog() {
        context.open.set(false);
    }
}

pub fn dialog_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = id.into();
    component_root(|| {
        let open = open.unwrap_or_else(|| {
            jadix::portable::use_scoped_signal(&format!("{id}__open"), || false)
        });
        provide_context(dialog_context(), DialogCtx { open, id });
        body()
    })
}

pub struct DialogBuilder {
    id: String,
    open: Option<Signal<bool>>,
}

pub fn dialog(id: impl Into<String>) -> DialogBuilder {
    DialogBuilder {
        id: id.into(),
        open: None,
    }
}

impl DialogBuilder {
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }
    pub fn open_signal(self, open: Signal<bool>) -> Self {
        self.open(open)
    }
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        dialog_scope(self.id, self.open, body)
    }
}

#[derive(Default)]
pub struct DialogTriggerProps {
    pub id: String,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn DialogTrigger(props: DialogTriggerProps) -> Element {
    let context = use_dialog().expect("DialogTrigger requires Dialog");
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
    if !matches!(element.descriptor.content, NodeContent::Button { .. }) {
        element = button(&props.id).child(element).into();
    }
    let previous = element.on_click.take();
    let element = element.on_click(move || {
        if let Some(previous) = &previous {
            (previous.borrow_mut())();
        }
        context.open.set(true);
    });
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}

pub struct DialogContentOpts {
    pub show_close: bool,
    pub overlay_color: Option<String>,
    pub overlay_opacity: Option<f32>,
    pub close_on_overlay_click: bool,
    pub overlay_blur: Option<f32>,
    pub hug_content: bool,
    pub panel: SlotChrome,
}

impl Default for DialogContentOpts {
    fn default() -> Self {
        Self {
            show_close: true,
            overlay_color: None,
            overlay_opacity: None,
            close_on_overlay_click: true,
            overlay_blur: None,
            hug_content: false,
            panel: SlotChrome::default(),
        }
    }
}

pub fn dialog_content(opts: DialogContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let context = use_dialog().expect("DialogContent requires Dialog");
    let mut element = Element::new()
        .id(format!("{}__content", context.id))
        .p(16.0)
        .gap(12.0)
        .bg([255; 4]);
    element.descriptor.content = NodeContent::Dialog {
        open: false,
        close_on_overlay_click: opts.close_on_overlay_click,
        revision: 0,
    };
    element.descriptor.accessible_name = Some(context.id.clone());
    element.dialog_open = Some(context.open);
    if opts.overlay_color.is_some()
        || opts.overlay_opacity.is_some()
        || opts.overlay_blur.is_some()
        || opts.hug_content
        || opts.panel.class.is_some()
    {
        element.set_attr("jadix_native_unsupported", "dialog style options");
    }
    let show_close = opts.show_close;
    element.deferred_children = Some(Box::new(move || {
        let mut content = Element::new().gap(12.0).child(body());
        if show_close {
            let close: Element = button(&format!("{}__close", context.id))
                .on_click(dialog_close)
                .child(Element::new().text("Close"))
                .into();
            content = content.child(close.h(44.0).flex_shrink(0.0));
        }
        content
    }));
    match opts.panel.base {
        Some(base) => base(element),
        None => element,
    }
}

macro_rules! dialog_slot {
    ($props:ident, $component:ident, $slot:literal) => {
        #[derive(Default)]
        pub struct $props {
            pub children: Option<Element>,
            pub base: Base,
        }
        #[allow(non_snake_case)]
        pub fn $component(props: $props) -> Element {
            let mut element = Element::new().gap(8.0).flex_shrink(0.0);
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

dialog_slot!(DialogHeaderProps, DialogHeader, "dialog-header");
dialog_slot!(DialogTitleProps, DialogTitle, "dialog-title");
dialog_slot!(
    DialogDescriptionProps,
    DialogDescription,
    "dialog-description"
);
dialog_slot!(DialogFooterProps, DialogFooter, "dialog-footer");

#[derive(Default)]
pub struct DialogBodyProps {
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn DialogBody(props: DialogBodyProps) -> Element {
    let context = use_dialog().expect("DialogBody requires Dialog");
    let mut element = jadix::portable::primitives::scroll()
        .id(format!("{}__body", context.id))
        .min_h(0.0);
    element.descriptor.style.grow = 1.0;
    if let Some(child) = props.children {
        element = element.child(child.flex_shrink(0.0));
    }
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}
