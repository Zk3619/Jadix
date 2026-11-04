use crate::{Element, Signal};
use jadix::{
    core::reactivity::{component_root, create_context, provide_context, use_context, Context},
    presentation::{AccessibleRole, NavigationKey, NodeContent},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Base = super::Base;

#[derive(Clone)]
struct Entry {
    value: String,
    enabled: bool,
    id: String,
}

#[derive(Clone)]
struct TabsContext {
    id: String,
    selected: Signal<String>,
    disabled: bool,
    vertical: bool,
    entries: Rc<RefCell<Vec<Entry>>>,
    lists: Rc<Cell<usize>>,
}

fn context() -> Context<TabsContext> {
    thread_local! { static CONTEXT: Context<TabsContext> = create_context(); }
    CONTEXT.with(|context| *context)
}

fn required() -> TabsContext {
    use_context(context()).expect("Tabs part requires tabs_scope or tabs(...).child(...)")
}

pub fn tabs_scope(
    id: impl Into<String>,
    value: Option<Signal<String>>,
    default_value: impl Into<String>,
    orientation: impl Into<String>,
    disabled: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = id.into();
    let default_value = default_value.into();
    let orientation = orientation.into();
    component_root(|| {
        let selected = value
            .unwrap_or_else(|| crate::use_scoped_signal(&format!("{id}__tabs"), || default_value));
        let context_value = TabsContext {
            id: id.clone(),
            selected,
            disabled,
            vertical: orientation == "vertical",
            entries: Rc::new(RefCell::new(Vec::new())),
            lists: Rc::new(Cell::new(0)),
        };
        provide_context(context(), context_value.clone());
        let mut root = if context_value.vertical {
            Element::new().flex_row()
        } else {
            Element::new().flex_col()
        };
        root.template_id = Some(id);
        root.children.push(body());
        if context_value.lists.get() == 0 {
            root.descriptor.accessible_role = Some(AccessibleRole::TabList);
        }
        let entries = context_value.entries.borrow();
        if orientation != "horizontal" && orientation != "vertical"
            || context_value.lists.get() > 1
            || entries.len() > 128
            || entries
                .iter()
                .any(|entry| entry.value.is_empty() || entry.value.len() > 256)
            || entries
                .iter()
                .map(|entry| &entry.value)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != entries.len()
        {
            root.set_attr(
                "jadix_native_unsupported",
                "Tabs orientation/value inventory",
            );
        }
        if selected.get().is_empty() {
            if let Some(entry) = entries.iter().find(|entry| entry.enabled) {
                selected.set(entry.value.clone());
            }
        }
        root
    })
}

pub struct TabsBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    orientation: String,
    disabled: bool,
}

pub fn tabs(id: impl Into<String>) -> TabsBuilder {
    TabsBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        orientation: "horizontal".into(),
        disabled: false,
    }
}

impl TabsBuilder {
    pub fn value_signal(mut self, value: Signal<String>) -> Self {
        self.value = Some(value);
        self
    }
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default_value = value.into();
        self
    }
    pub fn orientation(mut self, orientation: impl Into<String>) -> Self {
        self.orientation = orientation.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        tabs_scope(
            self.id,
            self.value,
            self.default_value,
            self.orientation,
            self.disabled,
            body,
        )
    }
}

#[derive(Default)]
pub struct TabsListProps {
    pub id: String,
    pub variant: Option<String>,
    pub value: Option<Signal<String>>,
    pub default_value: Option<String>,
    pub options: Option<Vec<(String, String)>>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn TabsList(props: TabsListProps) -> Element {
    let context = required();
    context.lists.set(context.lists.get() + 1);
    let mut root = if context.vertical {
        Element::new().flex_col()
    } else {
        Element::new().flex_row()
    };
    root.descriptor.accessible_role = Some(AccessibleRole::TabList);
    root.descriptor.style.gap = 4.0;
    if let Some(children) = props.children {
        root.children.push(children);
    }
    if !props.id.is_empty() {
        root.template_id = Some(props.id);
    }
    if props
        .variant
        .as_deref()
        .is_some_and(|variant| !matches!(variant, "default" | "line"))
        || props.value.is_some()
        || props.default_value.is_some()
        || props.options.is_some()
    {
        root.set_attr("jadix_native_unsupported", "TabsList options");
    }
    match props.base {
        Some(base) => base(root),
        None => root,
    }
}

#[derive(Default)]
pub struct TabsTriggerProps {
    pub value: Option<String>,
    pub disabled: Option<bool>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn TabsTrigger(props: TabsTriggerProps) -> Element {
    let context = required();
    let value = props.value.unwrap_or_default();
    let enabled = !context.disabled && !props.disabled.unwrap_or(false);
    let id = format!("{}:trigger:{}", context.id, value);
    context.entries.borrow_mut().push(Entry {
        value: value.clone(),
        enabled,
        id: id.clone(),
    });
    let selected = context.selected;
    let activate = value.clone();
    let mut element = Element::new().id(id).on_click(move || {
        if enabled {
            selected.set(activate.clone());
        }
    });
    element.descriptor.content = NodeContent::Button {
        label: String::new(),
        enabled,
    };
    element.descriptor.accessible_role = Some(AccessibleRole::TabItem);
    element.descriptor.accessible_selected = Some(context.selected.get() == value);
    element.descriptor.style.height = Some(34.0);
    if let Some(children) = props.children {
        element.children.push(children);
    } else {
        element.children.push(Element::new().text(&value));
    }
    let style_value = value.clone();
    element.descriptor_binding = Some(Rc::new(move |descriptor| {
        let is_selected = selected.get() == style_value;
        descriptor.accessible_selected = Some(is_selected);
        descriptor.style.background = if is_selected {
            [220, 232, 250, 255]
        } else {
            [240, 240, 240, 255]
        };
    }));
    if enabled {
        let entries = context.entries.clone();
        let vertical = context.vertical;
        element.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| {
            let entries = entries.borrow();
            let enabled: Vec<_> = entries.iter().filter(|entry| entry.enabled).collect();
            if enabled.is_empty() {
                return None;
            }
            let current = enabled
                .iter()
                .position(|entry| entry.value == selected.get())
                .unwrap_or(0);
            let next = match key {
                NavigationKey::Home => 0,
                NavigationKey::End => enabled.len() - 1,
                NavigationKey::Left if !vertical => (current + enabled.len() - 1) % enabled.len(),
                NavigationKey::Right if !vertical => (current + 1) % enabled.len(),
                NavigationKey::Up if vertical => (current + enabled.len() - 1) % enabled.len(),
                NavigationKey::Down if vertical => (current + 1) % enabled.len(),
                _ => return None,
            };
            selected.set(enabled[next].value.clone());
            Some(enabled[next].id.clone())
        }))));
    }
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}

pub struct TabsTriggerBuilder {
    value: String,
    disabled: bool,
    children: Vec<Element>,
}
pub fn tabs_trigger(value: impl Into<String>) -> TabsTriggerBuilder {
    TabsTriggerBuilder {
        value: value.into(),
        disabled: false,
        children: vec![],
    }
}
impl TabsTriggerBuilder {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.children.push(Element::new().text(text.into()));
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}
impl From<TabsTriggerBuilder> for Element {
    fn from(builder: TabsTriggerBuilder) -> Self {
        TabsTrigger(TabsTriggerProps {
            value: Some(builder.value),
            disabled: Some(builder.disabled),
            children: Some(Element::new().children(builder.children)),
            base: None,
        })
    }
}
impl crate::core::tree::IntoElementList for TabsTriggerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub fn tabs_content(value: impl Into<String>, body: impl Fn() -> Element + 'static) -> Element {
    let context = required();
    let value = value.into();
    let selected = context.selected;
    let mut element = Element::new().id(format!("{}:content:{}", context.id, value));
    element.descriptor.accessible_role = Some(AccessibleRole::TabPanel);
    element.visibility = Some(Rc::new(move || selected.get() == value));
    element.deferred_children = Some(Box::new(body));
    element
}
