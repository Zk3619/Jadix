use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{
    Document, Event, FocusOptions, HtmlDialogElement, HtmlElement, HtmlInputElement,
    HtmlOptionElement, HtmlSelectElement, HtmlTextAreaElement, ScrollIntoViewOptions,
    ScrollLogicalPosition,
};

use crate::{
    host::PlatformHost,
    layout::{LayoutError, MeasureMode, MeasureRequest},
    presentation::*,
};

pub type EventSink = Rc<RefCell<dyn FnMut(String) -> Result<(), String>>>;

mod popover;

struct Listener {
    target: web_sys::EventTarget,
    capture: bool,
    name: &'static str,
    callback: Closure<dyn FnMut(Event)>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback_and_bool(
            self.name,
            self.callback.as_ref().unchecked_ref(),
            self.capture,
        );
    }
}

struct View {
    element: HtmlElement,
    content: Rc<RefCell<NodeContent>>,
    revision: Rc<Cell<u64>>,
    _listeners: Vec<Listener>,
}

enum TextSelection {
    Input(u32, u32, String),
    Textarea(u32, u32, String),
}

struct FocusSnapshot {
    element: HtmlElement,
    selection: Option<TextSelection>,
}

pub struct DomHost {
    document: Document,
    container: HtmlElement,
    views: BTreeMap<NodeId, View>,
    sink: EventSink,
    publishing: Rc<Cell<bool>>,
    live: Rc<Cell<bool>>,
    active_modal: Rc<Cell<Option<NodeId>>>,
    modal_order: Vec<NodeId>,
    popovers: Rc<RefCell<Vec<popover::ActivePopover>>>,
    popover_anchors: Vec<HtmlElement>,
    overlay_listeners: Vec<Listener>,
}

fn error(error: JsValue) -> String {
    format!("DOM: {error:?}")
}

pub(super) fn node_html_id(id: NodeId) -> String {
    format!("jadix-node-{}-{}", id.root.token(), id.token())
}

fn focus_snapshot(document: &Document, subtree: &HtmlElement) -> Option<FocusSnapshot> {
    let active = document.active_element()?.dyn_into::<HtmlElement>().ok()?;
    if !subtree.contains(Some(active.as_ref())) {
        return None;
    }
    let selection = if let Some(input) = active.dyn_ref::<HtmlInputElement>() {
        input
            .selection_start()
            .ok()
            .flatten()
            .zip(input.selection_end().ok().flatten())
            .map(|(start, end)| {
                TextSelection::Input(
                    start,
                    end,
                    input
                        .selection_direction()
                        .unwrap_or_default()
                        .unwrap_or_default(),
                )
            })
    } else {
        active.dyn_ref::<HtmlTextAreaElement>().map(|textarea| {
            TextSelection::Textarea(
                textarea
                    .selection_start()
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                textarea.selection_end().ok().flatten().unwrap_or_default(),
                textarea
                    .selection_direction()
                    .unwrap_or_default()
                    .unwrap_or_default(),
            )
        })
    };
    Some(FocusSnapshot {
        element: active,
        selection,
    })
}

fn restore_focus(document: &Document, snapshot: FocusSnapshot) {
    let element = snapshot.element;
    if !element.is_connected()
        || element.has_attribute("disabled")
        || document.active_element().as_ref() == Some(element.as_ref())
    {
        return;
    }
    let options = FocusOptions::new();
    options.set_prevent_scroll(true);
    element.focus_with_options(&options).ok();
    if document.active_element().as_ref() != Some(element.as_ref()) {
        return;
    }
    match snapshot.selection {
        Some(TextSelection::Input(start, end, direction)) => {
            if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
                input
                    .set_selection_range_with_direction(start, end, &direction)
                    .ok();
            }
        }
        Some(TextSelection::Textarea(start, end, direction)) => {
            if let Some(textarea) = element.dyn_ref::<HtmlTextAreaElement>() {
                textarea
                    .set_selection_range_with_direction(start, end, &direction)
                    .ok();
            }
        }
        None => {}
    }
}

fn create(document: &Document, content: &NodeContent) -> Result<HtmlElement, String> {
    if matches!(
        content,
        NodeContent::Checkbox { .. } | NodeContent::Switch { .. }
    ) {
        let label = document.create_element("label").map_err(error)?;
        let input = document.create_element("input").map_err(error)?;
        input.set_attribute("type", "checkbox").map_err(error)?;
        if matches!(content, NodeContent::Switch { .. }) {
            input.set_attribute("role", "switch").map_err(error)?;
        }
        label.append_child(&input).map_err(error)?;
        let caption = document.create_element("span").map_err(error)?;
        label.append_child(&caption).map_err(error)?;
        return label
            .dyn_into()
            .map_err(|value: web_sys::Element| error(value.into()));
    }
    document
        .create_element(match content {
            NodeContent::Container | NodeContent::Scroll => "div",
            NodeContent::Text(_) => "span",
            NodeContent::Button { .. } => "button",
            NodeContent::Input { .. } => "input",
            NodeContent::Image { .. } => "img",
            NodeContent::Textarea { .. } => "textarea",
            NodeContent::Select { .. } => "select",
            NodeContent::Checkbox { .. } | NodeContent::Switch { .. } => unreachable!(),
            NodeContent::Dialog { .. } => "dialog",
            NodeContent::Popover { .. } => "div",
        })
        .map_err(error)?
        .dyn_into()
        .map_err(|value: web_sys::Element| error(value.into()))
}

fn rgba(color: [u8; 4]) -> String {
    format!(
        "rgba({},{},{},{})",
        color[0],
        color[1],
        color[2],
        f32::from(color[3]) / 255.0
    )
}

fn style(element: &HtmlElement, descriptor: &NodeDescriptor, has_children: bool) -> Result<(), String> {
    let style = element.style();
    let node = &descriptor.style;
    for (name, value) in [
        ("box-sizing", String::from("border-box")),
        ("margin", String::from("0")),
        ("font-family", node.font_family.clone()),
        ("font-size", format!("{}px", node.font_size)),
        ("font-weight", node.font_weight.to_string()),
        (
            "direction",
            match node.direction {
                Some(jadix_core::environment::LayoutDirection::RightToLeft) => "rtl",
                Some(jadix_core::environment::LayoutDirection::LeftToRight) | None => "ltr",
            }
            .into(),
        ),
        ("color", rgba(node.foreground)),
        ("background-color", rgba(node.background)),
        ("white-space", String::from("pre-wrap")),
    ] {
        style.set_property(name, &value).map_err(error)?;
    }
    match &descriptor.accessible_name {
        Some(name) => element.set_attribute("aria-label", name).map_err(error)?,
        None => element.remove_attribute("aria-label").map_err(error)?,
    }
    match &descriptor.accessible_description {
        Some(description) => element
            .set_attribute("aria-description", description)
            .map_err(error)?,
        None => element
            .remove_attribute("aria-description")
            .map_err(error)?,
    }
    let role = match descriptor.accessible_role {
        Some(AccessibleRole::RadioGroup) => Some("radiogroup"),
        Some(AccessibleRole::RadioItem { .. }) => Some("radio"),
        Some(AccessibleRole::TabList) => Some("tablist"),
        Some(AccessibleRole::TabItem) => Some("tab"),
        Some(AccessibleRole::TabPanel) => Some("tabpanel"),
        Some(AccessibleRole::Tree) => Some("tree"),
        Some(AccessibleRole::TreeItem) => Some("treeitem"),
        Some(AccessibleRole::ListBox) => Some("listbox"),
        Some(AccessibleRole::ListBoxOption { .. }) => Some("option"),
        Some(AccessibleRole::Heading) => Some("heading"),
        Some(
            AccessibleRole::Grid { .. }
            | AccessibleRole::GridCell { .. }
            | AccessibleRole::GridColumnHeader { .. },
        ) => None,
        None => None,
    };
    match role {
        Some(role) => element.set_attribute("role", role).map_err(error)?,
        None => element.remove_attribute("role").map_err(error)?,
    }
    let radio = matches!(
        descriptor.accessible_role,
        Some(AccessibleRole::RadioItem { .. })
    );
    let selected_attribute = if radio {
        "aria-checked"
    } else {
        "aria-selected"
    };
    element
        .remove_attribute(if radio {
            "aria-selected"
        } else {
            "aria-checked"
        })
        .map_err(error)?;
    match descriptor.accessible_selected {
        Some(selected) => element
            .set_attribute(selected_attribute, if selected { "true" } else { "false" })
            .map_err(error)?,
        None => element
            .remove_attribute(selected_attribute)
            .map_err(error)?,
    }
    match descriptor.accessible_expanded {
        Some(expanded) => element
            .set_attribute("aria-expanded", if expanded { "true" } else { "false" })
            .map_err(error)?,
        None => element.remove_attribute("aria-expanded").map_err(error)?,
    }
    match &descriptor.content {
        NodeContent::Text(text) => {
            if element.text_content().as_deref() != Some(text) {
                element.set_text_content(Some(text));
            }
        }
        NodeContent::Button { label, enabled } => {
            // Keep authored content attached when an unchanged button is republished.
            if (!has_children || element.child_element_count() == 0)
                && element.text_content().as_deref() != Some(label) {
                element.set_text_content(Some(label));
            }
            let button = element.unchecked_ref::<web_sys::HtmlButtonElement>();
            button.set_disabled(!enabled);
            button.set_type("button");
            if let Some(AccessibleRole::RadioItem { tab_stop } | AccessibleRole::ListBoxOption { tab_stop }) = descriptor.accessible_role {
                element.set_tab_index(if tab_stop { 0 } else { -1 });
            } else {
                element.set_tab_index(0);
            }
        }
        NodeContent::Input {
            value,
            placeholder,
            enabled,
            password,
            ..
        } => {
            let input = element.unchecked_ref::<HtmlInputElement>();
            let kind = if *password { "password" } else { "text" };
            if input.type_() != kind {
                input.set_type(kind);
                if input.type_() != kind {
                    return Err("platform rejected the controlled input type".into());
                }
            }
            if input.value() != *value {
                input.set_value(value);
                if input.value() != *value {
                    return Err("platform rejected the full controlled input value".into());
                }
            }
            input.set_placeholder(placeholder);
            input.set_disabled(!enabled);
        }
        NodeContent::Scroll => {
            style.set_property("overflow-y", "auto").map_err(error)?;
            style.set_property("overflow-x", "hidden").map_err(error)?;
            element.set_tab_index(0);
        }
        NodeContent::Textarea {
            value,
            placeholder,
            enabled,
            ..
        } => {
            let input = element.unchecked_ref::<HtmlTextAreaElement>();
            if input.value() != *value {
                input.set_value(value);
                if input.value() != *value {
                    return Err("platform rejected the full controlled textarea value".into());
                }
            }
            input.set_placeholder(placeholder);
            input.set_disabled(!enabled);
            style.set_property("resize", "none").map_err(error)?;
        }
        NodeContent::Checkbox {
            label,
            checked,
            enabled,
            ..
        }
        | NodeContent::Switch {
            label,
            checked,
            enabled,
            ..
        } => {
            let input = checkbox_input(element);
            match &descriptor.accessible_description {
                Some(description) => input
                    .set_attribute("aria-description", description)
                    .map_err(error)?,
                None => input.remove_attribute("aria-description").map_err(error)?,
            }
            input.set_checked(*checked);
            input.set_disabled(!enabled);
            input
                .set_attribute(
                    "aria-label",
                    descriptor.accessible_name.as_deref().unwrap_or(label),
                )
                .map_err(error)?;
            if let Some(caption) = element.last_element_child() {
                caption.set_text_content(Some(label));
            }
        }
        NodeContent::Select {
            options,
            value,
            placeholder,
            enabled,
            ..
        } => {
            let select = element.unchecked_ref::<HtmlSelectElement>();
            let document = element
                .owner_document()
                .ok_or_else(|| "missing select document".to_string())?;
            let choices: Vec<_> = options
                .iter()
                .map(|option| serde_json::json!([option.value, option.label, option.enabled]))
                .collect();
            let signature = serde_json::json!([placeholder, choices]).to_string();
            if element.get_attribute("data-jadix-options").as_deref() != Some(signature.as_str()) {
                element.set_text_content(None);
                let prompt: HtmlOptionElement = document
                    .create_element("option")
                    .map_err(error)?
                    .unchecked_into();
                prompt.set_value("");
                prompt.set_text(placeholder);
                prompt.set_disabled(true);
                element.append_child(&prompt).map_err(error)?;
                for option in options {
                    let native: HtmlOptionElement = document
                        .create_element("option")
                        .map_err(error)?
                        .unchecked_into();
                    native.set_value(&option.value);
                    native.set_text(&option.label);
                    native.set_disabled(!option.enabled);
                    element.append_child(&native).map_err(error)?;
                }
                element
                    .set_attribute("data-jadix-options", &signature)
                    .map_err(error)?;
            }
            select.set_value(value);
            select.set_disabled(!enabled);
            if select.value() != *value {
                return Err("native select value differs from publication".into());
            }
            element
                .set_attribute(
                    "aria-label",
                    descriptor.accessible_name.as_deref().unwrap_or(placeholder),
                )
                .map_err(error)?;
        }
        NodeContent::Container => {}
        NodeContent::Popover { .. } => {
            for (name, value) in [
                ("popover", "manual"),
                ("role", "dialog"),
                ("aria-modal", "false"),
                ("tabindex", "-1"),
            ] {
                element.set_attribute(name, value).map_err(error)?;
            }
            for (name, value) in [
                ("padding", "0"),
                ("border", "0"),
                ("margin", "0"),
                ("inset", "auto"),
                ("max-width", "none"),
                ("max-height", "none"),
                ("overflow", "auto"),
                ("box-shadow", "0 4px 16px rgba(0, 0, 0, 0.24)"),
                ("border-radius", "4px"),
            ] {
                style.set_property(name, value).map_err(error)?;
            }
        }
        NodeContent::Dialog {
            close_on_overlay_click,
            ..
        } => {
            element
                .set_attribute(
                    "data-jadix-dismiss-backdrop",
                    if *close_on_overlay_click {
                        "true"
                    } else {
                        "false"
                    },
                )
                .map_err(error)?;
            for (name, value) in [
                ("padding", "0"),
                ("border", "0"),
                ("max-width", "none"),
                ("max-height", "none"),
                ("overflow", "hidden"),
            ] {
                style.set_property(name, value).map_err(error)?;
            }
        }
        NodeContent::Image { source, alt } => {
            match &descriptor.accessible_description {
                Some(description) => element
                    .set_attribute("data-jadix-image-description", description)
                    .map_err(error)?,
                None => element
                    .remove_attribute("data-jadix-image-description")
                    .map_err(error)?,
            }
            let image = element.unchecked_ref::<web_sys::HtmlImageElement>();
            let [width, height] = descriptor
                .content
                .image_dimensions()
                .ok_or("invalid native PNG image")?;
            image.set_width(width);
            image.set_height(height);
            image.set_alt(alt);
            if image.src() != *source {
                element
                    .set_attribute("data-jadix-image-state", "loading")
                    .map_err(error)?;
                image.set_src(source);
            }
            if element.get_attribute("data-jadix-image-state").as_deref() == Some("error") {
                style
                    .set_property("background-color", "#eee")
                    .map_err(error)?;
                if descriptor.accessible_description.is_none() {
                    element
                        .set_attribute("aria-description", "Image unavailable")
                        .map_err(error)?;
                }
            }
            style.set_property("object-fit", "contain").map_err(error)?;
        }
    }
    Ok(())
}

fn checkbox_input(element: &HtmlElement) -> HtmlInputElement {
    element
        .first_element_child()
        .expect("checkbox input")
        .unchecked_into()
}

fn editing(element: &HtmlElement) -> (String, usize, usize) {
    let (value, start, end, password) =
        if let Some(input) = element.dyn_ref::<HtmlTextAreaElement>() {
            (
                input.value(),
                input.selection_start().ok().flatten().unwrap_or(0) as usize,
                input.selection_end().ok().flatten().unwrap_or(0) as usize,
                false,
            )
        } else {
            let input = element.unchecked_ref::<HtmlInputElement>();
            (
                input.value(),
                input.selection_start().ok().flatten().unwrap_or(0) as usize,
                input.selection_end().ok().flatten().unwrap_or(0) as usize,
                input.type_() == "password",
            )
        };
    if !password {
        return (value, start, end);
    }
    if start == end {
        let caret = utf16_boundary(&value, start, true);
        if caret != start {
            let input = element.unchecked_ref::<HtmlInputElement>();
            let _ = input.set_selection_range(caret as u32, caret as u32);
        }
        (value, caret, caret)
    } else {
        let normalized_start = utf16_boundary(&value, start, false);
        let normalized_end = utf16_boundary(&value, end, true);
        if (start, end) != (normalized_start, normalized_end) {
            let input = element.unchecked_ref::<HtmlInputElement>();
            let _ = input.set_selection_range(normalized_start as u32, normalized_end as u32);
        }
        (value, normalized_start, normalized_end)
    }
}

fn utf16_boundary(value: &str, offset: usize, round_up: bool) -> usize {
    let mut boundary = 0;
    for character in value.chars() {
        let next = boundary + character.len_utf16();
        if offset < next {
            return if round_up { next } else { boundary };
        }
        if offset == next {
            return offset;
        }
        boundary = next;
    }
    boundary
}

impl DomHost {
    pub fn new(container: HtmlElement, sink: EventSink) -> Result<Self, String> {
        if container.child_element_count() != 0 {
            return Err("native mount must be empty".into());
        }
        let document = container.owner_document().ok_or("detached native mount")?;
        container
            .style()
            .set_property("position", "relative")
            .map_err(error)?;
        let mut host = Self {
            document,
            container,
            views: BTreeMap::new(),
            sink,
            publishing: Rc::new(Cell::new(false)),
            live: Rc::new(Cell::new(true)),
            active_modal: Rc::new(Cell::new(None)),
            modal_order: Vec::new(),
            popovers: Rc::new(RefCell::new(Vec::new())),
            popover_anchors: Vec::new(),
            overlay_listeners: Vec::new(),
        };
        host.install_popover_events()?;
        Ok(host)
    }

    fn view(&self, node: &PresentationNode) -> Result<View, String> {
        let element = create(&self.document, &node.descriptor.content)?;
        element
            .set_attribute(
                "data-jadix-kind",
                match node.descriptor.content {
                    NodeContent::Container => "box",
                    NodeContent::Text(_) => "text",
                    NodeContent::Button { .. } => "button",
                    NodeContent::Input { .. } => "input",
                    NodeContent::Scroll => "scroll",
                    NodeContent::Textarea { .. } => "textarea",
                    NodeContent::Checkbox { .. } => "checkbox",
                    NodeContent::Switch { .. } => "switch",
                    NodeContent::Select { .. } => "select",
                    NodeContent::Image { .. } => "image",
                    NodeContent::Dialog { .. } => "dialog",
                    NodeContent::Popover { .. } => "popover",
                },
            )
            .map_err(error)?;
        element
            .set_attribute("data-jadix-node", &node.id.token())
            .map_err(error)?;
        element.set_id(&node_html_id(node.id));
        let revision = Rc::new(Cell::new(0u64));
        let content = Rc::new(RefCell::new(node.descriptor.content.clone()));
        let composing = Rc::new(Cell::new(None::<usize>));
        let mut listeners = Vec::new();
        if matches!(node.descriptor.content, NodeContent::Image { .. }) {
            for name in ["load", "error"] {
                let target = element.clone();
                let live = self.live.clone();
                let callback = Closure::wrap(Box::new(move |_: Event| {
                    if !live.get() {
                        return;
                    }
                    let failed = name == "error";
                    let _ = target.set_attribute(
                        "data-jadix-image-state",
                        if failed { "error" } else { "loaded" },
                    );
                    if failed {
                        let _ = target.style().set_property("background-color", "#eee");
                    }
                    if let Some(description) = target.get_attribute("data-jadix-image-description")
                    {
                        let _ = target.set_attribute("aria-description", &description);
                    } else if failed {
                        let _ = target.set_attribute("aria-description", "Image unavailable");
                    } else {
                        let _ = target.remove_attribute("aria-description");
                    }
                }) as Box<dyn FnMut(Event)>);
                element
                    .add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())
                    .map_err(error)?;
                listeners.push(Listener {
                    target: element.clone().into(),
                    capture: false,
                    name,
                    callback,
                });
            }
        }
        let events: &[&'static str] = match node.descriptor.content {
            NodeContent::Button { .. } => &["click", "focus", "blur"],
            NodeContent::Input { .. } => &[
                "input",
                "keydown",
                "compositionstart",
                "compositionend",
                "focus",
                "blur",
                "select",
            ],
            NodeContent::Textarea { .. } => &[
                "input",
                "compositionstart",
                "compositionend",
                "focus",
                "blur",
                "select",
            ],
            NodeContent::Checkbox { .. } | NodeContent::Switch { .. } => {
                &["change", "focusin", "focusout"]
            }
            NodeContent::Select { .. } => &["change", "focus", "blur"],
            NodeContent::Scroll => &["scroll"],
            NodeContent::Dialog { .. } => &["cancel", "click", "keydown"],
            NodeContent::Popover { .. } => &["beforetoggle", "toggle"],
            _ => &[],
        };
        for &name in events {
            let target = element.clone();
            let sink = self.sink.clone();
            let live = self.live.clone();
            let publishing = self.publishing.clone();
            let revision = revision.clone();
            let composing = composing.clone();
            let content = content.clone();
            let id = node.id;
            let active_modal = self.active_modal.clone();
            let is_dialog = matches!(node.descriptor.content, NodeContent::Dialog { .. });
            let is_popover = matches!(node.descriptor.content, NodeContent::Popover { .. });
            let callback = Closure::wrap(Box::new(move |event: Event| {
                if !live.get() || publishing.get() {
                    return;
                }
                let cleanup = matches!(name, "blur" | "focusout");
                let enclosing = target.closest("dialog").ok().flatten();
                if let Some(active) = active_modal.get().filter(|_| !cleanup) {
                    if enclosing
                        .as_ref()
                        .and_then(|element| element.get_attribute("data-jadix-node"))
                        != Some(active.token())
                    {
                        return;
                    }
                } else if !cleanup && enclosing.is_some() {
                    return;
                }
                if is_popover {
                    let controlled =
                        matches!(*content.borrow(), NodeContent::Popover { open: true, .. });
                    let shown = target.matches(":popover-open").unwrap_or(false);
                    if name == "beforetoggle" && !shown && !controlled {
                        event.prevent_default();
                    }
                    if name == "toggle" && !shown && controlled {
                        let accepted = popover::dismiss(&sink, id, &revision, false);
                        if accepted.is_err()
                            && live.get()
                            && target.is_connected()
                            && matches!(*content.borrow(), NodeContent::Popover { open: true, .. })
                        {
                            let _ = target.show_popover();
                        }
                    }
                    return;
                }
                if !cleanup
                    && target
                        .closest("[popover]")
                        .ok()
                        .flatten()
                        .is_some_and(|panel| !panel.matches(":popover-open").unwrap_or(false))
                {
                    return;
                }
                if is_dialog {
                    if !target.unchecked_ref::<HtmlDialogElement>().open() {
                        return;
                    }
                    if name == "keydown" {
                        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                            return;
                        };
                        if key.key() != "Tab" {
                            return;
                        }
                        event.stop_propagation();
                        let Ok(candidates) = target.query_selector_all("button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex='0']") else { return; };
                        let controls: Vec<_> = (0..candidates.length())
                            .filter_map(|index| candidates.item(index))
                            .filter_map(|node| node.dyn_into::<HtmlElement>().ok())
                            .filter(|element| {
                                let rect = element.get_bounding_client_rect();
                                rect.width() > 0.0
                                    && rect.height() > 0.0
                                    && element.tab_index() >= 0
                            })
                            .collect();
                        let active = target
                            .owner_document()
                            .and_then(|document| document.active_element());
                        let next = if key.shift_key()
                            && active.as_ref() == controls.first().map(|element| element.as_ref())
                        {
                            controls.last()
                        } else if !key.shift_key()
                            && active.as_ref() == controls.last().map(|element| element.as_ref())
                        {
                            controls.first()
                        } else {
                            None
                        };
                        if let Some(next) = next {
                            event.prevent_default();
                            let _ = next.focus();
                        } else if controls.is_empty() {
                            event.prevent_default();
                        }
                        return;
                    }
                    let backdrop = name == "click";
                    if backdrop {
                        if event.target().as_ref() != Some(target.as_ref()) {
                            return;
                        }
                        let Some(mouse) = event.dyn_ref::<web_sys::MouseEvent>() else {
                            return;
                        };
                        let rect = target.get_bounding_client_rect();
                        let x = f64::from(mouse.client_x());
                        let y = f64::from(mouse.client_y());
                        if (x >= rect.left()
                            && x < rect.right()
                            && y >= rect.top()
                            && y < rect.bottom())
                            || target
                                .get_attribute("data-jadix-dismiss-backdrop")
                                .as_deref()
                                != Some("true")
                        {
                            return;
                        }
                    }
                    event.prevent_default();
                    let Some(next) = revision.get().checked_add(1) else {
                        live.set(false);
                        return;
                    };
                    revision.set(next);
                    if let Ok(mut sink) = sink.try_borrow_mut() {
                        let _ = sink(serde_json::json!({"root": id.root.token(), "node": id.token(), "payload": {"kind": "dismiss", "backdrop": backdrop, "revision": next.to_string()}}).to_string());
                    }
                    return;
                }
                let payload = match name {
                    "blur" | "focusout" => serde_json::json!({"kind": "blur"}),
                    "keydown" => {
                        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                            return;
                        };
                        if key.key() != "Enter"
                            || key.ctrl_key()
                            || key.alt_key()
                            || key.meta_key()
                            || key.repeat()
                            || key.is_composing()
                            || key.key_code() == 229
                            || composing.get().is_some()
                            || !matches!(
                                *content.borrow(),
                                NodeContent::Input { enabled: true, .. }
                            )
                        {
                            return;
                        }
                        event.prevent_default();
                        event.stop_propagation();
                        let Some(next) = revision.get().checked_add(1) else {
                            live.set(false);
                            return;
                        };
                        revision.set(next);
                        serde_json::json!({"kind": "submit", "revision": next.to_string()})
                    }
                    "click" => {
                        let _ = target.focus();
                        serde_json::json!({"kind": "activate"})
                    }
                    "focus" | "focusin" => {
                        let options = ScrollIntoViewOptions::new();
                        options.set_block(ScrollLogicalPosition::Nearest);
                        options.set_inline(ScrollLogicalPosition::Nearest);
                        target.scroll_into_view_with_scroll_into_view_options(&options);
                        let focus_target = event
                            .target()
                            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                            .unwrap_or_else(|| target.clone().unchecked_into());
                        let kind = if focus_target.matches(":focus-visible").unwrap_or(false) {
                            "focusVisible"
                        } else {
                            "focus"
                        };
                        serde_json::json!({"kind": kind})
                    }
                    "change" => {
                        let Some(next) = revision.get().checked_add(1) else {
                            live.set(false);
                            return;
                        };
                        revision.set(next);
                        if target.tag_name() == "SELECT" {
                            serde_json::json!({"kind": "select", "value": target.unchecked_ref::<HtmlSelectElement>().value(), "revision": next.to_string()})
                        } else {
                            serde_json::json!({"kind": "toggle", "checked": checkbox_input(&target).checked(), "revision": next.to_string()})
                        }
                    }
                    "scroll" => {
                        serde_json::json!({"kind": "scroll", "x": target.scroll_left(), "y": target.scroll_top()})
                    }
                    _ => {
                        let (value, start, end) = editing(&target);
                        if name == "compositionstart" {
                            composing.set(Some(start));
                            return;
                        }
                        if name == "compositionend" {
                            composing.set(None);
                        }
                        let Some(next) = revision.get().checked_add(1) else {
                            live.set(false);
                            return;
                        };
                        revision.set(next);
                        let range = composing
                            .get()
                            .map(|origin| [origin.min(start), end.max(origin)]);
                        serde_json::json!({"kind": "edit", "value": value, "revision": next.to_string(), "selection": [start.min(end), end.max(start)], "composing": range})
                    }
                };
                if let Ok(mut sink) = sink.try_borrow_mut() {
                    let result = sink(serde_json::json!({"root": id.root.token(), "node": id.token(), "payload": payload}).to_string());
                    if result.is_err() && live.get() {
                        composing.set(None);
                        match &*content.borrow() {
                            NodeContent::Input { value, .. } => {
                                target.unchecked_ref::<HtmlInputElement>().set_value(value);
                            }
                            NodeContent::Select { value, .. } => {
                                target.unchecked_ref::<HtmlSelectElement>().set_value(value);
                            }
                            NodeContent::Textarea { value, .. } => {
                                target
                                    .unchecked_ref::<HtmlTextAreaElement>()
                                    .set_value(value);
                            }
                            NodeContent::Checkbox { checked, .. }
                            | NodeContent::Switch { checked, .. } => {
                                checkbox_input(&target).set_checked(*checked);
                            }
                            _ => {}
                        }
                    }
                }
            }) as Box<dyn FnMut(Event)>);
            element
                .add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())
                .map_err(error)?;
            listeners.push(Listener {
                target: element.clone().into(),
                capture: false,
                name,
                callback,
            });
        }
        Ok(View {
            element,
            content,
            revision,
            _listeners: listeners,
        })
    }

    fn sync_accessibility_relations(&self, roots: &[PresentationNode]) -> Result<(), String> {
        for view in self.views.values() {
            view.element
                .remove_attribute("aria-activedescendant")
                .map_err(error)?;
            view.element
                .remove_attribute("aria-controls")
                .map_err(error)?;
        }
        let mut pending: Vec<_> = roots.iter().rev().collect();
        while let Some(node) = pending.pop() {
            let Some(view) = self.views.get(&node.id) else {
                continue;
            };
            let relations = &node.descriptor.accessibility_relations;
            if let Some(target) = relations
                .active_descendant
                .filter(|target| target.root == node.id.root && self.views.contains_key(target))
            {
                view.element
                    .set_attribute("aria-activedescendant", &node_html_id(target))
                    .map_err(error)?;
            }
            let mut controls = BTreeSet::new();
            for target in &relations.controls {
                if target.root == node.id.root && self.views.contains_key(target) {
                    controls.insert(node_html_id(*target));
                }
            }
            if let Some(popovers) = view.element.get_attribute("data-jadix-popover-controls") {
                controls.extend(popovers.split_ascii_whitespace().map(str::to_owned));
            }
            if !controls.is_empty() {
                view.element
                    .set_attribute(
                        "aria-controls",
                        &controls.into_iter().collect::<Vec<_>>().join(" "),
                    )
                    .map_err(error)?;
            }
            pending.extend(node.children.iter().rev());
        }
        Ok(())
    }

    fn publish_nodes(
        &mut self,
        nodes: &[PresentationNode],
        parent: HtmlElement,
        origin: [f64; 2],
        live: &mut BTreeSet<NodeId>,
    ) -> Result<(), String> {
        for (index, node) in nodes.iter().enumerate() {
            live.insert(node.id);
            if !self.views.contains_key(&node.id) {
                self.views.insert(node.id, self.view(node)?);
            }
            let view = self.views.get_mut(&node.id).unwrap();
            if std::mem::discriminant(&*view.content.borrow())
                != std::mem::discriminant(&node.descriptor.content)
            {
                return Err("native node kind changed".into());
            }
            if let NodeContent::Input { revision, .. }
            | NodeContent::Textarea { revision, .. }
            | NodeContent::Checkbox { revision, .. }
            | NodeContent::Switch { revision, .. }
            | NodeContent::Select { revision, .. }
            | NodeContent::Dialog { revision, .. }
            | NodeContent::Popover { revision, .. } = node.descriptor.content
            {
                view.revision.set(view.revision.get().max(revision));
            }
            style(&view.element, &node.descriptor, !node.children.is_empty())?;
            let frame = node.descriptor.frame;
            let is_portal = matches!(
                node.descriptor.content,
                NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            );
            let viewport_origin = if is_portal {
                let rect = self.container.get_bounding_client_rect();
                [rect.left(), rect.top()]
            } else {
                [0.0; 2]
            };
            for (name, value) in [
                (
                    "position",
                    if is_portal { "fixed" } else { "absolute" }.into(),
                ),
                (
                    "left",
                    format!(
                        "{}px",
                        if is_portal {
                            frame.x + viewport_origin[0]
                        } else {
                            frame.x - origin[0]
                        }
                    ),
                ),
                (
                    "top",
                    format!(
                        "{}px",
                        if is_portal {
                            frame.y + viewport_origin[1]
                        } else {
                            frame.y - origin[1]
                        }
                    ),
                ),
                ("width", format!("{}px", frame.width)),
                ("height", format!("{}px", frame.height)),
            ] {
                view.element
                    .style()
                    .set_property(name, &value)
                    .map_err(error)?;
            }
            let current = parent.children().item(index as u32);
            if current.as_ref() != Some(view.element.as_ref()) {
                let focus = focus_snapshot(&self.document, &view.element);
                if focus.is_some()
                    && view.element.parent_element().as_ref() == Some(parent.as_ref())
                {
                    while parent.children().item(index as u32).as_ref()
                        != Some(view.element.as_ref())
                    {
                        let sibling = parent
                            .children()
                            .item(index as u32)
                            .ok_or("focused native subtree has no sibling at its authored index")?;
                        parent
                            .insert_before(&sibling, view.element.next_sibling().as_ref())
                            .map_err(error)?;
                    }
                } else {
                    parent
                        .insert_before(
                            &view.element,
                            current.as_ref().map(|element| element.as_ref()),
                        )
                        .map_err(error)?;
                    if let Some(focus) = focus {
                        restore_focus(&self.document, focus);
                    }
                }
            }
            *view.content.borrow_mut() = node.descriptor.content.clone();
            let element = view.element.clone();
            self.publish_nodes(&node.children, element, [frame.x, frame.y], live)?;
        }
        Ok(())
    }
}

impl PlatformHost for DomHost {
    fn capabilities(&self) -> capabilities::BackendCapabilities {
        let mut capabilities = capabilities::BackendCapabilities::headless();
        let supported = ["showPopover", "hidePopover"].into_iter().all(|method| {
            js_sys::Reflect::get(self.container.as_ref(), &JsValue::from_str(method))
                .is_ok_and(|value| value.is_function())
        });
        if !supported {
            capabilities
                .node_kinds
                .retain(|kind| *kind != capabilities::NodeKind::Popover);
        }
        capabilities
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        let result = (|| -> Result<[f64; 2], String> {
            let element = create(&self.document, &descriptor.content)?;
            style(&element, descriptor, false)?;
            let style = element.style();
            style.set_property("position", "fixed").map_err(error)?;
            style.set_property("visibility", "hidden").map_err(error)?;
            style.set_property("left", "-100000px").map_err(error)?;
            for (axis, mode, max) in [
                ("width", request.width_mode, request.key.max_width),
                ("height", request.height_mode, request.key.max_height),
            ] {
                let value = match mode {
                    MeasureMode::Definite => format!("{max}px"),
                    MeasureMode::MinContent => "min-content".into(),
                    MeasureMode::MaxContent => "max-content".into(),
                };
                style.set_property(axis, &value).map_err(error)?;
                style
                    .set_property(&format!("max-{axis}"), &format!("{max}px"))
                    .map_err(error)?;
            }
            let body = self.document.body().ok_or("missing DOM body")?;
            body.append_child(&element).map_err(error)?;
            let rect = element.get_bounding_client_rect();
            element.remove();
            Ok([
                rect.width().min(request.key.max_width),
                rect.height().min(request.key.max_height),
            ])
        })();
        let [width, height] =
            result.map_err(|_| LayoutError::MeasurementUnavailable(request.key.node))?;
        Ok(MeasureResult {
            key: request.key,
            width,
            height,
            baseline: None,
        })
    }

    fn publish(&mut self, _: RootId, _: u64, roots: &[PresentationNode]) -> Result<(), String> {
        self.publishing.set(true);
        let retained_popup_focus = focus_snapshot(&self.document, &self.container)
            .filter(|focus| focus.element.closest("[popover]").ok().flatten().is_some());
        let mut live = BTreeSet::new();
        let result = self.publish_nodes(roots, self.container.clone(), [0.0; 2], &mut live);
        let result = result.and_then(|_| self.retire_popovers(roots));
        for id in self
            .modal_order
            .iter()
            .rev()
            .filter(|id| !live.contains(id))
        {
            if let Some(view) = self.views.get(id) {
                view.element.unchecked_ref::<HtmlDialogElement>().close();
            }
        }
        self.views.retain(|id, view| {
            if live.contains(id) {
                true
            } else {
                if let Some(dialog) = view.element.dyn_ref::<HtmlDialogElement>() {
                    dialog.close();
                }
                view.element.remove();
                false
            }
        });
        let result = result.and_then(|_| {
            let mut opened = Vec::new();
            let mut pending: Vec<_> = roots.iter().rev().collect();
            while let Some(node) = pending.pop() {
                if matches!(
                    node.descriptor.content,
                    NodeContent::Popover { open: false, .. }
                ) {
                    continue;
                }
                if let NodeContent::Dialog { open, .. } = node.descriptor.content {
                    if !open {
                        continue;
                    }
                    opened.push(node.id);
                }
                pending.extend(node.children.iter().rev());
            }
            self.active_modal.set(opened.last().copied());
            let prefix = self
                .modal_order
                .iter()
                .zip(&opened)
                .take_while(|(previous, next)| previous == next)
                .count();
            for id in self.modal_order[prefix..].iter().rev() {
                if let Some(view) = self.views.get(id) {
                    view.element.unchecked_ref::<HtmlDialogElement>().close();
                }
            }
            for (id, view) in &self.views {
                if let Some(dialog) = view.element.dyn_ref::<HtmlDialogElement>() {
                    if !opened.contains(id) && dialog.open() {
                        dialog.close();
                    }
                }
            }
            for id in &opened {
                let dialog = self.views[id].element.unchecked_ref::<HtmlDialogElement>();
                if !dialog.open() {
                    dialog.show_modal().map_err(error)?;
                }
            }
            self.modal_order = opened;
            self.sync_popovers(roots)?;
            self.sync_accessibility_relations(roots)?;
            if let Some(focus) = retained_popup_focus {
                if focus
                    .element
                    .closest("[popover]")
                    .ok()
                    .flatten()
                    .is_some_and(|panel| panel.matches(":popover-open").unwrap_or(false))
                {
                    restore_focus(&self.document, focus);
                }
            }
            Ok(())
        });
        self.publishing.set(false);
        result
    }

    fn dispose(&mut self) {
        self.live.set(false);
        self.dispose_popovers();
        self.active_modal.set(None);
        for id in self.modal_order.iter().rev() {
            if let Some(view) = self.views.get(id) {
                view.element.unchecked_ref::<HtmlDialogElement>().close();
            }
        }
        self.modal_order.clear();
        self.views.clear();
        self.container.set_text_content(None);
    }
}

impl Drop for DomHost {
    fn drop(&mut self) {
        self.dispose();
    }
}
