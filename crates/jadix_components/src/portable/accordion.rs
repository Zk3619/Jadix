use crate::{Element, Signal};
use jadix::{
    core::reactivity::{component_root, create_context, provide_context, use_context, Context},
    presentation::{NavigationKey, NodeContent},
};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

#[derive(Clone)]
struct Entry {
    value: String,
    id: String,
    enabled: bool,
}

#[derive(Clone)]
pub struct AccordionCtx {
    pub id: String,
    pub value: Signal<Vec<String>>,
    pub multiple: bool,
    pub disabled: bool,
    entries: Rc<RefCell<Vec<Entry>>>,
}

#[derive(Clone)]
pub struct AccordionItemCtx {
    pub value: String,
    pub disabled: bool,
}

fn context() -> Context<AccordionCtx> {
    thread_local! { static CONTEXT: Context<AccordionCtx> = create_context(); }
    CONTEXT.with(|context| *context)
}

fn item_context() -> Context<AccordionItemCtx> {
    thread_local! { static CONTEXT: Context<AccordionItemCtx> = create_context(); }
    CONTEXT.with(|context| *context)
}

pub fn use_accordion() -> Option<AccordionCtx> {
    use_context(context())
}

pub fn use_accordion_item() -> Option<AccordionItemCtx> {
    use_context(item_context())
}

fn required() -> AccordionCtx {
    use_accordion().expect("Accordion part requires accordion_scope or accordion(...).child(...)")
}

fn required_item() -> AccordionItemCtx {
    use_accordion_item().expect("Accordion Trigger/Content requires AccordionItem")
}

pub fn accordion_scope(
    id: impl Into<String>,
    value: Option<Signal<Vec<String>>>,
    default_value: Vec<String>,
    multiple: bool,
    disabled: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = id.into();
    component_root(|| {
        let value = value.unwrap_or_else(|| {
            crate::use_scoped_signal(&format!("{id}__accordion"), || default_value)
        });
        let context = AccordionCtx {
            id: id.clone(),
            value,
            multiple,
            disabled,
            entries: Rc::new(RefCell::new(Vec::new())),
        };
        provide_context(self::context(), context.clone());
        let mut root = Element::new().id(id).flex_col().child(body());
        root.set_attr("data-slot", "accordion");
        let entries = context.entries.borrow();
        let inventory: BTreeSet<_> = entries.iter().map(|entry| entry.value.as_str()).collect();
        let selected = value.get();
        if inventory.len() != entries.len()
            || entries
                .iter()
                .any(|entry| entry.value.is_empty() || entry.value.len() > 256)
            || (!multiple && selected.len() > 1)
            || selected
                .iter()
                .any(|value| !inventory.contains(value.as_str()))
            || selected.iter().collect::<BTreeSet<_>>().len() != selected.len()
        {
            root.set_attr("jadix_native_unsupported", "Accordion value inventory");
        }
        root
    })
}

pub fn accordion_item_scope(
    value: impl Into<String>,
    disabled: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    let accordion = required();
    let value = value.into();
    component_root(|| {
        provide_context(
            item_context(),
            AccordionItemCtx {
                value: value.clone(),
                disabled,
            },
        );
        let mut root = Element::new()
            .id(format!("{}:item:{value}", accordion.id))
            .flex_col()
            .child(body());
        root.set_attr("data-slot", "accordion-item");
        root
    })
}

#[derive(Default)]
pub struct AccordionTriggerProps {
    pub disabled: Option<bool>,
    pub children: Option<Element>,
    pub base: super::Base,
}

#[allow(non_snake_case)]
pub fn AccordionTrigger(props: AccordionTriggerProps) -> Element {
    let context = required();
    let item = required_item();
    let enabled = !context.disabled && !item.disabled && !props.disabled.unwrap_or(false);
    let id = format!("{}:trigger:{}", context.id, item.value);
    context.entries.borrow_mut().push(Entry {
        value: item.value.clone(),
        id: id.clone(),
        enabled,
    });
    let selected = context.value;
    let activate = item.value.clone();
    let multiple = context.multiple;
    let mut element = Element::new().id(id).on_click(move || {
        if enabled {
            let mut values = selected.get();
            if values.contains(&activate) {
                values.retain(|value| value != &activate);
            } else if multiple {
                values.push(activate.clone());
            } else {
                values = vec![activate.clone()];
            }
            selected.set(values);
        }
    });
    element.descriptor.content = NodeContent::Button {
        label: String::new(),
        enabled,
    };
    element.descriptor.accessible_expanded = Some(selected.get().contains(&item.value));
    element.descriptor.style.height = Some(34.0);
    let value = item.value.clone();
    element.descriptor_binding = Some(Rc::new(move |descriptor| {
        descriptor.accessible_expanded = Some(selected.get().contains(&value));
    }));
    element.set_attr("data-slot", "accordion-trigger");
    element.children.push(
        props
            .children
            .unwrap_or_else(|| Element::new().text(&item.value)),
    );
    if enabled {
        let entries = context.entries.clone();
        let value = item.value;
        element.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| {
            let entries = entries.borrow();
            let enabled: Vec<_> = entries.iter().filter(|entry| entry.enabled).collect();
            let current = enabled.iter().position(|entry| entry.value == value)?;
            let next = match key {
                NavigationKey::Home => 0,
                NavigationKey::End => enabled.len() - 1,
                NavigationKey::Up => (current + enabled.len() - 1) % enabled.len(),
                NavigationKey::Down => (current + 1) % enabled.len(),
                _ => return None,
            };
            Some(enabled[next].id.clone())
        }))));
    }
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}

pub fn accordion_content(body: impl Fn() -> Element + 'static) -> Element {
    let context = required();
    let item = required_item();
    let mut element = Element::new()
        .id(format!("{}:content:{}", context.id, item.value))
        .flex_col();
    element.set_attr("data-slot", "accordion-content");
    element.focus_restore_target = Some(format!("{}:trigger:{}", context.id, item.value));
    element.visibility = Some(Rc::new(move || context.value.get().contains(&item.value)));
    element.deferred_children = Some(Box::new(body));
    element
}

pub struct AccordionTriggerBuilder {
    props: AccordionTriggerProps,
}

pub fn accordion_trigger() -> AccordionTriggerBuilder {
    AccordionTriggerBuilder {
        props: AccordionTriggerProps::default(),
    }
}

impl AccordionTriggerBuilder {
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

impl From<AccordionTriggerBuilder> for Element {
    fn from(builder: AccordionTriggerBuilder) -> Self {
        AccordionTrigger(builder.props)
    }
}

impl crate::core::tree::IntoElementList for AccordionTriggerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct AccordionBuilder {
    id: String,
    value: Option<Signal<Vec<String>>>,
    default_value: Vec<String>,
    multiple: bool,
    disabled: bool,
    items: Vec<(String, String, bool, Box<dyn Fn() -> Element>)>,
    body: Option<Box<dyn FnOnce() -> Element>>,
}

pub fn accordion(id: impl Into<String>) -> AccordionBuilder {
    AccordionBuilder {
        id: id.into(),
        value: None,
        default_value: Vec::new(),
        multiple: false,
        disabled: false,
        items: Vec::new(),
        body: None,
    }
}

impl AccordionBuilder {
    pub fn value_signal(mut self, value: Signal<Vec<String>>) -> Self {
        self.value = Some(value);
        self
    }
    pub fn default_value(mut self, value: Vec<String>) -> Self {
        self.default_value = value;
        self
    }
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.body = Some(Box::new(body));
        self
    }
    pub fn item(
        mut self,
        value: impl Into<String>,
        title: impl Into<String>,
        body: impl Fn() -> Element + 'static,
    ) -> Self {
        self.items
            .push((value.into(), title.into(), false, Box::new(body)));
        self
    }
    pub fn item_disabled(
        mut self,
        value: impl Into<String>,
        title: impl Into<String>,
        body: impl Fn() -> Element + 'static,
    ) -> Self {
        self.items
            .push((value.into(), title.into(), true, Box::new(body)));
        self
    }
}

impl From<AccordionBuilder> for Element {
    fn from(builder: AccordionBuilder) -> Self {
        accordion_scope(
            builder.id,
            builder.value,
            builder.default_value,
            builder.multiple,
            builder.disabled,
            || {
                let mut children = Vec::new();
                for (value, title, disabled, body) in builder.items {
                    children.push(accordion_item_scope(value, disabled, || {
                        Element::new()
                            .child(accordion_trigger().text(title))
                            .child(accordion_content(body))
                    }));
                }
                if let Some(body) = builder.body {
                    children.push(body());
                }
                Element::new().children(children)
            },
        )
    }
}

impl crate::core::tree::IntoElementList for AccordionBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}
