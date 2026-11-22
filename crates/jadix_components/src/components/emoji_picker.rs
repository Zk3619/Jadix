use crate::{core::{reactivity::Signal, tree::Element}, emoji_picker::PickerRenderer};
pub use jadix::portable::emoji::EmojiQuery;
pub use crate::emoji_picker::{EmojiAnnotation, EmojiGroup, EmojiPickerController, EmojiPickerParts};
use jadix::portable::{emoji::{EmojiGrid, EmojiMatch}, presentation::{AccessibleRole, NavigationKey}};
use std::{cell::RefCell, rc::Rc};
use jadix::virtualization::ListScrollAlign;
use jadix::portable::presentation::style::ScrollBehavior;

#[doc(hidden)]
pub struct GpuPicker;
pub type EmojiPickerBuilder = crate::emoji_picker::EmojiPickerBuilder<GpuPicker>;
pub fn emoji_picker(id: impl Into<String>) -> EmojiPickerBuilder { EmojiPickerBuilder::new(id) }
impl PickerRenderer for GpuPicker {
    type Element = Element;
    type ListState = super::virtual_list::VirtualListHandle;
    fn container(id: &str, row: bool, children: Vec<Element>) -> Element {
        let mut element = Element::new().gap(4.).children(children);
        if !id.is_empty() { element = element.template_id(id); }
        if row { element.flex_row() } else { element.flex_col() }
    }
    fn flexible(element: Element) -> Element { element.flex_grow(1.).flex_shrink(1.).min_h(0.) }
    fn placeholder(item_size: f32) -> Element { Element::new().w(0.).h(item_size).flex_grow(1.) }
    fn input(id: &str, value: Signal<String>, placeholder: &str, disabled: bool,
        navigation: Box<dyn FnMut(NavigationKey)>) -> Element {
        let mut element: Element = super::input(id).value_signal(value).max_length(256).placeholder(placeholder).disabled(disabled).into();
        let previous = element.text_edit_key_handler.take();
        let mut navigation = navigation;
        element.text_edit_key_handler = Some(crate::authoring::text::navigation_handler(
            move || if disabled { Vec::new() } else { crate::emoji_picker::navigation_keys(&value.get()) },
            move |key| navigation(key), previous,
        ));
        element
    }

    fn semantics(mut element: Element, role: Option<AccessibleRole>, label: &str) -> Element {
        if let Some(role) = role { element.attrs.insert("accesskit_role".into(), role.aria_role().into()); }
        element.attrs.insert("aria-label".into(), label.into());
        element
    }
    fn select(id: &str, value: Signal<String>, options: Vec<(String, String)>, disabled: bool) -> Element {
        let mut element: Element = super::select(id).value_signal(value).options(options).disabled(disabled).into();
        element.attrs.insert("aria-label".into(), if id.ends_with("__tones") { "Skin tone" } else { "Emoji category" }.into());
        element
    }
    fn navigation_surface(id: &str, disabled: bool, mut navigation: Box<dyn FnMut(NavigationKey)>) -> Element {
        let navigation = Rc::new(RefCell::new(move |key| navigation(key)));
        let activate = navigation.clone();
        let mut element: Element = super::button(id).text("Emoji").disabled(disabled)
            .on_click(move || (activate.borrow_mut())(NavigationKey::Enter)).into();
        let target = element.focus_id.clone();
        let focused = crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new);
        crate::authoring::keys::attach(&mut element, move |event| {
            if disabled || target.as_deref() != Some(focused.get().as_str()) || !event.modifiers.is_empty()
                || event.state != winit::event::ElementState::Pressed { return false; }
            let Some(key) = crate::authoring::text::navigation_key(event.key)
                .filter(|key| crate::emoji_picker::panel_navigation_keys().contains(key)) else { return false; };
            (navigation.borrow_mut())(key); true
        });
        element
    }
    fn focus_target(mut element: Element, id: &str, request: Signal<bool>, label: &str, grid: Rc<RefCell<Rc<EmojiGrid>>>, highlight: Signal<Option<usize>>) -> Element {
        let target = element.focus_id.clone().unwrap_or_else(|| id.into());
        element.attrs.insert("aria-label".into(), label.into());
        let instance = id.rsplit_once("__").expect("picker focus ID").0.to_owned();
        element.attrs.insert("aria-controls".into(), format!("{instance}__scroll"));
        element.reactive_sync(move |element, _| {
            let active = highlight.get().map(|index| format!("{instance}__emoji_{index}"));
            let description = highlight.get().and_then(|i| grid.borrow().items.get(i).map(|entry| entry.name.clone()));
            let changed = element.attrs.get("aria-activedescendant") != active.as_ref()
                || element.attrs.get("aria-description") != description.as_ref();
            for (key, value) in [("aria-activedescendant", active), ("aria-description", description)] {
                if let Some(value) = value { element.attrs.insert(key.into(), value); }
                else { element.attrs.remove(key); }
            }
            if request.get() { request.set(false); crate::utils::roving::focus_by_id(target.clone()); }
            if changed { crate::core::tree::LayoutStatus::StyleChanged } else { crate::core::tree::LayoutStatus::Clean }
        })
    }
    fn button(id: &str, emoji: &EmojiMatch, item_size: f32, disabled: bool, selected: Box<dyn Fn() -> bool>, child: Option<Element>, pointer_move: Box<dyn FnMut([f64; 2])>, action: Box<dyn FnMut()>) -> Element {
        let builder = super::button(id).disabled(disabled).w(0.).h(item_size).flex_grow(1.);
        let mut element: Element = match child { Some(child) => builder.child(child), None => builder.text(&emoji.text) }.into();
        // Options belong to the search field's navigation scope. Activation must
        // not run Button's ordinary focus transfer.
        let mut action = action;
        element = element.on_click(move || { if !disabled { action(); } }).activate_on_press();
        let mut pointer_move = pointer_move;
        element = element.on_mousemove(move |event| pointer_move([event.x.into(), event.y.into()]));
        element.attrs.insert("aria-label".into(), emoji.name.clone());
        element.attrs.insert("accesskit_role".into(), AccessibleRole::ListBoxOption { tab_stop: false }.aria_role().into());
        element.set_focusable(false);
        let background = element.style.bg_color;
        element.reactive_sync(move |element, _| {
            let active = selected();
            let next = active.to_string();
            if element.attrs.get("aria-selected") == Some(&next) { return crate::core::tree::LayoutStatus::Clean; }
            element.attrs.insert("aria-selected".into(), next);
            element.style.bg_color = if active { [80. / 255., 130. / 255., 220. / 255., 80. / 255.] } else { background };
            crate::core::tree::LayoutStatus::StyleChanged
        })
    }
    fn text(text: String) -> Element { super::label(text).into() }
    fn virtual_rows(handle: Self::ListState, id: &str, count: usize, row_height: f32, height: Option<f32>, visible: Box<dyn Fn(std::ops::Range<usize>)>, focus: Box<dyn Fn() -> Option<(usize, usize, ListScrollAlign, ScrollBehavior)>>, render: Box<dyn Fn(usize) -> Element>) -> Element {
        let previous = std::cell::Cell::new(None);
        let navigation = handle.clone();
        let request = move || {
            let next = focus();
            if previous.get() != next {
                if let Some((row, _, align, behavior)) = next {
                    let queued = if behavior == ScrollBehavior::Smooth { navigation.scroll_to_index_smooth(row, align, 0.) }
                        else { navigation.scroll_to_index(row, align, 0.) };
                    if !queued { return None; }
                }
                previous.set(next);
            }
            None
        };
        let mut list = super::virtual_list(id).handle(handle).item_count(count).item_height(row_height).on_visible_range_change(move |range| { if let Some(range) = range { visible(range); } }).focused_index_fn(request).render_item(render);
        if let Some(height) = height { list = list.height(height); }
        let mut element: Element = list.into();
        if height.is_none() { element = element.h("auto"); }
        Self::flexible(element)
    }
    fn region(id: &str, key: Box<dyn Fn() -> EmojiQuery>, mut render: Box<dyn FnMut(EmojiQuery) -> Element>) -> Element {
        crate::app::reactive::reactive_view(id, move || render(key()))
    }
}
crate::__component_contract! {
EmojiPickerProps [];
#[allow(non_snake_case)]
pub fn EmojiPicker(
identity [] id: String,
plain [] query: Option<EmojiQuery>,
plain [] parts: Option<EmojiPickerParts<Element>>,
plain [] value: Option<Signal<String>>,
default [] disabled: bool,
plain [] query_signal: Option<Signal<String>>,
plain [] query_fn: Option<Rc<dyn Fn() -> String>>,
plain [] default_query: Option<String>,
plain [] skin_tone_signal: Option<Signal<Option<u8>>>,
plain [] annotations_fn: Option<Rc<dyn Fn() -> Vec<EmojiAnnotation>>>,
plain [] groups_fn: Option<Rc<dyn Fn() -> Option<Vec<u8>>>>,
plain [] recent_fn: Option<Rc<dyn Fn() -> Vec<String>>>,
plain [] emoji_version_fn: Option<Rc<dyn Fn() -> Option<String>>>,
plain [] searchable: Option<bool>,
plain [] show_filters: Option<bool>,
plain [] placeholder: Option<String>,
plain [] columns: Option<usize>,
plain [] row_height: Option<f32>,
plain [] item_size: Option<f32>,
plain [] height: Option<f32>,
default [] rtl: bool,
default [] auto_focus: bool,
plain [] accessibility_label: Option<String>,
plain [] search_accessibility_label: Option<String>,
plain [] group_labels: Option<Vec<String>>,
plain [] tone_labels: Option<Vec<String>>,
plain [] controller: Option<EmojiPickerController>,
plain [] header: Option<Element>,
plain [] footer: Option<Element>,
plain [] search_prefix: Option<Element>,
plain [] search_suffix: Option<Element>,
plain [] render_item: Option<Rc<dyn Fn(&EmojiMatch) -> Element>>,
plain [] render_group: Option<Rc<dyn Fn(EmojiGroup) -> Element>>,
plain [] empty: Option<Rc<dyn Fn() -> Element>>,
plain [mut] on_select: Option<crate::core::tree::Handler<String>>,
plain [mut] on_select_entry: Option<crate::core::tree::Handler<EmojiMatch>>,
plain [mut] on_highlight_change: Option<crate::core::tree::Handler<Option<EmojiMatch>>>,
plain [mut] on_query_change: Option<crate::core::tree::Handler<String>>,
plain [mut] on_group_change: Option<crate::core::tree::Handler<Option<u8>>>,
plain [mut] on_cancel: Option<crate::core::tree::Handler<()>>,
) -> Element {
    let mut builder = emoji_picker(id).query(query.unwrap_or_default()).disabled(disabled).rtl(rtl).auto_focus(auto_focus)
        .on_select(move |value| { if let Some(handler) = &mut on_select { handler.call(value); } })
        .on_select_entry(move |value| { if let Some(handler) = &mut on_select_entry { handler.call(value); } })
        .on_highlight_change(move |value| { if let Some(handler) = &mut on_highlight_change { handler.call(value); } })
        .on_query_change(move |value| { if let Some(handler) = &mut on_query_change { handler.call(value); } })
        .on_group_change(move |value| { if let Some(handler) = &mut on_group_change { handler.call(value); } })
        .on_cancel(move || { if let Some(handler) = &mut on_cancel { handler.call(()); } });
    if let Some(value) = parts { builder = builder.parts(value); }
    if let Some(value) = value { builder = builder.value_signal(value); }
    if let Some(value) = default_query { builder = builder.default_query(value); }
    if let Some(value) = query_fn { builder = builder.query_fn(move || value()); }
    if let Some(value) = query_signal { builder = builder.query_signal(value); }
    if let Some(value) = skin_tone_signal { builder = builder.skin_tone_signal(value); }
    if let Some(value) = annotations_fn { builder = builder.annotations_fn(move || value()); }
    if let Some(value) = groups_fn { builder = builder.groups_fn(move || value()); }
    if let Some(value) = recent_fn { builder = builder.recent_fn(move || value()); }
    if let Some(value) = emoji_version_fn { builder = builder.emoji_version_fn(move || value()); }
    if let Some(value) = show_filters { builder = builder.show_filters(value); }
    if let Some(value) = searchable { builder = builder.searchable(value); }
    if let Some(value) = placeholder { builder = builder.placeholder(value); }
    if let Some(value) = row_height { builder = builder.row_height(value); }
    if let Some(value) = item_size { builder = builder.item_size(value); }
    if let Some(value) = height { builder = builder.height(value); }
    if let Some(value) = columns { builder = builder.columns(value); }
    if let Some(value) = accessibility_label { builder = builder.accessibility_label(value); }
    if let Some(value) = search_accessibility_label { builder = builder.search_accessibility_label(value); }
    if let Some(value) = group_labels { builder = builder.group_labels(value); }
    if let Some(value) = tone_labels { builder = builder.tone_labels(value); }
    if let Some(value) = controller { builder = builder.controller(value); }
    if let Some(value) = header { builder = builder.header(value); }
    if let Some(value) = footer { builder = builder.footer(value); }
    if let Some(value) = search_prefix { builder = builder.search_prefix(value); }
    if let Some(value) = search_suffix { builder = builder.search_suffix(value); }
    if let Some(value) = render_item { builder = builder.render_item(move |entry| value(entry)); }
    if let Some(value) = render_group { builder = builder.render_group(move |group| value(group)); }
    if let Some(value) = empty { builder = builder.empty(move || value()); }
    builder.build()
}}
impl From<EmojiPickerBuilder> for Element { fn from(builder: EmojiPickerBuilder) -> Self { builder.build() } }
impl crate::core::tree::IntoElementList for EmojiPickerBuilder { fn into_element_list(self) -> Vec<Element> { vec![self.build()] } }
