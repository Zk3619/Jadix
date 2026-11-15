use jadix::portable::{Element, Signal};
use jadix::{
    core::reactivity::{component_root, create_context, provide_context, use_context, Context},
    presentation::{AccessibleRole, NavigationKey, NodeContent},
};
use std::{cell::RefCell, rc::Rc};

type Base = super::Base;

#[derive(Clone)]
struct Entry {
    value: String,
    enabled: bool,
    id: String,
}

#[derive(Clone)]
pub struct RadioGroupCtx {
    pub id: String,
    pub value: Signal<String>,
    pub disabled: bool,
    entries: Rc<RefCell<Vec<Entry>>>,
}

fn context() -> Context<RadioGroupCtx> {
    thread_local! { static CONTEXT: Context<RadioGroupCtx> = create_context(); }
    CONTEXT.with(|context| *context)
}

pub fn use_radio_group() -> Option<RadioGroupCtx> {
    use_context(context())
}

pub fn radio_group_scope(
    id: impl Into<String>,
    value: Option<Signal<String>>,
    default_value: impl Into<String>,
    disabled: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    radio_group_scope_named(id, value, default_value, disabled, None, body)
}

fn radio_group_scope_named(
    id: impl Into<String>,
    value: Option<Signal<String>>,
    default_value: impl Into<String>,
    disabled: bool,
    accessibility_label: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = id.into();
    let default_value = default_value.into();
    component_root(|| {
        let value = value.unwrap_or_else(|| {
            jadix::portable::use_scoped_signal(&format!("{id}__radio"), || default_value)
        });
        let entries = Rc::new(RefCell::new(Vec::new()));
        provide_context(
            context(),
            RadioGroupCtx {
                id: id.clone(),
                value,
                disabled,
                entries: entries.clone(),
            },
        );
        let mut root = Element::new().id(id).flex_col().gap(8.0);
        root.descriptor.accessible_role = Some(AccessibleRole::RadioGroup);
        if let Some(label) = accessibility_label.filter(|label| !label.trim().is_empty()) {
            root.descriptor.accessible_name = Some(label);
        }
        root.children.push(body());
        let entries = entries.borrow();
        let unique: std::collections::BTreeSet<_> =
            entries.iter().map(|entry| &entry.value).collect();
        if entries.len() > 128
            || unique.len() != entries.len()
            || entries
                .iter()
                .any(|entry| entry.value.is_empty() || entry.value.len() > 256)
        {
            root.set_attr("jadix_native_unsupported", "RadioGroup value inventory");
        }
        root
    })
}

#[derive(Default)]
pub struct RadioGroupItemProps {
    pub value: Option<String>,
    pub disabled: Option<bool>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn RadioGroupItem(props: RadioGroupItemProps) -> Element {
    let context = use_radio_group().expect("RadioGroupItem requires radio_group_scope");
    let value = props.value.unwrap_or_default();
    let enabled = !context.disabled && !props.disabled.unwrap_or(false);
    let id = format!("{}:{}", context.id, value);
    context.entries.borrow_mut().push(Entry {
        value: value.clone(),
        enabled,
        id: id.clone(),
    });
    let selected = context.value;
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
    element.descriptor.accessible_role = Some(AccessibleRole::RadioItem { tab_stop: false });
    element.descriptor.style.height = Some(32.0);
    element.children.push(
        props
            .children
            .unwrap_or_else(|| Element::new().text(&value)),
    );
    let entries = context.entries.clone();
    let bound_value = value.clone();
    element.descriptor_binding = Some(Rc::new(move |descriptor| {
        let current = selected.get();
        let entries = entries.borrow();
        let stop = entries
            .iter()
            .find(|entry| entry.enabled && entry.value == current)
            .or_else(|| entries.iter().find(|entry| entry.enabled));
        descriptor.accessible_role = Some(AccessibleRole::RadioItem {
            tab_stop: stop.is_some_and(|entry| entry.value == bound_value),
        });
        descriptor.accessible_selected = Some(current == bound_value);
    }));
    if enabled {
        let entries = context.entries.clone();
        element.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| {
            let entries = entries.borrow();
            let enabled: Vec<_> = entries.iter().filter(|entry| entry.enabled).collect();
            let current = enabled.iter().position(|entry| entry.value == value)?;
            let next = match key {
                NavigationKey::Home => 0,
                NavigationKey::End => enabled.len() - 1,
                NavigationKey::Left | NavigationKey::Up => {
                    (current + enabled.len() - 1) % enabled.len()
                }
                NavigationKey::Right | NavigationKey::Down => (current + 1) % enabled.len(),
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

#[derive(Default)]
pub struct RadioGroupItemLabelProps {
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn RadioGroupItemLabel(props: RadioGroupItemLabelProps) -> Element {
    let element = props.children.unwrap_or_default();
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}

pub struct RadioGroupItemBuilder {
    props: RadioGroupItemProps,
}

pub fn radio_group_item(value: impl Into<String>) -> RadioGroupItemBuilder {
    RadioGroupItemBuilder {
        props: RadioGroupItemProps {
            value: Some(value.into()),
            ..Default::default()
        },
    }
}

impl RadioGroupItemBuilder {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.props.disabled = Some(disabled);
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.props.children = Some(child.into());
        self
    }
    pub fn text(self, text: impl Into<String>) -> Self {
        self.child(Element::new().text(text.into()))
    }
}

impl From<RadioGroupItemBuilder> for Element {
    fn from(builder: RadioGroupItemBuilder) -> Self {
        RadioGroupItem(builder.props)
    }
}

impl jadix::portable::core::tree::IntoElementList for RadioGroupItemBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct RadioOption {
    value: String,
    label: String,
    disabled: bool,
}

pub fn radio_option(value: impl Into<String>, label: impl Into<String>) -> RadioOption {
    RadioOption {
        value: value.into(),
        label: label.into(),
        disabled: false,
    }
}

impl RadioOption {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl<Value: Into<String>, Label: Into<String>> From<(Value, Label)> for RadioOption {
    fn from((value, label): (Value, Label)) -> Self {
        radio_option(value, label)
    }
}

pub struct RadioGroupBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    disabled: bool,
    accessibility_label: Option<String>,
    options: Vec<RadioOption>,
    children: Vec<Element>,
}

pub fn radio_group(id: impl Into<String>) -> RadioGroupBuilder {
    RadioGroupBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        disabled: false,
        accessibility_label: None,
        options: vec![],
        children: vec![],
    }
}

impl RadioGroupBuilder {
    pub fn value_signal(mut self, value: Signal<String>) -> Self {
        self.value = Some(value);
        self
    }
    pub fn selected_signal(self, value: Signal<String>) -> Self {
        self.value_signal(value)
    }
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default_value = value.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
    pub fn options(mut self, options: impl IntoIterator<Item = impl Into<RadioOption>>) -> Self {
        self.options = options.into_iter().map(Into::into).collect();
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<RadioGroupBuilder> for Element {
    fn from(builder: RadioGroupBuilder) -> Self {
        radio_group_scope_named(
            builder.id,
            builder.value,
            builder.default_value,
            builder.disabled,
            builder.accessibility_label,
            || {
                let mut children: Vec<Element> = builder
                    .options
                    .into_iter()
                    .map(|option| {
                        radio_group_item(option.value)
                            .text(option.label)
                            .disabled(option.disabled)
                            .into()
                    })
                    .collect();
                children.extend(builder.children);
                Element::new().children(children)
            },
        )
    }
}

impl jadix::portable::core::tree::IntoElementList for RadioGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[derive(Default)]
pub struct RadioGroupProps {
    pub id: String,
    pub value: Option<Signal<String>>,
    pub selected: Option<Signal<String>>,
    pub default_value: Option<String>,
    pub options: Option<Vec<(String, String)>>,
    pub disabled: Option<bool>,
    pub accessibility_label: Option<String>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn RadioGroup(props: RadioGroupProps) -> Element {
    let mut builder = radio_group(props.id)
        .default_value(props.default_value.unwrap_or_default())
        .disabled(props.disabled.unwrap_or(false));
    if let Some(label) = props.accessibility_label {
        builder = builder.accessibility_label(label);
    }
    builder.value = props.value.or(props.selected);
    if let Some(options) = props.options {
        builder = builder.options(options);
    }
    if let Some(children) = props.children {
        builder = builder.child(children);
    }
    let element = builder.into();
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}
