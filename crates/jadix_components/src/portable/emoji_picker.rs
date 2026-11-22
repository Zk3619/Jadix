use super::{Base, Element, Signal};
use crate::emoji_picker::PickerRenderer;
use jadix::virtualization::ListScrollAlign;
use jadix::portable::presentation::style::ScrollBehavior;
pub use jadix::portable::emoji::EmojiQuery;
pub use crate::emoji_picker::{EmojiAnnotation, EmojiGroup, EmojiPickerController, EmojiPickerParts};
use jadix::portable::{emoji::{EmojiGrid, EmojiMatch}, presentation::{AccessibleRole, NavigationKey}};
use std::{cell::RefCell, rc::Rc};

#[doc(hidden)]
pub struct PortablePicker;
pub type EmojiPickerBuilder = crate::emoji_picker::EmojiPickerBuilder<PortablePicker>;
pub fn emoji_picker(id: impl Into<String>) -> EmojiPickerBuilder { EmojiPickerBuilder::new(id) }

impl PickerRenderer for PortablePicker {
    type Element = Element;
    type ListState = jadix::portable::VirtualListState;
    fn container(id: &str, row: bool, children: Vec<Element>) -> Element {
        let mut element = Element::new().gap(4.).children(children);
        if !id.is_empty() { element = element.id(id); }
        if row { element.flex_row() } else { element.flex_col() }
    }
    fn flexible(element: Element) -> Element { element.flex_grow(1.).min_h(0.) }
    fn placeholder(item_size: f32) -> Element { Element::new().w(0.).h(item_size).flex_grow(1.) }
    fn input(id: &str, value: Signal<String>, placeholder: &str, disabled: bool,
        mut navigation: Box<dyn FnMut(NavigationKey)>) -> Element {
        let mut element: Element = super::input(id).value_signal(value).max_length(256).placeholder(placeholder).disabled(disabled).into();
        element.descriptor.accessible_name = Some(placeholder.into());
        element.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| { navigation(key); None }))));
        let previous = element.descriptor_binding.take();
        element.descriptor_binding = Some(Rc::new(move |descriptor| {
            if let Some(previous) = &previous { previous(descriptor); }
            descriptor.navigation_keys = crate::emoji_picker::navigation_keys(&value.get());
        }));
        element
    }
    fn semantics(mut element: Element, role: Option<AccessibleRole>, label: &str) -> Element {
        element.descriptor.accessible_role = role;
        element.descriptor.accessible_name = Some(label.into());
        element
    }
    fn select(id: &str, value: Signal<String>, options: Vec<(String, String)>, disabled: bool) -> Element {
        let mut element: Element = super::select(id).value_signal(value).options(options).disabled(disabled).into();
        element.descriptor.accessible_name = Some(if id.ends_with("__tones") { "Skin tone" } else { "Emoji category" }.into()); element
    }
    fn navigation_surface(id: &str, disabled: bool, mut navigation: Box<dyn FnMut(NavigationKey)>) -> Element {
        let mut element: Element = super::button(id).text("Emoji").disabled(disabled).into();
        element.descriptor.navigation_keys = crate::emoji_picker::panel_navigation_keys();
        element.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| { navigation(key); None }))));
        element
    }
    fn focus_target(mut element: Element, id: &str, request: Signal<bool>, label: &str, grid: Rc<RefCell<Rc<EmojiGrid>>>, highlight: Signal<Option<usize>>) -> Element {
        let instance = id.rsplit_once("__").expect("picker focus ID").0.to_owned();
        let list = format!("{instance}__scroll");
        let previous = element.descriptor_binding.take();
        element.descriptor_binding = Some(Rc::new(move |descriptor| {
            if let Some(previous) = &previous { previous(descriptor); }
            descriptor.accessible_description = highlight.get().and_then(|i| grid.borrow().items.get(i).map(|entry| entry.name.clone()));
        }));
        element.accessibility_binding = Some(Rc::new(move || (
            highlight.get().map(|index| format!("{instance}__emoji_{index}")), vec![list.clone()],
        )));
        element.focus_requested = Some(request);
        element.descriptor.accessible_name = Some(label.into());
        element
    }
    fn button(id: &str, emoji: &EmojiMatch, item_size: f32, disabled: bool, selected: Box<dyn Fn() -> bool>, child: Option<Element>, pointer_move: Box<dyn FnMut([f64; 2])>, action: Box<dyn FnMut()>) -> Element {
        let builder = super::button(id).disabled(disabled).on_click(action);
        let mut element: Element = match child { Some(child) => builder.child(child), None => builder.text(&emoji.text) }.into();
        element = element.on_pointer_move(pointer_move);
        element.descriptor.accessible_name = Some(emoji.name.clone());
        element.descriptor.accessible_role = Some(AccessibleRole::ListBoxOption { tab_stop: false });
        element.descriptor_binding = Some(Rc::new(move |descriptor| {
            let active = selected();
            descriptor.accessible_selected = Some(active);
            descriptor.style.background = if active { [80, 130, 220, 80] } else { [0, 0, 0, 0] };
        }));
        element.w(0.).h(item_size).flex_grow(1.)
    }
    fn text(text: String) -> Element { Element::new().text(text) }
    fn virtual_rows(state: Self::ListState, id: &str, count: usize, row_height: f32, height: Option<f32>, visible: Box<dyn Fn(std::ops::Range<usize>)>, focus: Box<dyn Fn() -> Option<(usize, usize, ListScrollAlign, ScrollBehavior)>>, render: Box<dyn Fn(usize) -> Element>) -> Element {
        let mut list = jadix::portable::virtual_list(count, row_height as f64, render).state(state).id(id).smooth_scroll(true).on_visible_range_change(move |range| { if let Some(range) = range { visible(range); } }).scroll_request_with_behavior_fn(focus);
        if let Some(height) = height { list = list.h(height); }
        Self::flexible(list.build())
    }
    fn region(id: &str, key: Box<dyn Fn() -> EmojiQuery>, mut render: Box<dyn FnMut(EmojiQuery) -> Element>) -> Element {
        Element::new().id(id).flex_col().bind_children_with(
            key, move |query| vec![render(query)],
            |region| Self::flexible(region.flex_col()),
        )
    }
}

#[derive(Default)]
pub struct EmojiPickerProps {
    pub id: String,
    pub query: Option<EmojiQuery>,
    pub value: Option<Signal<String>>,
    pub disabled: Option<bool>,
    pub query_signal: Option<Signal<String>>,
    pub query_fn: Option<Rc<dyn Fn() -> String>>,
    pub default_query: Option<String>,
    pub skin_tone_signal: Option<Signal<Option<u8>>>,
    pub annotations_fn: Option<Rc<dyn Fn() -> Vec<EmojiAnnotation>>>,
    pub groups_fn: Option<Rc<dyn Fn() -> Option<Vec<u8>>>>,
    pub recent_fn: Option<Rc<dyn Fn() -> Vec<String>>>,
    pub emoji_version_fn: Option<Rc<dyn Fn() -> Option<String>>>,
    pub searchable: Option<bool>,
    pub show_filters: Option<bool>,
    pub placeholder: Option<String>,
    pub columns: Option<usize>,
    pub row_height: Option<f32>,
    pub item_size: Option<f32>,
    pub height: Option<f32>,
    pub rtl: Option<bool>,
    pub auto_focus: Option<bool>,
    pub accessibility_label: Option<String>,
    pub search_accessibility_label: Option<String>,
    pub group_labels: Option<Vec<String>>,
    pub tone_labels: Option<Vec<String>>,
    pub controller: Option<EmojiPickerController>,
    pub header: Option<Element>,
    pub footer: Option<Element>,
    pub search_prefix: Option<Element>,
    pub search_suffix: Option<Element>,
    pub render_item: Option<Rc<dyn Fn(&EmojiMatch) -> Element>>,
    pub render_group: Option<Rc<dyn Fn(EmojiGroup) -> Element>>,
    pub empty: Option<Rc<dyn Fn() -> Element>>,
    pub on_select: Option<Box<dyn FnMut(String)>>,
    pub on_select_entry: Option<Box<dyn FnMut(EmojiMatch)>>,
    pub on_highlight_change: Option<Box<dyn FnMut(Option<EmojiMatch>)>>,
    pub on_query_change: Option<Box<dyn FnMut(String)>>,
    pub on_group_change: Option<Box<dyn FnMut(Option<u8>)>>,
    pub on_cancel: Option<Box<dyn FnMut()>>,
    pub parts: Option<EmojiPickerParts<Element>>,
    pub base: Base,
}
#[allow(non_snake_case)]
pub fn EmojiPicker(props: EmojiPickerProps) -> Element {
    let mut builder = emoji_picker(props.id).query(props.query.unwrap_or_default()).disabled(props.disabled.unwrap_or(false));
    if let Some(value) = props.parts { builder = builder.parts(value); }
    if let Some(value) = props.value { builder = builder.value_signal(value); }
    if let Some(callback) = props.on_select { builder = builder.on_select(callback); }
    if let Some(value) = props.default_query { builder = builder.default_query(value); }
    if let Some(value) = props.query_fn { builder = builder.query_fn(move || value()); }
    if let Some(value) = props.query_signal { builder = builder.query_signal(value); }
    if let Some(value) = props.skin_tone_signal { builder = builder.skin_tone_signal(value); }
    if let Some(value) = props.annotations_fn { builder = builder.annotations_fn(move || value()); }
    if let Some(value) = props.groups_fn { builder = builder.groups_fn(move || value()); }
    if let Some(value) = props.recent_fn { builder = builder.recent_fn(move || value()); }
    if let Some(value) = props.emoji_version_fn { builder = builder.emoji_version_fn(move || value()); }
    if let Some(value) = props.show_filters { builder = builder.show_filters(value); }
    if let Some(value) = props.searchable { builder = builder.searchable(value); }
    if let Some(value) = props.placeholder { builder = builder.placeholder(value); }
    if let Some(value) = props.row_height { builder = builder.row_height(value); }
    if let Some(value) = props.item_size { builder = builder.item_size(value); }
    if let Some(value) = props.height { builder = builder.height(value); }
    if let Some(value) = props.columns { builder = builder.columns(value); }
    if let Some(value) = props.rtl { builder = builder.rtl(value); }
    if let Some(value) = props.auto_focus { builder = builder.auto_focus(value); }
    if let Some(value) = props.accessibility_label { builder = builder.accessibility_label(value); }
    if let Some(value) = props.search_accessibility_label { builder = builder.search_accessibility_label(value); }
    if let Some(value) = props.group_labels { builder = builder.group_labels(value); }
    if let Some(value) = props.tone_labels { builder = builder.tone_labels(value); }
    if let Some(value) = props.controller { builder = builder.controller(value); }
    if let Some(value) = props.header { builder = builder.header(value); }
    if let Some(value) = props.footer { builder = builder.footer(value); }
    if let Some(value) = props.search_prefix { builder = builder.search_prefix(value); }
    if let Some(value) = props.search_suffix { builder = builder.search_suffix(value); }
    if let Some(value) = props.render_item { builder = builder.render_item(move |entry| value(entry)); }
    if let Some(value) = props.render_group { builder = builder.render_group(move |group| value(group)); }
    if let Some(value) = props.empty { builder = builder.empty(move || value()); }
    if let Some(value) = props.on_select_entry { builder = builder.on_select_entry(value); }
    if let Some(value) = props.on_highlight_change { builder = builder.on_highlight_change(value); }
    if let Some(value) = props.on_query_change { builder = builder.on_query_change(value); }
    if let Some(value) = props.on_group_change { builder = builder.on_group_change(value); }
    if let Some(value) = props.on_cancel { builder = builder.on_cancel(value); }
    let element = builder.build();
    match props.base { Some(base) => base(element), None => element }
}
impl From<EmojiPickerBuilder> for Element { fn from(builder: EmojiPickerBuilder) -> Self { builder.build() } }
impl jadix::portable::core::tree::IntoElementList for EmojiPickerBuilder { fn into_element_list(self) -> Vec<Element> { vec![self.build()] } }
