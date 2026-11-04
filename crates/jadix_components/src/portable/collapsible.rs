use crate::{Element, Signal};
use jadix::{
    core::reactivity::{
        component_root, create_context, create_effect, on_cleanup, provide_context, use_context,
        Context,
    },
    presentation::NodeContent,
};
use std::rc::Rc;

#[derive(Clone)]
pub struct CollapsibleCtx {
    pub id: String,
    pub open: Signal<bool>,
    pub disabled: bool,
}

fn context() -> Context<CollapsibleCtx> {
    thread_local! { static CONTEXT: Context<CollapsibleCtx> = create_context(); }
    CONTEXT.with(|context| *context)
}

pub fn use_collapsible() -> Option<CollapsibleCtx> {
    use_context(context())
}

fn required() -> CollapsibleCtx {
    use_collapsible()
        .expect("Collapsible part requires collapsible_scope or collapsible(...).child(...)")
}

fn bind_exclusive(id: String, open: Signal<bool>, winner: Signal<Option<String>>) {
    if open.get() {
        match winner.get() {
            None => winner.set(Some(id.clone())),
            Some(current) if current != id => open.set(false),
            _ => {}
        }
    }
    let opening = id.clone();
    create_effect(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };
        let Some(current) = winner.try_peek() else {
            return;
        };
        if is_open && current.as_deref() != Some(opening.as_str()) {
            let _ = winner.try_set(Some(opening.clone()));
        } else if !is_open && current.as_deref() == Some(opening.as_str()) {
            let _ = winner.try_set(None);
        }
    });
    let following = id.clone();
    create_effect(move || {
        if let Some(Some(current)) = winner.try_get() {
            if current != following && open.try_peek() == Some(true) {
                let _ = open.try_set(false);
            }
        }
    });
    on_cleanup(move || {
        if winner
            .try_peek()
            .as_ref()
            .and_then(|value| value.as_deref())
            == Some(id.as_str())
        {
            let _ = winner.try_set(None);
        }
    });
}

pub fn collapsible_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    default_open: bool,
    disabled: bool,
    exclusive: bool,
    group: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = id.into();
    let group = group.filter(|group| !group.trim().is_empty());
    let winner = if exclusive {
        group.as_ref().map(|group| {
            crate::use_scoped_signal(&format!("__jadix_native_collapsible_group:{group}"), || {
                None::<String>
            })
        })
    } else {
        None
    };
    let unsupported = exclusive != group.is_some();
    component_root(|| {
        let open = open.unwrap_or_else(|| {
            crate::use_scoped_signal(&format!("{id}__collapsible"), || default_open)
        });
        if let Some(winner) = winner {
            bind_exclusive(id.clone(), open, winner);
        }
        provide_context(
            context(),
            CollapsibleCtx {
                id: id.clone(),
                open,
                disabled,
            },
        );
        let mut root = Element::new().id(id).flex_col().child(body());
        root.set_attr("data-slot", "collapsible");
        if unsupported {
            root.set_attr(
                "jadix_native_unsupported",
                "Collapsible exclusive requires group",
            );
        }
        root
    })
}

pub struct CollapsibleBuilder {
    id: String,
    open: Option<Signal<bool>>,
    default_open: bool,
    disabled: bool,
    exclusive: bool,
    group: Option<String>,
    trigger: Option<Box<dyn FnOnce() -> Element>>,
    content: Option<Box<dyn Fn() -> Element>>,
}

pub fn collapsible(id: impl Into<String>) -> CollapsibleBuilder {
    CollapsibleBuilder {
        id: id.into(),
        open: None,
        default_open: false,
        disabled: false,
        exclusive: false,
        group: None,
        trigger: None,
        content: None,
    }
}

impl CollapsibleBuilder {
    pub fn open_signal(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }
    pub fn open(self, open: Signal<bool>) -> Self {
        self.open_signal(open)
    }
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn exclusive(mut self, exclusive: bool) -> Self {
        self.exclusive = exclusive;
        self
    }
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
    pub fn trigger(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.trigger = Some(Box::new(body));
        self
    }
    pub fn content(mut self, body: impl Fn() -> Element + 'static) -> Self {
        self.content = Some(Box::new(body));
        self
    }
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        collapsible_scope(
            self.id,
            self.open,
            self.default_open,
            self.disabled,
            self.exclusive,
            self.group,
            body,
        )
    }
}

impl From<CollapsibleBuilder> for Element {
    fn from(builder: CollapsibleBuilder) -> Self {
        let CollapsibleBuilder {
            id,
            open,
            default_open,
            disabled,
            exclusive,
            group,
            trigger,
            content,
        } = builder;
        collapsible_scope(id, open, default_open, disabled, exclusive, group, || {
            let mut children = Vec::new();
            if let Some(trigger) = trigger {
                children.push(CollapsibleTrigger(CollapsibleTriggerProps {
                    children: Some(trigger()),
                    ..Default::default()
                }));
            }
            if let Some(content) = content {
                children.push(collapsible_content(content));
            }
            Element::new().children(children)
        })
    }
}

impl crate::core::tree::IntoElementList for CollapsibleBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[derive(Default)]
pub struct CollapsibleTriggerProps {
    pub disabled: Option<bool>,
    pub children: Option<Element>,
    pub base: super::Base,
}

#[allow(non_snake_case)]
pub fn CollapsibleTrigger(props: CollapsibleTriggerProps) -> Element {
    let context = required();
    let enabled = !context.disabled && !props.disabled.unwrap_or(false);
    let open = context.open;
    let mut element = Element::new()
        .id(format!("{}:trigger", context.id))
        .on_click(move || {
            if enabled {
                open.set(!open.get());
            }
        });
    element.descriptor.content = NodeContent::Button {
        label: String::new(),
        enabled,
    };
    element.descriptor.accessible_expanded = Some(open.get());
    element.descriptor_binding = Some(Rc::new(move |descriptor| {
        descriptor.accessible_expanded = Some(open.get());
    }));
    element.set_attr("data-slot", "collapsible-trigger");
    if let Some(child) = props.children {
        element.children.push(child);
    }
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}

pub struct CollapsibleTriggerBuilder {
    props: CollapsibleTriggerProps,
}

pub fn collapsible_trigger() -> CollapsibleTriggerBuilder {
    CollapsibleTriggerBuilder {
        props: CollapsibleTriggerProps::default(),
    }
}

impl CollapsibleTriggerBuilder {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.props.disabled = Some(disabled);
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.props
            .children
            .get_or_insert_with(Element::new)
            .children
            .push(child.into());
        self
    }
    pub fn text(self, text: impl Into<String>) -> Self {
        self.child(Element::new().text(text.into()))
    }
}

impl From<CollapsibleTriggerBuilder> for Element {
    fn from(builder: CollapsibleTriggerBuilder) -> Self {
        CollapsibleTrigger(builder.props)
    }
}

impl crate::core::tree::IntoElementList for CollapsibleTriggerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub fn collapsible_content(body: impl Fn() -> Element + 'static) -> Element {
    let context = required();
    let open = context.open;
    let mut element = Element::new()
        .id(format!("{}:content", context.id))
        .flex_col();
    element.set_attr("data-slot", "collapsible-content");
    element.focus_restore_target = Some(format!("{}:trigger", context.id));
    element.visibility = Some(Rc::new(move || open.get()));
    element.deferred_children = Some(Box::new(body));
    element
}
