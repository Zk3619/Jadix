use super::{select, Base, Element, NodeContent, Signal};
use crate::authoring::SlotChrome;
use jadix::presentation::SelectOption;

const SLOT: &str = "jadix_native_select_slot";

fn slot(name: &str, children: Option<Element>, base: Base) -> Element {
    let mut element = Element::new();
    element.set_attr(SLOT, name);
    if let Some(children) = children {
        element.children.push(children);
    }
    match base {
        Some(base) => base(element),
        None => element,
    }
}

#[derive(Default)]
pub struct SelectTriggerProps {
    pub size: Option<String>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn SelectTrigger(props: SelectTriggerProps) -> Element {
    let children = props
        .children
        .or_else(|| Some(SelectValue(Default::default())));
    let mut element = slot("trigger", children, props.base);
    if props.size.is_some() {
        element.set_attr("jadix_native_unsupported", "Select trigger size");
    }
    element
}

#[derive(Default)]
pub struct SelectValueProps {
    pub placeholder: Option<String>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn SelectValue(props: SelectValueProps) -> Element {
    let mut element = slot("value", props.children, props.base);
    if let Some(placeholder) = props.placeholder {
        element.set_attr("placeholder", placeholder);
    }
    element
}

#[derive(Default)]
pub struct SelectItemProps {
    pub value: Option<String>,
    pub disabled: Option<bool>,
    pub align: Option<String>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn SelectItem(props: SelectItemProps) -> Element {
    let mut element = slot("item", props.children, props.base);
    element.set_attr("value", props.value.unwrap_or_default());
    element.set_attr("disabled", props.disabled.unwrap_or(false));
    if props.align.is_some() {
        element.set_attr("jadix_native_unsupported", "Select item alignment");
    }
    element
}

#[derive(Default)]
pub struct SelectContentOpts {
    pub panel: SlotChrome,
    pub match_trigger: Option<bool>,
    pub menu_width: Option<String>,
    pub menu_height: Option<String>,
    pub min_height: Option<String>,
    pub max_height: Option<String>,
    pub placement: Option<String>,
    pub scroll_selected: Option<String>,
}

pub fn select_content(opts: SelectContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let mut element = slot("content", Some(body()), None);
    if opts.panel.class.is_some()
        || opts.panel.base.is_some()
        || opts.match_trigger.is_some()
        || opts.menu_width.is_some()
        || opts.menu_height.is_some()
        || opts.min_height.is_some()
        || opts.max_height.is_some()
        || opts.placement.is_some()
        || opts.scroll_selected.is_some()
    {
        element.set_attr("jadix_native_unsupported", "Select menu options");
    }
    element
}

#[allow(clippy::too_many_arguments)]
pub fn select_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    value: Option<Signal<String>>,
    placeholder: impl Into<String>,
    trigger_class: impl Into<String>,
    content_class: impl Into<String>,
    scroll_selected: impl Into<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let mut builder = select(id).placeholder(placeholder);
    if let Some(value) = value {
        builder = builder.value_signal(value);
    }
    let mut element: Element = builder.into();
    normalize(&mut element, body());
    if open.is_some()
        || !trigger_class.into().is_empty()
        || !content_class.into().is_empty()
        || !scroll_selected.into().is_empty()
    {
        element.set_attr("jadix_native_unsupported", "Select scope options");
    }
    element
}

fn valid_shell(element: &Element) -> bool {
    !element.attrs.contains_key("jadix_native_unsupported")
        && element.on_click.is_none()
        && element.input.is_none()
        && element.checked.is_none()
        && element.dialog_open.is_none()
        && element.deferred_children.is_none()
        && element.text_binding.is_none()
        && element.visibility.is_none()
        && element.descriptor_binding.is_none()
        && element.on_navigation.is_none()
        && matches!(element.descriptor.content, NodeContent::Container)
}

fn flatten(
    element: Element,
    output: &mut Vec<Element>,
    depth: usize,
    budget: &mut usize,
) -> Result<(), ()> {
    if depth >= 256 || *budget == 0 {
        return Err(());
    }
    *budget -= 1;
    if matches!(element.descriptor.content, NodeContent::Container)
        && !element.attrs.contains_key(SLOT)
    {
        if !valid_shell(&element)
            || element.attrs.keys().any(|key| key != "jadix_fragment")
            || element.descriptor.style != Default::default()
        {
            return Err(());
        }
        for child in element.children {
            flatten(child, output, depth + 1, budget)?;
        }
    } else {
        output.push(element);
    }
    Ok(())
}

fn label(
    element: Element,
    output: &mut String,
    depth: usize,
    budget: &mut usize,
) -> Result<(), ()> {
    if depth >= 256
        || *budget == 0
        || element.descriptor.style != Default::default()
        || element.on_click.is_some()
        || element.input.is_some()
        || element.checked.is_some()
        || element.dialog_open.is_some()
        || element.deferred_children.is_some()
        || element.text_binding.is_some()
        || element.attrs.keys().any(|key| key != "jadix_fragment")
    {
        return Err(());
    }
    *budget -= 1;
    match element.descriptor.content {
        NodeContent::Text(text) => output.push_str(&text),
        NodeContent::Container => {}
        _ => return Err(()),
    }
    if output.len() > 4096 {
        return Err(());
    }
    for child in element.children {
        label(child, output, depth + 1, budget)?;
    }
    Ok(())
}

fn normalize_inner(element: &mut Element, children: Element) -> Result<(), ()> {
    let mut budget = 4096;
    let mut slots = Vec::new();
    flatten(children, &mut slots, 0, &mut budget)?;
    if slots.len() != 2
        || slots[0].attrs.get(SLOT).map(String::as_str) != Some("trigger")
        || slots[1].attrs.get(SLOT).map(String::as_str) != Some("content")
    {
        return Err(());
    }
    let content = slots.pop().ok_or(())?;
    let trigger = slots.pop().ok_or(())?;
    if !valid_shell(&trigger)
        || !valid_shell(&content)
        || trigger.attrs.len() != 1
        || content.attrs.len() != 1
        || content.descriptor.style != Default::default()
    {
        return Err(());
    }
    let mut values = Vec::new();
    for child in trigger.children {
        flatten(child, &mut values, 0, &mut budget)?;
    }
    if values.len() != 1 {
        return Err(());
    }
    let value = values.pop().ok_or(())?;
    if !valid_shell(&value)
        || value.attrs.get(SLOT).map(String::as_str) != Some("value")
        || !value.children.is_empty()
        || value.descriptor.style != Default::default()
        || value
            .attrs
            .keys()
            .any(|key| key != SLOT && key != "placeholder")
    {
        return Err(());
    }
    let mut items = Vec::new();
    for child in content.children {
        flatten(child, &mut items, 0, &mut budget)?;
    }
    let mut options = Vec::new();
    for item in items {
        if !valid_shell(&item)
            || item.attrs.get(SLOT).map(String::as_str) != Some("item")
            || item.attrs.len() != 3
            || item.descriptor.style != Default::default()
        {
            return Err(());
        }
        let value = item.attrs.get("value").ok_or(())?.clone();
        let enabled = match item.attrs.get("disabled").map(String::as_str) {
            Some("true") => false,
            Some("false") => true,
            _ => return Err(()),
        };
        let mut text = String::new();
        for child in item.children {
            label(child, &mut text, 0, &mut budget)?;
        }
        if text.is_empty() {
            text = value.clone();
        }
        options.push(SelectOption {
            value,
            label: text,
            enabled,
        });
        if options.len() > 256 {
            return Err(());
        }
    }
    let NodeContent::Select {
        options: current,
        placeholder,
        ..
    } = &mut element.descriptor.content
    else {
        return Err(());
    };
    if !current.is_empty() {
        return Err(());
    }
    *current = options;
    if let Some(custom) = value.attrs.get("placeholder") {
        *placeholder = custom.clone();
    }
    element.descriptor.style = trigger.descriptor.style;
    element.descriptor.accessible_name = trigger.descriptor.accessible_name;
    Ok(())
}

pub(super) fn normalize(element: &mut Element, children: Element) {
    if normalize_inner(element, children).is_err() {
        element.set_attr(
            "jadix_native_unsupported",
            "Select compound structure or options",
        );
    }
}
