use crate::{authoring::BaseComponentBuilder, core::tree::Handler, Element, Signal};
use jadix::presentation::NodeContent;

type Base = Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>;

#[path = "portable/dialog.rs"]
mod dialog;
pub use dialog::*;

#[path = "portable/popover.rs"]
mod popover;
pub use popover::*;

#[path = "portable/select.rs"]
mod select;
pub use select::*;

#[path = "portable/tabs.rs"]
mod tabs;
pub use tabs::*;

#[path = "portable/collapsible.rs"]
mod collapsible;
pub use collapsible::*;

#[path = "portable/accordion.rs"]
mod accordion;
pub use accordion::*;

#[path = "portable/radio_group.rs"]
mod radio_group;
pub use radio_group::*;

pub struct ButtonBuilder {
    element: Element,
}

pub fn button(id: &str) -> ButtonBuilder {
    let mut element = Element::new().id(id);
    element.descriptor.content = NodeContent::Button {
        label: String::new(),
        enabled: true,
    };
    ButtonBuilder { element }
}

impl ButtonBuilder {
    pub fn text(self, content: impl Into<String>) -> Self {
        self.child(Element::new().text(content.into()))
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.element.children.push(child.into());
        self
    }
    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.element = self.element.on_click(callback);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        if let NodeContent::Button { enabled, .. } = &mut self.element.descriptor.content {
            *enabled = !disabled;
        }
        self
    }
}

impl From<ButtonBuilder> for Element {
    fn from(builder: ButtonBuilder) -> Self {
        builder.element
    }
}
impl crate::core::tree::IntoElementList for ButtonBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[derive(Default)]
pub struct ButtonProps {
    pub id: String,
    pub disabled: Option<bool>,
    pub on_click: Option<Handler>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn Button(props: ButtonProps) -> Element {
    let mut builder = button(&props.id).disabled(props.disabled.unwrap_or(false));
    if let Some(child) = props.children {
        builder = builder.child(child);
    }
    if let Some(mut callback) = props.on_click {
        builder = builder.on_click(move || callback.call(()));
    }
    let element = builder.into();
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}

pub struct InputBuilder {
    element: Element,
}

pub fn input(id: &str) -> InputBuilder {
    let value = crate::use_scoped_signal(&format!("{id}_value"), String::new);
    let mut element = Element::new().id(id);
    element.descriptor.content = NodeContent::Input {
        value: value.get(),
        placeholder: String::new(),
        enabled: true,
        password: false,
        revision: 0,
    };
    element.input = Some(value);
    InputBuilder { element }
}

impl InputBuilder {
    pub fn placeholder(mut self, placeholder: impl ToString) -> Self {
        if let NodeContent::Input {
            placeholder: current,
            ..
        }
        | NodeContent::Textarea {
            placeholder: current,
            ..
        } = &mut self.element.descriptor.content
        {
            *current = placeholder.to_string();
        }
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        if let NodeContent::Input { enabled, .. } | NodeContent::Textarea { enabled, .. } =
            &mut self.element.descriptor.content
        {
            *enabled = !disabled;
        }
        self
    }
    pub fn password(mut self, password: bool) -> Self {
        if let NodeContent::Input {
            password: current, ..
        } = &mut self.element.descriptor.content
        {
            *current = password;
        }
        self
    }
    pub fn value_signal(mut self, value: Signal<String>) -> Self {
        self.element.input = Some(value);
        self
    }
}

impl From<InputBuilder> for Element {
    fn from(builder: InputBuilder) -> Self {
        builder.element
    }
}
impl crate::core::tree::IntoElementList for InputBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[derive(Default)]
pub struct InputProps {
    pub id: String,
    pub placeholder: Option<String>,
    pub disabled: Option<bool>,
    pub password: Option<bool>,
    pub value: Option<Signal<String>>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn Input(props: InputProps) -> Element {
    let mut builder = input(&props.id)
        .placeholder(props.placeholder.unwrap_or_default())
        .disabled(props.disabled.unwrap_or(false))
        .password(props.password.unwrap_or(false));
    if let Some(value) = props.value {
        builder = builder.value_signal(value);
    }
    let element = builder.into();
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}
pub fn scroll_area(id: &str) -> Element {
    jadix::primitives::scroll().id(id)
}

pub fn textarea(id: &str) -> InputBuilder {
    let mut builder = input(id);
    if let NodeContent::Input {
        value,
        placeholder,
        enabled,
        revision,
        ..
    } = builder.element.descriptor.content
    {
        builder.element.descriptor.content = NodeContent::Textarea {
            value,
            placeholder,
            enabled,
            revision,
        };
    }
    builder
}

pub struct CheckboxBuilder {
    element: Element,
}

pub fn checkbox(id: impl Into<String>) -> CheckboxBuilder {
    let id = id.into();
    let value = crate::use_scoped_signal(&format!("{id}__checkbox"), || false);
    let mut element = Element::new().id(id);
    element.descriptor.content = NodeContent::Checkbox {
        label: String::new(),
        checked: value.get(),
        enabled: true,
        revision: 0,
    };
    element.checked = Some(value);
    CheckboxBuilder { element }
}

impl CheckboxBuilder {
    pub fn checked_signal(mut self, value: Signal<bool>) -> Self {
        self.element.checked = Some(value);
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.element.children.push(child.into());
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        if let NodeContent::Checkbox { enabled, .. } | NodeContent::Switch { enabled, .. } =
            &mut self.element.descriptor.content
        {
            *enabled = !disabled;
        }
        self
    }
}
impl From<CheckboxBuilder> for Element {
    fn from(builder: CheckboxBuilder) -> Self {
        builder.element
    }
}
impl crate::core::tree::IntoElementList for CheckboxBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[derive(Default)]
pub struct CheckboxProps {
    pub id: String,
    pub checked: Option<Signal<bool>>,
    pub disabled: Option<bool>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn Checkbox(props: CheckboxProps) -> Element {
    let builder = checkbox(&props.id);
    boolean_control(props, builder)
}

fn boolean_control(props: CheckboxProps, builder: CheckboxBuilder) -> Element {
    let mut builder = builder.disabled(props.disabled.unwrap_or(false));
    if let Some(value) = props.checked {
        builder = builder.checked_signal(value);
    }
    if let Some(child) = props.children {
        builder = builder.child(child);
    }
    let element = builder.into();
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}

pub type SwitchBuilder = CheckboxBuilder;
pub type SwitchProps = CheckboxProps;

pub fn switch(id: impl Into<String>) -> SwitchBuilder {
    let id = id.into();
    let value = crate::use_scoped_signal(&format!("{id}__switch"), || false);
    let mut element = Element::new().id(id);
    element.descriptor.content = NodeContent::Switch {
        label: String::new(),
        checked: value.get(),
        enabled: true,
        revision: 0,
    };
    element.checked = Some(value);
    CheckboxBuilder { element }
}

#[allow(non_snake_case)]
pub fn Switch(props: SwitchProps) -> Element {
    let builder = switch(&props.id);
    boolean_control(props, builder)
}

pub type TextareaBuilder = InputBuilder;
pub struct ImageBuilder {
    element: Element,
}

pub fn image() -> ImageBuilder {
    let mut element = Element::new();
    element.descriptor.content = NodeContent::Image {
        source: String::new(),
        alt: String::new(),
    };
    ImageBuilder { element }
}

impl ImageBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.element.template_id = Some(id.into());
        self
    }
    pub fn src(mut self, source: impl Into<String>) -> Self {
        if let NodeContent::Image {
            source: current, ..
        } = &mut self.element.descriptor.content
        {
            *current = source.into();
        }
        self
    }
    pub fn alt(mut self, alt: impl Into<String>) -> Self {
        if let NodeContent::Image { alt: current, .. } = &mut self.element.descriptor.content {
            *current = alt.into();
        }
        self
    }
}
impl From<ImageBuilder> for Element {
    fn from(builder: ImageBuilder) -> Self {
        builder.element
    }
}
impl crate::core::tree::IntoElementList for ImageBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[derive(Default)]
pub struct ImageProps {
    pub id: String,
    pub src: Option<String>,
    pub alt: Option<String>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn Image(props: ImageProps) -> Element {
    let mut element: Element = image()
        .id(props.id)
        .src(props.src.unwrap_or_default())
        .alt(props.alt.unwrap_or_default())
        .into();
    if let Some(child) = props.children {
        element.children.push(child);
    }
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}
pub type TextareaProps = InputProps;

#[allow(non_snake_case)]
pub fn Textarea(props: TextareaProps) -> Element {
    let mut builder = textarea(&props.id)
        .placeholder(props.placeholder.unwrap_or_default())
        .disabled(props.disabled.unwrap_or(false));
    if let Some(value) = props.value {
        builder = builder.value_signal(value);
    }
    let element = builder.into();
    match props.base {
        Some(base) => base(element),
        None => element,
    }
}
