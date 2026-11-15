use jadix::portable::{authoring::BaseComponentBuilder, Element, Signal};
use jadix::presentation::{NodeContent, SelectOption};

type Base = Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>;

#[path = "select_compound.rs"]
mod compound;
pub use compound::*;

pub struct SelectBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Vec<SelectOption>,
    placeholder: String,
    disabled: bool,
}

pub fn select(id: impl Into<String>) -> SelectBuilder {
    SelectBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        options: Vec::new(),
        placeholder: String::new(),
        disabled: false,
    }
}

#[derive(Clone)]
pub struct SelectItemData {
    option: SelectOption,
}

impl SelectItemData {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            option: SelectOption {
                value: value.into(),
                label: label.into(),
                enabled: true,
            },
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.option.enabled = !disabled;
        self
    }
}

impl SelectBuilder {
    pub fn value_signal(mut self, value: Signal<String>) -> Self {
        self.value = Some(value);
        self
    }
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default_value = value.into();
        self
    }
    pub fn options(mut self, options: Vec<(String, String)>) -> Self {
        self.options
            .extend(options.into_iter().map(|(value, label)| SelectOption {
                value,
                label,
                enabled: true,
            }));
        self
    }
    pub fn item(mut self, item: SelectItemData) -> Self {
        self.options.push(item.option);
        self
    }
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl From<SelectBuilder> for Element {
    fn from(builder: SelectBuilder) -> Self {
        let value = builder.value.unwrap_or_else(|| {
            jadix::portable::use_scoped_signal(&format!("{}__select_value", builder.id), || {
                builder.default_value
            })
        });
        let mut element = Element::new().id(builder.id);
        element.descriptor.content = NodeContent::Select {
            options: builder.options,
            value: value.get(),
            placeholder: builder.placeholder,
            enabled: !builder.disabled,
            revision: 0,
        };
        element.input = Some(value);
        element
    }
}
impl jadix::portable::core::tree::IntoElementList for SelectBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[derive(Default)]
pub struct SelectProps {
    pub id: String,
    pub value: Option<Signal<String>>,
    pub default_value: Option<String>,
    pub options: Option<Vec<(String, String)>>,
    pub placeholder: Option<String>,
    pub disabled: Option<bool>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn Select(props: SelectProps) -> Element {
    let mut builder = select(props.id)
        .options(props.options.unwrap_or_default())
        .default_value(props.default_value.unwrap_or_default())
        .placeholder(props.placeholder.unwrap_or_default())
        .disabled(props.disabled.unwrap_or(false));
    if let Some(value) = props.value {
        builder = builder.value_signal(value);
    }
    let mut element: Element = builder.into();
    if let Some(children) = props.children {
        compound::normalize(&mut element, children);
    }
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}
